#!/bin/bash -e


RUSTFLAGS='-C target-feature=+atomics,+bulk-memory,+mutable-globals' \
  cargo +nightly build --target wasm32-unknown-unknown --release -Z build-std=std,panic_abort

wasm-bindgen \
  ./target/wasm32-unknown-unknown/release/wasm_thread_pool.wasm \
  --out-dir . \
  --target no-modules
