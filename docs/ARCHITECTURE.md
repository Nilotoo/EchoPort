# EchoPort Architecture / 架构说明

> **中文摘要** — EchoPort 是一个用 Rust + egui 编写的轻量、本地优先桌面音乐播放器。
> 代码分为界面层（`src/ui`）、应用外壳（`src/app.rs`，独占状态与线程调度）、领域模型（`model`/`state`）、
> 音频引擎（`audio`：cpal 实时回调 + symphonia 解码线程 + 无锁环形缓冲）、解析器（`resolver`：外部
> yt-dlp/ffmpeg 拉取音频，绝不把媒体 URL 交给 HTTP 客户端）、目录提供者（`providers`/`art`/`background`）
> 与本地持久化（`store`：`$XDG_DATA_HOME/echoport/profiles/*.json`，原子写入）。
> UI 线程从不阻塞：所有网络/解码/下载任务在 `std::thread` 中运行，通过一条 crossbeam 通道回传
> `AppEvent`，每帧集中排空；只有任务进行中才请求重绘，空闲时 CPU 接近 0。
> 平台采用「目录与音频分离」：Spotify/Apple Music 只提供公开元数据，完整播放通过 yt-dlp 匹配公开音频
> （30 秒试听仅作后备），不绕过任何 DRM；YouTube/SoundCloud/Bilibili/网易云经 yt-dlp；本地文件由 symphonia 解码。
> 所有用户可见文案都经过 `i18n::t`（中文/English 双语），所有设置与资料都是本机 JSON，无账号、无遥测、无云端。

This document is the technical overview for contributors. It describes what the
code does **today**; contracts and fixed APIs live in
[`CONTRACT.md`](CONTRACT.md).

---

## 1. Design constraints

The architecture follows directly from the product rules:

| Constraint | Consequence in the code |
|---|---|
| No accounts, no cloud | No auth code, no token storage, no sync. One JSON profile per user under the platform data dir. |
| No DRM circumvention | Spotify/Apple provide public metadata only; full playback is matched to public audio via yt-dlp, with the 30 s preview as a fallback. No Widevine/FairPlay code is linked. |
| Lightweight | eframe/egui with the **glow (OpenGL)** renderer (no wgpu/webview); no async runtime; no background polling when idle. |
| Bilingual | Every user-visible string goes through `i18n::t(lang, Key)`; both languages live in one match table. |
| yt-dlp/ffmpeg are external | Auto-detected optional tools; never bundled as source; all web audio is fetched by yt-dlp itself. |
| Local-first | All state persists to local JSON; network is only used for explicit user actions (search, import, play, download, background info). |

Toolchain: Rust 1.99 / edition 2021, eframe+egui 0.32 (glow, wayland+x11), cpal 0.18,
symphonia 0.6, ureq 3 (blocking HTTP), crossbeam-channel, serde/serde_json, image 0.25.

---

## 2. Module map

```
src/
├── main.rs          native entry point; eframe viewport (1200x800, glow), window icon
├── lib.rs           crate root; re-exports all modules (integration tests use it)
├── app.rs           EchoPortApp: owns state, worker channels, player, screenshot/demo hooks
├── state.rs         AppState + Page + Command (UI → app) + AppEvent (workers → app)
├── model.rs         domain types: Platform, Track, Playlist, Profile, Settings, Playback*, ...
├── store.rs         AppPaths + ProfileStore: atomic JSON persistence, cache size helpers
├── net.rs           blocking Http wrapper (ureq): GET text/json/bytes + download with progress
├── theme.rs         Theme (colours), ThemeMode, cover palette extraction, contrast helpers
├── fonts.rs         system CJK font discovery + egui font installation
├── i18n.rs          Language (En/Zh) + Key enum + t() translation table
├── audio/           cpal output + symphonia decoder thread + lock-free ring buffer
├── resolver/        yt-dlp detection, media resolution/caching, yt-dlp search helpers
├── download.rs      yt-dlp download pipeline with progress callbacks
├── library.rs       local folder scanning (walkdir + lofty tags + embedded covers)
├── providers/       per-platform catalogue search, link import, song background
├── art.rs           cover-art download cache (sha256 file names, .part + rename)
├── background.rs    artist bio / interview links helper (delegates to providers in app)
├── scripting/       Lua config/plugin paths + plugin interface (see docs/SCRIPTING.md,
│                    docs/PLUGINS.md); user scripts live under <config>/echoport
└── ui/              egui layout: shell (mod.rs), style, widgets, cover cache, pages/*
```

### Layering

```
┌──────────────────────────── UI thread ─────────────────────────────┐
│ src/ui (layout)  ──Command──►  EchoPortApp::dispatch  ──► AppState │
│      ▲                              │                              │
│      │                              ├─► resolver / providers /     │
│      │                              │   download / library / art   │
│      │                              │   (std::thread workers)      │
│      │        AppEvent channel ◄────┘                              │
│      └──────── EchoPortApp::tick (drain once per frame) ◄──────────┘
│                                                                     │
│ src/audio::Player (cheap handle)  ──► decoder thread ──► ringbuf    │
│                                            │                        │
│                                            └─► cpal realtime callback
└─────────────────────────────────────────────────────────────────────┘
```

Rules that keep the layers honest:

* `ui/` calls only `EchoPortApp`'s public surface (`dispatch`, `set_page`,
  `state`, `version`, …). It does no file IO, no network, no blocking waits.
* Workers never touch egui. They return plain data in `AppEvent`.
* `model.rs`, `store.rs` and `theme.rs` contain no UI/network/process code, so
  integration tests (`tests/`) can use them directly.

---

## 3. Startup sequence

