# wasm-futures-executor

[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/wngr/wasm-futures-executor)
[![Cargo](https://img.shields.io/crates/v/wasm-futures-executor.svg)](https://crates.io/crates/wasm-futures-executor)
[![Documentation](https://docs.rs/wasm-futures-executor/badge.svg)](https://docs.rs/wasm-futures-executor)

This crate provides an executor for asynchronous task with the same
API as [`futures_executor::ThreadPool`] targeting the web browser
environment. Instead of using spawning threads via `std::thread`, web
workers are created. This crate tries hard to make this process as
seamless and painless as possible.

[`futures_executor::ThreadPool`]: https://docs.rs/futures-executor/0.3.16/futures_executor/struct.ThreadPool.html

## Sample Usage
```rust
use futures::channel::mpsc;
use futures::StreamExt;
use js_sys::Promise;
use wasm_bindgen::prelude::*;
use wasm_futures_executor::ThreadPool;

#[wasm_bindgen]
pub fn start() -> Promise {
    let pool = ThreadPool::max_threads().unwrap();
    let (tx, mut rx) = mpsc::channel(10);
    for i in 0..20 {
        let mut tx_c = tx.clone();
        pool.spawn_ok(async move {
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
```
.. and using it:
```javascript
import init, { start } from './sample.js';

async function run() {
  await init();

  const res = await start();
  console.log("result", res);
}
run();
```

And build your project with:
```sh
RUSTFLAGS='-C target-feature=+atomics,+bulk-memory,+mutable-globals' \
  cargo +nightly build --target wasm32-unknown-unknown --release -Z build-std=std,panic_abort

wasm-bindgen \
  ./target/wasm32-unknown-unknown/release/sample.wasm \
  --out-dir . \
  --target web \
  --weak-refs
``` 
.. or if you want to use `wasm-pack build -t web`, make sure to set up
the nightly toolchain and the proper `RUSTFLAGS` (for example by
providing creating the `rust-toolchain.toml` and `.cargo/config` files
like done in this repo).

Please have a look at the [sample](./sample) for a complete end-to-end
example project.

Note: This crate assumes usage of the `web` target of
`wasm-bindgen`/`wasm-pack`. Given the broad standardization on ES
Modules, this should be mostly fine.

## How it works

Similar to the async executor provided by
[`futures-executor`](https://crates.io/crates/futures-executor),
multiple worker "threads" are spawned upon instantation of the
`ThreadPool`. Each thread is a web worker, which loads some js glue
code. The glue code is inlined and passed as an object url to the
Worker's constructor. In order to load the main js file, a small hack
is used to figure out the origin and the script's filename
(inspiration taken from the
[wasm_thread](https://github.com/chemicstry/wasm_thread) crate).
Each web worker expects exactly two messages:
1. The first to initialize the WebAssembly module and its shared memory
([`SharedArrayBuffer`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/SharedArrayBuffer)).
2. The second one is a pointer to some shared state, including a channel,
where the async tasks are passed in. The library provides the 
`worker_entry_point` function for this purpose.

Once the `ThreadPool` is dropped, all channels are closed and the web
workers are terminated.

Unfortunately, this requires a nightly compilers because Rust's
standard library needs to be recompiled with the following unstable
features:
* `atomics`: (rust feature) supports for wasm atomics, see
  https://github.com/rust-lang/rust/issues/77839
* `bulk-memory`: (llvm feature) generation of atomic instruction,
  shared memory, passive segments, etc.
* `mutable-globals`: (llvm feature)

A note: When workers are destroyed, some memory might be leaked (for
example thread local storage or the thead's stack). I recommend
passing the `--weak-ref` option to `wasm-bindgen` in order to let
wasm-bindgen create user defined finalizers according to the [WeakRef
proposal](https://github.com/tc39/proposal-weakrefs).

In general, implementing wasm threads in Rust seems to have lost some
of its traction ..


## Browser support

Sharing memory via
[`SharedArrayBuffer`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/SharedArrayBuffer)
will soon (starting with Chrome 92) require setting the right headers
to mark the website as "cross-origin isolated", meaning that the
following headers need to be sent with the main document:
```
Cross-Origin-Embedder-Policy: require-corp
Cross-Origin-Opener-Policy: same-origin
```
For more information check out [this](https://web.dev/coop-coep/)
link.

Loading modules in web workers is currently only supported in
Chromium. There seems the be finally some
[progress](https://bugzilla.mozilla.org/show_bug.cgi?id=1247687) in
Firefox. As a workaround, you can include a
[workers-polyfill](https://unpkg.com/module-workers-polyfill).

If you're targeting older browser, you might want to do some feature
detection and fall back gracefully -- but that's out of scope for this
documentation.

## Related crates and further information

* The first and foremost resource is the great raytracing demo of the
[wasm-bindgen
docs](https://rustwasm.github.io/wasm-bindgen/examples/raytrace.html)
and its background [blog
post](https://rustwasm.github.io/2018/10/24/multithreading-rust-and-wasm.html).

* The
[wasm-bindgen-rayon](https://github.com/GoogleChromeLabs/wasm-bindgen-rayon)
crate provides extensive documentation and a nice end-to-end example.

* The [wasm_thread](https://github.com/chemicstry/wasm_thread) crate
  is quite similar in its approach as this crate. The main difference
  is the usage of ES modules.

## License

Licensed under either of

 * Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

#### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
