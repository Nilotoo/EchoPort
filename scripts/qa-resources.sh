#!/usr/bin/env bash
# EchoPort resource-usage QA harness (qa-docs).
#
# Measures the *release* binary on this host and prints markdown-friendly
# numbers for docs/QA_REPORT.md §"资源占用 / Resource usage":
#   - binary / dist tarball size
#   - GL context info (glxinfo, llvmpipe on the QA VM)
#   - startup time (time-to-window, 5 runs)
#   - idle CPU (8 s utime+stime samples; retries known software-GL anomalies),
#     idle RSS / PSS / RssAnon / RssFile / threads / FDs
#   - demo mode (animation + simulated playback) CPU / RSS
#   - live streamed track: resolve/cache/cover memory, FDs, threads
#   - streamed-audio cache growth over N tracks (default 1 GiB limit)
#   - no-cache mode (cache_limit_mb=0) startup purge behaviour
#   - profile/config file sizes
#
# Usage:
#   scripts/qa-resources.sh [--bin PATH] [--with-network] [--tracks N]
#
# The script never touches the real profile: every app run uses an isolated
# XDG_DATA_HOME / XDG_CACHE_HOME / XDG_CONFIG_HOME under .testing/resources.
# It only kills processes it started itself (never `pkill -f echoport`).
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${ECHOPORT_BIN:-$ROOT/target/release/echoport}"
WITH_NETWORK=0
NETWORK_ONLY=0
TRACKS=5
while [ $# -gt 0 ]; do
    case "$1" in
        --bin) BIN="$2"; shift 2 ;;
        --with-network) WITH_NETWORK=1; shift ;;
        --network-only) WITH_NETWORK=1; NETWORK_ONLY=1; shift ;;
        --tracks) TRACKS="$2"; shift 2 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done
[ -x "$BIN" ] || { echo "missing release binary: $BIN" >&2; exit 1; }

WORK="$ROOT/.testing/resources"
rm -rf "$WORK"
mkdir -p "$WORK"
cleanup() {
    local p
    for p in $(jobs -p 2>/dev/null); do kill "$p" 2>/dev/null; done
    rm -rf "$WORK"
}
trap cleanup EXIT

now_ms() { date +%s%3N; }

# Run the app with an isolated profile in $1 (dir) and extra env in the rest.
start_app() {
    local dir="$1"
    shift
    mkdir -p "$dir/data" "$dir/cache" "$dir/config"
    env -u WAYLAND_DISPLAY \
        XDG_DATA_HOME="$dir/data" XDG_CACHE_HOME="$dir/cache" XDG_CONFIG_HOME="$dir/config" \
        "$@" "$BIN" >>"$dir/app.log" 2>&1 &
    echo $!
}

# Wait until the app window exists (up to 5 s); prints elapsed milliseconds.
wait_window_ms() {
    local pid="$1" start wid t
    start=$(now_ms)
    while :; do
        wid=$(wmctrl -lp 2>/dev/null | awk -v p="$pid" '$3 == p {print $1; exit}')
        [ -n "$wid" ] && break
        t=$(( $(now_ms) - start ))
        [ "$t" -gt 5000 ] && { echo 0; return; }
        sleep 0.01
    done
    echo $(( $(now_ms) - start ))
}

