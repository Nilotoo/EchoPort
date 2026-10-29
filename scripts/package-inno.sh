#!/usr/bin/env bash
# Build the Inno Setup installer. Runs natively on Windows (iscc in PATH) or on
# Linux through Wine when ISCC points at a Windows ISCC.exe.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

EXE="target/x86_64-pc-windows-gnu/release/echoport.exe"
TOOLS_DIR=""
ISCC="${ISCC:-}"
WINE="${WINE:-}"
OUT=""
while [ $# -gt 0 ]; do
    case "$1" in
        --exe) EXE="$2"; shift 2 ;;
        --tools-dir) TOOLS_DIR="$2"; shift 2 ;;
        --iscc) ISCC="$2"; shift 2 ;;
        --wine) WINE="$2"; shift 2 ;;
        -h|--help) sed -n '1,20p' "$0"; exit 0 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done
copy() { mkdir -p "$(dirname "$2")"; cp "$1" "$2"; }
[ -f "$EXE" ] || { echo "missing $EXE" >&2; exit 1; }
if [ -z "$ISCC" ]; then
    if command -v iscc >/dev/null 2>&1; then ISCC="$(command -v iscc)"; fi
    if [ -z "$ISCC" ] && [ -x "/home/nilotoo/EchoPort/.testing/wineprefix64/drive_c/InnoSetup/ISCC.exe" ]; then
        ISCC="/home/nilotoo/EchoPort/.testing/wineprefix64/drive_c/InnoSetup/ISCC.exe"
    fi
fi
[ -n "$ISCC" ] || { echo "ISCC.exe not found (install Inno Setup or set ISCC)" >&2; exit 1; }

VERSION=$(tr -d '[:space:]' < VERSION)
NAME="echoport-$VERSION-windows-x86_64"
STAGE="$ROOT/dist/_inno/$NAME"
OUTDIR="$ROOT/dist"
rm -rf "$STAGE"
mkdir -p "$STAGE/docs" "$STAGE/examples"
copy "$EXE" "$STAGE/echoport.exe"
copy packaging/README-WINDOWS.md "$STAGE/README-WINDOWS.txt"
copy README.md "$STAGE/README.md"; copy LICENSE "$STAGE/LICENSE"; copy NOTICE "$STAGE/NOTICE"
copy assets/echoport.ico "$STAGE/echoport.ico"
copy packaging/windows/clean-uninstall.cmd "$STAGE/clean-uninstall.cmd"
copy packaging/windows/THIRD-PARTY.md "$STAGE/THIRD-PARTY.md"
printf '%s\n' "EchoPort $VERSION" "This installer places EchoPort in your user profile and does not need administrator rights." "It includes yt-dlp and ffmpeg; the uninstaller can optionally remove settings and caches." > "$STAGE/INSTALL-INFO.txt"
if [ -n "$TOOLS_DIR" ] && [ -d "$TOOLS_DIR" ]; then
    mkdir -p "$STAGE/tools"
    cp -a "$TOOLS_DIR"/. "$STAGE/tools/"
fi
for f in SCRIPTING.md PLUGINS.md plugins.html config.html; do copy "docs/$f" "$STAGE/docs/$f"; done
copy examples/config.lua "$STAGE/examples/config.lua"; copy examples/config.zh-CN.lua "$STAGE/examples/config.zh-CN.lua"; copy examples/config.en.lua "$STAGE/examples/config.en.lua"
copy examples/plugins/example.lua "$STAGE/examples/example.lua"

ISS="$STAGE/installer.iss"
python3 - "$STAGE" "$ROOT" "$OUTDIR" "$VERSION" <<'PY'
import pathlib, sys
stage, root, outdir, version = sys.argv[1:]
def winpath(path):
    # Inno/Wine paths: use Z:\ for Unix absolute paths under Wine, native otherwise.
    p=pathlib.Path(path).resolve()
    if sys.platform != 'win32':
        return 'Z:' + str(p).replace('/', '\\')
    return str(p)
text = pathlib.Path(root,'packaging/windows/installer-inno.iss.in').read_text()
text = text.replace('@VERSION@', version)
text = text.replace('@STAGE@', winpath(stage))
text = text.replace('@OUTDIR@', winpath(outdir))
text = text.replace('@ICON@', winpath(pathlib.Path(stage,'echoport.ico')))
text = text.replace('@LICENSE@', winpath(pathlib.Path(stage,'LICENSE')))
text = text.replace('@INFOFILE@', winpath(pathlib.Path(stage,'INSTALL-INFO.txt')))
pathlib.Path(stage,'installer.iss').write_text(text)
PY

if [ -n "$WINE" ]; then
    # Inno's command line parser treats a Unix absolute path as an option;
    # pass the equivalent Wine (Z:) Windows path instead.
    WIN_ISS="Z:$(printf '%s' "$ISS" | sed 's#/#\\#g')"
    WINEPREFIX="${WINEPREFIX:-$ROOT/.testing/wineprefix64}" WINEDEBUG=-all \
        "$WINE" "$ISCC" "$WIN_ISS"
else
    "$ISCC" "$ISS"
fi
OUT="$OUTDIR/$NAME-inno-setup.exe"
[ -s "$OUT" ] || { echo "ISCC did not produce $OUT" >&2; exit 1; }
( cd "$OUTDIR" && sha256sum "$(basename "$OUT")" > "$(basename "$OUT").sha256" )
echo "artifact: $OUT"
ls -lh "$OUT"
