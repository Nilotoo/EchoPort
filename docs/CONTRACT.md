# EchoPort — Engineering Contract

Read this before touching code. It is the single source of truth for interfaces,
ownership and quality bars. If you need to change a public interface, ask the
Lead first.

## 1. What we are building

A lightweight, local-first, cross-platform desktop music player in Rust + egui.

Non-negotiable product rules:

* **No accounts.** Never ask for or store credentials/tokens. All user data is
  local JSON under `dirs::data_dir()/echoport`.
* **No DRM circumvention.** Spotify and Apple Music cannot be streamed without
  DRM. We only use public metadata + 30 s previews and match playback to public
  audio with yt-dlp. Never add Widevine/FairPlay code, never ship decryption
  keys.
* **Minimal internals.** The wordmark's decorative dot ("EchoPort." with an
  accent-coloured dot) exists **only in the GUI**. No dots in crate/binary
  names, file names, data directories, config keys or identifiers.
* **Bilingual.** Every user-visible string goes through `i18n::t` (Chinese +
  English). No hard-coded UI strings.
* **Chinese + English UI, light resource use.** Idle UI must not repaint
  continuously. No blocking calls on the UI thread.
* **yt-dlp/ffmpeg are external tools.** Auto-detected, optional, never bundled
  as source. Playback/download of web audio always goes through yt-dlp.

## 2. Build / run

```bash
cd $HOME/EchoPort
cargo check                     # must stay green in your scope
cargo test                      # 13 unit tests currently
cargo run                       # normal app (real network + audio)
# Offline demo mode (fake data, generated cover art) used for UI dev/screens:
ECHOPORT_DEMO=1 cargo run
# Headless screenshot (writes PNG then exits); combine with DEMO:
ECHOPORT_SHOT=$PWD/screenshots/home.png ECHOPORT_PAGE=home ECHOPORT_DEMO=1 cargo run
# ECHOPORT_PAGE: home|search|library|playlists|downloads|now_playing|profile|settings
# ECHOPORT_THEME: pulse|tide|ink (legacy blue_white|midnight|album are aliases)
# ECHOPORT_FOLLOW_COVER=1: enable cover-art accent tinting in demo/screenshots
# ECHOPORT_LIVE=1: real network/audio path even when ECHOPORT_SHOT is set
```

Toolchain: Rust 1.99, edition 2021. GUI: **eframe/egui 0.32 with the glow
(OpenGL) renderer** — do not enable wgpu. Audio: cpal 0.18 + symphonia 0.6.
HTTP: `ureq` 3 behind `src/net.rs` (blocking; call from worker threads only).

The app runs on this machine's Wayland/X11 session (software EGL fallback is
fine). Screenshot function is built in (`ECHOPORT_SHOT`), so you can verify
visuals without external tools.

## 3. File ownership (one writer per file)

| Workstream | Owns (write) | Must not touch |
|---|---|---|
| **Lead** | `Cargo.toml`, `src/main.rs`, `src/lib.rs`, `src/app.rs`, `src/model.rs`, `src/state.rs`, `src/store.rs`, `src/net.rs`, `src/theme.rs`, `src/fonts.rs`, `docs/CONTRACT.md` | — |
| **audio-core** | `src/audio/**`, `src/resolver/**`, `src/download.rs`, `src/library.rs` | UI, providers, Lead files |
| **providers** | `src/providers/**`, `src/art.rs`, `src/background.rs` | UI, audio, Lead files |
| **gui** | `src/ui/**`, `src/i18n.rs` (may append keys) | audio, providers, Lead files |
| **qa-docs** | `README.md`, `docs/**` (except CONTRACT.md), `tests/**`, `scripts/**`, `packaging/**`, `screenshots/**` | `src/**`, `Cargo.toml` |

Rules:

* Never edit another workstream's files. If their stub blocks you, message them
  and the Lead.
* Keep `cargo check` errors limited to your own files. Transient errors from a
  teammate mid-edit are expected; re-run later, don't "fix" their code.
