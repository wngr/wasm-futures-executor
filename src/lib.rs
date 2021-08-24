use futures::channel::mpsc;
use futures::StreamExt;
use js_sys::Promise;
use log::*;
use wasm_bindgen::prelude::*;

use self::pool::WorkerPool;

mod pool;

#[wasm_bindgen(start)]
pub fn main() {
    let _ = console_log::init_with_level(log::Level::Info);
    ::console_error_panic_hook::set_once();
    info!("Set up logging");
}
#[wasm_bindgen]
pub struct Export {}

#[wasm_bindgen]
pub fn start(pool: WorkerPool) -> Promise {
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
