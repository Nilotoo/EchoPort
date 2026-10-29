# EchoPort for Windows 10 / 11 (x86_64)

> Status: this build is **source-audited and cross-built**; the Windows `.exe` has not been
> executed on a real Windows machine in this QA round. Please report anything that looks off.

**Bundled dependencies:** both the MSI and the portable zip ship `tools\yt-dlp.exe` and
`tools\ffmpeg.exe` (plus licence notes), so search, online playback and conversion work
without a separate dependency installation. For normal use download the **MSI**; use the
zip only if you want a portable folder.

## Run

1. Extract the zip anywhere (no installer, no admin rights needed).
2. Double-click `echoport.exe`.
3. Windows SmartScreen may warn about an **unsigned** application: choose **More info →
   Run anyway**. (The binary is not code-signed yet; a signed installer is future work.)
4. Keep the whole folder together if you want the bundled `echoport.ico` for shortcuts.

Requirements: **Windows 10 (1903+) or Windows 11, x86_64**, a working **OpenGL 3.3+ driver**
(current AMD/Intel/NVIDIA drivers provide it; the Microsoft Basic Display Adapter / some RDP
sessions only expose OpenGL 1.1 and the app will not start), and an audio output device
(WASAPI). No WebView2, no .NET runtime and no separate installer are needed.

## Optional: online playback and downloads

EchoPort delegates online audio to external tools:

* `yt-dlp.exe` — <https://github.com/yt-dlp/yt-dlp/releases> (keep it updated; extractors change)
* `ffmpeg.exe` — <https://ffmpeg.org/download.html> (used for transcoding/merging)

Put them **on `PATH`** or **next to `echoport.exe`**. EchoPort auto-detects them; local files
play without either. You can also set explicit paths in Settings, or the environment variables
`ECHOPORT_YT_DLP` / `ECHOPORT_FFMPEG`.

Windows Defender/SmartScreen may flag newly downloaded `yt-dlp.exe`/`ffmpeg.exe`; only download
them from the official links above.

## Where data lives

|内容|Location|
|---|---|
| Profiles / playlists / favourites / settings | `%APPDATA%\echoport\profiles\*.json` |
| Lua config + plugins | `%APPDATA%\echoport\config.lua`, `%APPDATA%\echoport\plugins\*.lua` |
| Cover + streamed-audio cache | `%LOCALAPPDATA%\echoport\cache` (`covers/`, `audio/`) |
| Downloaded music | `%USERPROFILE%\Music\EchoPort` (configurable in Settings) |

The cache cap is configurable in Settings → Storage (No cache / 512 MB / 1 GB / 2 GB);
downloads are never auto-deleted. Nothing is uploaded; there is no account, telemetry or cloud.

## Notes / known caveats

* **Unsigned exe** → SmartScreen prompt; no auto-updater and no installer yet.
* **OpenGL 3.3+ driver required** (glow renderer). No software fallback is bundled; update your
  GPU driver if the window cannot be created.
* **DPI scaling**: per-monitor DPI is handled by winit/egui; the in-app UI scale setting
  (100/110/125/150 %) is available in Settings → Appearance.
* **Audio**: WASAPI output via cpal; pick the device in Settings if the default is wrong.
* Historical builds may spawn a brief console window when yt-dlp/ffmpeg run; report it if you see
  it (this is being addressed on the source side).
* Linux/macOS share the same code base; this document covers only the Windows package.

## Uninstall

- **Standard uninstall:** Windows Settings → Apps → EchoPort → Uninstall (or Add/Remove Programs).
- **Clean uninstall (removes app data too):** Start Menu → EchoPort → **Uninstall EchoPort (clean app data)**.
  It stops EchoPort, removes `%APPDATA%\echoport` (profiles/config) and `%LOCALAPPDATA%\echoport` (cache),
  then uninstalls the MSI. Downloaded music in `%USERPROFILE%\Music\EchoPort` is intentionally kept.
