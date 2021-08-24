use futures::{Future, FutureExt};
use futures_task::{waker_ref, ArcWake, Context, FutureObj, Poll, Spawn, SpawnError};
use log::*;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use wasm_bindgen::{prelude::*, JsCast};
use web_sys::{DedicatedWorkerGlobalScope, Worker, WorkerOptions, WorkerType};

use crate::unpark_mutex::UnparkMutex;

trait AssertSendSync: Send + Sync {}
impl AssertSendSync for ThreadPool {}
pub struct ThreadPool {
    state: Arc<PoolState>,
}

impl Clone for ThreadPool {
    fn clone(&self) -> Self {
        self.state.cnt.fetch_add(1, Ordering::Relaxed);
        Self {
            state: self.state.clone(),
        }
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        if self.state.cnt.fetch_sub(1, Ordering::Relaxed) == 1 {
            for _ in 0..self.state.size {
                self.state.send(Message::Close);
            }
        }
    }
}

impl Spawn for ThreadPool {
    fn spawn_obj(&self, future: FutureObj<'static, ()>) -> Result<(), SpawnError> {
        self.spawn_obj_ok(future);
        Ok(())
    }
}

impl ThreadPool {
    pub fn new(size: usize) -> Result<ThreadPool, JsValue> {
        let (tx, rx) = mpsc::channel();
        let pool = ThreadPool {
            state: Arc::new(PoolState {
                tx: Mutex::new(tx),
                rx: Mutex::new(rx),
                cnt: AtomicUsize::new(1),
                size,
            }),
        };

        for idx in 0..size {
            let state = pool.state.clone();

            let mut opts = WorkerOptions::new();
            opts.type_(WorkerType::Module);
            opts.name(&*format!("Worker-{}", idx));
            let worker = Worker::new_with_options("./worker.js", &opts)?;

            // With a worker spun up send it the module/memory so it can start
            // instantiating the wasm module. Later it might receive further
            // messages about code to run on the wasm module.
            let array = js_sys::Array::new();
            array.push(&wasm_bindgen::module());
            array.push(&wasm_bindgen::memory());
            worker.post_message(&array)?;
            let ptr = Arc::into_raw(state);
            worker.post_message(&JsValue::from(ptr as u32))?;
        }
        Ok(pool)
    }
    /// Spawns a future that will be run to completion.
    ///
    /// > **Note**: This method is similar to `Spawn::spawn_obj`, except that
    /// >           it is guaranteed to always succeed.
    pub fn spawn_obj_ok(&self, future: FutureObj<'static, ()>) {
        let task = Task {
            future,
            wake_handle: Arc::new(WakeHandle {
                exec: self.clone(),
                mutex: UnparkMutex::new(),
            }),
            exec: self.clone(),
        };
        self.state.send(Message::Run(task));
    }

    /// Spawns a task that polls the given future with output `()` to
    /// completion.
    ///
    /// ```
    /// use futures::executor::ThreadPool;
    ///
    /// let pool = ThreadPool::new().unwrap();
    ///
    /// let future = async { /* ... */ };
    /// pool.spawn_ok(future);
    /// ```
    ///
    /// > **Note**: This method is similar to `SpawnExt::spawn`, except that
    /// >           it is guaranteed to always succeed.
    pub fn spawn_ok<Fut>(&self, future: Fut)
    where
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.spawn_obj_ok(FutureObj::new(Box::new(future)))
    }
}

enum Message {
    Run(Task),
    Close,
}

pub struct PoolState {
    tx: Mutex<mpsc::Sender<Message>>,
    rx: Mutex<mpsc::Receiver<Message>>,
    cnt: AtomicUsize,
    size: usize,
}
impl PoolState {
    fn send(&self, msg: Message) {
        self.tx.lock().send(msg).unwrap();
    }

    fn work(&self) {
        //        let _scope = enter().unwrap();
        loop {
            let msg = self.rx.lock().recv().unwrap();
            match msg {
                Message::Run(task) => task.run(),
                Message::Close => break,
            }
        }
    }
}
/// A task responsible for polling a future to completion.
struct Task {
    future: FutureObj<'static, ()>,
    exec: ThreadPool,
    wake_handle: Arc<WakeHandle>,
}

impl Task {
    /// Actually run the task (invoking `poll` on the future) on the current
    /// thread.
    fn run(self) {
        let Self {
            mut future,
            wake_handle,
            mut exec,
        } = self;
        let waker = waker_ref(&wake_handle);
        let mut cx = Context::from_waker(&waker);

        // Safety: The ownership of this `Task` object is evidence that
        // we are in the `POLLING`/`REPOLL` state for the mutex.
        unsafe {
            wake_handle.mutex.start_poll();

            loop {
                let res = future.poll_unpin(&mut cx);
                match res {
                    Poll::Pending => {}
                    Poll::Ready(()) => return wake_handle.mutex.complete(),
                }
                let task = Self {
                    future,
                    wake_handle: wake_handle.clone(),
                    exec,
                };
                match wake_handle.mutex.wait(task) {
                    Ok(()) => return, // we've waited
                    Err(task) => {
                        // someone's notified us
                        future = task.future;
                        exec = task.exec;
                    }
                }
            }
        }
    }
}

impl ArcWake for WakeHandle {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        match arc_self.mutex.notify() {
            Ok(task) => arc_self.exec.state.send(Message::Run(task)),
            Err(()) => {}
        }
    }
}

struct WakeHandle {
    mutex: UnparkMutex<Task>,
    exec: ThreadPool,
}

/// Entry point invoked by `worker.js`
#[wasm_bindgen]
pub fn worker_entry_point(state_ptr: u32) {
    let state = unsafe { Arc::<PoolState>::from_raw(state_ptr as *const PoolState) };

    let global = js_sys::global().unchecked_into::<DedicatedWorkerGlobalScope>();
    debug!("{} spawned", global.name());
    state.work();
    debug!("{} yield", global.name());
}
