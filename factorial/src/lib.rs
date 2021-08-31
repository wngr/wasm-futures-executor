use futures::future::try_join_all;
use num::{BigUint, One};
use std::ops::Mul;
use wasm_bindgen::prelude::*;
use wasm_futures_executor::ThreadPool;

#[wasm_bindgen(start)]
pub fn main() {
    let _ = console_log::init_with_level(log::Level::Debug);
    ::console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub fn factorial_single(n: u32) -> String {
    let x = _factorial(1, n);
    format!("{}", x)
}

fn _factorial(a: u32, b: u32) -> BigUint {
    (a..=b).map(BigUint::from).fold(BigUint::one(), Mul::mul)
}

#[wasm_bindgen]
pub async fn factorial_multi(tp: ThreadPool, n: u32) -> Result<JsValue, JsValue> {
    let chunk_size = n / 8;
    let mut x = 1;
    let mut futs = vec![];
    while x < n {
        let to = n.min(x + chunk_size);
        futs.push(tp.spawn(async move { _factorial(x, to) }));
        x = to + 1;
    }
    let result = try_join_all(futs).await.unwrap();
    let r = result.into_iter().fold(BigUint::from(1u32), Mul::mul);
    Ok(r.to_string().into())
}

#[wasm_bindgen]
pub async fn start_tp() -> Result<JsValue, JsValue> {
    let pool = ThreadPool::max_threads().await?;
    Ok(pool.into())
}