proc_field() { # file key (e.g. /proc/pid/status VmRSS)
    awk -v k="$2:" '$1 == k {print $2}' "$1" 2>/dev/null
}
sample_cpu() { # pid seconds -> hundredths of CPU percent (ticks consumed)
    local pid="$1" secs="$2" u0 s0 u1 s1
    read -r u0 s0 < <(awk '{print $14, $15}' "/proc/$pid/stat" 2>/dev/null)
    [ -z "${u0:-}" ] && { echo 0; return; }
    sleep "$secs"
    read -r u1 s1 < <(awk '{print $14, $15}' "/proc/$pid/stat" 2>/dev/null)
    [ -z "${u1:-}" ] && { echo 0; return; }
    echo $(( (u1 + s1) - (u0 + s0) ))
}
proc_mem() { # pid -> "rss pss anon file shmem threads fds"
    local pid="$1" status smaps dir
    status="/proc/$pid/status"; smaps="/proc/$pid/smaps_rollup"
    local rss pss anon file shmem threads fds
    rss=$(proc_field "$status" VmRSS); pss=$(proc_field "$smaps" Pss)
    anon=$(proc_field "$status" RssAnon); file=$(proc_field "$status" RssFile)
    shmem=$(proc_field "$status" RssShmem); threads=$(proc_field "$status" Threads)
    fds=$(ls "/proc/$pid/fd" 2>/dev/null | wc -l)
    echo "${rss:-0} ${pss:-0} ${anon:-0} ${file:-0} ${shmem:-0} ${threads:-0} ${fds:-0}"
}
kb_to_mib() { awk -v v="$1" 'BEGIN { printf "%.1f", v / 1024 }'; }
du_bytes() { du -sb "$1" 2>/dev/null | cut -f1; }
du_mib() { awk -v b="$1" 'BEGIN { printf "%.1f", b / 1048576 }'; }
cache_stats() { # dir -> bytes files
    local b f
    b=$(du_bytes "$1"); [ -z "$b" ] && b=0
    f=$(find "$1" -type f 2>/dev/null | wc -l)
    echo "$b $f"
}

echo "=== EchoPort resource report ($(date -u +%Y-%m-%dT%H:%M:%SZ)) ==="
echo "binary: $BIN"
echo "binary_size: $(stat -c %s "$BIN") bytes ($(du_mib "$(stat -c %s "$BIN")") MiB)"
echo "binary_sha256: $(sha256sum "$BIN" | cut -d' ' -f1)"
for t in "$ROOT"/dist/echoport-*.tar.gz; do
    [ -e "$t" ] || continue
    echo "tarball: $(basename "$t") $(stat -c %s "$t") bytes ($(du_mib "$(stat -c %s "$t")") MiB) sha256=$(sha256sum "$t" | cut -d' ' -f1)"
done
echo "glibc_max: $(objdump -T "$BIN" 2>/dev/null | grep -o 'GLIBC_[0-9.]*' | sort -V | tail -1)"
echo

echo "--- GL / GPU ---"
if command -v glxinfo >/dev/null 2>&1; then
    glxinfo -B 2>/dev/null | sed -n '/OpenGL renderer string\|OpenGL version string\|OpenGL core profile version string\|Accelerated\|Device:\|Version:/p' | sed 's/^[[:space:]]*/glxinfo: /'
else
    echo "glxinfo: not installed"
fi
echo

if [ "$NETWORK_ONLY" -eq 0 ]; then
echo "--- startup (time-to-window, normal mode) ---"
for i in 1 2 3 4 5; do
    dir="$WORK/startup-$i"
    pid=$(start_app "$dir")
    ms=$(wait_window_ms "$pid")
    kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null
    printf 'startup_run%d_ms: %s\n' "$i" "$ms"
done
echo

echo "--- first-frame latency (screenshot mode, frame 40; process start to PNG written) ---"
for i in 1 2 3; do
    dir="$WORK/firstframe-$i"
    mkdir -p "$dir/data" "$dir/cache" "$dir/config"
    shot="$WORK/firstframe-$i.png"
    t0=$(date +%s%3N)
    env -u WAYLAND_DISPLAY XDG_DATA_HOME="$dir/data" XDG_CACHE_HOME="$dir/cache"         XDG_CONFIG_HOME="$dir/config" ECHOPORT_DEMO=1 ECHOPORT_PAGE=home         ECHOPORT_SHOT="$shot" "$BIN" >>"$dir/app.log" 2>&1
    t1=$(date +%s%3N)
    printf 'firstframe_run%d_ms: %s (screenshot written: %s)\n' "$i" "$((t1 - t0))" "$([ -f "$shot" ] && echo yes || echo no)"
done
echo

echo "--- idle (8 s samples; retries on the known software-GL repaint anomaly) ---"
IDLE_DIR="$WORK/idle"; mkdir -p "$IDLE_DIR"
pid=$(start_app "$IDLE_DIR")
sleep 15
CLEAN=(); ANOM=0; ATTEMPTS=0
while [ "${#CLEAN[@]}" -lt 5 ] && [ "$ATTEMPTS" -lt 12 ]; do
    ATTEMPTS=$((ATTEMPTS + 1))
    ticks=$(sample_cpu "$pid" 8)
    pct=$(awk -v d="$ticks" 'BEGIN { printf "%.3f", d / 8 }')
    if awk -v p="$pct" 'BEGIN { exit !(p > 5) }'; then
        ANOM=$((ANOM + 1))
        echo "idle_anomaly_sample_${ATTEMPTS}_pct: $pct (retry)"
        kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null
        pid=$(start_app "$IDLE_DIR"); sleep 15
    else
        CLEAN+=("$pct")
        echo "idle_sample_${#CLEAN[@]}_pct: $pct"
    fi
