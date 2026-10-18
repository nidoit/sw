#!/usr/bin/env sh
set -e
cd "$(dirname "$0")"
wasm-pack build --target web --release --out-dir pkg
echo
echo "Built ./pkg. Serve this folder, e.g.:"
echo "  python3 -m http.server 8080   →  http://localhost:8080"
