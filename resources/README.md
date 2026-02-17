# App Icon Resources

This directory holds the source icon and generated platform assets for the Claw application.

## Files

| File                 | Purpose                                  |
| -------------------- | ---------------------------------------- |
| `icon.svg`           | Source vector icon (edit this)            |
| `icon.ico`           | Windows installer icon (generated)       |
| `icon.icns`          | macOS installer icon (generated)         |
| `generate-icons.sh`  | Conversion script (SVG to ico/icns)      |

## Required Formats

### Windows (.ico)

Sizes: 16x16, 24x24, 32x32, 48x48, 64x64, 128x128, 256x256

### macOS (.icns)

Sizes (with @2x retina variants): 16x16 through 512x512@2x (1024x1024)

## Generating Icons

### Prerequisites

- **ImageMagick 7+** (`magick` command) — [download](https://imagemagick.org/script/download.php)
- **macOS only:** `iconutil` (ships with Xcode Command Line Tools)

### Steps

```bash
# From the repository root:
bash resources/generate-icons.sh
```

This produces `icon.ico` (all platforms) and `icon.icns` (requires macOS for the final step).

On non-macOS systems the script generates the `.iconset` folder; transfer it to a Mac and run:

```bash
iconutil -c icns resources/icon.iconset -o resources/icon.icns
```

## Referencing from cargo-packager

In the packager config, point to these files:

```toml
[package.metadata.packager]
icons = ["resources/icon.ico", "resources/icon.icns"]
```
