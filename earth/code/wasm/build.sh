#!/usr/bin/env sh
# Builds earth.wasm (no wasm-bindgen; plain C-ABI exports) and copies it to earth/earth.wasm (next to index.html).
set -e
cd "$(dirname "$0")"
rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true
cargo build --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/earth_wasm.wasm ../../earth.wasm
ls -l ../../earth.wasm
