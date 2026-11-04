#!/usr/bin/env bash
# Build the Android (arm64-v8a) APK on this machine.
#
#   ./scripts/build-android.sh            # debug APK (debug keystore)
#   ECHOPORT_ANDROID_KEYSTORE=/path/to/release.keystore \
#   ECHOPORT_ANDROID_KEYSTORE_PASSWORD=... \
#   ./scripts/build-android.sh --release  # formally signed APK
#
# The release keystore must be a single-key keystore accepted by apksigner;
# the password is read from the environment and never written to the repo.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

RELEASE=0
for arg in "$@"; do
    case "$arg" in
        --release) RELEASE=1 ;;
        *) echo "unknown argument: $arg" >&2; exit 2 ;;
    esac
done

if [ -z "${JAVA_HOME:-}" ] && [ -x "$HOME/Android/jdk/bin/java" ]; then
    export JAVA_HOME="$HOME/Android/jdk"
fi
if [ -z "${JAVA_HOME:-}" ]; then
    echo "JAVA_HOME is not set and ~/Android/jdk is missing" >&2
    exit 1
fi
export PATH="$JAVA_HOME/bin:$HOME/.cargo/bin:$PATH"

export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
if [ -z "${ANDROID_NDK_HOME:-}" ]; then
    ANDROID_NDK_HOME="$(find "$ANDROID_HOME/ndk" -mindepth 1 -maxdepth 1 -type d 2>/dev/null | sort -V | tail -1)"
fi
if [ -z "${ANDROID_NDK_HOME:-}" ] || [ ! -d "$ANDROID_NDK_HOME" ]; then
    echo "Android NDK not found; set ANDROID_NDK_HOME" >&2
    exit 1
fi
export ANDROID_NDK_HOME ANDROID_NDK_ROOT="$ANDROID_NDK_HOME"

TOOLCHAIN="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin"
API=35
export CC_aarch64_linux_android="$TOOLCHAIN/aarch64-linux-android${API}-clang"
export AR_aarch64_linux_android="$TOOLCHAIN/llvm-ar"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$CC_aarch64_linux_android"

rustup target add aarch64-linux-android >/dev/null

if [ "$RELEASE" -eq 1 ]; then
    if [ -z "${ECHOPORT_ANDROID_KEYSTORE:-}" ] || [ -z "${ECHOPORT_ANDROID_KEYSTORE_PASSWORD:-}" ]; then
        echo "release build requires ECHOPORT_ANDROID_KEYSTORE and ECHOPORT_ANDROID_KEYSTORE_PASSWORD" >&2
        exit 1
    fi
    export CARGO_APK_RELEASE_KEYSTORE="$ECHOPORT_ANDROID_KEYSTORE"
    export CARGO_APK_RELEASE_KEYSTORE_PASSWORD="$ECHOPORT_ANDROID_KEYSTORE_PASSWORD"
    cargo apk build --lib --release --no-default-features
    APK="$ROOT/target/release/apk/echoport.apk"
    VERSION="$(tr -d '[:space:]' < VERSION)"
    mkdir -p "$ROOT/dist"
    OUT="$ROOT/dist/echoport-${VERSION}-android-arm64.apk"
else
    cargo apk build --lib --no-default-features
    APK="$ROOT/target/debug/apk/echoport.apk"
    VERSION="$(tr -d '[:space:]' < VERSION)"
    mkdir -p "$ROOT/dist"
    OUT="$ROOT/dist/echoport-${VERSION}-android-arm64-debug.apk"
fi

cp "$APK" "$OUT"
sha256sum "$OUT" > "$OUT.sha256"
echo "built: $OUT"
ls -lh "$OUT"
