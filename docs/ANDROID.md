# EchoPort Android (arm64, Android 8+)

This is the first phone port of EchoPort: the same Rust/egui UI and audio engine,
with a touch-first shell. It is intentionally an **MVP**, not full desktop parity.

## What works

- Local audio import via the Android system document picker. Picked files are
  copied into the app-private `Music/` directory, then the normal local scanner
  reads them. No broad storage permission is requested.
- Local playback (cpal + symphonia), playlists, history, favourites, lyrics
  (local `.lrc` first, LRCLIB fallback), theme/language settings.
- Metadata search against the public providers. Desktop-only download / yt-dlp /
  plugin / self-update flows are hidden because Android cannot run the desktop
  helper binaries.
- Phone layout: bottom navigation, compact two-row player, single-tap playback,
  scrolling 5-line lyrics.

## What is intentionally not included yet

- YouTube/Spotify/Apple playback or downloads (no yt-dlp process on Android; no
  DRM bypass).
- Desktop auto-updater; APK updates are done by reinstalling the APK.
- Lua plugins in the current Android build (`--no-default-features` for size and
  to avoid a second interpreter).
- Tablet/foldable multi-column layouts beyond the responsive breakpoints.

## Build

Requirements: JDK 17, Android SDK platform 35, build-tools 35, NDK 29, Rust
`aarch64-linux-android` target, and the patched `cargo-apk`:

```sh
rustup target add aarch64-linux-android
cargo install --git https://github.com/parasyte/cargo-apk.git \
  --rev 282639508eeed7d73f2e1eaeea042da2716436d5 cargo-apk
```

The repo script auto-detects `~/Android/jdk`, `~/Android/Sdk` and the newest NDK
under `~/Android/Sdk/ndk`, then builds the arm64 APK:

```sh
./scripts/build-android.sh              # debug-signed APK for local testing
```

### Formal signing

The keystore and password stay outside the repository:

```sh
ECHOPORT_ANDROID_KEYSTORE=/absolute/path/echoport-release.keystore \
ECHOPORT_ANDROID_KEYSTORE_PASSWORD='...' \
./scripts/build-android.sh --release
```

Output: `dist/echoport-<version>-android-arm64.apk`.

A single-key keystore is expected by `cargo-apk`. Keep the keystore backed up;
losing it prevents updating an installed app.

## Install

```sh
adb install -r dist/echoport-<version>-android-arm64.apk
```

The APK is arm64-v8a only and targets Android 8.0 (API 26) or newer.
