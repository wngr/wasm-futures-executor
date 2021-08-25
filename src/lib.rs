///! This crate provides an executor for asynchronous task with the same
///! API as [`futures-executor::ThreadPool`] targeting the web browser
///! environment. Instead of using spawning threads via `std::thread`, web
///! workers are created. This crate tries hard to make this process as
///! seamless and painless as possible.
///!
///! For further information and usage examples please check the [repository].
///!
///! [`futures_executor::ThreadPool`]: https://docs.rs/futures-executor/0.3.16/futures_executor/struct.ThreadPool.html
///! [repository]: https://github.com/wngr/wasm-futures-executor
mod pool;
mod unpark_mutex;

pub use self::pool::ThreadPool;

#[cfg(not(any(target_feature = "atomics", doc)))]
compile_error!("Make sure to build std with `RUSTFLAGS='-C target-feature=+atomics,+bulk-memory,+mutable-globals'`");
