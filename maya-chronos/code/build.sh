#!/usr/bin/env sh
# Builds ../pkg/maya_chronos.js + ../pkg/maya_chronos_bg.wasm (the only two files the page needs).
#
# The wasm must go through wasm-bindgen. The raw cargo output
# (target/wasm32-unknown-unknown/release/maya_chronos.wasm) imports "__wbindgen_placeholder__"
# and fails in the browser with:
#   TypeError: import object field '__wbindgen_placeholder__' is not an Object
# so never copy that file into pkg/ by hand.
set -e
cd "$(dirname "$0")"
rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true

# wasm-bindgen CLI must match the wasm-bindgen crate version in Cargo.lock exactly.
want=$(grep -A1 '^name = "wasm-bindgen"$' Cargo.lock | sed -n 's/^version = "\(.*\)"/\1/p')
have=$(wasm-bindgen --version 2>/dev/null | awk '{print $2}')
if [ "$have" != "$want" ]; then
  echo "wasm-bindgen CLI $want needed (found: ${have:-none}). Install with:"
  echo "  cargo install wasm-bindgen-cli --version $want --locked"
  exit 1
fi

cargo build --release --target wasm32-unknown-unknown
rm -rf ../pkg
wasm-bindgen target/wasm32-unknown-unknown/release/maya_chronos.wasm \
  --target web --no-typescript --out-dir ../pkg
rm -f ../pkg/.gitignore ../pkg/package.json
# Optional: shrink the wasm (~45%) if binaryen's wasm-opt is installed (npm i -g binaryen).
if command -v wasm-opt >/dev/null 2>&1; then
  wasm-opt -Os --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext \
    --enable-mutable-globals --enable-reference-types --enable-multivalue \
    ../pkg/maya_chronos_bg.wasm -o ../pkg/maya_chronos_bg.wasm
fi
ls -l ../pkg
# Cache-bust: tag the pkg URLs in index.html with the new wasm's hash.
v=$(cksum ../pkg/maya_chronos_bg.wasm | cut -d' ' -f1)
sed -i.bak "s/\(maya_chronos[_a-z]*\.[a-z]*\)?v=[A-Za-z0-9]*/\1?v=$v/g" ../index.html && rm -f ../index.html.bak
echo
echo "Upload index.html + pkg/ (not code/). Local test, from the maya-chronos folder:"
echo "  python3 -m http.server 8080   →  http://localhost:8080"
