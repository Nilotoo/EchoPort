#!/usr/bin/env bash
# Build the Windows MSI installer for EchoPort on Linux using msitools/wixl.
#
# This produces an unsigned, per-user MSI (%LOCALAPPDATA%\EchoPort + Start Menu shortcut).
# It has NOT been executed on Windows; verify on a real Windows 10/11 machine before release.
#
# Requirements:
#   * a cross-built Windows binary: target/x86_64-pc-windows-gnu/release/echoport.exe
#   * wixl + msiinfo from msitools. On Arch: sudo pacman -S msitools.
#     Without root you can extract the Arch packages locally and point WIXL_DIR at the
#     extracted tree (the script then sets LD_LIBRARY_PATH automatically).
#
# Usage: ./scripts/package-msi.sh [--exe PATH] [--out PATH]
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

EXE="target/x86_64-pc-windows-gnu/release/echoport.exe"
OUT=""
TOOLS_DIR=""
while [ $# -gt 0 ]; do
    case "$1" in
        --exe) EXE="$2"; shift 2 ;;
        --out) OUT="$2"; shift 2 ;;
        --tools-dir) TOOLS_DIR="$2"; shift 2 ;;
        -h|--help) sed -n '1,30p' "$0"; exit 0 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done
[ -f "$EXE" ] || { echo "missing $EXE (run the cross build first)" >&2; exit 1; }

VERSION=$(tr -d '[:space:]' < VERSION)
VERSION4="$(python3 - "$VERSION" <<'PY'
import re,sys
v=sys.argv[1]
m=re.match(r'^(\d+)\.(\d+)\.(\d+)',v)
if not m: raise SystemExit(f"cannot map {v!r} to a 4-part MSI version")
print('.'.join(m.groups())+'.0')
PY
)"

# Locate wixl/msiinfo (installed or extracted locally).
WIXL="${WIXL:-$(command -v wixl || true)}"
MSIINFO="${MSIINFO:-$(command -v msiinfo || true)}"
if [ -z "$WIXL" ] && [ -n "${WIXL_DIR:-}" ]; then
    WIXL="$WIXL_DIR/usr/bin/wixl"
    MSIINFO="$WIXL_DIR/usr/bin/msiinfo"
    export LD_LIBRARY_PATH="$WIXL_DIR/usr/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
    export PATH="$WIXL_DIR/usr/bin:$PATH"
fi
if [ -z "$WIXL" ] || [ ! -x "$WIXL" ]; then
    echo "wixl not found — install msitools or set WIXL_DIR to an extracted msitools tree" >&2
    exit 1
fi

NAME="echoport-$VERSION-windows-x86_64"
STAGE="dist/_msi/$NAME"
OUT="${OUT:-dist/$NAME.msi}"
mkdir -p "$STAGE/docs" "$STAGE/examples" dist
cp "$EXE" "$STAGE/echoport.exe"
cp packaging/README-WINDOWS.md "$STAGE/README-WINDOWS.txt"
cp LICENSE NOTICE README.md "$STAGE/"
cp assets/echoport.ico "$STAGE/echoport.ico"
cp packaging/windows/clean-uninstall.cmd "$STAGE/clean-uninstall.cmd"
if [ -n "$TOOLS_DIR" ] && [ -d "$TOOLS_DIR" ]; then
    mkdir -p "$STAGE/tools"
    cp -a "$TOOLS_DIR"/. "$STAGE/tools/"
else
    echo "note: --tools-dir not provided; MSI will not bundle yt-dlp/ffmpeg" >&2
fi
for f in SCRIPTING.md PLUGINS.md plugins.html config.html; do cp "docs/$f" "$STAGE/docs/$f"; done
cp examples/config.lua examples/config.zh-CN.lua examples/config.en.lua "$STAGE/examples/"
cp examples/plugins/example.lua "$STAGE/examples/example.lua"

GUID_NS="6f1c4c5a-1c1c-5dc1-9d0c-echoport-0001" # placeholder replaced below
guid() { python3 -c "import uuid;print(uuid.uuid5(uuid.NAMESPACE_URL,'https://github.com/Nilotoo/EchoPort/$1'))"; }

sed -e "s|@VERSION4@|$VERSION4|g" \
    -e "s|@UPGRADE_GUID@|$(guid upgrade)|g" \
    -e "s|@GUID_EXE@|$(guid exe)|g" \
    -e "s|@GUID_DOCS@|$(guid docs)|g" \
    -e "s|@GUID_MENU@|$(guid menu)|g" \
    -e "s|@GUID_CLEAN@|$(guid clean)|g" \
    -e "s|@STAGE@|$STAGE|g" \
    packaging/windows/echoport.wxs.in > "$STAGE/echoport.wxs"

python3 - "$STAGE" <<'PY'
import os, sys
stage = sys.argv[1]
tools = os.path.join(stage, "tools")
component = ""
ref = ""
if os.path.isdir(tools):
    files = [
        ("ToolsYtDlp", "yt-dlp.exe"),
        ("ToolsFfmpeg", "ffmpeg.exe"),
        ("ToolsFfmpegLicense", "LICENSE-ffmpeg.txt"),
        ("ToolsYtDlpLicense", "LICENSE-yt-dlp.txt"),
        ("ToolsThirdParty", "THIRD-PARTY.md"),
    ]
    lines = ['          <Directory Id="ToolsDir" Name="tools">']
    lines.append('            <Component Id="BundledTools" Guid="{9A7E5C1D-3B44-4A6B-9B7E-2F3D5C6A7B81}">')
    for file_id, name in files:
        path = os.path.join(tools, name)
        if os.path.isfile(path):
            lines.append(
                f'              <File Id="{file_id}" Name="{name}" Source="{path}" />'
            )
    lines.append("            </Component>")
    lines.append("          </Directory>")
    component = "\n".join(lines)
    ref = '<ComponentRef Id="BundledTools"/>'
wxs = os.path.join(stage, "echoport.wxs")
text = open(wxs, encoding="utf-8").read()
text = text.replace("@TOOLS_COMPONENT@", component).replace("@TOOLS_REF@", ref)
open(wxs, "w", encoding="utf-8").write(text)
PY

echo "==> wixl: $OUT"
"$WIXL" -o "$OUT" "$STAGE/echoport.wxs"
[ -s "$OUT" ] || { echo "wixl did not produce $OUT" >&2; exit 1; }
(cd "$(dirname "$OUT")" && sha256sum "$(basename "$OUT")") > "$OUT.sha256"

if [ -n "$MSIINFO" ] && [ -x "$MSIINFO" ]; then
    echo "==> msiinfo tables:"
    "$MSIINFO" tables "$OUT" | head -5
    echo "==> msiinfo files:"
    "$MSIINFO" export "$OUT" File 2>/dev/null | head -15 || true
fi
rm -rf "$STAGE"
echo ""
echo "artifact: $OUT"
echo "  size:   $(stat -c %s "$OUT") bytes"
echo "  sha256: $(cut -d' ' -f1 "$OUT.sha256")"
echo "  note:   cross-built, unsigned, per-user MSI; NOT executed on Windows."