* Don't run `cargo fmt` across the repo (Lead does it at integration). You may
  run `rustfmt` on your own files.
* Never run `cargo clean`. Avoid release builds unless asked (target dir is
  shared; the Lead/QA will do them).

## 4. Architecture & ownership boundaries

```
UI (src/ui)  --Command-->  EchoPortApp (src/app.rs)  --AppEvent-->  state
                                |         |
                                |         +--> ArtCache / providers::background (workers)
                                +--> resolver / download (workers) -> yt-dlp/ffmpeg
                                +--> Player (audio engine, UI thread handle)
```

* The UI thread never blocks. It calls `drain`ed channels once per frame and
  `dispatch(Command)`.
* Workers are plain `std::thread`s that send `AppEvent` over one crossbeam
  channel. No async runtime, no tokio.
* `AppState` (src/state.rs) holds all UI-visible state. Ephemeral widget state
  (scroll, textures, dialog draft) belongs in `ui::UiState`.
* Persistence: `ProfileStore` in src/store.rs, JSON at
  `<data>/echoport/profiles/<name>.json`, atomic write. Adding fields is fine
  (`#[serde(default)]`); never break old profiles silently.

### Fixed public APIs (do not rename/signature-change)

`src/audio/mod.rs`:

```rust
pub struct Player;
impl Player {
    pub fn new() -> anyhow::Result<Self>;
    pub fn load(&self, path: &Path) -> anyhow::Result<()>; // replaces current
    pub fn play(&self);
    pub fn pause(&self);
    pub fn toggle(&self);
    pub fn seek(&self, seconds: f64);
    pub fn set_volume(&self, volume: f32);   // 0..=1
    pub fn stop(&self);
    pub fn status(&self) -> PlaybackStatus;  // Clone, includes ended flag
    pub fn clear_ended(&self);
    pub fn output_devices() -> Vec<String>;
}
```

`src/resolver/mod.rs`:

```rust
pub struct ResolveConfig { pub ytdlp: Option<PathBuf>, pub ffmpeg: Option<PathBuf>, pub cache_dir: PathBuf }
pub struct ResolvedAudio { pub path: PathBuf, pub temporary: bool }
pub enum ResolveError { YtDlpMissing, Unsupported(String), Network(String), Media(String), Other(String) }
pub struct SearchLink { pub title: String, pub url: String, pub channel: Option<String>, pub duration_secs: Option<f64>, pub thumbnail: Option<String> }
pub fn detect_ytdlp() -> Option<PathBuf>;
pub fn detect_ffmpeg() -> Option<PathBuf>;
pub fn tool_version(path: &Path) -> Option<String>;
pub fn resolve(config: &ResolveConfig, track: &Track) -> Result<ResolvedAudio, ResolveError>;
pub fn ytdlp_search(config: &ResolveConfig, platform: Platform, query: &str, limit: usize) -> Result<Vec<Track>, ResolveError>;
pub fn search_links(config: &ResolveConfig, query: &str, limit: usize) -> Result<Vec<SearchLink>, ResolveError>;
```

`src/download.rs`:

```rust
pub struct DownloadOptions { pub dir: PathBuf, pub format: DownloadFormat, pub embed_metadata: bool }
pub fn download_track(track: &Track, config: &ResolveConfig, options: &DownloadOptions,
    progress: &mut dyn FnMut(DownloadProgress)) -> Result<PathBuf, String>;
```

`src/library.rs`:

```rust
pub fn scan_dirs(dirs: &[PathBuf]) -> Vec<Track>;
pub fn embedded_cover(audio_path: &Path) -> Option<Vec<u8>>;
pub fn is_audio_file(path: &Path) -> bool;
```

`src/providers/mod.rs`:

```rust
pub fn search(http: &Http, config: &ResolveConfig, query: &str,
    platforms: &[Platform], limit: usize) -> Vec<PlatformResults>;
pub fn track_from_url(http: &Http, config: &ResolveConfig, url: &str) -> Result<Track, String>;
pub fn background(http: &Http, config: &ResolveConfig, track: &Track) -> SongBackground;
```

