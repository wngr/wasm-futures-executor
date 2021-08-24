use std::time::Duration;

use futures::channel::mpsc;
use futures::StreamExt;
use instant::Instant;
use js_sys::Promise;
use log::*;
use wasm_bindgen::{prelude::*, JsCast};
use web_sys::{DedicatedWorkerGlobalScope, Navigator};

use self::pool::WorkerPool;
use self::thread_pool::ThreadPool;

mod pool;
mod thread_pool;
mod unpark_mutex;
// Note: `atomics` is whitelisted in `target_feature` detection, but `bulk-memory` isn't,
// so we can check only presence of the former. This should be enough to catch most common
// mistake (forgetting to pass `RUSTFLAGS` altogether).
#[cfg(not(target_feature = "atomics"))]
compile_error!("Did you forget to enable `atomics` and `bulk-memory` features as outlined in wasm-bindgen-rayon README?");

#[wasm_bindgen(start)]
pub fn main() {
    let _ = console_log::init_with_level(log::Level::Info);
    ::console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub struct Export {}

#[wasm_bindgen]
pub fn start() -> Promise {
    let pool = WorkerPool::new(8).unwrap();
    let (tx, mut rx) = mpsc::channel(10);
    for i in 0..10 {
        let mut tx_c = tx.clone();
        pool.run(move || {
            tx_c.start_send(i * i).unwrap();
        })
        .unwrap();
    }
    wasm_bindgen_futures::future_to_promise(async move {
        let mut i = 0;
        while let Some(x) = rx.next().await {
            i += x;
        }
        Ok(i.into())
    })
}

#[wasm_bindgen]
pub fn start_async() -> Promise {
    let pool_size = web_sys::window()
        .unwrap()
        .navigator()
        .hardware_concurrency() as usize;
    let pool = ThreadPool::new(pool_size).unwrap();
    let (tx, mut rx) = mpsc::channel(10);
    for i in 0..10 {
        let mut tx_c = tx.clone();
        pool.spawn_ok(async move {
            let global = js_sys::global().unchecked_into::<DedicatedWorkerGlobalScope>();
            info!("Task {} running on {}", i, global.name());
            let now = Instant::now();
            while now.elapsed() < Duration::from_secs(5) {}
            tx_c.start_send(i * i).unwrap();
        });
    }
    wasm_bindgen_futures::future_to_promise(async move {
        let mut i = 0;
        while let Some(x) = rx.next().await {
            i += x;
        }
        Ok(i.into())
    })
}
