#!/usr/bin/env bash
# EchoPort — generate the demo screenshot set used by README.md and QA.
#
#   ./scripts/run-demo.sh                 # every page (Pulse) + Tide/Ink/follow-cover showcase
#   ./scripts/run-demo.sh --release       # use target/release/echoport
#   ./scripts/run-demo.sh --prefix qa-    # output name prefix (default: qa-)
#   ./scripts/run-demo.sh --page home     # one page only (repeatable; theme shots skipped)
#   ./scripts/run-demo.sh --no-themes     # only the per-page Pulse screenshots
#
# Output goes to screenshots/ (13 files by default):
#   All pages in the default Pulse theme:
#     qa-home.png qa-search.png qa-library.png qa-playlists.png
#     qa-downloads.png qa-now_playing.png qa-profile.png qa-settings.png
#   Theme showcase:
#     qa-tide-home.png qa-tide-now_playing.png
#     qa-ink-home.png qa-ink-now_playing.png
#     qa-follow-cover-now_playing.png   (Pulse + cover-derived accent, ECHOPORT_FOLLOW_COVER=1)
#
# Themes: ECHOPORT_THEME=pulse|tide|ink (legacy blue_white/midnight/album are
# still accepted as aliases by the app). Demo screenshots keep the strict theme
# palette by default; ECHOPORT_FOLLOW_COVER=1 enables the cover-derived accent.
#
# Headless / remote shells: eframe's Wayland path can stall before the first
# frame on some Mesa/ZINK setups (no GPU). The reliable fallback on this host is
# the X11 backend with software GL. The script forces X11 by unsetting
# WAYLAND_DISPLAY unless ECHOPORT_WAYLAND=1 is set. If your machine only has
# Wayland, try: ECHOPORT_WAYLAND=1 LIBGL_ALWAYS_SOFTWARE=1 ./scripts/run-demo.sh
#
# This script only launches the app in demo mode; it never edits source. Safe to
# re-run.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

PROFILE="debug"
PREFIX="qa-"
ONLY_PAGES=()
THEMES=1

while [ $# -gt 0 ]; do
    case "$1" in
        --release) PROFILE="release" ;;
        --debug) PROFILE="debug" ;;
        --prefix)
            [ $# -ge 2 ] || { echo "error: --prefix needs a value" >&2; exit 2; }
            PREFIX="$2"; shift ;;
        --page)
            [ $# -ge 2 ] || { echo "error: --page needs a value" >&2; exit 2; }
            ONLY_PAGES+=("$2"); shift ;;
        --no-themes) THEMES=0 ;;
        -h|--help)
            sed -n '2,27p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
            exit 0 ;;
        *)
            echo "error: unknown argument '$1' (try --help)" >&2
            exit 2 ;;
    esac
    shift
done

if ! command -v cargo >/dev/null 2>&1; then
    echo "error: cargo was not found. Install Rust from https://rustup.rs (Rust 1.85+)." >&2
    exit 1
fi

mkdir -p screenshots

# Demo mode seeds fake data into the profile/cache. Use disposable XDG roots so
# screenshots are deterministic and never pollute the real user profile/cache.
# `.testing/` is git-ignored.
DEMO_XDG="$ROOT/.testing/run-demo"
rm -rf "$DEMO_XDG"
mkdir -p "$DEMO_XDG/data" "$DEMO_XDG/cache" "$DEMO_XDG/config"

echo "EchoPort demo screenshots"
echo "  profile: $PROFILE"
echo "  prefix:  $PREFIX"
if [ -z "${ECHOPORT_BIN:-}" ]; then
    echo "  building ($PROFILE, target dir is shared — a short lock wait is normal)…"
    if [ "$PROFILE" = "release" ]; then
        cargo build --release --quiet
    else
        cargo build --quiet
    fi
    ECHOPORT_BIN="$ROOT/target/$PROFILE/echoport"
fi
if [ ! -x "$ECHOPORT_BIN" ]; then
    echo "error: binary not found or not executable: $ECHOPORT_BIN" >&2
    exit 1
fi

# Pick a display backend that works headlessly. X11 + swrast is the reliable
# path on this host; keep Wayland when explicitly requested.
RUN_ENV=(env)
if [ "${ECHOPORT_WAYLAND:-0}" != "1" ] && [ -n "${DISPLAY:-}" ] && [ -n "${WAYLAND_DISPLAY:-}" ]; then
    RUN_ENV=("env" "-u" "WAYLAND_DISPLAY")
    echo "  display: X11 (XWayland); set ECHOPORT_WAYLAND=1 to force Wayland"
elif [ "${ECHOPORT_WAYLAND:-0}" = "1" ]; then
    echo "  display: Wayland"
else
    echo "  display: as-is (DISPLAY=${DISPLAY:-unset}, WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-unset})"
fi

TIMEOUT_CMD=()
if command -v timeout >/dev/null 2>&1; then
    TIMEOUT_CMD=(timeout 90)
fi

# shot <page> <theme> <output> [extra env assignment…]
shot() {
    local page="$1" theme="$2" out="$3"
    shift 3
    if [ -e "$out" ]; then
        rm -f "$out"
    fi
    echo "  shooting page=$page theme=$theme -> $out"
    "${RUN_ENV[@]}" \
        XDG_DATA_HOME="$DEMO_XDG/data" \
        XDG_CACHE_HOME="$DEMO_XDG/cache" \
        XDG_CONFIG_HOME="$DEMO_XDG/config" \
        ECHOPORT_DEMO=1 \
        ECHOPORT_PAGE="$page" \
        ECHOPORT_THEME="$theme" \
        ECHOPORT_SHOT="$ROOT/screenshots/$out" \
        "$@" \
        ${TIMEOUT_CMD[@]+"${TIMEOUT_CMD[@]}"} "$ECHOPORT_BIN" >"$ROOT/screenshots/.shot.log" 2>&1 || {
            echo "error: screenshot ($page/$theme) failed; last log lines:" >&2
            tail -n 15 "$ROOT/screenshots/.shot.log" >&2
            exit 1
        }
    if [ ! -s "$ROOT/screenshots/$out" ]; then
        echo "error: expected screenshot was not written: screenshots/$out" >&2
        tail -n 15 "$ROOT/screenshots/.shot.log" >&2
        exit 1
    fi
}

PAGES=("home" "search" "library" "playlists" "downloads" "now_playing" "profile" "settings")
if [ ${#ONLY_PAGES[@]} -gt 0 ]; then
    PAGES=("${ONLY_PAGES[@]}")
fi

# Every page in the default Pulse theme.
for page in "${PAGES[@]}"; do
    shot "$page" "pulse" "${PREFIX}${page}.png"
done

# Theme showcase: Tide, Ink and the optional cover-derived accent.
if [ ${#ONLY_PAGES[@]} -eq 0 ] && [ "$THEMES" = "1" ]; then
    shot "home" "tide" "${PREFIX}tide-home.png"
    shot "now_playing" "tide" "${PREFIX}tide-now_playing.png"
    shot "home" "ink" "${PREFIX}ink-home.png"
    shot "now_playing" "ink" "${PREFIX}ink-now_playing.png"
    shot "now_playing" "pulse" "${PREFIX}follow-cover-now_playing.png" ECHOPORT_FOLLOW_COVER=1
fi

rm -f "$ROOT/screenshots/.shot.log"

echo
echo "screenshots written to $ROOT/screenshots:"
ls -lh "$ROOT"/screenshots/"${PREFIX}"*.png 2>/dev/null | awk '{print "  " $9 "  (" $5 ")"}'