`src/art.rs`:

```rust
pub struct ArtCache { pub dir: PathBuf }
impl ArtCache {
    pub fn new(dir: PathBuf) -> Self;
    pub fn resolve(&self, http: &Http, track: &Track) -> Option<PathBuf>; // cache file
    pub fn cached(&self, track: &Track) -> Option<PathBuf>;
    pub fn embedded_path(&self, source: &Path) -> PathBuf;
    pub fn ensure(&self) -> std::io::Result<()>;
    pub fn clear(&self) -> std::io::Result<()>;
}
```

`src/background.rs`: `pub fn fetch(...) -> SongBackground` (see providers API above;
app prefers `providers::background`; keep both in sync or delegate).

`src/ui/mod.rs` (only entry point):

```rust
pub struct UiState;
impl UiState { pub fn new(cc: &eframe::CreationContext<'_>, state: &AppState) -> Self; }
pub fn layout(ctx: &egui::Context, app: &mut EchoPortApp);
```

`EchoPortApp` public surface for the UI: `pub state: AppState`,
`pub ui: UiState`, `pub dispatch(&mut self, Command)`, `pub set_page(Page)`,
`pub cache_size() -> u64`, `pub version() -> &'static str`, `pub save_now()`,
`pub mark_dirty()`, `pub paths: AppPaths`, `pub store`, `pub art`, `pub http`.
UI must not call `resolver`/`providers`/`player` directly, and must not block.

## 5. Research findings that constrain implementations

Verified on 2026-10-29 from this host (full report:
`$HOME/PROVIDERS_RESEARCH.md`):

* **Spotify**: anonymous search is impossible (`get_access_token` → 403,
  `api-partner` → 401, API → 401, web search page is client-rendered). Anonymous
  **per-track** metadata works via `https://open.spotify.com/embed/track/<id>`
  (`__NEXT_DATA__` JSON: title, artists, duration ms, cover, 30 s preview mp3).
  So: Spotify is **link import + preview + YouTube match**, not a searchable
  catalogue.
* **Apple Music**: iTunes Search API works for search/metadata/30 s previews.
  `music.apple.com` is unsupported by yt-dlp (FairPlay). Same treatment as
  Spotify: searchable via iTunes, playback via YouTube match or preview.
* **Netease**: public search API works:
  `https://music.163.com/api/search/get?s=<query>&type=1&limit=<n>` with
  `Referer: https://music.163.com/`. yt-dlp can resolve
  `https://music.163.com/song?id=<id>` (legally gray: document, keep working).
* **SoundCloud**: `yt-dlp scsearch<n>:<query>`; some tracks are DRM/HLS-only,
  handle failures gracefully. `yt-dlp <url>` works for full tracks when allowed.
* **Bilibili**: `yt-dlp bilisearch<n>:<query>`; extract audio only
  (`-x`/`bestaudio`). Geo blocks are possible; fail gracefully.
* **YouTube**: `yt-dlp ytsearch<n>:<query>` — 1–3 s per search, ~7–10 s per
  audio download. This is the universal audio backend.
* **CRITICAL**: raw URLs from `yt-dlp -g` return HTTP 403 to curl/ureq/rodio
  (IP-signed CDN). **Never pass a resolved media URL to `Http`.** Let yt-dlp
  itself download the file into the cache (or pipe its stdout). Resolution must
  cache **by track/video id**, never by media URL.
* **Deezer** (no auth, `api.deezer.com/search`) and **iTunes** are available
  fallback catalogues if a provider API fails; use them for resilience.
* **Background info (all keyless)**: Wikipedia REST
  `https://en.wikipedia.org/api/rest_v1/page/summary/<artist>`; MusicBrainz
  `https://musicbrainz.org/ws/2/...&fmt=json`; Genius search
  `https://genius.com/api/search/multi?q=...`; YouTube interview/explanations via
  `yt-dlp ytsearch3:"<artist> <song> interview"`.
