#!/usr/bin/env bash
# Build the macOS .app bundle + zip. CI-only: requires macOS (sips/iconutil/lipo).
# Locally on Linux this script only documents the steps and exits with an error.
#
# CI usage (see packaging/ci/release-workflow.patch):
#   rustup target add aarch64-apple-darwin x86_64-apple-darwin
#   cargo build --release --target aarch64-apple-darwin
#   cargo build --release --target x86_64-apple-darwin
#   lipo -create -output echoport-universal <arm-binary> <x86_64-binary>
#   ./scripts/package-macos.sh --binary echoport-universal
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
BIN=""
while [ $# -gt 0 ]; do
    case "$1" in
        --binary) BIN="$2"; shift 2 ;;
        -h|--help) sed -n '1,20p' "$0"; exit 0 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done
if [ "$(uname -s)" != "Darwin" ]; then
    echo "package-macos.sh must run on macOS (CI macos-latest)" >&2
    exit 1
fi
command -v sips >/dev/null && command -v iconutil >/dev/null || { echo "missing sips/iconutil" >&2; exit 1; }

VERSION=$(tr -d '[:space:]' < VERSION)
BIN="${BIN:-target/release/echoport}"
[ -f "$BIN" ] || { echo "missing binary: $BIN" >&2; exit 1; }

APP="dist/EchoPort.app"
rm -rf "$APP" "dist/EchoPort.app.zip"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources/docs" "$APP/Contents/Resources/examples"
install -m0755 "$BIN" "$APP/Contents/MacOS/echoport"
install -m0644 README.md LICENSE "$APP/Contents/Resources/"
for doc in SCRIPTING.md PLUGINS.md plugins.html config.html; do
    cp "docs/$doc" "$APP/Contents/Resources/docs/$doc"
done
for cfg in config.lua config.zh-CN.lua config.en.lua; do
    cp "examples/$cfg" "$APP/Contents/Resources/examples/$cfg"
done
cp examples/plugins/example.lua "$APP/Contents/Resources/examples/example.lua"

ICONSET="dist/EchoPort.iconset"
rm -rf "$ICONSET"; mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" assets/icon.png --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
    sips -z "$((size*2))" "$((size*2))" assets/icon.png --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null 2>&1 || true
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/echoport.icns"
rm -rf "$ICONSET"

cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>EchoPort</string>
  <key>CFBundleDisplayName</key><string>EchoPort</string>
  <key>CFBundleIdentifier</key><string>com.nilotoo.echoport</string>
  <key>CFBundleVersion</key><string>__VERSION__</string>
  <key>CFBundleShortVersionString</key><string>__VERSION__</string>
  <key>CFBundleExecutable</key><string>echoport</string>
  <key>CFBundleIconFile</key><string>echoport.icns</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
sed -i '' "s/__VERSION__/$VERSION/g" "$APP/Contents/Info.plist" 2>/dev/null || sed -i "s/__VERSION__/$VERSION/g" "$APP/Contents/Info.plist"
( cd dist && zip -qry "EchoPort.app.zip" "EchoPort.app" )
sha256sum "dist/EchoPort.app.zip" > "dist/EchoPort.app.zip.sha256"
echo "artifact: dist/EchoPort.app.zip"
echo "  note:   CI-only build; ad-hoc unsigned bundle, not notarized."
