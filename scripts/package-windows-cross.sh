#!/usr/bin/env bash
# EchoPort — Linux → Windows x86_64 cross build + zip (CI / QA artifact).
#
# This is *not* the official Windows build path: official packages are built
# natively on Windows with the MSVC toolchain (see scripts/build-windows.ps1).
# The cross build is useful for CI smoke checks and producing a PE artifact on
# Linux hosts without Windows.
#
# Prerequisites (tested on Arch Linux):
#   rustup target add x86_64-pc-windows-gnu
#   # mingw-w64 GCC + binutils for the target. Examples:
#   #   Arch:   sudo pacman -S --needed mingw-w64-gcc
#   #   Ubuntu: sudo apt install gcc-mingw-w64-x86-64-posix \
#   #              g++-mingw-w64-x86-64-posix binutils-mingw-w64-x86-64
#   # If the compiler is not on PATH, set MINGW_PREFIX to its prefix, e.g.
#   #   MINGW_PREFIX=/opt/mingw ./scripts/package-windows-cross.sh
#
# Output:
#   dist/echoport-<version>-windows-x86_64.zip
#     echoport.exe
#     README-WINDOWS.txt   (from packaging/README-WINDOWS.md)
#     echoport.ico
#     README.md, LICENSE, docs/, examples/
#   and the matching .sha256 file.
#
# The resulting .exe is unsigned and has **not been executed on Windows** by
# this script; verify it on a real Windows machine before distributing.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
TARGET="x86_64-pc-windows-gnu"
SKIP_BUILD=0
for arg in "$@"; do
    case "$arg" in
        --skip-build) SKIP_BUILD=1 ;;
        -h|--help) sed -n '2,30p' "$0"; exit 0 ;;
        *) echo "unknown argument: $arg" >&2; exit 2 ;;
    esac
done

if [ -n "${MINGW_PREFIX:-}" ]; then
    export PATH="$MINGW_PREFIX/bin:$PATH"
fi
if ! rustup target list --installed 2>/dev/null | grep -qx "$TARGET"; then
    echo "missing Rust target $TARGET — run: rustup target add $TARGET" >&2
    exit 1
fi
if [ "$SKIP_BUILD" -eq 0 ]; then
    if ! command -v "${CC_x86_64_pc_windows_gnu:-x86_64-w64-mingw32-gcc}" >/dev/null 2>&1; then
        echo "missing x86_64-w64-mingw32-gcc — install mingw-w64 (see header)" >&2
        exit 1
    fi
    echo "==> cargo build --release --target $TARGET"
    cargo build --release --target "$TARGET"
fi

EXE="target/$TARGET/release/echoport.exe"
[ -f "$EXE" ] || { echo "missing $EXE" >&2; exit 1; }

VERSION=$(python3 - <<'PY'
import json, subprocess
meta = json.loads(subprocess.check_output(["cargo", "metadata", "--no-deps", "--format-version", "1"]))
print(meta["packages"][0]["version"])
PY
)
NAME="echoport-$VERSION-windows-x86_64"
STAGE="dist/$NAME"
ZIP="dist/$NAME.zip"
echo "==> staging $STAGE"
rm -rf "$STAGE" "$ZIP" "$ZIP.sha256"
mkdir -p "$STAGE/docs" "$STAGE/examples/plugins"

cp "$EXE" "$STAGE/echoport.exe"
cp "packaging/README-WINDOWS.md" "$STAGE/README-WINDOWS.txt"
cp "assets/echoport.ico" "$STAGE/echoport.ico"
cp "README.md" "$STAGE/README.md"
cp "LICENSE" "$STAGE/LICENSE"
for doc in SCRIPTING.md PLUGINS.md plugins.html config.html; do
    [ -f "docs/$doc" ] && cp "docs/$doc" "$STAGE/docs/$doc"
done
for f in examples/config.lua examples/config.zh-CN.lua examples/config.en.lua; do
    [ -f "$f" ] && cp "$f" "$STAGE/examples/$(basename "$f")"
done
cp "examples/plugins/example.lua" "$STAGE/examples/plugins/example.lua"

{
    echo ""
    echo "=========================================================="
    echo "EchoPort v$VERSION — Windows x86_64 package (cross-built, unsigned)"
    echo "Built: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "This .exe was cross-compiled on Linux and has NOT been run on Windows."
    echo "Native Windows builds are documented in scripts/build-windows.ps1."
    echo "=========================================================="
} >> "$STAGE/README-WINDOWS.txt"

echo "==> creating $ZIP"
python3 - "$STAGE" "$ZIP" <<'PY'
import os, sys, zipfile
stage, out = sys.argv[1], sys.argv[2]
with zipfile.ZipFile(out, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as z:
    for root, _dirs, files in os.walk(stage):
        for name in sorted(files):
            path = os.path.join(root, name)
            z.write(path, os.path.relpath(path, os.path.dirname(stage)))
PY
(cd "$(dirname "$ZIP")" && sha256sum "$(basename "$ZIP")") > "$ZIP.sha256"
echo ""
echo "artifact: $ZIP"
echo "  size:   $(stat -c %s "$ZIP") bytes"
echo "  sha256: $(cut -d' ' -f1 "$ZIP.sha256")"
echo "  note:   cross-built/unsigned; not executed on Windows."