done
printf 'idle_clean_cpu_pct: %s\n' "${CLEAN[*]}"
echo "idle_anomaly_samples: $ANOM / $ATTEMPTS"
read -r rss pss anon file shmem threads fds <<<"$(proc_mem "$pid")"
printf 'idle_rss_mib: %s (PSS %s, RssAnon %s, RssFile %s, RssShmem %s)\n' \
    "$(kb_to_mib "$rss")" "$(kb_to_mib "$pss")" "$(kb_to_mib "$anon")" "$(kb_to_mib "$file")" "$(kb_to_mib "$shmem")"
printf 'idle_threads: %s\nidle_fds: %s\n' "$threads" "$fds"
kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null
echo

echo "--- demo mode (animation + simulated playback, home page) ---"
DEMO_DIR="$WORK/demo"
pid=$(start_app "$DEMO_DIR" ECHOPORT_DEMO=1 ECHOPORT_PAGE=home)
sleep 6
ticks=$(sample_cpu "$pid" 8)
printf 'demo_cpu_pct: %.2f\n' "$(awk -v d="$ticks" 'BEGIN { print d / 8 }')"
read -r rss pss anon file shmem threads fds <<<"$(proc_mem "$pid")"
printf 'demo_rss_mib: %s (PSS %s)\ndemo_threads: %s\ndemo_fds: %s\n' \
    "$(kb_to_mib "$rss")" "$(kb_to_mib "$pss")" "$threads" "$fds"
kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null
echo

echo "--- profile / config sizes ---"
prof="$DEMO_DIR/data/echoport/profiles/Me.json"
[ -f "$prof" ] && echo "profile_json_bytes: $(stat -c %s "$prof")"
cfg="$DEMO_DIR/config/echoport/config.lua"
[ -f "$cfg" ] && echo "config_lua_bytes: $(stat -c %s "$cfg")"
echo "profile_data_dir_bytes: $(du_bytes "$DEMO_DIR/data")"
echo "config_dir_bytes: $(du_bytes "$DEMO_DIR/config")"
# Non-demo launch writes the commented config.lua template on first run.
CFG_DIR="$WORK/config-template"
pid=$(start_app "$CFG_DIR" ECHOPORT_LIVE=1 ECHOPORT_PAGE=home)
sleep 5
kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null
cfg="$CFG_DIR/config/echoport/config.lua"
if [ -f "$cfg" ]; then
    echo "config_lua_bytes: $(stat -c %s "$cfg")"
    echo "live_profile_json_bytes: $(stat -c %s "$CFG_DIR/data/echoport/profiles/Me.json" 2>/dev/null || echo 0)"
fi
echo

fi  # NETWORK_ONLY base sections

if [ "$WITH_NETWORK" -eq 1 ]; then
    echo "--- live streamed track (resolve + cover; no audio device on QA host) ---"
    LIVE_DIR="$WORK/live"
    mkdir -p "$LIVE_DIR/config/echoport"
    cat >"$LIVE_DIR/config/echoport/config.lua" <<'LUA'