* MusicBrainz asks for a descriptive User-Agent and ≤1 req/s. yt-dlp searches
  are serialised by the resolver; keep concurrency low (≤2 per platform).

## 6. egui 0.32 cheat sheet (this version, not 0.37!)

```rust
// App
impl eframe::App for EchoPortApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) { }
}
// Panels
egui::SidePanel::left("id").exact_width(214.0).frame(frame).show(ctx, |ui| {});
egui::TopBottomPanel::bottom("id").show(ctx, |ui| {});
egui::CentralPanel::default().frame(frame).show(ctx, |ui| {});
// Frames / radius
egui::Frame::new().fill(color).inner_margin(egui::Margin::same(16)).corner_radius(16);
ui.painter().rect_filled(rect, egui::CornerRadius::same(10), color);
// Text & painter
ui.label(egui::RichText::new("x").size(14.0).strong().color(c));
ui.painter().text(pos, egui::Align2::LEFT_CENTER, "x".to_owned(),
    egui::FontId::proportional(13.0), color);
// Animation
let t = ctx.animate_bool_with_time(ui.id().with("k"), flag, 0.18);
let v = ctx.animate_value_with_time(ui.id().with("k"), target, 0.25);
// Repaint only when needed
ctx.request_repaint_after(std::time::Duration::from_millis(100));
// Texture from a file
let img = image::open(path)?.to_rgba8();
let texture = ctx.load_texture(name, egui::ColorImage::from_rgba_unmultiplied([w,h], &img),
    egui::TextureOptions::LINEAR);
ui.image((texture.id(), size));
// Fonts are installed in src/fonts.rs (CJK fallback).
```

Screenshot flag is handled by `app.rs`; UI just renders. Don't add your own
event loop.

## 7. Visual + UX spec (gui workstream)

Themes (user-picked references, see `src/theme.rs`):
**Pulse** (default, porcelain white + vivid blue + fluorescent lime),
**Tide** (light aqua + teal + navy), **Ink** (low-saturation dark slate).
Album-cover colour extraction is a toggle (`Settings.follow_cover_accent`), not
a theme mode: it tints accent/highlight/macaron/glow only. `Theme`
already provides: `bg, panel, surface, surface_hover, border, text, text_weak,
accent, accent_soft, on_accent, highlight, on_highlight, macaron[5], glow[2]`,
plus transitions through `state.theme` (already lerped in `app.rs`; never set
`state.theme` directly). Highlight is the second strong accent (lime for Pulse,
teal for Tide, periwinkle for Ink) for playing indicators/badges. When cover
accent following is enabled, `state.theme_target` changes automatically; show
`state.theme.macaron` tastefully (playing indicator, chips, background dots).

Required UX:

* Sidebar: brand `EchoPort` + a single accent dot (the only "." decoration),
  nav (Home/Search/Library/Playlists/Downloads, Profile/Settings at bottom),
  language quick toggle, version.
* Home: greeting, recently played, favourites, playlists, "listen from any
  source" cards. Cover thumbnails when available (`state.cover.path`) else a
  deterministic macaron placeholder from the track title hash.
* Search: one input; Enter searches all platforms; platform filter chips;
  results grouped per platform with platform colour dot; row actions: play,
  add to queue, add to playlist, download, favourite, open source page.
  URL pasted into the input → Import link flow. Show per-platform error text
  (e.g. Spotify hint) without breaking the page.
* Now Playing: large cover (cached texture), blurred/soft glow from palette,
  title/artist/album, scrubber, controls, favourite, download, open source,
  tabs **[Track | Background]**; Background shows artist bio, interview /
  explanation video links and further reading (all clickable, `Command::OpenUrl`).
* Player bar (always visible): cover thumb, title/artist, prev/play/next,
  shuffle/repeat, position/duration, volume, queue toggle, favourite,
  "background" shortcut. Loading state = subtle spinner/pulse, never frozen.
* Playlists: list + detail; mixed platforms; create/rename/delete; add/remove
  tracks; play all/shuffle all; empty states.
* Library: scan button, folder management, local tracks with embedded covers,
  empty state with "add music folder".