1. `main()` sets up `env_logger`, builds the `eframe::ViewportBuilder`
   (1200×800 logical, minimum 980×620, `app_id = "echoport"`, generated window
   icon) and forces `eframe::Renderer::Glow`.
2. `EchoPortApp::new(cc)`:
   * installs fonts (`fonts::install`: Latin baked into egui + system CJK fallback),
   * resolves `AppPaths`, creates `profiles/`, `covers/`, `audio/` and the Lua
     config/plugin folders (`<config>/echoport/`),
   * loads the preferred `Profile` via `ProfileStore::load_or_create`,
   * creates `Http`, `ArtCache`, `Player` and the crossbeam command/event channel,
   * in demo mode (`ECHOPORT_DEMO` or `ECHOPORT_SHOT`) seeds offline fixtures and
     applies `ECHOPORT_PAGE` / `ECHOPORT_THEME`; otherwise starts the tool
     detection and library scan workers.
3. The eframe update loop calls `EchoPortApp::update_frame` once per frame.

---

## 4. Threading model

| Thread | Owns | Rules |
|---|---|---|
| **UI thread** | `EchoPortApp`, `AppState`, egui context | Never blocks. Drains `AppEvent`s (`try_recv`) once per frame, dispatches `Command`s, paints. |
| **Decoder thread** (`audio`) | file decoder (symphonia) | Pushes decoded samples into a lock-free `ringbuf`; handles seek via generation counter; reports status through a `Mutex<PlaybackStatus>`. |
| **cpal realtime callback** | audio output device | Only reads the ring buffer and applies volume; no allocation/locks (writes silence on underrun). |
| **Resolver worker** | yt-dlp/ffmpeg child processes | One per playback request; resolves a `Track` to a cached local file; returns `AppEvent::Resolved` / `ResolveFailed`. |
| **Search worker** | provider fan-out | Runs the requested platforms, returns `AppEvent::SearchDone(SearchResults)`; per-platform failures are values, not panics. |
| **Download worker** | yt-dlp child process | Emits `AppEvent::DownloadUpdate` progress; final path/failure in the same event. |
| **Background worker** | Wikipedia / yt-dlp search links | Returns `AppEvent::BackgroundDone`; 8 s budget. |
| **Library scan worker** | walkdir + lofty | Returns `AppEvent::LibraryScanned(Vec<Track>)`. |
| **Tool detection** | `yt-dlp --version`, `ffmpeg -version` | Returns `AppEvent::ToolsDetected`. |
| **Cache prune** (startup, non-demo) | audio cache dir | Best-effort `prune_audio_cache`: oldest-first down to the cap, skips protected/partial files; never fails playback. |
| **Cover worker** | `ArtCache::resolve` | Downloads/decodes cover, returns `AppEvent::CoverReady` with optional palette. |

Scheduling rules:

* All workers send into the **single** `crossbeam_channel` created in
  `EchoPortApp::new`; events are value types, so failures cannot deadlock the UI.
* Repaint policy: while playing, the app asks for a frame every ~66 ms; while
  any worker/toast is live, every ~90 ms; theme transitions request repaints
  until the lerp settles. When idle nothing requests frames, so egui sleeps and
  idle CPU stays near zero.
* Persistence is debounced: mutations call `mark_dirty()`, `tick()` saves after
  ~1.2 s of quiet (and on window close/quit) via `ProfileStore::save` (temp file
  + rename).

---

## 5. Data flow

### 5.1 Commands and events

```
Click "play" row
  └─ ui::layout builds Command::PlayTrack(track)
       └─ EchoPortApp::dispatch
            ├─ sets queue/now_playing immediately (UI feels instant)
            └─ spawns resolver worker: resolver::resolve(config, track)
                 ├─ cache hit  → AppEvent::Resolved{path}
                 └─ download   → yt-dlp writes under <cache>/audio
                                  → (ffmpeg transcode if needed)
       └─ next tick: AppEvent::Resolved → player.load(path); player.play()
```

Search:

```
Command::Search{query, platforms}
  └─ worker: providers::search(&http, &config, query, platforms, limit)
       ├─ youtube/soundcloud/bilibili → resolver::ytdlp_search (yt-dlp -J ...)
       ├─ netease → public search API (music.163.com/api/search/get)
       ├─ apple   → iTunes Search API (entity=song)
       └─ spotify → no anonymous search; returns a hint instead
  └─ AppEvent::SearchDone(SearchResults) → state.search.results (grouped per platform)
```

Link import (`Command::ImportLink`) uses `providers::track_from_url`, which
sniffs the URL host and builds a `Track` from public metadata (Spotify embed
JSON, iTunes lookup, yt-dlp `-J` for the other sites). Playback then follows the
normal resolution path.

### 5.2 Playback path (audio-core)

```
Track → resolver::resolve(config, track)
      → cache/<platform>-<track.cache_key>.<ext>   (hit if exists and > 32 KiB)
      → misses: yt-dlp --no-playlist -f bestaudio… → temp file → rename
                (webm/opus is transcoded to m4a with ffmpeg when needed)
      → Player::load(path) → decoder thread (symphonia) → ringbuf
      → cpal output callback → speakers
```

The resolver **never** returns a media URL to the HTTP client: raw CDN URLs are
IP/session-signed and return 403 outside yt-dlp (verified in
[`PROVIDERS.md`](PROVIDERS.md) §why). Cache keys are stable (`model::Track::cache_key`)
and derived from the platform + track id, never from an expiring URL.

### 5.3 Cover art and palette

