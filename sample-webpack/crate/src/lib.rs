use std::time::Duration;

use futures::channel::mpsc;
use futures::StreamExt;
use instant::Instant;
use log::*;
use wasm_bindgen::{prelude::*, JsCast};
use wasm_futures_executor::ThreadPool;
use web_sys::DedicatedWorkerGlobalScope;

#[wasm_bindgen(start)]
pub fn main() {
    let _ = console_log::init_with_level(log::Level::Info);
    ::console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub async fn start() -> Result<JsValue, JsValue> {
    let pool = ThreadPool::max_threads().await?;
    let (tx, mut rx) = mpsc::channel(10);
    for i in 0..20 {
        let mut tx_c = tx.clone();
        pool.spawn_ok(async move {
            let global = js_sys::global().unchecked_into::<DedicatedWorkerGlobalScope>();
            info!("Task {} running on {}", i, global.name());
            let now = Instant::now();
            // Block worker
            while now.elapsed() < Duration::from_secs(2) {}
            tx_c.start_send(i * i).unwrap();
        });
    }
    drop(tx);
    let mut i = 0;
    while let Some(x) = rx.next().await {
        i += x;
    }
    Ok(i.into())
}