echoport.config = { startup_page = "now_playing" }
echoport.on("startup", function()
  echoport.play_url("https://music.apple.com/us/album/never-gonna-give-you-up/1559885420?i=1559885421")
end)
LUA
    pid=$(start_app "$LIVE_DIR" ECHOPORT_LIVE=1)
    sleep 20
    read -r rss pss anon file shmem threads fds <<<"$(proc_mem "$pid")"
    printf 'live_rss_mib: %s (PSS %s, RssAnon %s, RssFile %s)\n' \
        "$(kb_to_mib "$rss")" "$(kb_to_mib "$pss")" "$(kb_to_mib "$anon")" "$(kb_to_mib "$file")"
    printf 'live_threads: %s\nlive_fds: %s\n' "$threads" "$fds"
    read -r cbytes cfiles <<<"$(cache_stats "$LIVE_DIR/cache/echoport/audio")"
    printf 'live_audio_cache: %s bytes, %s files\n' "$cbytes" "$cfiles"
    kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null
    echo

    echo "--- streamed-audio cache growth (default cache_limit_mb=1024, ${TRACKS} tracks) ---"
    CACHE_DIR="$WORK/cache-growth"
    mkdir -p "$CACHE_DIR/config/echoport"
    downloads_kb_before=$(du -sk "$HOME/Music/EchoPort" 2>/dev/null | cut -f1 || echo 0)
    # Apple Music links resolve via yt-dlp ytsearch (full public audio) and did not
    # hit the YouTube bot check that can break direct watch-URL runs.
    urls=(
        "https://music.apple.com/us/album/never-gonna-give-you-up/1559885420?i=1559885421"
        "https://music.apple.com/us/album/bohemian-rhapsody/6781024026?i=6781024437"
        "https://music.apple.com/us/album/get-lucky/617154241?i=617154366"
        "https://music.apple.com/us/album/rolling-in-the-deep/1544491232?i=1544491233"
        "https://music.apple.com/us/album/smells-like-teen-spirit/1440783617?i=1440783625"
    )
    i=0
    for url in "${urls[@]}"; do
        [ "$i" -ge "$TRACKS" ] && break
        i=$((i + 1))
        cat >"$CACHE_DIR/config/echoport/config.lua" <<LUA
echoport.config = { startup_page = "now_playing" }
echoport.on("startup", function() echoport.play_url("$url") end)
LUA
        pid=$(start_app "$CACHE_DIR" ECHOPORT_LIVE=1)
        sleep 35
        kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null
        read -r cbytes cfiles <<<"$(cache_stats "$CACHE_DIR/cache/echoport/audio")"
        printf 'cache_after_%d_tracks: %s bytes (%s MiB), %s files\n' "$i" "$cbytes" "$(du_mib "$cbytes")" "$cfiles"
    done
    downloads_kb_after=$(du -sk "$HOME/Music/EchoPort" 2>/dev/null | cut -f1 || echo 0)
    echo "cache_files: $(find "$CACHE_DIR/cache/echoport/audio" -type f -printf '%f ' 2>/dev/null)"
    echo "download_dir_kb_before: ${downloads_kb_before:-0} / after: ${downloads_kb_after:-0} (must be equal; playback never writes downloads)"
    echo

    echo "--- no-cache mode (cache_limit_mb=0): startup purge of old cache files ---"
    NC_DIR="$WORK/no-cache"
    mkdir -p "$NC_DIR/data/echoport/profiles" "$NC_DIR/cache/echoport/audio" "$NC_DIR/config"
    # 3 fake "old" streamed files (mtime 2 h ago) + 1 protected fresh file.
    for n in 1 2 3; do
        head -c 1048576 /dev/urandom >"$NC_DIR/cache/echoport/audio/old-$n.m4a"
        touch -d '2 hours ago' "$NC_DIR/cache/echoport/audio/old-$n.m4a"
    done
    head -c 1048576 /dev/urandom >"$NC_DIR/cache/echoport/audio/fresh.m4a"
    cat >"$NC_DIR/data/echoport/profiles/Me.json" <<JSON
{"schema":1,"name":"Me","created_at":1793242743,"settings":{"language":"en","theme_mode":"pulse","ui_scale":1.0,"follow_cover_accent":true,"cache_limit_mb":0,"download_format":"mp3","download_dir":"$NC_DIR/downloads","music_dirs":["$NC_DIR/music"],"ytdlp_path":null,"volume":0.72,"results_per_platform":12},"tracks":{},"playlists":[],"favorites":[],"history":[]}
JSON
    pid=$(start_app "$NC_DIR" ECHOPORT_LIVE=1)
    sleep 10
    read -r cbytes cfiles <<<"$(cache_stats "$NC_DIR/cache/echoport/audio")"
    printf 'no_cache_after_startup: %s bytes, %s files (expected only the protected fresh file)\n' "$cbytes" "$cfiles"
    kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null
fi

echo
echo "=== done ==="
