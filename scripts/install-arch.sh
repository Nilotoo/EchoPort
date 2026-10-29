#!/usr/bin/env bash
# Install EchoPort for the current Arch user without sudo.
#   ./scripts/install-arch.sh [path-to-echoport-binary]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${1:-$ROOT/target/release/echoport}"
PREFIX="${PREFIX:-$HOME/.local}"
APPS="$PREFIX/share/applications"
ICONS="$PREFIX/share/icons/hicolor"

if [ ! -x "$BIN" ]; then
    echo "missing executable: $BIN (run: cargo build --release)" >&2
    exit 1
fi

install -Dm755 "$BIN" "$PREFIX/bin/echoport"
install -Dm644 "$ROOT/packaging/echoport.desktop" "$APPS/echoport.desktop"
install -Dm644 "$ROOT/assets/icon-32.png" "$ICONS/32x32/apps/echoport.png"
install -Dm644 "$ROOT/assets/icon-48.png" "$ICONS/48x48/apps/echoport.png"
install -Dm644 "$ROOT/assets/icon-128.png" "$ICONS/128x128/apps/echoport.png"
install -Dm644 "$ROOT/assets/icon.png" "$ICONS/256x256/apps/echoport.png"

if command -v desktop-file-validate >/dev/null 2>&1; then
    desktop-file-validate "$APPS/echoport.desktop" || true
fi
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$APPS" >/dev/null 2>&1 || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -t -f "$PREFIX/share/icons/hicolor" >/dev/null 2>&1 || true
fi

echo "Installed EchoPort:"
echo "  binary : $PREFIX/bin/echoport"
echo "  desktop: $APPS/echoport.desktop"
echo "  icons  : $ICONS/{32x32,48x48,128x128,256x256}/apps/echoport.png"
echo
echo "Make sure $PREFIX/bin is on PATH; then launch 'EchoPort' from your app menu"
echo "or run: echoport"
