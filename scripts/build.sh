#!/usr/bin/env bash
# EchoPort — release build helper.
#
#   ./scripts/build.sh            # cargo build --release
#   ./scripts/build.sh --check    # cargo check only (fast sanity check)
#   ./scripts/build.sh --debug    # debug build (used by run-demo.sh)
#
# The script is safe to re-run and is quiet unless something fails; it never
# deletes the shared target directory. All non-release flags are for developer
# convenience, packaging and CI only use the plain release build.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MODE="release"
for arg in "$@"; do
    case "$arg" in
        --release) MODE="release" ;;
        --debug) MODE="debug" ;;
        --check) MODE="check" ;;
        -h|--help)
            sed -n '2,9p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "error: unknown argument '$arg' (try --help)" >&2
            exit 2
            ;;
    esac
done

if ! command -v cargo >/dev/null 2>&1; then
    echo "error: cargo was not found. Install Rust from https://rustup.rs (Rust 1.85+)." >&2
    exit 1
fi

CARGO_VERSION="$(cargo --version 2>/dev/null || echo 'cargo (unknown)')"
echo "EchoPort build — $(basename "$ROOT")"
echo "  $CARGO_VERSION"
echo "  mode: $MODE"

case "$MODE" in
    check)
        cargo check --locked 2>/dev/null || cargo check
        echo "cargo check: OK"
        ;;
    debug)
        cargo build
        ;;
    release)
        # `--locked` keeps a packaged build reproducible; fall back gracefully
        # when Cargo.lock is out of sync (e.g. a dependency was just added).
        cargo build --release --locked 2>/dev/null || cargo build --release
        ;;
esac

if [ "$MODE" = "check" ]; then
    exit 0
fi

PROFILE_DIR="$ROOT/target/$MODE"
BIN="$PROFILE_DIR/echoport"
if [ ! -f "$BIN" ]; then
    echo "error: expected binary not found at $BIN" >&2
    exit 1
fi

SIZE_BYTES="$(stat -c '%s' "$BIN" 2>/dev/null || stat -f '%z' "$BIN")"
SIZE_HUMAN="$(du -h "$BIN" | cut -f1)"
echo "built: $BIN (${SIZE_HUMAN}, ${SIZE_BYTES} bytes)"
echo "run:   $BIN"
echo "demo:  ECHOPORT_DEMO=1 $BIN"
