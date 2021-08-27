use std::time::Duration;

use futures::channel::mpsc;
use futures::StreamExt;
use js_sys::Promise;
use log::*;
use wasm_bindgen::{prelude::*, JsCast};
use wasm_futures_executor::ThreadPool;
use web_sys::DedicatedWorkerGlobalScope;

#[wasm_bindgen(start)]
pub fn main() {
    let _ = console_log::init_with_level(log::Level::Debug);
    ::console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub fn start() -> Promise {
    let pool = ThreadPool::new(2).unwrap();
    let (tx, mut rx) = mpsc::channel(10);
    for i in 0..5 {
        let mut tx_c = tx.clone();
        let p_c = pool.clone();
        pool.spawn_ok(async move {
            let name = js_sys::global()
                .unchecked_into::<DedicatedWorkerGlobalScope>()
                .name()
                .as_str()
                .to_string();
            info!("Task {} running on {}", i, name);
            let mut x = 0;
            for j in 0..20 {
                let p_c = p_c.clone();
                x += async move {
                    let name = js_sys::global()
                        .unchecked_into::<DedicatedWorkerGlobalScope>()
                        .name()
                        .as_str()
                        .to_string();
                    info!("Task {}-{} running on {}", i, j, name);
                    let mut y = 0;
                    for k in 0..3 {
                        y += p_c
                            .spawn(async move {
                                let name = js_sys::global()
                                    .unchecked_into::<DedicatedWorkerGlobalScope>()
                                    .name()
                                    .as_str()
                                    .to_string();
                                info!("Task {}-{}-{} running on {}", i, j, k, name);
                                k
                            })
                            .await
                            .unwrap();
                    }
                    futures_timer::Delay::new(Duration::from_secs(1)).await;
                    j + y
                }
                .await;
            }
            tx_c.start_send(i * x).unwrap();
        });
    }
    wasm_bindgen_futures::future_to_promise(async move {
        // Don't drop the pool until we're finished
        let _x = pool;
        let mut i = 0;
        while let Some(x) = rx.next().await {
            i += x;
        }
        Ok(i.into())
    })
}
