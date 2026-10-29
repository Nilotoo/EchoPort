#!/usr/bin/env bash
# QA helper: capture the v0.1.7 mini player floating window and exercise its controls.
# Linux/X11 only (uses wmctrl + XTest + ffmpeg x11grab). Not part of the app.
#
# Usage: ./scripts/qa-mini-player.sh [screenshot-path]
#   ECHOPORT_BIN may point at the release binary (default target/release/echoport).
# Exit code 0 when all checks pass; evidence images are written to a temp dir and
# the final mini-window capture lands in the screenshot path.
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
BIN="${ECHOPORT_BIN:-$ROOT/target/release/echoport}"
OUT="${1:-$ROOT/screenshots/qa17-mini-player.png}"
TMP="$(mktemp -d)"
cleanup() { [ -n "${APP:-}" ] && kill "$APP" 2>/dev/null; rm -rf "$TMP"; }
trap cleanup EXIT

mkdir -p "$TMP/data" "$TMP/cache" "$TMP/config"
env -u WAYLAND_DISPLAY XDG_DATA_HOME="$TMP/data" XDG_CACHE_HOME="$TMP/cache" \
    XDG_CONFIG_HOME="$TMP/config" ECHOPORT_DEMO=1 ECHOPORT_PAGE=now_playing \
    ECHOPORT_MINI=1 "$BIN" > "$TMP/log" 2>&1 < /dev/null &
APP=$!

for _ in $(seq 1 800); do
    n=$(wmctrl -lp 2>/dev/null | awk -v p="$APP" '$3==p' | wc -l)
    [ "$n" -ge 2 ] && break
    sleep 0.02
done
sleep 2

python3 - "$APP" "$TMP/windows.json" <<'PY'
import ctypes, ctypes.util, json, subprocess, sys
pid = sys.argv[1]
out = subprocess.check_output(["wmctrl", "-lp", "-G"]).decode()
wins = []
X = ctypes.CDLL(ctypes.util.find_library("X11"))
X.XOpenDisplay.restype = ctypes.c_void_p
X.XDefaultRootWindow.restype = ctypes.c_ulong
dpy = X.XOpenDisplay(None)
root = X.XDefaultRootWindow(ctypes.c_void_p(dpy))
X.XTranslateCoordinates.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_ulong,
                                    ctypes.c_int, ctypes.c_int,
                                    ctypes.POINTER(ctypes.c_int), ctypes.POINTER(ctypes.c_int),
                                    ctypes.POINTER(ctypes.c_ulong)]
for line in out.splitlines():
    f = line.split(None, 8)
    if len(f) < 9 or f[2] != pid:
        continue
    wid = int(f[0], 16)
    rx, ry, child = ctypes.c_int(), ctypes.c_int(), ctypes.c_ulong()
    # Root coordinates of the client area; wmctrl's -G on this XWayland/KWin setup
    # reports doubled root offsets, so translate explicitly for input injection.
    X.XTranslateCoordinates(ctypes.c_void_p(dpy), ctypes.c_ulong(wid), ctypes.c_ulong(root),
                            0, 0, ctypes.byref(rx), ctypes.byref(ry), ctypes.byref(child))
    wins.append({"id": f[0], "x": rx.value, "y": ry.value,
                 "w": int(f[5]), "h": int(f[6]), "title": f[8]})
wins.sort(key=lambda w: w["w"])
json.dump(wins, open(sys.argv[2], "w"))
print("windows (translated root coords):", wins)
PY
cat "$TMP/windows.json"

read -r MINI MX MY < <(python3 -c "import json,sys;w=json.load(open('$TMP/windows.json'))[0];print(w['id'],w['x'],w['y'])")
read -r MAIN AX AY < <(python3 -c "import json,sys;w=json.load(open('$TMP/windows.json'))[-1];print(w['id'],w['x'],w['y'])")
echo "APP=$APP MINI=$MINI at $MX,$MY MAIN=$MAIN at $AX,$AY"

capture() { ffmpeg -y -loglevel error -f x11grab -window_id "$1" -i "$DISPLAY" -frames:v 1 "$2"; }
click() { python3 "$ROOT/scripts/qa-ui.py" warpclick "$@" >/dev/null 2>&1; sleep 0.4; }
diffpx() { python3 - "$1" "$2" <<'PY'
import sys
from PIL import Image, ImageChops
a, b = (Image.open(p).convert("RGB") for p in sys.argv[1:3])
d = ImageChops.difference(a, b).tobytes()
print(sum(1 for i in range(0, len(d), 3) if d[i:i+3] != b"\x00\x00\x00"))
PY
}

capture "$MAIN" "$TMP/main-before.png"
capture "$MINI" "$OUT"
echo "mini capture: $OUT ($(file -b "$OUT" | cut -d, -f1-2))"
[ -s "$OUT" ] || { echo "FAIL: mini screenshot missing"; exit 1; }

# 1) play/pause toggle in the mini player (play button centre, logical 384,53 * 1.25)
click $((MX + 480)) $((MY + 66)); capture "$MINI" "$TMP/mini-pause.png"
d1=$(diffpx "$OUT" "$TMP/mini-pause.png"); echo "play/pause changed pixels: $d1"
# 2) previous / next
click $((MX + 406)) $((MY + 66)); capture "$MINI" "$TMP/mini-prev.png"
click $((MX + 437)) $((MY + 66)); capture "$MINI" "$TMP/mini-next.png"
d2=$(diffpx "$TMP/mini-prev.png" "$TMP/mini-next.png"); echo "prev/next changed pixels: $d2"
# 3) open main window (external icon, top right)
click $((MX + 465)) $((MY + 31)); sleep 0.4
stacking=$(xprop -root _NET_CLIENT_LIST_STACKING 2>/dev/null | sed 's/.*# //')
topmost=$(printf '%s\n' "$stacking" | tr ',' '\n' | tail -1 | tr -d ' ')
echo "topmost window after open-main: $topmost (main window: $((MAIN)))"
# 4) close mini
click $((MX + 502)) $((MY + 31)); sleep 0.5
remaining=$(wmctrl -lp 2>/dev/null | awk -v p="$APP" '$3==p' | wc -l)
echo "windows remaining after close: $remaining"

pass=0; fail=0
chk() { if [ "$1" = true ]; then echo "PASS: $2"; pass=$((pass+1)); else echo "FAIL: $2"; fail=$((fail+1)); fi; }
chk "$([ "${d1:-0}" -gt 100 ] && echo true)" "mini play/pause toggles the button/state"
chk "$([ "${d2:-0}" -gt 100 ] && echo true)" "mini prev/next change the track art/title"
chk "$([ "$topmost" = "$((MAIN))" ] && echo true)" "mini open-main raises/focuses the main window"
chk "$([ "$remaining" = "1" ] && echo true)" "mini close button removes the mini window"
echo "RESULT: $pass passed, $fail failed (tmp evidence: $TMP)"
[ "$fail" -eq 0 ]
