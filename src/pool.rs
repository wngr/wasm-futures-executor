use futures::channel::oneshot;
use futures::future::LocalBoxFuture;
use futures::{channel::mpsc, Future};
use futures::{FutureExt, StreamExt};
use js_sys::{JsString, Promise};
use log::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use wasm_bindgen::{prelude::*, JsCast};
use web_sys::{DedicatedWorkerGlobalScope, WorkerOptions, WorkerType};

trait AssertSendSync: Send + Sync {}
impl AssertSendSync for ThreadPool {}

type JsTask = Box<dyn FnOnce() -> LocalBoxFuture<'static, ()> + Send + 'static>;

/// A general-purpose thread pool for scheduling tasks that poll futures to
/// completion.
///
/// The thread pool multiplexes any number of tasks onto a fixed number of
/// worker threads.
///
/// This type is a clonable handle to the threadpool itself.
/// Cloning it will only create a new reference, not a new threadpool.
///
/// The API follows [`futures_executor::ThreadPool`].
///
/// [`futures_executor::ThreadPool`]: https://docs.rs/futures-executor/0.3.16/futures_executor/struct.ThreadPool.html
#[wasm_bindgen]
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

#[wasm_bindgen]
pub struct LoaderHelper {}
#[wasm_bindgen]
impl LoaderHelper {
    #[wasm_bindgen(js_name = mainJS)]
    pub fn main_js(&self) -> JsString {
        #[wasm_bindgen]
        extern "C" {
            #[wasm_bindgen(js_namespace = ["import", "meta"], js_name = url)]
            static URL: JsString;
        }

        URL.clone()
    }
}

#[wasm_bindgen(module = "/worker.js")]
extern "C" {
    #[wasm_bindgen(js_name = "startWorker")]
    /// Returns Promise<Worker>
    fn start_worker(
        module: JsValue,
        memory: JsValue,
        shared_data: JsValue,
        opts: WorkerOptions,
        builder: LoaderHelper,
    ) -> Promise;
}

impl ThreadPool {
    /// Creates a new [`ThreadPool`] with the provided count of web workers. The returned future
    /// will resolve after all workers have spawned and are ready to accept work.
    pub async fn new(size: usize) -> Result<ThreadPool, JsValue> {
        let (tx, rx) = mpsc::channel(64);
        let pool = ThreadPool {
            state: Arc::new(PoolState {
                tx: parking_lot::Mutex::new(tx),
                rx: tokio::sync::Mutex::new(rx),
                cnt: AtomicUsize::new(1),
                size,
            }),
        };

        for idx in 0..size {
            let state = pool.state.clone();

            let mut opts = WorkerOptions::new();
            opts.type_(WorkerType::Module);
            opts.name(&*format!("Worker-{}", idx));

            // With a worker spun up send it the module/memory so it can start
            // instantiating the wasm module. Later it might receive further
            // messages about code to run on the wasm module.
            let ptr = Arc::into_raw(state);
            let _worker = wasm_bindgen_futures::JsFuture::from(start_worker(
                wasm_bindgen::module(),
                wasm_bindgen::memory(),
                JsValue::from(ptr as u32),
                opts,
                LoaderHelper {},
            ))
            .await?;
            // TODO: Check that workers actually spawned.
        }
        Ok(pool)
    }

    /// Creates a new [`ThreadPool`] with `Navigator.hardwareConcurrency` web workers.
    pub async fn max_threads() -> Result<Self, JsValue> {
        #[wasm_bindgen]
        extern "C" {
            #[wasm_bindgen(js_namespace = navigator, js_name = hardwareConcurrency)]
            static HARDWARE_CONCURRENCY: usize;
        }
        let pool_size = std::cmp::max(*HARDWARE_CONCURRENCY, 1);
        Self::new(pool_size).await
    }

    pub fn spawn_lazy<Gen>(&self, gen: Gen)
    where
        Gen: FnOnce() -> LocalBoxFuture<'static, ()> + Send + 'static,
    {
        let t: JsTask = Box::new(gen);
        self.state.send(Message::Run(Box::new(t)));
    }

    /// Spawns a task that polls the given future with output `()` to
    /// completion.
    ///
    /// ```
    /// use wasm_futures_executor::ThreadPool;
    ///
    /// let pool = ThreadPool::new().await.unwrap();
    ///
    /// let future = async { /* ... */ };
    /// pool.spawn_ok(future);
    /// ```
    pub fn spawn_ok<Fut>(&self, future: Fut)
    where
        Fut: Future<Output = ()> + Send + 'static,
    {
        let future = future.boxed();
        self.spawn_lazy(|| future);
    }

    /// Spawns a task. This function returns a future which eventually resolves to the output of
    /// the computation.
    /// Note: The provided future is polled on the thread pool, no matter whether the returned
    /// future is polled or not.
    pub fn spawn<Fut>(
        &self,
        future: Fut,
    ) -> impl Future<Output = Result<Fut::Output, oneshot::Canceled>> + 'static
    where
        Fut: Future + Send + 'static,
        Fut::Output: Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        let f = async move {
            let res = future.await;
            let _ = tx.send(res);
        };

        self.spawn_ok(f);
        rx
    }
}

enum Message {
    Run(JsTask),
    Close,
}

pub struct PoolState {
    tx: parking_lot::Mutex<mpsc::Sender<Message>>,
    rx: tokio::sync::Mutex<mpsc::Receiver<Message>>,
    cnt: AtomicUsize,
    size: usize,
}

impl PoolState {
    fn send(&self, msg: Message) {
        self.tx.lock().start_send(msg).unwrap()
    }

    fn work(slf: Arc<PoolState>) {
        let driver = async move {
            let global = js_sys::global().unchecked_into::<DedicatedWorkerGlobalScope>();
            while let Some(msg) = slf.rx.lock().await.next().await {
                match msg {
                    Message::Run(gen) => wasm_bindgen_futures::spawn_local(gen()),
                    Message::Close => break,
                }
            }
            info!("{}: Shutting down", global.name());
            global.close();
        };
        wasm_bindgen_futures::spawn_local(driver);
    }
}

/// Entry point invoked by the web worker. The passed pointer will be unconditionally interpreted
/// as an `Arc<PoolState>`.
#[wasm_bindgen(skip_typescript)]
pub fn worker_entry_point(state_ptr: u32) {
    let state = unsafe { Arc::<PoolState>::from_raw(state_ptr as *const PoolState) };

    let name = js_sys::global()
        .unchecked_into::<DedicatedWorkerGlobalScope>()
        .name();
    debug!("{}: Entry", name);
    PoolState::work(state);
}
