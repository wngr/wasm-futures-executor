///! TODO
mod pool;
mod unpark_mutex;

pub use self::pool::ThreadPool;
// Note: `atomics` is whitelisted in `target_feature` detection, but `bulk-memory` isn't,
// so we can check only presence of the former. This should be enough to catch most common
// mistake (forgetting to pass `RUSTFLAGS` altogether).
#[cfg(not(target_feature = "atomics"))]
compile_error!("Did you forget to enable `atomics` and `bulk-memory` features as outlined in wasm-bindgen-rayon README?");