`ArtCache::resolve` downloads the track's `art_url` to `<cache>/covers/<hash>.<ext>`
(sniffed from bytes, `.part` + rename) and the worker extracts a 4-colour
`Palette` (`theme::extract_palette_image`, 96×96 bucket sampling). The UI loads
the file as an egui texture (cached in `UiState`) and `state.theme_target`
becomes `Theme::for_mode(mode, Some(palette))`, which `tick()` lerps toward.
In `ThemeMode::Album` the UI is repainted by the cover; without art it falls back
to porcelain blue-white.

---

## 6. Persistence schema

Everything lives under the platform data directory, never in the cloud:

| OS | Data (profiles) | Cache (covers/audio) |
|---|---|---|
| Linux | `$XDG_DATA_HOME/echoport/profiles/` (default `~/.local/share/echoport/…`) | `$XDG_CACHE_HOME/echoport` (default `~/.cache/echoport`) |
| macOS | `~/Library/Application Support/echoport/profiles/` | `~/Library/Caches/echoport` |
| Windows | `%APPDATA%\echoport\profiles\` | `%LOCALAPPDATA%\echoport` |

Lua configuration and plugins live separately under the platform config directory:

| OS | Config / plugins |
|---|---|
| Linux | `$XDG_CONFIG_HOME/echoport/` (default `~/.config/echoport`) → `config.lua`, `plugins/*.lua` |
| macOS | `~/Library/Application Support/echoport/config.lua` + `plugins/` |
| Windows | `%APPDATA%\echoport\config.lua` + `plugins\` |

`AppPaths` exposes `config_dir`, `config_file` and `plugins_dir`; `AppPaths::portable`
places them under the portable root (handy for tests). See [SCRIPTING.md](SCRIPTING.md)
and [PLUGINS.md](PLUGINS.md) for the user-facing Lua reference.

`ProfileStore` uses `dirs::data_dir()`; `AppPaths::resolve()` derives all paths,
`AppPaths::portable(root)` exists for tests/portable builds. Profile names are
sanitised (`store::sanitize_name`) so a name can never become a path escape.

A profile is a hand-editable JSON document (schema version 1):

```json
{
  "schema": 1,
  "name": "Me",
  "created_at": 1761700000,
  "settings": {
    "language": "zh",
    "theme_mode": "pulse",
    "ui_scale": 1.0,
    "follow_cover_accent": true,
    "download_format": "mp3",
    "download_dir": "/home/user/Music/EchoPort",
    "music_dirs": ["/home/user/Music"],
    "ytdlp_path": null,
    "volume": 0.72,
    "results_per_platform": 12
  },
  "tracks": {
    "spotify:…": {
      "id": "spotify:…", "platform": "spotify", "title": "…", "artist": "…",
      "album": null, "duration_secs": 213.5, "art_url": "https://…",
      "page_url": "https://open.spotify.com/track/…", "stream_url": null,
      "local_path": null, "year": 1987
    }
  },
  "playlists": [
    {
      "id": "pl-…", "name": "混合歌单 / Mixed", "description": "",
      "track_ids": ["spotify:…"], "created_at": 1761700000, "updated_at": 1761700100
    }
  ],
  "favorites": ["spotify:…"],
  "history": [{ "track_id": "spotify:…", "played_at": 1761700200 }]
}
```

Compatibility rules:

* `schema` is checked by readers; unknown fields are ignored by serde.
* New fields must use `#[serde(default)]` so old profiles keep loading.
* `tracks` is the metadata map referenced by `playlists`, `favorites` and
  `history`; `Profile::remember` bounded it (5 000 entries, referenced tracks
  win). History is truncated to 300 entries, newest first.
* Writes are atomic: pretty JSON → `<name>.json.tmp-<pid>` → `rename`.
  A corrupt profile is logged and treated as missing (the app falls back to the
  default profile instead of crashing).

Cache layout: `covers/<sha256>.<ext>` and `audio/<platform>-<cache_key>.<ext>`;
both can be cleared from Settings ("clear cache"). Settings → download folder
defaults to `~/Music/EchoPort`.

## Search pipeline, artist pages & play dedupe / 搜索管线、艺术家页与防连点

* **Parallel search** (`app.rs` search spawn): `thread::scope` fetches `providers::search` (tracks) and
  `providers::search_artists` (artists) concurrently for all selected platforms, then sends one
  `SearchResults`; measured ~2.5 s for six platforms (was ~5 s sequential).
* **Relevance**: providers rank tracks per platform by exact/prefix/contains matches of query tokens over
  title + artist; derived producer cards only keep names matching the query tokens (exact → prefix →
  contains, fewer tokens first). `SearchResults.artists` carries per-platform ranked artists.
* **UI sections** (`src/ui/pages/search.rs`): Best matches (top 3 across platforms), Producers & artists
  cards (first 2/platform, single click → `Command::OpenArtist`), then per-platform lists. Tracks collapse to
  5 with a local Show more/Show less (no request); `Command::SearchMore(platform)` requests
  `current + results_per_platform` (12) and appends/dedupes by id, capped at 60, with
  `SearchView.loading_more` driving inline loading dots.
* **Artist pages** (`Page::Artist`, `src/ui/pages/artist.rs`): `AppState.artist_page` holds
  `{ artist, bio, hot_tracks, local_tracks, loading, error, platform }`. `app.rs::open_artist` builds
  `local_tracks` from profile history filtered by artist name (case-insensitive) and spawns
  `providers::artist_detail`, which reuses the background Wikipedia helper for the bio plus platform hot
  tracks (Netease artist API, Apple `lookup?entity=song`, resolver search for YouTube/SoundCloud/Bilibili;
  Spotify empty). UI shows loading/error/no-bio states and Back to search.
* **Play-spam dedupe** (`app.rs::same_track_loading` + PlayTrack/PlayTracks/PlayPlaylist paths): while
  `PlaybackState::Loading` and the requested track is already the current one, the command is dropped;
  `Pause`/`Resume` are no-ops during Loading. Row/card triangles remain single-click play/toggle; general
  rows/cards keep double-click activation.

### Release dates & profile extras / 发行日期与资料扩展

* **Release dates**: `Track.release_date: Option<String>` is populated by providers where available:
  Apple iTunes `releaseDate`, Netease album/publish time, Spotify embed page `releaseDate`, yt-dlp
  metadata (`release_date`/`upload_date` for full imports) and local file tags; flat YouTube/SoundCloud/
  Bilibili search results may leave it `None`. The NowPlaying page renders it as `Released <date>`
  under the macaron palette dots, or `—` when absent. Tracks remembered in the profile keep the date.
* **Profile editor (v0.1.8)**: the editor card is rendered only when `state.profile_edit_open`
  (toggled by the "Edit profile" button / `ECHOPORT_PROFILE_EDIT=1`); the bio textarea keeps a draft,
  Save copies it into `Profile.bio` and closes, Cancel/× discard the draft, avatar choose/remove
  mutates the profile immediately and switching profiles closes a stale editor. The listening-history
  card renders 12 entries (full page scroll, no inner scroll area).
* **Profiles (v0.1.6)**: `Profile { bio: String, avatar: Option<String> }` where the avatar is a
  `data:image/png;base64,…` URL (256 px PNG, encoded via `image` + base64). `ProfileShare` is the
  portable JSON export/import format (schema, avatar, bio, settings, tracks, playlists, favourites,
  history); importing creates/overwrites a profile after validation. Without an avatar the UI paints a
  deterministic pastel letter avatar. All persisted through the existing atomic `ProfileStore`.
* **Packaging (v0.1.6)**: Linux tarball and Windows zip include `docs/plugins.html` + `docs/config.html`
  (bilingual HTML manuals) and `examples/config.{lua,zh-CN.lua,en.lua}`; the Windows zip additionally
  has `README-WINDOWS`, `echoport.ico` and the Windows build scripts.

### Background performance mode / 后台模式

* **Background performance mode**: in `app.rs::update` the viewport focus/minimized flags set
  `state.ui_backgrounded`; repaint intervals become 1000 ms when backgrounded (66 ms playing,
  90 ms busy, layout repaint off) and normal otherwise. Audio runs on its own thread/stream and is not
  throttled; returning to the foreground restores cadence immediately. On the software-GL QA host the
  CPU effect is masked by the known continuous-repaint anomaly (see QA_REPORT §6).
* **Icon geometry (v0.1.7)**: reload icon redrawn as a connected arc with a tangential arrowhead
  (rotated ~30°); shuffle icon built from the icon-pack SVG path (`M7 17L17 7` + arrow head, second
  copy rotated 90°) so it stays a clean X at 16–20 px. Source button = #34 share, search globe = #25.

### Update check, splash & release pipeline / 更新检查、启动动画与发布流程（v0.1.9）

* **`update.rs`**: anonymous `GET https://api.github.com/repos/<owner>/<repo>/releases/latest` with a
  fixed User-Agent, no token/telemetry; compares `tag_name`/`version` semver-like against
  `CARGO_PKG_VERSION`. If the repo has no Release it falls back to the raw `VERSION` file on
  `main`/`master`. Repo defaults to `nilotoo/EchoPort` and is runtime-overridable with
  `ECHOPORT_UPDATE_REPO=owner/name`. Startup check + every 6 h; `About → Check for updates` triggers one
  manually. An available update exposes an Update button which downloads the platform asset, verifies its published sha256, then silently applies it and restarts (Windows setup.exe /S; Linux atomic binary replace); macOS opens the release page — there is
  **no auto-download or install**. `ECHOPORT_NO_UPDATE_CHECK=1` disables the checker;
  `ECHOPORT_FAKE_UPDATE=9.9.9` injects a fake result for tests/screenshots.
* **`ui/splash.rs`**: short startup overlay (target ~0.9 s, held up to ~1.4 s while the first check is in
  flight) with the ringless app mark, EchoPort wordmark and a thin progress line. If a newer version is
  known during the splash it shows `Update available vX` + **Stay on this version / Update** buttons
  (Update downloads/verifies/installs and restarts; Stay dismisses). `ECHOPORT_SPLASH=1` forces the splash for QA,
  `ECHOPORT_NO_SPLASH=1` skips it; `ECHOPORT_SHOT_AFTER=<secs>` delays screenshot capture for QA.
* **Icon assets (v0.1.9)**: `assets/icon.png` (256), `icon-128.png`, `icon-48.png`, `icon-32.png` and the
  multi-size `echoport.ico` are ringless redraws; `main.rs` loads `assets/icon.png` at runtime and the
  splash reuses the same mark. `build.rs` embeds the ICO into Windows GNU builds via windres
  (PE `.rsrc` present).
* **Release pipeline**: `VERSION` (currently 0.1.9) is committed; `.github/workflows/release.yml` runs on
  `v*` tags, builds the Linux tarball + Windows zip and publishes a GitHub Release. Cargo
  `repository`/`homepage` point at `https://github.com/nilotoo/EchoPort` (still a placeholder until the
  user confirms the remote). `scripts/install-arch.sh` performs a no-sudo user-local install
  (`~/.local/bin`, desktop entry, hicolor 32/48/128/256 icons); release tarballs ship it plus those icons.
* **Verification status**: update-check/version-comparison logic is covered by unit tests and verified with
  fake/local data; **no live GitHub request was made this round** because the repo URL/auth are pending.

### Security, slimming & installers (v0.3.0) / 安全、精简与安装包

* **Update-check hardening**: `update::repo()` only accepts `owner/name` (GitHub-safe chars), release
  URLs are restricted to `https://github.com/…`, version strings are whitelisted, and the checker
  never downloads/installs. Default repo is `Nilotoo/EchoPort` (public repo not pushed yet).
* **URL & import safety**: `Command::OpenUrl` is limited to http/https; `ProfileShare`/playlist import
  has an 8 MB file cap plus avatar/bio/name/playlist/track count limits. No personal paths remain in
  source/tests (`ECHOPORT_TEST_MP3` replaces the old local-path test constant).
* **Binary size / memory**: release profile uses `lto = "fat"`, `strip = true`,
  `codegen-units = 1` → Linux binary **15,364,248 B** (v0.1.9: 16,130,920 B, −4.75%) despite ~100 KB
  of embedded docs. Cover-texture cache: 96 textures @320 px → 64 textures @256 px, texture budget
  roughly 39 MB → 17 MB.
* **Embedded docs**: `docs/plugins.html` + `docs/config.html` (+ `SCRIPTING.md`/`PLUGINS.md`) are
  embedded with `include_str!`; `Command::OpenPluginDocs` / `OpenConfigDocs` write them to the data dir
  (`<data_dir>/docs/`) and open the browser. Settings → Scripting shows the buttons.
* **Installer pipeline**: `scripts/package-linux.sh` (tarball), `scripts/package-arch.sh`
  (+ AUR `packaging/arch/PKGBUILD`), `scripts/package-windows-cross.sh` (zip), `scripts/package-msi.sh`
  (per-user MSI via msitools/wixl) and `scripts/package-macos.sh` (universal .app, CI-only).
  `.github/workflows/release.yml` builds Linux tarball + Arch pkg, Windows zip + MSI, macOS zip and
  uploads every `.sha256` (see `packaging/ci/`). The MSI and macOS bundle are cross/CI-built and
  **not executed on Windows/macOS** in this round; the Linux artifacts are smoke-tested here.

## Windows 10/11 notes / Windows 说明

Source-audited this round; a PE executable was cross-built but **not executed on Windows**.

* **Paths**: `dirs` maps data/config to `%APPDATA%\echoport` (profiles, config.lua, plugins) and cache
  to `%LOCALAPPDATA%\echoport\cache` (covers, audio); downloads default to
  `%USERPROFILE%\Music\EchoPort`. All paths are `PathBuf` (UTF-16-safe); cache keys use lossy
  strings only for bookkeeping.
* **Audio**: cpal `default_host()` on Windows is WASAPI; `choose_output_config` probes
  `supported_output_configs()` (F32/I16/...) and falls back to the device default — source-verified,
  not run on Windows.
* **Rendering**: eframe `Renderer::Glow` uses OpenGL via WGL; requires a vendor driver with OpenGL
  3.3+ (Basic Display Adapter/RDP only expose 1.1 → window creation fails). No WebView2 involved.
* **Fonts**: `fonts.rs` Windows candidates are `msyh.ttc/msyh.ttf/simhei.ttf/simsun.ttc` (Microsoft
  YaHei ships with Windows 10/11), with system fontconfig fallback only on Linux.
* **External tools**: `resolver::detect_tool` also probes `<dir>\<name>.exe` on Windows, plus
  `ECHOPORT_YT_DLP`/`ECHOPORT_FFMPEG`; the hardcoded "common install locations" are Unix-only, so
  tools must be on PATH or next to the exe (documented).
* **Icon**: v0.1.6 embeds the multi-size `assets/echoport.ico` into Windows GNU builds via
  `build.rs`/windres (PE `.rsrc`); the runtime window icon still comes from `main.rs::build_icon()`
  (64 px) and `assets/echoport.ico` also ships for shortcuts. MSVC builds need windres/llvm-windres
  available or the build script skips the icon with a warning.
* **Windows fixes landed in v0.1.6**: `src/main.rs` sets
  `#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]` (PE `Subsystem 00000002` GUI);
  `src/process.rs::quiet()` applies `CREATE_NO_WINDOW` to every external-tool spawn; `build.rs` embeds
  `assets/echoport.ico` via windres (PE `.rsrc` resource directory present); tool discovery additionally
  probes next-to-exe, `%LOCALAPPDATA%\Programs`, winget Links, Python Scripts, scoop shims and
  Program Files. Remaining unknowns (unverified on Windows): rfd dialogs, WASAPI device pick/audio
  output, DPI behaviour, real OpenGL context creation; a single-instance lock is still not implemented.
* **DPI**: winit/egui handle per-monitor DPI; in-app UI scale 100–150 % also available.
* **Verification status**: Linux: full tests/build/screenshots. Windows: source audit + cross-build
  (`x86_64-pc-windows-gnu`, PE/imports checked) with mingw-w64 GCC; **runtime behaviour on Windows
  is unverified** (no Windows/Wine host). Native Windows build via `scripts/build-windows.ps1`
  (MSVC) is the supported release path.

## Resource footprint & minimum configuration / 资源占用与最低配置

Measured with `scripts/qa-resources.sh` on the v0.1.3 release binary on this QA host
(Arch Linux, 32-core KVM, **no GPU → Mesa llvmpipe software rendering**, no sound card);
raw output: `screenshots/qa-resources.log`.

| 项目 / Item | 实测 / Measured |
|---|---|
| 二进制 / binary | 15,364,248 B (≈14.7 MiB stripped, LTO fat), sha256 `a004315e…` |
| tar.gz | 7,015,806 B (≈6.7 MiB), sha256 `6e125189…` |
| 启动 / startup（进程→窗口 / 首帧） | 163–183 ms (median 177) / 425–439 ms (frame 40 incl. PNG write) |
| 空闲 CPU / idle CPU (8 s ×5) | 0.000–0.125 % (software GL; historical repaint anomaly in QA_REPORT) |
| 空闲内存 / idle memory | RSS ≈254.7 MiB, PSS 127.9, RssAnon 81.2, RssFile 167.6, RssShmem 5.9 |
| 线程 / FD | 68 / 10 |
| 播放一首流媒体 / one streamed track | RSS ≈288.8 MiB, PSS ≈146–159, RssAnon ≈108, threads 73, FDs 39 |
| Demo 动画+模拟播放 / demo animation | CPU ≈156 % (llvmpipe), RSS ≈274 MiB |
| 缓存增长 / cache growth（1 GiB cap, 5 tracks） | 20.6 MiB / 5 files; downloads unchanged (8408 KB before/after) |
| 无缓存模式 / no-cache | startup purge + previous-track delete verified (old files removed, protected fresh file kept) |
| GL 上下文 / GL context | Mesa 26.2.5 llvmpipe (LLVM 23.1.3), OpenGL 4.6, not accelerated |
| 动态依赖 / dynamic deps | libasound / libgcc_s / libm / libc (GL/X11 loaded at runtime); GLIBC symbols up to **2.44** |

**Why the memory/CPU numbers look high on this host**: eframe/egui uses the **glow (OpenGL)** renderer;
on a GPU-less VM Mesa falls back to **llvmpipe**, so `RssFile` is dominated by shared Mesa/LLVM mappings
(~167 MiB) and animation/playback consumes a lot of CPU (~156 % in demo on 32 cores). On a normal
GPU-backed desktop the driver stack is different; those numbers were **not measured here** and are not
claimed. Private (`RssAnon`) usage is ≈80 MiB idle / ≈108 MiB while playing.

**Minimum configuration (estimate)**: x86_64 + SSE4.2, dual-core 1.5 GHz+ (4+ cores without a GPU);
512 MB–1 GB free RAM; OpenGL 3.3+ (llvmpipe fallback works, slower); ≈15 MiB app + configurable 0–2 GiB
audio cache + optional yt-dlp/ffmpeg; Linux x86_64 — **prebuilt tarball needs glibc ≥ 2.44** (rebuild from
source with Rust 1.85+ for older distros), Windows 10+/macOS 11+ build targets (unverified); an audio
output device (no device → playback error) and HTTPS for online sources (local playback is offline).

### Cache policy (verified by `scripts/qa-cache-prune.sh`)

* Streamed audio is cached **on disk only** (`<cache>/audio`), decoded from the
  file; the player never keeps whole tracks in RAM.
* A best-effort startup worker (in `app.rs`) calls `resolver::prune_cache`, which
  trims the **whole audio-cache directory (recursive)** towards the per-profile cap
  (`Settings.cache_limit_mb`, default **1024 MiB**, GUI chips 0/512/1024/2048;
  `ECHOPORT_CACHE_LIMIT_MB` overrides for QA/CI, floor 64 MiB), deleting the oldest
  `mtime` first. `Command::SetCacheLimit(0)` disables prefetch, deletes the previous
  track's cache file when switching and purges old cache at startup (the currently
  playing file stays on disk while playing).
* Files modified within the last **120 s** (`PRUNE_PROTECT`) are protected — they
  may be the currently playing file or the prefetched next track — and are never
  deleted, as are `*.part` / `*.tmp` / `*.ytdl` temporary entries. Protected and
  temporary files still count towards the directory total, so an actively playing
  track can keep the cache briefly above the cap until it ages out.
* Only the **next** queue track is prefetched; playlists are never pre-downloaded.
* User downloads (`settings.download_dir`, default `~/Music/EchoPort`) are never
  touched by cache housekeeping; clearing the cache does not remove them.
* Cover art lives in `<cache>/covers`; in memory `CoverCache` keeps at most
  **96** textures (LRU eviction) at ≤ **320 px** (`MAX_TEXTURES` /
  `MAX_TEXTURE_SIDE`), so browsing a large library cannot grow GPU/RAM use
  without bound.
* Settings displays the combined audio+cover cache size and can clear it; this
  is local, user-scoped data — nothing is uploaded and no credentials exist to
  leak.

---

## 7. UI, theme and i18n layers

* `UiState` (in `ui/mod.rs`) holds ephemeral UI data: texture cache keyed by
  file path, animation flashes. It is *not* persisted.
* `AppState` (in `state.rs`) holds everything the UI can display: current page,
  typed views (`SearchView`, `LibraryView`, `DownloadsView`, `CoverView`,
  `BackgroundView`), queue/playback/repeat/shuffle, toasts.
* `Theme` is a plain value of colours (`bg, panel, surface, text, accent,
  on_accent, highlight, on_highlight, macaron[5], glow[2]`, …).
  `Theme::to_visuals()` maps it onto egui `Visuals`; `Theme::lerp()` powers
  smooth transitions. Contrast helpers (`theme::contrast`, `readable_on`,
  `best_on`) keep body text ≥ 4.0:1 and button labels legible (checked by
  `tests/theme_contrast.rs`).
* **Now playing page** uses the original minimal design: plain theme background,
  large cover on the left, track info/controls and the
  album/duration/source info block on the right. No cover tint, glow, blurred
  backdrop, floating decoration or continuous animation; the only motion comes from
  functional loading indicators. Cover textures are capped at 96 entries / ≤320 px
  in `CoverCache`.
* **Cover geometry**: every slot paints covers through the same rounded
  `RectShape` clipping path — real textures, generated placeholder bands/letters,
  playlist/sidebar/player-bar/card/row/Now-playing covers all share the slot's
  radius, so a placeholder can never appear square inside a rounded frame.
* **Player bar alignment**: `player_bar` receives the sidebar width and centres its
  transport + scrubber on the content column (`(panel.width + sidebar) / 2`), using
  the same order/gaps/button sizes as the Now-playing transport. All transport buttons
  share one vertical centre line (no top-aligned outliers), only the left divider is painted, and the right tool cluster is tightened
  (left-shifted) so it stays clear of the transport at every window size.
* **Playlist sharing**: `model::PlaylistShare` is a versioned metadata-only JSON
  document (tracks with source ids/URLs, no audio, no cache paths); export writes it
  to the download folder, import validates/normalises and creates a new playlist.
* **Background relevance**: `background.rs` normalises artist/album/title, gates every
  candidate on artist tokens + (album or title) tokens (song content: artist+title) and
  returns dedicated `album`/`song` sections; unrelated results are dropped and empty
  sections are omitted. Wikipedia/MusicBrainz/YouTube links stay human pages, 8 s budget.
* **Double-click UX**: `UiState.selected_track` / `selected_playlist` hold the current
  single-click selection; double-click dispatches `PlayTrack`/`OpenPlaylist`. Hover
  tooltips use `DoubleClickPlay`/`DoubleClickOpen`.
* **Icons**: in-app UI icons are user-supplied assets embedded from
  `assets/icons/white/png/icon-<n>-48px.png` via `include_bytes!` and tinted with
  `theme.text`/`accent` at paint time (`style::asset_icon`, textures cached in
  `UiState`, loaded once — idle safe). Asset glyphs are fitted into a centred
  square (`min(w,h)`, aspect preserved) and the play triangle is rotated +90° to
  point right. Unmapped glyphs (home/profile/volume/repeat…)
  stay hand-painted; the app/launcher icon (`assets/icon.png`, generated by
  `scripts/gen-icon.py`) is intentionally unchanged. Asset licence: MIT-0,
  © Singular Programmer (`assets/icons/LICENSE.txt`).
* **Interface scale**: `Settings.ui_scale` (persisted, clamp 0.8–2.0) is applied with
  `egui::Context::set_zoom_factor` in the frame loop; `Command::SetUiScale(f32)` changes
  it and re-lays out the whole UI. The Settings page exposes 100 / 110 / 125 / 150 %
  chips. Screenshot/env QA can set it by writing the profile `settings.ui_scale`.
* Three hand-picked themes: **Pulse** (default; porcelain-white canvas, vivid
  blue primary, fluorescent-lime highlight), **Tide** (light aqua, teal primary,
  deep-navy text) and **Ink** (low-saturation dark slate). `ThemeMode::from_key`
  and the serde aliases still accept the pre-redesign keys
  `blue_white`/`midnight`/`album` so old profiles and scripts keep working.
* Cover art does not replace the theme: with **Settings → follow cover accent**
  (`Settings.follow_cover_accent`, default on; demo screenshots default off) the
  accent / highlight / macaron colours are tinted from the current cover via
  `Theme::apply_cover_palette`, while background and text stay theme-fixed for
  readability.
* `i18n::Key` is a plain enum; `Key::pair()` returns `(English, 中文)` and
  `t(lang, key)` selects one. No hard-coded user-visible strings are allowed in
  the UI; `tests/i18n_coverage.rs` parses `src/i18n.rs` to prove every key has
  both translations.
* `fonts::install` registers a system CJK face (Noto Sans CJK, WenQuanYi,
  PingFang, Microsoft YaHei, …, or `$ECHOPORT_CJK_FONT`) as a fallback family, so
  Chinese titles render without bundling a huge font. Missing CJK fonts are
  logged, not fatal.

---

## 8. Provider architecture & why there is no DRM bypass

EchoPort separates **catalogue** (metadata/search) from **audio** (bytes):

```
providers::search  ──► provider APIs / yt-dlp search ──► Vec<PlatformResults>
providers::track_from_url ──► public metadata ──► Track
resolver::resolve  ──► yt-dlp (matched public audio) ──► local file ──► Player
```

* A `Track` stores metadata plus an optional `page_url`/`stream_url`. Playback of
  a remote track is always delegated to `resolver`, which uses yt-dlp (or, for
  public 30 s previews, a plain CDN download) — never a streamed media URL.
* Spotify is **link-import only**: anonymous Spotify search no longer exists
  (`get_access_token` → 403, API → 401). The `open.spotify.com/embed/track/<id>`
  page still returns public `__NEXT_DATA__` metadata (title, artists, duration,
  cover, 30 s preview), which is enough to import and match.
* Apple Music is searchable through the public iTunes Search API (metadata, art,
  30 s previews). Its own full tracks are FairPlay-protected, so EchoPort resolves
  playback through yt-dlp to **full-length public audio** (title/artist/duration
  sanity checks) and only falls back to the 30 s preview when no match is available.
  DRM is never bypassed and no account credentials are used.
* Netease uses its public search endpoint and yt-dlp for playback; SoundCloud /
  Bilibili / YouTube use yt-dlp search prefixes (`scsearch`, `bilisearch`,
  `ytsearch`).
* Background info is keyless too: Wikipedia REST summary, MusicBrainz, Genius
  search links and `ytsearch` interview/explanation links.
  As a last resort, SoundCloud/Bilibili source lists append a `ytsearch1:` public-audio
  match after their native URL/search sources fail (412/anti-bot/HLS cases); media URLs
  are still only consumed by yt-dlp — never handed to the HTTP client.

Why not decrypt Spotify/Apple audio? EchoPort has a hard product rule: **no DRM
circumvention, no account tokens, no decryption keys**. Shipping Widevine or
FairPlay code would violate the project's legal/ethical constraints (and the
providers' terms), so the app deliberately caps those sources at public
metadata + previews and uses YouTube as the universal public audio backend.
This is documented honestly in [PROVIDERS.md](PROVIDERS.md) and the README.

Rate limiting: yt-dlp processes are serialised/limited (contract: ≤ 2 concurrent
total); MusicBrainz/Genius calls are low volume with a descriptive User-Agent.
Per-platform search failures become `PlatformResults::error` strings and never
abort the other platforms.

---

## 8b. Scripting, configuration and plugins

EchoPort also offers an optional user-scripting layer; it is additive and never
needed for normal playback.

* **Lua configuration**: `<config>/echoport/config.lua` (see `AppPaths::config_file`
  and `scripting::ScriptPaths`) is the user-editable config for startup options,
  theme preferences, keyboard shortcuts, playback tuning and paths. Invalid
  scripts are reported as warnings; they can never panic the app.
* **Lua plugins**: `<config>/echoport/plugins/*.lua` are loaded in sorted order.
  Compiled-in Rust extensions use the `Plugin` / `PluginRegistry` interface in
  `src/scripting/mod.rs`.
* **Commands / events**: scripts can trigger the same commands as the UI
  (`Command::SetFollowCoverAccent`, `Command::ReloadScripts`,
  `Command::RunScriptAction`, `Command::OpenScriptsFolder`, …); the app reports
  results back as `AppEvent::RunCommand(Command)` and `AppEvent::ScriptToast`.
  Script-triggered work goes through the normal command dispatch path, so the UI
  threading rules (nothing blocking on the UI thread) still apply.
* Reference docs for the script/plugin authoring APIs: [SCRIPTING.md](SCRIPTING.md)
  and [PLUGINS.md](PLUGINS.md); runnable samples live in `examples/config.lua`
  and `examples/plugins/example.lua`.

## 9. Performance profile

Targets from the contract (verified in [QA_REPORT.md](QA_REPORT.md)):

| Metric | Target | How it is achieved |
|---|---|---|
| Release binary | < ~25 MB | LTO thin, codegen-units 1, strip, panic=abort; no font/asset bundling. |
| Idle CPU | < 1.5 % | No continuous repaint; animations only while changing; workers sleep. |
| Idle memory | < ~200 MB | One window, texture cache downscaled, bounded track map/history. |
| UI responsiveness | never freeze | All IO/decoding off the UI thread; search shows a spinner and stays interactive. |
| Startup | fast | JSON profile load + tool detection on a worker. |

---

## 10. QA hooks (used by scripts and CI)

| Env var | Effect |
|---|---|
| `ECHOPORT_DEMO=1` | offline demo fixtures (fake tracks/playlists/covers); no network needed |
| `ECHOPORT_SHOT=/path/file.png` | render, write a PNG, exit (also implies demo) |
| `ECHOPORT_PAGE=home\|search\|library\|playlists\|downloads\|now_playing\|profile\|settings` | start on a page (demo) |
| `ECHOPORT_THEME=pulse\|tide\|ink` | force a theme in demo screenshots (legacy `blue_white`/`midnight`/`album` aliases still accepted) |
| `ECHOPORT_FOLLOW_COVER=1` | enable cover-derived accent colours in demo screenshots |
| `ECHOPORT_LIVE=1` | keep the real network/audio path even when `ECHOPORT_SHOT` is set (live QA screenshots) |
| `ECHOPORT_CJK_FONT=/path/font.ttc` | override the CJK fallback font |
| `ECHOPORT_YT_DLP` / `ECHOPORT_FFMPEG` | explicit tool paths (same as Settings) |

`scripts/run-demo.sh` drives the screenshot set; `scripts/build.sh` and
`scripts/package-linux.sh` build/pack the release; `cargo test` covers the
platform-independent layers (persistence, playlists, i18n, theme, model,
packaging assets). See [QA_REPORT.md](QA_REPORT.md) for the exact commands and
measured numbers of the current release.

---

## 11. Known limitations

* Spotify cannot be searched anonymously; Apple Music cannot be streamed
  legitimately without DRM. Both are import/preview/match sources by design.
* Bilibili and some SoundCloud tracks are region-blocked or DRM/HLS-only; the
  UI degrades gracefully but those tracks may fail to play.
* Netease playback is legally gray in some jurisdictions and can change without
  notice (it uses public endpoints only, no account).
* YouTube extractors break regularly; keep yt-dlp updated. EchoPort auto-detects
  yt-dlp/ffmpeg and reports missing tools in Settings.
* Only a subset of codecs is decoded natively (mp3/aac/alac/flac/wav/ogg); other
  containers are transcoded by ffmpeg before playback.
* Windows and macOS builds are supported by Cargo but releases are validated on
  Linux; CI/packaging scripts for other OSes are best-effort (see README).
* Idle memory target (<200 MB) is measured at 252 MiB RSS on a GPU-less CI-style
  host that loads the Mesa/LLVM software rasterizer (~167 MiB shared); the
  proportional footprint is 133 MiB. See QA_REPORT.md §6.3 for the breakdown.
* The Lua scripting / plugin API is new in 0.1.x and may evolve; script errors are
  collected as warnings and never crash the app. Reference docs live in
  SCRIPTING.md / PLUGINS.md.
