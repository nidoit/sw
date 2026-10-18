#!/usr/bin/env sh
# Cache-bust: tags the css/js URLs in index.html with a hash of their contents (?v=...),
# so browsers never mix a new index.html with old cached scripts. Run after editing css/ or js/.
set -e
cd "$(dirname "$0")/.."
v=$(cat css/*.css js/*.js | cksum | cut -d' ' -f1)
sed -E -i.bak "s/(\.(js|css))\?v=[A-Za-z0-9]*/\1?v=$v/g" index.html && rm -f index.html.bak
echo "index.html → ?v=$v"