* Downloads: active with progress bars + completed with "open folder"; format
  and folder setting; per-job failure message.
* Profile: local profile switcher, privacy note, stats (plays/favourites/joined),
  history.
* Settings: language, theme mode, tooling status (yt-dlp/ffmpeg + detect button),
  results per platform, download format/folder, cache size + clear, about.
* Toasts for actions; keyboard: Space play/pause, ←/→ seek, Esc closes overlays.
* Animations: subtle (≤200 ms), e.g. playing-bar equalizer, hover lifts, page
  fades, theme colour lerp, cover cross-fade; must not repaint when idle.
* Accessibility: contrast ≥4.0 for body text; don't encode state by colour only.
* Performance: cache textures in `UiState` keyed by path; downscale covers to
  display size; no file IO or network inside `layout` (app already pre-fetches
  covers in workers; for local playlist covers use `art.cached()`).

## 8. Provider behaviour spec (providers workstream)

* `search()` runs all requested platforms, one result group each; per-platform
  errors must not abort the others. Cap `limit` to `settings.results_per_platform`.
* Platform implementations:
  * `youtube`: `resolver::ytdlp_search(YouTube, ...)`; set page_url.
  * `soundcloud`: `resolver::ytdlp_search(SoundCloud, ...)`.
  * `bilibili`: `resolver::ytdlp_search(Bilibili, ...)`; audio-only.
  * `netease`: public search API; `Track::new(Netease, id, name, artists)` with
    `page_url = https://music.163.com/song?id=<id>`, cover `album.picUrl`
    (https), duration ms → secs. Watch for empty artist arrays.
  * `apple`: iTunes Search API `entity=song`, `country` from locale (default US
    or CN?). Use `artworkUrl100` upgraded to `600x600bb`; `previewUrl` →
    `stream_url`; `trackViewUrl` → page_url; `trackTimeMillis`.
  * `spotify`: no search; return `PlatformResults { tracks: [], error: Some(hint) }`
    (hint text from `i18n::Key::SpotifySearchHint`? providers must not depend on
    i18n for now — return a short English hint; the UI can display it).
  * Also support **optional Deezer-backed "Web" search**? No — keep platforms
    as declared. Spotify button in the search page is still useful for link
    import.
* `track_from_url()`: detect by host:
  * `open.spotify.com/track/<id>` → fetch `https://open.spotify.com/embed/track/<id>`,
    parse `__NEXT_DATA__` JSON (title, artists, duration ms, coverArt, 30 s
    preview from `audioPreview.url`, releaseDate). `open.spotify.com/album|playlist/<id>`
    → best effort (parse embed JSON; if no track list, error with a clear message).
  * `music.apple.com/<cc>/.../<id>` → iTunes lookup API (`https://itunes.apple.com/lookup?id=<id>`
    works for track ids; album links: `/lookup?id=<collectionId>&entity=song`).
  * YouTube / SoundCloud / Bilibili / Netease URLs → yt-dlp metadata
    (`-J --no-playlist`), build Track.
  * Reject anything else with a helpful error.
* `background()`: Wikipedia bio (artist name, fallback `"<artist> (musician)"`),
  Genius search link, YouTube interview/search links via
  `resolver::search_links`, plus source page link. Never block >8 s total; use
  sequential calls; cache nothing (app calls once per track).
* `art.rs`: download to `dir/<sha256 of art_url>.<ext by image sniff>` with
  `.part` temp file + rename; skip tiny/broken images; respect `image` crate
  formats (jpeg/png/webp). Return cached path if present. Also support embedded
  artwork path helper.
* Rate limits: keep ≤2 concurrent yt-dlp processes total (coordinate by a
  process-wide `Mutex` in resolver if needed); add tiny sleeps between
  MusicBrainz/Genius requests.

## 9. Audio engine spec (audio-core workstream)

* cpal output stream at the file's sample rate when supported, else resample.
* Symphonia 0.6: enable `mp3, aac, isomp4, flac, alac, wav, ogg, vorbis` (already
  in Cargo.toml; add features there via Lead if needed). Unsupported codecs
  (opus/webm) are converted by the resolver with ffmpeg to m4a/mp3 before
  playback, so the decoder never sees them.
