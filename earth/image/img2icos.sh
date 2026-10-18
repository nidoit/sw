#!/bin/bash

# img2icos.sh - Convert input.png to various icon sizes
# Usage: ./img2icos.sh [input.png]

INPUT="${1:-input.png}"

if [ ! -f "$INPUT" ]; then
  echo "Error: File '$INPUT' not found."
  echo "Usage: $0 [input.png]"
  exit 1
fi

echo "Converting '$INPUT' to icon sizes..."

magick "$INPUT" -resize 32x32!   -colorspace sRGB -type TrueColorAlpha 32x32.png
echo "✓ 32x32.png"

magick "$INPUT" -resize 128x128! -colorspace sRGB -type TrueColorAlpha 128x128.png
echo "✓ 128x128.png"

magick "$INPUT" -resize 256x256! -colorspace sRGB -type TrueColorAlpha 128x128@2x.png
echo "✓ 128x128@2x.png"

magick "$INPUT" -resize 256x256! -colorspace sRGB -type TrueColorAlpha icon.png
echo "✓ icon.png"

magick "$INPUT" -resize 256x256! -colorspace sRGB -type TrueColorAlpha icon.ico
echo "✓ icon.ico"

magick input.png -resize 640x480 -background white -gravity center -extent 640x480 logo.png
echo "✓ logo.png"

echo "Done!"
