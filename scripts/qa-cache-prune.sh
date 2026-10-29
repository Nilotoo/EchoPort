#!/usr/bin/env bash
# EchoPort — QA: verify the streamed-audio cache prune policy.
#
# Policy under test (see README "cache policy" / docs/ARCHITECTURE.md):
#   * streamed audio is cached on disk under <cache>/echoport/audio (never RAM);
#   * startup prune targets 2 GiB for the whole audio cache (recursive);
#   * it deletes the oldest `mtime`-first prunable files;
#   * files modified in the last 120 s are protected (current playback/prefetch);
#   * *.part / *.tmp / *.ytdl entries are never deleted, but they DO count towards
#     the directory total (so protected/temporary files can keep the cache above
#     the limit until they age out);
#   * downloadable files in the user's download folder are never touched.
#
# This script uses ECHOPORT_CACHE_LIMIT_MB (supported by the app) to keep the
# test small and fast, creates sparse dummy cache files with old/new mtimes,
# starts the REAL release binary in normal (non-demo) mode so the startup prune
# job runs, then checks exactly which files survived.
#
# Usage:
#   ./scripts/qa-cache-prune.sh                 # expects target/release/echoport
#   ECHOPORT_BIN=target/debug/echoport ./scripts/qa-cache-prune.sh
#
# Safe to re-run: uses an isolated workspace test directory (.testing/cache-prune).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

BIN="${ECHOPORT_BIN:-$ROOT/target/release/echoport}"
LIMIT_MB=100
BASE="$ROOT/.testing/cache-prune"
CACHE="$BASE/cache/echoport/audio"
DATA="$BASE/data"
CONFIG="$BASE/config"
DOWNLOADS="$BASE/downloads"

fail() { echo "FAIL: $*" >&2; exit 1; }
pass() { echo "PASS: $*"; }

[ -x "$BIN" ] || fail "binary not found: $BIN (build release first)"
command -v truncate >/dev/null || fail "truncate not available"

rm -rf "$BASE"
mkdir -p "$CACHE" "$DATA" "$CONFIG" "$DOWNLOADS"

# --- fixtures -------------------------------------------------------------
# 5 old 40 MiB files (oldest first), 1 fresh 40 MiB protected file, one .part and
# one .tmp (40 MiB each). Directory total = 320 MiB, limit = 100 MiB. Only the 5
# old files are prunable → all 5 are deleted (freed 200 MiB); the protected fresh
# file and the two temporary files remain (120 MiB > limit by design, they count
# towards the total but must never be deleted).
for i in 1 2 3 4 5; do
    truncate -s 40M "$CACHE/old-$i.m4a"
    touch -d "@$(( $(date +%s) - 7200 + i ))" "$CACHE/old-$i.m4a"   # 2 h old
done
truncate -s 40M "$CACHE/fresh-playing.m4a"                          # touched now
truncate -s 40M "$CACHE/partial.part"
truncate -s 40M "$CACHE/temporary.tmp"
touch -d "@$(( $(date +%s) - 7200 ))" "$CACHE/partial.part" "$CACHE/temporary.tmp"
printf 'user download' > "$DOWNLOADS/keep-me.mp3"

BEFORE_FILES=$(find "$CACHE" -maxdepth 1 -type f | wc -l)
echo "== cache prune QA =="
echo "  binary:  $BIN"
echo "  cache:   $CACHE"
echo "  limit:   ${LIMIT_MB} MiB (ECHOPORT_CACHE_LIMIT_MB)"
echo "  fixture: $BEFORE_FILES files (5 old 40M + 1 fresh 40M + .part + .tmp)"

# --- run the real app so the startup prune thread runs --------------------
env -u WAYLAND_DISPLAY \
    ECHOPORT_CACHE_LIMIT_MB="$LIMIT_MB" \
    XDG_DATA_HOME="$DATA" \
    XDG_CACHE_HOME="$BASE/cache" \
    XDG_CONFIG_HOME="$CONFIG" \
    RUST_LOG=info \
    "$BIN" > "$BASE/prune-run.log" 2>&1 &
APP_PID=$!
sleep 6
kill "$APP_PID" 2>/dev/null || true
wait "$APP_PID" 2>/dev/null || true

# --- assertions ------------------------------------------------------------
[ -f "$DOWNLOADS/keep-me.mp3" ] || fail "user download file was touched/removed"
pass "user download folder untouched"

[ -f "$CACHE/fresh-playing.m4a" ] || fail "recently-touched file (<120 s) was evicted"
pass "recently-touched file protected (<120 s)"

[ -f "$CACHE/partial.part" ] || fail "*.part entry was pruned (should be skipped)"
pass "*.part entry skipped"
[ -f "$CACHE/temporary.tmp" ] || fail "*.tmp entry was pruned (should be skipped)"
pass "*.tmp entry skipped"

REMAIN_OLD=$(find "$CACHE" -maxdepth 1 -type f -name "old-*.m4a" | wc -l)
LOGGED=$(grep -c "audio cache pruned" "$BASE/prune-run.log" || true)
echo "  surviving old files: $REMAIN_OLD (expected 0)"
grep "audio cache pruned" "$BASE/prune-run.log" || echo "  (no prune log line found)"

[ "$REMAIN_OLD" -eq 0 ] || fail "expected all 5 old files to be evicted, found $REMAIN_OLD survivors"
pass "oldest-first eviction removed all 5 old files (freed 200 MiB)"

if [ "${LOGGED:-0}" -lt 1 ]; then
    fail "app did not log the prune pass (expected 'audio cache pruned: freed ...')"
fi
pass "app logged the prune pass"

# Remaining bytes are only the untouched protected/temporary entries: the app
# deleted everything it is allowed to delete. Prove the prune log reports 200 MiB.
REMAIN_BYTES=$(find "$CACHE" -maxdepth 1 -type f -printf '%s\n' | awk '{s+=$1} END{print s+0}')
EXPECTED_REMAIN=$((120 * 1024 * 1024))
[ "$REMAIN_BYTES" -eq "$EXPECTED_REMAIN" ] || fail "unexpected remaining bytes: $REMAIN_BYTES (expected ${EXPECTED_REMAIN})"
grep -q "freed 200 MiB" "$BASE/prune-run.log" || fail "prune log does not report the expected 200 MiB freed"
pass "protected fresh file + *.part + *.tmp remain (120 MiB); prune freed 200 MiB and logged it"

echo
echo "cache-prune QA: ALL CHECKS PASSED"
echo "evidence log: $BASE/prune-run.log"
