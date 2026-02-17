#!/usr/bin/env bash
# generate-icons.sh — Convert icon.svg to .ico (Windows) and .icns (macOS)
#
# Requirements:
#   - ImageMagick 7+ (provides `magick` command)
#   - macOS only: `iconutil` (ships with Xcode Command Line Tools)
#
# Usage:
#   bash resources/generate-icons.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SVG="$SCRIPT_DIR/icon.svg"

if [ ! -f "$SVG" ]; then
  echo "Error: $SVG not found" >&2
  exit 1
fi

# ---------------------------------------------------------------------------
# Windows .ico  (sizes: 16, 24, 32, 48, 64, 128, 256)
# ---------------------------------------------------------------------------
generate_ico() {
  local ico_sizes=(16 24 32 48 64 128 256)
  local tmp_pngs=()

  echo "Generating Windows icon..."
  for size in "${ico_sizes[@]}"; do
    local out="$SCRIPT_DIR/icon_${size}.png"
    magick "$SVG" -resize "${size}x${size}" "$out"
    tmp_pngs+=("$out")
  done

  magick "${tmp_pngs[@]}" "$SCRIPT_DIR/icon.ico"
  rm -f "${tmp_pngs[@]}"
  echo "Created $SCRIPT_DIR/icon.ico"
}

# ---------------------------------------------------------------------------
# macOS .icns  (sizes: 16, 32, 64, 128, 256, 512, 1024)
# ---------------------------------------------------------------------------
generate_icns() {
  local iconset="$SCRIPT_DIR/icon.iconset"
  mkdir -p "$iconset"

  echo "Generating macOS icon..."

  # Standard and @2x variants required by iconutil
  local -A sizes=(
    ["icon_16x16"]=16
    ["icon_16x16@2x"]=32
    ["icon_32x32"]=32
    ["icon_32x32@2x"]=64
    ["icon_128x128"]=128
    ["icon_128x128@2x"]=256
    ["icon_256x256"]=256
    ["icon_256x256@2x"]=512
    ["icon_512x512"]=512
    ["icon_512x512@2x"]=1024
  )

  for name in "${!sizes[@]}"; do
    local px="${sizes[$name]}"
    magick "$SVG" -resize "${px}x${px}" "$iconset/${name}.png"
  done

  if command -v iconutil &>/dev/null; then
    iconutil -c icns "$iconset" -o "$SCRIPT_DIR/icon.icns"
    rm -rf "$iconset"
    echo "Created $SCRIPT_DIR/icon.icns"
  else
    echo "Warning: iconutil not found (macOS only). Iconset left at $iconset"
    echo "Run on macOS: iconutil -c icns $iconset -o $SCRIPT_DIR/icon.icns"
  fi
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
if ! command -v magick &>/dev/null; then
  echo "Error: ImageMagick 7+ (magick) is required but not found." >&2
  echo "Install: https://imagemagick.org/script/download.php" >&2
  exit 1
fi

generate_ico
generate_icns

echo "Done."
