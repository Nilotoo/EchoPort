#!/usr/bin/env bash
# Build the Windows NSIS installer (with directory selection + uninstaller) on Linux.
# Requires makensis (Debian/Ubuntu: nsis + nsis-common; Arch/AUR: nsis).
# Unsigned; not executed on Windows here.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

EXE="target/x86_64-pc-windows-gnu/release/echoport.exe"
TOOLS_DIR=""
OUT=""
VARIANT="full"
while [ $# -gt 0 ]; do
    case "$1" in
        --exe) EXE="$2"; shift 2 ;;
        --tools-dir) TOOLS_DIR="$2"; shift 2 ;;
        --out) OUT="$2"; shift 2 ;;
        --lean) VARIANT="lean"; shift ;;
        -h|--help) sed -n '1,20p' "$0"; exit 0 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done
[ -f "$EXE" ] || { echo "missing $EXE (run the cross build first)" >&2; exit 1; }
command -v makensis >/dev/null 2>&1 || { echo "makensis not found (install nsis)" >&2; exit 1; }

VERSION=$(tr -d '[:space:]' < VERSION)
VERSION4="$(python3 - "$VERSION" <<'PY'
import re,sys
m=re.match(r'^(\d+)\.(\d+)\.(\d+)',sys.argv[1])
if not m: raise SystemExit('bad version')
print('.'.join(m.groups())+'.0')
PY
)"
if [ "$VARIANT" = "lean" ]; then
    NAME="echoport-$VERSION-windows-x86_64-lean"
else
    NAME="echoport-$VERSION-windows-x86_64"
fi
STAGE="$ROOT/dist/_nsis/$NAME"
OUT="${OUT:-$ROOT/dist/$NAME-setup.exe}"
rm -rf "$STAGE"
mkdir -p "$STAGE/docs" "$STAGE/examples"

install -m755 "$EXE" "$STAGE/echoport.exe"
install -m644 packaging/README-WINDOWS.md "$STAGE/README-WINDOWS.txt"
install -m644 README.md LICENSE NOTICE "$STAGE/"
install -m644 assets/echoport.ico "$STAGE/echoport.ico"
install -m644 packaging/windows/clean-uninstall.cmd "$STAGE/clean-uninstall.cmd"
install -m644 packaging/windows/THIRD-PARTY.md "$STAGE/THIRD-PARTY.md"
if [ -n "$TOOLS_DIR" ] && [ -d "$TOOLS_DIR" ]; then
    mkdir -p "$STAGE/tools"
    cp -a "$TOOLS_DIR"/. "$STAGE/tools/"
fi
if [ "$VARIANT" = "full" ]; then
    for f in SCRIPTING.md PLUGINS.md plugins.html config.html; do
        install -m644 "docs/$f" "$STAGE/docs/$f"
    done
    install -m644 examples/config.lua examples/config.zh-CN.lua examples/config.en.lua "$STAGE/examples/"
    install -m644 examples/plugins/example.lua "$STAGE/examples/example.lua"
fi

sed -e "s|@VERSION4@|$VERSION4|g" \
    -e "s|@VERSION@|$VERSION|g" \
    -e "s|@STAGE@|$STAGE|g" \
    -e "s|@LICENSE@|$ROOT/LICENSE|g" \
    -e "s|@ICON@|$ROOT/assets/echoport.ico|g" \
    -e "s|@OUTFILE@|$OUT|g" \
    packaging/windows/installer.nsi.in > "$STAGE/installer.nsi"

echo "==> makensis: $OUT"
makensis -V2 "$STAGE/installer.nsi"
[ -s "$OUT" ] || { echo "makensis did not produce $OUT" >&2; exit 1; }
( cd "$(dirname "$OUT")" && sha256sum "$(basename "$OUT")" > "$(basename "$OUT").sha256" )
echo "artifact: $OUT"
ls -lh "$OUT"
