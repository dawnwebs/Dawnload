#!/bin/zsh
# Downloads the SIL OFL fonts the brand build outlines into SVG paths.
set -euo pipefail
DIR="$(cd "$(dirname "$0")" && pwd)/fonts"
mkdir -p "$DIR"
base="https://raw.githubusercontent.com/google/fonts/main/ofl"
curl -sfL -o "$DIR/unbounded.ttf" "$base/unbounded/Unbounded%5Bwght%5D.ttf"
curl -sfL -o "$DIR/martianmono.ttf" "$base/martianmono/MartianMono%5Bwdth,wght%5D.ttf"
curl -sfL -o "$DIR/manrope.ttf" "$base/manrope/Manrope%5Bwght%5D.ttf"
echo "Fonts in $DIR"