* Lock-free ring buffer (e.g. `ringbuf` crate — ask Lead to add) with silence
  fallback if the producer stalls; no allocations/locks in the audio callback.
* Seek: flush buffer, re-seek decoder, resume; keep volume applied in callback.
* Status polling: update `position_secs` every ~100 ms; set `ended = true` once;
  duration from the decoder; `error` on decode/device errors.
* Resolver: cache audio under `cache_dir/<platform>-<track.cache_key>.<ext>`;
  if file exists and size > 32 KiB return it. `yt-dlp` args: `--no-playlist
  --no-warnings --newline -f bestaudio[ext=m4a]/bestaudio[ext=mp3]/bestaudio`;
  if resulting container is webm/opus, transcode with ffmpeg to m4a. Add
  `--force-overwrites` false; use temp dir + rename. Timeout ~120 s.
  For Spotify/Apple tracks: use `ytsearch1:"<artist> <title>"` and validate
  duration within ±8 s + fuzzy title match; on mismatch still allow (warn).
  For preview `stream_url` (Apple 30 s) download via ureq `Http` (allowed: these
  are public CDNs, not IP-signed media URLs).
  Netease: try `https://music.163.com/song?id=<id>` with yt-dlp first, fall back
  to ytsearch.
  SoundCloud: track page_url directly; fall back to scsearch.
  Bilibili: page_url with `-x`/bestaudio; fall back to bilisearch.
  YouTube: page_url or ytsearch.
  Cache key must be stable; never hand URLs to Http.
* ytdlp_search: `-J --flat-playlist --no-warnings "ytsearch<N>:<q>"` etc. (yt-dlp
  search prefixes: `ytsearch`, `scsearch`, `bilisearch`). Build `Track`s with
  page_url, duration, thumbnail, channel as artist. Return `ResolveError` on
  tool missing/network.
* Downloads: `yt-dlp -x --audio-format <fmt> --audio-quality 0 --embed-metadata
  --embed-thumbnail -o "<dir>/%(title)s - %(artist)s.%(ext)s"` (sanitize names,
  avoid overwriting: `%(title)s [%(id)s]`), parse `[download] 42.3%` progress into
  `DownloadProgress`, print final path. Transcode to chosen format only when
  needed. Handle non-ASCII names.
* Local library: `scan_dirs` recursive walk (skip hidden dirs, symlink loops),
  read tags with `lofty` (title, artist, album, duration), fall back to file
  name; dedupe by canonical path; return sorted. `embedded_cover` reads
  front-cover picture bytes. `Track::from_local_path` exists.
* Test on the local file `$HOME/waht1010 (1).mp3` with
  `cargo test` unit tests (decoder frame count, cache-key stability) and a
  `cargo run` smoke test. No audio hardware required for decoder tests.

## 10. QA / delivery spec (qa-docs workstream)

* `README.md` (bilingual 中文 first, then English) — features, screenshots,
  install/build/run on Linux/Windows/macOS, dependencies (Rust, yt-dlp, ffmpeg),
  data locations, privacy statement, legal note (only download content you have
  rights to; no DRM bypass; sources may be region-blocked), roadmap.
* `docs/ARCHITECTURE.md` (module map, threading, data flow), `docs/USAGE.md`
  (short illustrated guide + keyboard shortcuts), `docs/PROVIDERS.md` (what is
  supported, limitations, zero-auth rationale — reuse PROVIDERS_RESEARCH.md).
* `scripts/` : `build.sh` (release), `package-linux.sh` (binary + `.desktop` +
  icon + README → `dist/echoport-<ver>-linux-x86_64.tar.gz`),
  `build-windows.ps1`, `run-demo.sh` (screenshots). Keep scripts
  dependency-light and executable.
* `packaging/echoport.desktop` + generated `assets/icon.png` (pure Python PNG
  writer or ffmpeg is fine; no external image editors).
