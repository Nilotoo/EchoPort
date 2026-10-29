# Bundled tools (Windows)

EchoPort's Windows package bundles two external command-line tools so online
playback, search and format conversion work without a separate setup step:

- **yt-dlp** — https://github.com/yt-dlp/yt-dlp — Unlicense.
  The executable is downloaded from the project's official GitHub release.
- **FFmpeg** — https://ffmpeg.org/ — LGPL-2.1-or-later build from
  https://github.com/BtbN/FFmpeg-Builds (win64 LGPL build).
  FFmpeg is a separate program; EchoPort invokes it as a subprocess.
  Source code and licence text are available at the links above and in
  `LICENSE-ffmpeg.txt` next to the bundled executable.

Neither tool is linked into EchoPort's binary. Rebuilding or replacing them is
allowed; EchoPort also detects system-installed yt-dlp/ffmpeg from PATH or the
`ECHOPORT_YT_DLP` / `ECHOPORT_FFMPEG` environment variables.