* `tests/` integration tests using the library crate: persistence roundtrip,
  i18n coverage (every Key has non-empty en+zh — expose a helper if needed),
  theme contrast, playlist mixed-platform operations, cache key stability.
* Final QA when T1–T3 report done: `cargo test`, `cargo build --release`,
  demo screenshots of all 8 pages in `blue_white` and `now_playing` in `album`
  and `midnight` themes, check idle CPU (<1.5%) via `ps -o %cpu` while the app
  idles for ~8 s, binary size report, no UI freeze while a search is running.
  Report numbers + screenshots in `docs/QA_REPORT.md`.
* Never edit `src/**`; report bugs to the owning workstream and the Lead.

## 11. Definition of done

* `cargo check` and `cargo test` green; no new warnings in your files.
* Every user-visible string in the UI comes from i18n (both languages).
* No blocking IO on the UI thread; no unbounded repaint loops when idle.
* Demo screenshots look polished in both themes and at 1200×800 and 980×620.
* Error paths are handled with toasts/hints, not panics. `unwrap()` only where
  infallible; no `panic!` machinery.
* Release build under ~25 MB, idle memory <200 MB, idle CPU <1.5%.
* Honest documentation of DRM / region / legality constraints.


## 12. Scripting & plugins (added after user request)

Public paths: `<config_dir>/echoport/config.lua` and
`<config_dir>/echoport/plugins/*.lua` (`AppPaths::config_file`, `plugins_dir`).

* `src/scripting/**` is owned by the scripting workstream. The UI/app must use
  only its public API:
  `Scripts::load(ScriptPaths, EventSender)`, `Scripts::config()`,
  `Scripts::reload()`, `Scripts::fire(&ScriptEvent, &EventSender)`,
  `Scripts::run_action(id, &EventSender)`, `Scripts::poll_binding(ctx)`,
  `Scripts::set_now_playing(Option<Track>)`, `ScriptPaths::resolve()`.
* `Scripts` is `!Send` (mlua without the `send` feature): keep it on the UI
  thread in `EchoPortApp`; call `fire`/`run_action` from `App::tick` or command
  handling, never from worker threads.
* App events added for scripting: `AppEvent::RunCommand(Command)`,
  `AppEvent::ScriptToast(String)`, `AppEvent::ScriptBinding(BindingAction)`.
  `EchoPortApp::run_binding_action(BindingAction)` is the single handler for
  keyboard shortcuts, Lua `echoport.command(...)` and custom actions.
* `ScriptConfig` overrides are applied at startup and on reload: language,
  theme mode, follow-cover-accent, accent/highlight/macaron, startup page,
  volume, seek/volume step, results per platform, download format/dir, music
  dirs, keybindings, custom actions. `None` keeps the persisted profile value.
* Lua API (documented in docs/SCRIPTING.md): `echoport.config`,
  `echoport.on(event, fn)` (startup/track_start/track_end/search_done/
  download_done/library_scanned/playback_paused/playback_resumed),
  `echoport.actions.add{...}`, `echoport.bind(chord, fn_or_action)`,
  `echoport.notify`, `echoport.command`, `echoport.search`, `echoport.open_url`,
  `echoport.set_volume`, `echoport.play_url`, `echoport.current_track`,
  `echoport.log`, plus `echoport.version` / `echoport.paths`.
* Rust extension point (documented in docs/PLUGINS.md): `scripting::Plugin`
  trait + `PluginHost` (dispatch commands / notify) + `PluginRegistry`
  (fire events). Compiled-in plugins can be registered in `EchoPortApp::new`;
  the registry is empty by default.
* Safety: user scripts are trusted local code executed with full Lua stdlib;
  every callback is pcall-protected and errors become `ScriptConfig::warnings`
  shown in Settings (never panics). Scripts can request commands but cannot
  read credentials (there are none) or access the network beyond the exposed
  helpers.
* Settings UI shows a "Scripting & plugins" card: config path, plugin list,
  custom actions (run buttons), warnings, reload button, open folder.
