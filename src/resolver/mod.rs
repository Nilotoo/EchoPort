//! Resolves a `Track` into a locally playable audio file, and performs the
//! yt-dlp powered platform searches (owned by the audio-core workstream).
//!
//! Contract: all functions are blocking; call them from worker threads. The
//! returned `ResolvedAudio` must stay on disk until the track is unloaded.
//!
//! Design notes
//! ------------
//! * Playback of web audio always goes through yt-dlp. Raw media URLs returned by
//!   `yt-dlp -g` are IP-signed and return HTTP 403 to ureq/curl on this host, so
//!   they must never be handed to `crate::net::Http`. Only *public preview* URLs
//!   (Apple Music / Spotify 30 s clips) are fetched over HTTP, and only when the
//!   host is on a known preview CDN allow list.
//! * Apple Music / Spotify tracks match full-length audio through yt-dlp first;
//!   the public 30 s preview is only a fallback when yt-dlp is missing/fails or
//!   the match is clearly the wrong song. Preview cache files carry a `.preview`
//!   marker (and legacy ones are detected by duration) so they can never shadow
//!   full-length playback.
//! * Cached audio lives in `config.cache_dir/<platform>-<cache_key>.<ext>` and is
//!   reused when the file is larger than 32 KiB. Downloads happen in a temporary
//!   subdirectory and are renamed into place atomically.
//! * At most two yt-dlp processes run at once, process wide (see `YtdlpSlot`).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime};

use serde::Deserialize;

use crate::model::{Platform, Track};
use crate::process;

/// Minimum size for a cache file to be considered complete.
pub(crate) const MIN_CACHE_BYTES: u64 = 32 * 1024;
/// yt-dlp process cap across every workstream.
const MAX_YTDLP_PROCESSES: usize = 3;
/// Resolution timeout (see CONTRACT §9).
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(120);
/// Search timeout: yt-dlp searches take 1–3 s, this is only a safety net.
const SEARCH_TIMEOUT: Duration = Duration::from_secs(60);
/// ffmpeg transcode timeout.
const TRANSCODE_TIMEOUT: Duration = Duration::from_secs(180);
/// Duration tolerance when matching a catalogue track to downloaded audio
/// (CONTRACT §9: ±8 s).
const DURATION_TOLERANCE_SECS: f64 = 8.0;
/// A cached clip at or below this length is treated as a 30 s preview when the
/// catalogue says the track is longer.
const PREVIEW_MAX_SECS: f64 = 45.0;
/// Explicit marker inserted before the extension of cached preview clips, e.g.
/// `spotify-<key>.preview.mp3`. Previews can therefore never shadow full audio.
const PREVIEW_MARKER: &str = "preview";
/// Files modified more recently than this are protected from cache pruning so a
/// currently playing or prefetched track cannot disappear mid-playback.
const PRUNE_PROTECT: Duration = Duration::from_secs(120);

#[derive(Clone, Debug)]
pub struct ResolveConfig {
    /// Explicit yt-dlp binary, `None` = auto detect.
    pub ytdlp: Option<PathBuf>,
    /// ffmpeg binary for transcoding unsupported codecs.
    pub ffmpeg: Option<PathBuf>,
    /// Where cached decoded audio files live.
    pub cache_dir: PathBuf,
}

impl ResolveConfig {
    pub fn ytdlp(&self) -> Option<PathBuf> {
        self.ytdlp.clone().or_else(detect_ytdlp)
    }

    pub fn ffmpeg(&self) -> Option<PathBuf> {
        self.ffmpeg.clone().or_else(detect_ffmpeg)
    }
}

#[derive(Clone, Debug)]
pub struct ResolvedAudio {
    pub path: PathBuf,
    /// Temporary cache file that may be evicted later.
    pub temporary: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("yt-dlp is not available")]
    YtDlpMissing,
    #[error("unsupported track: {0}")]
    Unsupported(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("media error: {0}")]
    Media(String),
    #[error("{0}")]
    Other(String),
}

/// One entry from a yt-dlp search (YouTube / SoundCloud / Bilibili).
#[derive(Clone, Debug, Default, Deserialize)]
pub struct SearchLink {
    pub title: String,
    pub url: String,
    pub channel: Option<String>,
    pub duration_secs: Option<f64>,
    pub thumbnail: Option<String>,
}

// ---------------------------------------------------------------------------
// Tool discovery
// ---------------------------------------------------------------------------

pub fn detect_ytdlp() -> Option<PathBuf> {
    detect_tool("yt-dlp")
}

pub fn detect_ffmpeg() -> Option<PathBuf> {
    detect_tool("ffmpeg")
}

fn detect_tool(name: &str) -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var(format!(
        "ECHOPORT_{}",
        name.to_uppercase().replace('-', "_")
    )) {
        let path = PathBuf::from(explicit);
        if path.is_file() {
            return Some(path);
        }
    }
    // Tools bundled with the Windows installer/zip live in `tools/` next to
    // the executable (also useful for portable Linux bundles).
    if let Some(path) = bundled_tool(name) {
        return Some(path);
    }
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
            #[cfg(windows)]
            {
                let exe = dir.join(format!("{name}.exe"));
                if exe.is_file() {
                    return Some(exe);
                }
            }
        }
    }
    #[cfg(windows)]
    if let Some(path) = windows_tool_candidates(name) {
        return Some(path);
    }
    // Common install locations that may not be on PATH (pipx, macOS bundles).
    #[cfg(unix)]
    {
        let home = dirs::home_dir()?;
        let candidates = [
            home.join(".local/bin").join(name),
            PathBuf::from("/usr/local/bin").join(name),
            PathBuf::from("/opt/homebrew/bin").join(name),
            PathBuf::from("/snap/bin").join(name),
            PathBuf::from("/usr/bin").join(name),
        ];
        if let Some(path) = candidates.into_iter().find(|path| path.is_file()) {
            return Some(path);
        }
    }
    None
}

/// Common Windows install locations that are not always on PATH (winget,
/// scoop, pip user installs, portable tools next to EchoPort).
#[cfg(windows)]
fn windows_tool_candidates(name: &str) -> Option<PathBuf> {
    let exe = format!("{name}.exe");
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(dir) = std::env::current_exe() {
        if let Some(parent) = dir.parent() {
            candidates.push(parent.join(&exe));
        }
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let local = PathBuf::from(local);
        candidates.push(local.join("Programs").join(name).join(&exe));
        candidates.push(
            local
                .join("Microsoft")
                .join("WinGet")
                .join("Links")
                .join(&exe),
        );
        candidates.push(local.join("Programs").join("ffmpeg").join("bin").join(&exe));
        let python_root = local.join("Programs").join("Python");
        if let Ok(entries) = std::fs::read_dir(&python_root) {
            for entry in entries.flatten() {
                candidates.push(entry.path().join("Scripts").join(&exe));
            }
        }
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        candidates.push(
            PathBuf::from(appdata)
                .join("Python")
                .join("Scripts")
                .join(&exe),
        );
    }
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join("scoop").join("shims").join(&exe));
    }
    for var in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(root) = std::env::var_os(var) {
            let root = PathBuf::from(root);
            candidates.push(root.join("ffmpeg").join("bin").join(&exe));
            candidates.push(root.join(name).join(&exe));
        }
    }
    candidates.into_iter().find(|path| path.is_file())
}

/// Look for a bundled `tools/<name>` next to the running executable.
fn bundled_tool(name: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    #[allow(unused_mut)]
    let mut names = vec![dir.join(name), dir.join("tools").join(name)];
    #[cfg(windows)]
    {
        names.push(dir.join(format!("{name}.exe")));
        names.push(dir.join("tools").join(format!("{name}.exe")));
    }
    names.into_iter().find(|path| path.is_file())
}

pub fn tool_version(path: &Path) -> Option<String> {
    // yt-dlp uses `--version`; ffmpeg only accepts the single dash form.
    for flag in ["--version", "-version"] {
        let mut command = process::quiet(path);
        command.arg(flag);
        let Ok(output) = run_process(command, Duration::from_secs(10)) else {
            continue;
        };
        if !output.success() {
            continue;
        }
        let text = if output.stdout.trim().is_empty() {
            output.stderr
        } else {
            output.stdout
        };
        if let Some(line) = text.lines().map(str::trim).find(|line| !line.is_empty()) {
            return Some(line.to_owned());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Process wide yt-dlp concurrency limit
// ---------------------------------------------------------------------------

static YTDLP_ACTIVE: Mutex<usize> = Mutex::new(0);
static YTDLP_ACTIVE_CV: Condvar = Condvar::new();

/// RAII slot for the process wide "at most `MAX_YTDLP_PROCESSES` yt-dlp
/// processes" rule. Shared with `crate::download`.
pub(crate) struct YtdlpSlot;

impl YtdlpSlot {
    pub(crate) fn acquire() -> Self {
        let mut active = YTDLP_ACTIVE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while *active >= MAX_YTDLP_PROCESSES {
            active = YTDLP_ACTIVE_CV
                .wait(active)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        *active += 1;
        Self
    }
}

impl Drop for YtdlpSlot {
    fn drop(&mut self) {
        let mut active = YTDLP_ACTIVE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *active = active.saturating_sub(1);
        YTDLP_ACTIVE_CV.notify_one();
    }
}

// ---------------------------------------------------------------------------
// Captured process output
// ---------------------------------------------------------------------------

pub(crate) struct ProcessOutput {
    pub(crate) status: Option<std::process::ExitStatus>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) timed_out: bool,
}

impl ProcessOutput {
    pub(crate) fn success(&self) -> bool {
        !self.timed_out && self.status.map(|status| status.success()).unwrap_or(false)
    }

    /// Human readable one-line tail of stderr (for error messages/UI toasts).
    pub(crate) fn error_tail(&self) -> String {
        if let Some(line) = self
            .stderr
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .last()
        {
            return truncate(line, 280);
        }
        if self.timed_out {
            return "process timed out".to_owned();
        }
        "process failed".to_owned()
    }
}

pub(crate) fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Run `command` capturing stdout/stderr with a hard timeout.
///
/// stdout/stderr are drained on helper threads so a chatty child cannot deadlock
/// on a full pipe. On timeout the child is killed and `timed_out` is set.
pub(crate) fn run_process(
    mut command: Command,
    timeout: Duration,
) -> Result<ProcessOutput, ResolveError> {
    let program = command.get_program().to_string_lossy().into_owned();
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|err| ResolveError::Other(format!("cannot run {program}: {err}")))?;

    // Drain the pipes on helper threads. They report back over a channel instead
    // of being joined, so a grandchild that inherited a pipe (yt-dlp spawning
    // ffmpeg) can never hang the caller if the direct child had to be killed.
    let (output_tx, output_rx) = std::sync::mpsc::channel::<(bool, String)>();
    if let Some(pipe) = child.stdout.take() {
        let sender = output_tx.clone();
        std::thread::spawn(move || {
            let _ = sender.send((true, read_stream(Some(pipe))));
        });
    }
    if let Some(pipe) = child.stderr.take() {
        let sender = output_tx.clone();
        std::thread::spawn(move || {
            let _ = sender.send((false, read_stream(Some(pipe))));
        });
    }
    drop(output_tx);

    let deadline = Instant::now() + timeout;
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {}
            Err(err) => {
                let _ = child.kill();
                return Err(ResolveError::Other(format!(
                    "cannot wait for {program}: {err}"
                )));
            }
        }
        if Instant::now() >= deadline {
            timed_out = true;
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };

    // Collect the two streams, waiting at most a couple of seconds more.
    let collect_deadline = Instant::now() + Duration::from_secs(2);
    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut pending = 2;
    while pending > 0 {
        let remaining = collect_deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match output_rx.recv_timeout(remaining) {
            Ok((true, text)) => {
                stdout = text;
                pending -= 1;
            }
            Ok((false, text)) => {
                stderr = text;
                pending -= 1;
            }
            Err(_) => break,
        }
    }

    Ok(ProcessOutput {
        status,
        stdout,
        stderr,
        timed_out,
    })
}

fn read_stream(pipe: Option<impl std::io::Read>) -> String {
    let Some(mut pipe) = pipe else {
        return String::new();
    };
    let mut bytes = Vec::new();
    let _ = pipe.read_to_end(&mut bytes);
    String::from_utf8_lossy(&bytes).into_owned()
}

// ---------------------------------------------------------------------------
// Cache helpers
// ---------------------------------------------------------------------------

fn cache_stem(track: &Track) -> String {
    format!("{}-{}", track.platform.key(), track.cache_key())
}

/// Final cache path for full-length `track` audio with the given extension.
pub(crate) fn cache_path(config: &ResolveConfig, track: &Track, extension: &str) -> PathBuf {
    config
        .cache_dir
        .join(format!("{}.{}", cache_stem(track), extension))
}

/// Cache path for a public 30 s preview clip. The explicit `.preview` marker
/// (plus the legacy duration heuristic below) guarantees a preview file can
/// never satisfy full-length playback.
pub(crate) fn preview_cache_path(config: &ResolveConfig, track: &Track) -> PathBuf {
    config.cache_dir.join(format!(
        "{}.{}.{}",
        cache_stem(track),
        PREVIEW_MARKER,
        preview_extension(track)
    ))
}

/// Platforms whose catalogue only offers metadata + public 30 s previews.
fn is_preview_capable(platform: Platform) -> bool {
    matches!(platform, Platform::AppleMusic | Platform::Spotify)
}

fn is_preview_marker(path: &Path) -> bool {
    path.file_name()
        .map(|name| {
            name.to_string_lossy()
                .contains(&format!(".{PREVIEW_MARKER}."))
        })
        .unwrap_or(false)
}

fn file_duration_secs(path: &Path) -> Option<f64> {
    crate::audio::probe_audio(path)
        .ok()
        .and_then(|info| info.duration_secs)
}

/// Pure cache heuristic: does this duration look like a 30 s preview of a
/// longer catalogue track? Legacy previews cached by older builds have no
/// marker, so their ~30 s length is the only signal.
fn preview_duration_mismatch(actual: Option<f64>, expected: Option<f64>) -> bool {
    let Some(actual) = actual else {
        return false;
    };
    if !(10.0..=PREVIEW_MAX_SECS).contains(&actual) {
        return false;
    }
    // An unknown catalogue duration for a streaming track is presumed longer
    // than a preview (providers always send durations; this is a safety net).
    expected.map(|expected| expected > 60.0).unwrap_or(true)
}

/// Is this cached file a 30 s preview rather than full-length audio?
fn looks_like_preview(path: &Path, track: &Track) -> bool {
    if is_preview_marker(path) {
        return true;
    }
    if !is_preview_capable(track.platform) {
        return false;
    }
    preview_duration_mismatch(file_duration_secs(path), track.duration_secs)
}

/// Complete (>= 32 KiB) cache files belonging to `track`.
fn cache_candidates(config: &ResolveConfig, track: &Track) -> Vec<PathBuf> {
    let prefix = format!("{}.", cache_stem(track));
    let Ok(entries) = std::fs::read_dir(&config.cache_dir) else {
        return Vec::new();
    };
    let mut candidates = Vec::new();
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if !file_name.starts_with(&prefix) {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.is_file() && metadata.len() >= MIN_CACHE_BYTES {
            candidates.push(entry.path());
        }
    }
    candidates.sort();
    candidates
}

/// Find an existing full-length cache file for `track`.
///
/// Preview clips are skipped (explicitly marked, or detected through the legacy
/// duration heuristic) so a cached 30 s preview can never shadow full playback.
pub(crate) fn find_cached(config: &ResolveConfig, track: &Track) -> Option<PathBuf> {
    if track.local_path.is_some() {
        return None;
    }
    cache_candidates(config, track)
        .into_iter()
        .find(|path| !looks_like_preview(path, track))
}

/// Find a cached preview clip for `track`, if one exists.
pub(crate) fn find_cached_preview(config: &ResolveConfig, track: &Track) -> Option<PathBuf> {
    if track.local_path.is_some() {
        return None;
    }
    let candidates = cache_candidates(config, track);
    // Explicitly marked previews win over the legacy duration heuristic.
    if let Some(marked) = candidates.iter().find(|path| is_preview_marker(path)) {
        return Some(marked.clone());
    }
    candidates
        .into_iter()
        .find(|path| looks_like_preview(path, track))
}

// ---------------------------------------------------------------------------
// Cache statistics / pruning
// ---------------------------------------------------------------------------

/// Size and file count of the audio cache directory (recursive, regular files).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheStats {
    pub bytes: u64,
    pub files: usize,
}

/// Recursively collect `(path, size, modified)` for regular files under `dir`.
fn cache_files(dir: &Path) -> Vec<(PathBuf, u64, Option<SystemTime>)> {
    let mut files = Vec::new();
    let walker = walkdir::WalkDir::new(dir)
        .follow_links(false)
        .sort_by_file_name();
    for entry in walker.into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        files.push((entry.into_path(), metadata.len(), metadata.modified().ok()));
    }
    files
}

/// Aggregate size/count of the audio cache directory.
pub fn cache_stats(dir: &Path) -> CacheStats {
    let files = cache_files(dir);
    CacheStats {
        bytes: files.iter().map(|(_, size, _)| *size).sum(),
        files: files.len(),
    }
}

/// Never prune in-flight downloads or files being written right now.
fn is_prunable_cache_file(path: &Path) -> bool {
    let Some(name) = path.file_name() else {
        return false;
    };
    let name = name.to_string_lossy().to_ascii_lowercase();
    !(name.ends_with(".part")
        || name.ends_with(".tmp")
        || name.ends_with(".ytdl")
        || name.contains(".part-"))
}

/// Delete the oldest cache files until `dir` holds at most `max_bytes`, skipping
/// in-flight temp files and anything modified in the last 120 s (a track that is
/// currently playing or being prefetched must not vanish). Returns bytes freed.
pub fn prune_cache(dir: &Path, max_bytes: u64) -> u64 {
    let files = cache_files(dir);
    let mut total: u64 = files.iter().map(|(_, size, _)| *size).sum();
    if total <= max_bytes {
        return 0;
    }
    let cutoff = SystemTime::now()
        .checked_sub(PRUNE_PROTECT)
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let mut candidates: Vec<(SystemTime, u64, PathBuf)> = files
        .into_iter()
        .filter_map(|(path, size, modified)| {
            if !is_prunable_cache_file(&path) {
                return None;
            }
            let modified = modified?;
            // Only files that are both old and untouched by a running job.
            (modified < cutoff).then_some((modified, size, path))
        })
        .collect();
    candidates.sort_by(|left, right| left.0.cmp(&right.0));

    let mut freed = 0u64;
    for (_, size, path) in candidates {
        if total <= max_bytes {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(size);
            freed += size;
        }
    }
    freed
}

// ---------------------------------------------------------------------------
// Public preview URLs (Apple Music / Spotify 30 s clips)
// ---------------------------------------------------------------------------

/// Hosts that serve public preview clips. These are *not* IP-signed yt-dlp
/// media URLs, so fetching them over plain HTTP is allowed.
const PREVIEW_HOSTS: [&str; 6] = [
    "mzstatic.com",
    "itunes.apple.com",
    "scdn.co",
    "spotifycdn.com",
    "deezer.com",
    "dzcdn.net",
];

/// Return the public preview URL for `track` when it is safe to fetch directly.
///
/// This is the guard that enforces the "never fetch a yt-dlp-resolved URL" rule:
/// only Apple Music / Spotify tracks with a stream URL on a known preview CDN
/// qualify. Anything else must go through yt-dlp itself.
pub(crate) fn preview_url(track: &Track) -> Option<&str> {
    if !matches!(track.platform, Platform::AppleMusic | Platform::Spotify) {
        return None;
    }
    let url = track.stream_url.as_deref()?;
    let parsed = url::Url::parse(url).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    let allowed = PREVIEW_HOSTS
        .iter()
        .any(|allowed| host == *allowed || host.ends_with(&format!(".{allowed}")));
    allowed.then_some(url)
}

/// File extension to use for a cached preview URL.
pub(crate) fn preview_extension(track: &Track) -> &'static str {
    let url = track
        .stream_url
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if url.contains(".m4a") {
        "m4a"
    } else if url.contains(".aac") {
        "aac"
    } else if url.contains(".mp3") || url.contains("mp3-preview") {
        "mp3"
    } else if matches!(track.platform, Platform::AppleMusic) {
        "m4a"
    } else {
        "mp3"
    }
}

fn fetch_preview(config: &ResolveConfig, track: &Track, url: &str) -> Result<PathBuf, String> {
    // Reuse an explicitly marked preview, or a legacy ~30 s preview cached by an
    // older build (detected by duration), before hitting the network again.
    if let Some(cached) = find_cached_preview(config, track) {
        return Ok(cached);
    }
    let destination = preview_cache_path(config, track);
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|err| format!("cache dir: {err}"))?;
    }
    let http = crate::net::Http::new();
    http.download(url, &[], &destination, |_, _| {})
        .map_err(|err| format!("preview download: {err}"))?;
    let size = std::fs::metadata(&destination)
        .map(|m| m.len())
        .unwrap_or(0);
    if size < MIN_CACHE_BYTES {
        let _ = std::fs::remove_file(&destination);
        return Err(format!("preview too small ({size} bytes)"));
    }
    Ok(destination)
}

// ---------------------------------------------------------------------------
// Sources for a track
// ---------------------------------------------------------------------------

/// Where yt-dlp should look for the audio of a track.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Source {
    /// A public page/URL yt-dlp can resolve directly.
    Url(String),
    /// A yt-dlp search expression, e.g. `ytsearch1:artist title`.
    Search(String),
}

impl Source {
    pub(crate) fn argument(&self) -> &str {
        match self {
            Self::Url(url) => url,
            Self::Search(query) => query,
        }
    }
}

/// Ordered list of yt-dlp sources to try for `track` (first successful wins).
pub(crate) fn sources_for(track: &Track) -> Vec<Source> {
    let search_query = |prefix: &str| Source::Search(format!("{prefix}:{}", track.search_text()));
    let page_url = track.page_url.as_deref().filter(|url| !url.is_empty());
    match track.platform {
        Platform::Local => Vec::new(),
        Platform::YouTube => match page_url {
            Some(url) => vec![Source::Url(url.to_owned()), search_query("ytsearch1")],
            None => vec![search_query("ytsearch1")],
        },
        // SoundCloud / Bilibili can be DRM/HLS-only or anti-bot-blocked for a
        // given network, so keep a YouTube match as the last resort: the user
        // asked for playback rather than a raw HTTP 412. The hard rule still
        // holds — yt-dlp downloads the audio, Python Http never receives URLs.
        Platform::SoundCloud => match page_url {
            Some(url) => vec![
                Source::Url(url.to_owned()),
                search_query("scsearch1"),
                search_query("ytsearch1"),
            ],
            None => vec![search_query("scsearch1"), search_query("ytsearch1")],
        },
        Platform::Bilibili => match page_url {
            Some(url) => vec![
                Source::Url(url.to_owned()),
                search_query("bilisearch1"),
                search_query("ytsearch1"),
            ],
            None => vec![search_query("bilisearch1"), search_query("ytsearch1")],
        },
        Platform::Netease => match page_url {
            Some(url) => vec![Source::Url(url.to_owned()), search_query("ytsearch1")],
            None => vec![search_query("ytsearch1")],
        },
        // No anonymous streaming for these: always match through YouTube.
        Platform::Spotify | Platform::AppleMusic => vec![search_query("ytsearch1")],
    }
}

// ---------------------------------------------------------------------------
// Playback priority
// ---------------------------------------------------------------------------

/// How playback audio should be sourced for a track.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlaybackPriority {
    /// Full-length audio only (yt-dlp); a 30 s preview is never acceptable.
    FullOnly,
    /// Full-length audio first; the public 30 s preview is only a fallback when
    /// yt-dlp is missing/fails or the match is clearly the wrong song.
    FullThenPreview,
}

/// Apple Music / Spotify only publish metadata + 30 s previews publicly, so their
/// full playback is matched through yt-dlp. The preview must never win over a
/// successful full match (user reported only hearing 30 s).
pub(crate) fn playback_priority(track: &Track) -> PlaybackPriority {
    if is_preview_capable(track.platform) {
        PlaybackPriority::FullThenPreview
    } else {
        PlaybackPriority::FullOnly
    }
}

/// Classification of a yt-dlp audio match against the catalogue metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MatchLength {
    /// Duration unknown, or within the CONTRACT ±8 s tolerance.
    Matched,
    /// Real full-length audio whose duration differs (radio edit, live version,
    /// extended mix). Still playback the user wants: accept with a warning.
    FullButDifferent,
    /// A clip no longer than a public preview while the catalogue track is long.
    /// This is not acceptable as full playback (the reported 30 s bug).
    PreviewLength,
}

fn classify_match(actual: Option<f64>, expected: Option<f64>) -> MatchLength {
    let (Some(actual), Some(expected)) = (actual, expected) else {
        return MatchLength::Matched;
    };
    if (actual - expected).abs() <= DURATION_TOLERANCE_SECS {
        return MatchLength::Matched;
    }
    if actual <= PREVIEW_MAX_SECS && expected > 60.0 {
        return MatchLength::PreviewLength;
    }
    MatchLength::FullButDifferent
}

// ---------------------------------------------------------------------------
// resolve()
// ---------------------------------------------------------------------------

pub fn resolve(config: &ResolveConfig, track: &Track) -> Result<ResolvedAudio, ResolveError> {
    // Local files are returned as-is.
    if let Some(path) = &track.local_path {
        if !path.is_file() {
            return Err(ResolveError::Media(format!(
                "local file not found: {path:?}"
            )));
        }
        return Ok(ResolvedAudio {
            path: path.clone(),
            temporary: false,
        });
    }

    std::fs::create_dir_all(&config.cache_dir)
        .map_err(|err| ResolveError::Other(format!("cannot create audio cache: {err}")))?;

    // 1. Reuse a completed full-length cache entry. Preview clips are ignored so
    //    a 30 s preview cached by an older build cannot shadow full playback.
    if let Some(path) = find_cached(config, track) {
        return Ok(ResolvedAudio {
            path,
            temporary: true,
        });
    }

    let priority = playback_priority(track);
    let ytdlp = config.ytdlp();
    let mut last_error: Option<ResolveError> = None;
    let mut mismatched_full: Option<PathBuf> = None;

    // 2. Full-length audio through yt-dlp always wins, for every platform.
    if let Some(ytdlp_path) = ytdlp.as_ref() {
        let sources = sources_for(track);
        if sources.is_empty() {
            if priority == PlaybackPriority::FullOnly {
                return Err(ResolveError::Unsupported(format!(
                    "no audio source known for {}",
                    track.title
                )));
            }
        } else {
            let _slot = YtdlpSlot::acquire();
            for source in sources {
                match download_source(config, ytdlp_path, track, &source) {
                    Ok(path) => {
                        let length = if priority == PlaybackPriority::FullOnly {
                            MatchLength::Matched
                        } else {
                            classify_match(file_duration_secs(&path), track.duration_secs)
                        };
                        match length {
                            MatchLength::Matched => {
                                validate_duration(&path, track);
                                return Ok(ResolvedAudio {
                                    path,
                                    temporary: true,
                                });
                            }
                            // Radio edits / live versions are still full songs; do
                            // not fall back to a 30 s preview for them.
                            MatchLength::FullButDifferent => {
                                validate_duration(&path, track);
                                log::info!(
                                    "accepting full-length yt-dlp match with a different duration for {} ({:?}s vs catalogue {:?}s)",
                                    track.title,
                                    file_duration_secs(&path),
                                    track.duration_secs
                                );
                                return Ok(ResolvedAudio {
                                    path,
                                    temporary: true,
                                });
                            }
                            MatchLength::PreviewLength => {
                                log::warn!(
                                    "yt-dlp matched a preview-length clip for {} ({:?}s vs catalogue {:?}s); trying other sources",
                                    track.title,
                                    file_duration_secs(&path),
                                    track.duration_secs
                                );
                                // Last resort only: a longer-but-wrong match still
                                // beats silence if no preview exists.
                                mismatched_full = Some(path);
                            }
                        }
                    }
                    Err(err) => {
                        log::warn!(
                            "yt-dlp source {:?} failed for {}: {err}",
                            source.argument(),
                            track.title
                        );
                        last_error = Some(err);
                    }
                }
            }
        }
    }

    // 3. Public 30 s preview as fallback only (Apple Music / Spotify).
    if priority == PlaybackPriority::FullThenPreview {
        if let Some(url) = preview_url(track) {
            match fetch_preview(config, track, url) {
                Ok(path) => {
                    log::info!(
                        "using public 30 s preview as playback fallback for {} (full match unavailable)",
                        track.title
                    );
                    return Ok(ResolvedAudio {
                        path,
                        temporary: true,
                    });
                }
                Err(err) => {
                    log::warn!("preview fallback failed for {}: {err}", track.title);
                    last_error = Some(ResolveError::Other(format!("preview fallback: {err}")));
                }
            }
        }
    }

    // 4. A duration-mismatched full download still beats silence; keep it last.
    if let Some(path) = mismatched_full {
        log::warn!(
            "using duration-mismatched yt-dlp match as last resort for {}: {:?}",
            track.title,
            path
        );
        return Ok(ResolvedAudio {
            path,
            temporary: true,
        });
    }

    if ytdlp.is_none() {
        return Err(ResolveError::YtDlpMissing);
    }
    Err(last_error.unwrap_or_else(|| {
        ResolveError::Media(format!("could not resolve audio for {}", track.title))
    }))
}

/// Warn (but allow) when a downloaded file's duration differs from the
/// catalogue metadata by more than the CONTRACT allowed ±8 s.
fn validate_duration(path: &Path, track: &Track) {
    let Some(expected) = track.duration_secs else {
        return;
    };
    let Some(actual) = file_duration_secs(path) else {
        return;
    };
    if (actual - expected).abs() > DURATION_TOLERANCE_SECS {
        log::warn!(
            "duration mismatch for {}: catalogue {expected:.1}s, downloaded {actual:.1}s",
            track.title
        );
    }
}

/// Download one source with yt-dlp into a temporary directory, then move the
/// result into the cache under its stable name. Transcodes webm/opus with ffmpeg.
fn download_source(
    config: &ResolveConfig,
    ytdlp: &Path,
    track: &Track,
    source: &Source,
) -> Result<PathBuf, ResolveError> {
    std::fs::create_dir_all(&config.cache_dir)
        .map_err(|err| ResolveError::Other(format!("cannot create audio cache: {err}")))?;
    let temp_dir = config
        .cache_dir
        .join(format!(".tmp-{}-{}", cache_stem(track), unique_suffix()));
    std::fs::create_dir_all(&temp_dir)
        .map_err(|err| ResolveError::Other(format!("cannot create temp dir: {err}")))?;
    let guard = TempDirGuard::new(temp_dir.clone());

    let output_template = temp_dir.join("%(id)s.%(ext)s");
    let mut command = process::quiet(ytdlp);
    command.args([
        "--no-playlist",
        "--no-warnings",
        "--newline",
        // Bound failure feedback on geo-blocked/anti-bot sources (default is 10).
        "--retries",
        "3",
        "--fragment-retries",
        "3",
        "-f",
        "bestaudio[ext=m4a]/bestaudio[ext=mp3]/bestaudio",
        "-o",
    ]);
    command.arg(&output_template);
    command.arg(source.argument());

    let output = run_process(command, RESOLVE_TIMEOUT)?;
    if !output.success() {
        return Err(classify_failure(&output));
    }

    let downloaded = find_downloaded_file(&temp_dir).ok_or_else(|| {
        ResolveError::Media(format!("yt-dlp produced no file: {}", output.error_tail()))
    })?;

    let final_path = if crate::audio::is_decodable(&downloaded) {
        let extension = downloaded
            .extension()
            .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_else(|| "m4a".to_owned());
        move_into_cache(config, track, &downloaded, &extension)?
    } else {
        // webm/opus (or anything symphonia cannot decode) → transcode to m4a.
        let ffmpeg = config.ffmpeg().ok_or_else(|| {
            ResolveError::Media(
                "this source only provides webm/opus audio and ffmpeg is not available".to_owned(),
            )
        })?;
        let transcoded = temp_dir.join("transcoded.m4a");
        transcode_to_m4a(&ffmpeg, &downloaded, &transcoded)?;
        if !crate::audio::is_decodable(&transcoded) {
            return Err(ResolveError::Media(
                "ffmpeg transcode produced an undecodable file".to_owned(),
            ));
        }
        move_into_cache(config, track, &transcoded, "m4a")?
    };

    drop(guard);
    Ok(final_path)
}

fn move_into_cache(
    config: &ResolveConfig,
    track: &Track,
    source: &Path,
    extension: &str,
) -> Result<PathBuf, ResolveError> {
    let destination = cache_path(config, track, extension);
    if destination.is_file() {
        // Another worker may have produced the full file already. Keep it, unless
        // it is a 30 s preview cached by an older build: a preview must never
        // shadow the full-length download.
        if !looks_like_preview(&destination, track) {
            return Ok(destination);
        }
        let _ = std::fs::remove_file(&destination);
    }
    match std::fs::rename(source, &destination) {
        Ok(()) => Ok(destination),
        Err(rename_error) => {
            // Cross-device rename (unlikely, same dir) → copy + remove.
            std::fs::copy(source, &destination).map_err(|copy_error| {
                ResolveError::Other(format!(
                    "cannot cache audio: {rename_error}; copy fallback: {copy_error}"
                ))
            })?;
            let _ = std::fs::remove_file(source);
            Ok(destination)
        }
    }
}

pub(crate) fn find_downloaded_file(directory: &Path) -> Option<PathBuf> {
    let mut best: Option<(u64, PathBuf)> = None;
    let entries = std::fs::read_dir(directory).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let metadata = entry.metadata().ok()?;
        if !metadata.is_file() {
            continue;
        }
        let extension = path
            .extension()
            .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if matches!(extension.as_str(), "part" | "ytdl" | "tmp") {
            continue;
        }
        let size = metadata.len();
        if size == 0 {
            continue;
        }
        if best
            .as_ref()
            .map(|(best_size, _)| size > *best_size)
            .unwrap_or(true)
        {
            best = Some((size, path));
        }
    }
    best.map(|(_, path)| path)
}

pub(crate) fn transcode_to_m4a(
    ffmpeg: &Path,
    input: &Path,
    output: &Path,
) -> Result<(), ResolveError> {
    let mut command = process::quiet(ffmpeg);
    command.args(["-hide_banner", "-loglevel", "error", "-y", "-i"]);
    command.arg(input);
    command.args(["-vn", "-c:a", "aac", "-b:a", "192k"]);
    command.arg(output);
    let result = run_process(command, TRANSCODE_TIMEOUT)?;
    if !result.success() {
        return Err(ResolveError::Media(format!(
            "ffmpeg transcode failed: {}",
            result.error_tail()
        )));
    }
    if !output.is_file() {
        return Err(ResolveError::Media(
            "ffmpeg produced no output file".to_owned(),
        ));
    }
    Ok(())
}

fn classify_failure(output: &ProcessOutput) -> ResolveError {
    if output.timed_out {
        return ResolveError::Network("yt-dlp timed out".to_owned());
    }
    let stderr = output.stderr.to_ascii_lowercase();
    let tail = output.error_tail();
    let network_markers = [
        "unable to download",
        "getaddrinfo",
        "temporary failure",
        "name or service not known",
        "network is unreachable",
        "connection refused",
        "connection reset",
        "timed out",
        "ssl",
        "http error 5",
        "http error 429",
        "proxy",
    ];
    if network_markers.iter().any(|marker| stderr.contains(marker)) {
        return ResolveError::Network(tail);
    }
    ResolveError::Media(tail)
}

struct TempDirGuard {
    path: PathBuf,
    armed: bool,
}

impl TempDirGuard {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

fn unique_suffix() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.subsec_nanos())
        .unwrap_or(0);
    format!("{}-{counter}-{nanos}", std::process::id())
}

// ---------------------------------------------------------------------------
// yt-dlp searches
// ---------------------------------------------------------------------------

/// yt-dlp search prefix for a platform (`ytsearch`, `scsearch`, `bilisearch`).
pub(crate) fn search_prefix(platform: Platform) -> &'static str {
    match platform {
        Platform::SoundCloud => "scsearch",
        Platform::Bilibili => "bilisearch",
        // YouTube is the universal backend; Netease / Spotify / Apple / local
        // results are matched through YouTube.
        _ => "ytsearch",
    }
}

#[derive(Debug, Default, Deserialize)]
struct FlatSearch {
    #[serde(default)]
    entries: Option<Vec<FlatEntry>>,
}

#[derive(Debug, Default, Deserialize)]
struct FlatEntry {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    webpage_url: Option<String>,
    #[serde(default)]
    duration: Option<f64>,
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    uploader: Option<String>,
    #[serde(default)]
    artist: Option<String>,
    #[serde(default)]
    thumbnails: Option<Vec<FlatThumbnail>>,
    #[serde(default)]
    thumbnail: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct FlatThumbnail {
    #[serde(default)]
    url: Option<String>,
}

impl FlatEntry {
    fn page_url(&self, platform: Platform) -> Option<String> {
        if let Some(url) = self.webpage_url.as_deref().filter(|url| !url.is_empty()) {
            return Some(url.to_owned());
        }
        if let Some(url) = self.url.as_deref().filter(|url| !url.is_empty()) {
            if url.starts_with("http://") || url.starts_with("https://") {
                return Some(url.to_owned());
            }
            // Some flat entries only carry an id-like `url`.
            let id = url.trim();
            return match platform {
                Platform::YouTube => Some(format!("https://www.youtube.com/watch?v={id}")),
                Platform::Bilibili => Some(format!("https://www.bilibili.com/video/{id}")),
                Platform::Netease => Some(format!("https://music.163.com/song?id={id}")),
                _ => None,
            };
        }
        self.id.as_deref().map(|id| match platform {
            Platform::YouTube => format!("https://www.youtube.com/watch?v={id}"),
            Platform::Bilibili => format!("https://www.bilibili.com/video/{id}"),
            Platform::Netease => format!("https://music.163.com/song?id={id}"),
            _ => id.to_owned(),
        })
    }

    fn artist(&self) -> String {
        self.channel
            .as_deref()
            .or(self.uploader.as_deref())
            .or(self.artist.as_deref())
            .unwrap_or_default()
            .trim()
            .to_owned()
    }

    fn thumbnail_url(&self) -> Option<String> {
        if let Some(url) = self.thumbnail.as_deref().filter(|url| !url.is_empty()) {
            return Some(url.to_owned());
        }
        self.thumbnails
            .as_deref()
            .and_then(|thumbnails| {
                thumbnails
                    .iter()
                    .filter_map(|thumb| thumb.url.as_deref())
                    .filter(|url| !url.is_empty())
                    .last()
            })
            .map(|url| url.to_owned())
    }

    fn title(&self) -> Option<String> {
        self.title
            .as_deref()
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(str::to_owned)
    }

    /// Title, or a readable fallback derived from the platform id. Some
    /// extractors (e.g. Bilibili flat search) only return ids for entries.
    fn title_or_fallback(&self, platform: Platform) -> String {
        if let Some(title) = self.title() {
            return title;
        }
        let identifier = self
            .id
            .as_deref()
            .filter(|id| !id.is_empty())
            .or_else(|| self.url.as_deref())
            .unwrap_or("unknown");
        format!("{} video {identifier}", platform.label())
    }

    fn duration_secs(&self) -> Option<f64> {
        self.duration
            .filter(|duration| duration.is_finite() && *duration > 0.0)
    }
}

fn run_search(
    config: &ResolveConfig,
    platform: Platform,
    query: &str,
    limit: usize,
) -> Result<Vec<FlatEntry>, ResolveError> {
    let ytdlp = config.ytdlp().ok_or(ResolveError::YtDlpMissing)?;
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let limit = limit.clamp(1, 30);
    let expression = format!("{}{}:{}", search_prefix(platform), limit, query);

    let _slot = YtdlpSlot::acquire();
    let mut command = process::quiet(ytdlp);
    command.args(["-J", "--flat-playlist", "--no-warnings", &expression]);
    let output = run_process(command, SEARCH_TIMEOUT)?;
    if !output.success() {
        return Err(classify_failure(&output));
    }

    let parsed: FlatSearch = serde_json::from_str(&output.stdout)
        .map_err(|err| ResolveError::Other(format!("cannot parse yt-dlp search results: {err}")))?;
    // `-J` on a single video returns the video object itself (no `entries`).
    if parsed.entries.is_none() {
        if let Ok(single) = serde_json::from_str::<FlatEntry>(&output.stdout) {
            return Ok(vec![single]);
        }
    }
    Ok(parsed.entries.unwrap_or_default())
}

/// Search a platform via yt-dlp and build `Track`s for the results.
pub fn ytdlp_search(
    config: &ResolveConfig,
    platform: Platform,
    query: &str,
    limit: usize,
) -> Result<Vec<Track>, ResolveError> {
    if platform == Platform::Local {
        return Err(ResolveError::Unsupported(
            "local files cannot be searched online".to_owned(),
        ));
    }
    let entries = run_search(config, platform, query, limit)?;
    let mut tracks = Vec::with_capacity(entries.len());
    for entry in entries {
        let Some(page_url) = entry.page_url(platform) else {
            continue;
        };
        let id_source = page_url.clone();
        let mut track = Track::new(
            platform,
            &id_source,
            entry.title_or_fallback(platform),
            entry.artist(),
        );
        track.page_url = Some(page_url);
        if let Some(duration) = entry.duration_secs() {
            track.duration_secs = Some(duration);
        }
        if let Some(thumbnail) = entry.thumbnail_url() {
            track.art_url = Some(thumbnail);
        }
        tracks.push(track);
    }
    Ok(tracks)
}

/// YouTube search simplified to links, used by the background info provider.
pub fn search_links(
    config: &ResolveConfig,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchLink>, ResolveError> {
    let entries = run_search(config, Platform::YouTube, query, limit.clamp(1, 8))?;
    let mut links = Vec::with_capacity(entries.len());
    for entry in entries {
        let Some(url) = entry.page_url(Platform::YouTube) else {
            continue;
        };
        links.push(SearchLink {
            title: entry.title_or_fallback(Platform::YouTube),
            url,
            channel: Some(entry.artist()).filter(|artist| !artist.is_empty()),
            duration_secs: entry.duration_secs(),
            thumbnail: entry.thumbnail_url(),
        });
    }
    Ok(links)
}

// ---------------------------------------------------------------------------
// Tests (no network, no audio hardware)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Platform;

    fn youtube_track() -> Track {
        let mut track = Track::new(
            Platform::YouTube,
            "https://youtube.test/watch?v=abc",
            "Song",
            "Artist",
        );
        track.page_url = Some("https://www.youtube.com/watch?v=abc".to_owned());
        track
    }

    #[test]
    fn cache_keys_are_stable() {
        let a = Track::new(Platform::YouTube, "video-id-1", "Title", "Artist");
        let b = Track::new(
            Platform::YouTube,
            "video-id-1",
            "Other title",
            "Other artist",
        );
        assert_eq!(a.cache_key(), b.cache_key());
        let c = Track::new(Platform::SoundCloud, "video-id-1", "Title", "Artist");
        assert_ne!(a.cache_key(), c.cache_key());

        let mut local = Track::from_local_path(PathBuf::from("/tmp/a.mp3"));
        assert!(local.cache_key().starts_with("local-"));
        local.local_path = Some(PathBuf::from("/tmp/b.mp3"));
        assert_ne!(
            local.cache_key(),
            Track::from_local_path(PathBuf::from("/tmp/a.mp3")).cache_key()
        );
    }

    #[test]
    fn search_prefixes_match_platforms() {
        assert_eq!(search_prefix(Platform::YouTube), "ytsearch");
        assert_eq!(search_prefix(Platform::SoundCloud), "scsearch");
        assert_eq!(search_prefix(Platform::Bilibili), "bilisearch");
        assert_eq!(search_prefix(Platform::Netease), "ytsearch");
        assert_eq!(search_prefix(Platform::Spotify), "ytsearch");
    }

    #[test]
    fn sources_prefer_page_urls_then_fallback_search() {
        let youtube = sources_for(&youtube_track());
        assert_eq!(
            youtube[0],
            Source::Url("https://www.youtube.com/watch?v=abc".to_owned())
        );
        assert!(
            matches!(youtube[1], Source::Search(ref prefix) if prefix.starts_with("ytsearch1:"))
        );

        let mut spotify = Track::new(Platform::Spotify, "spotify:track:1", "Song", "Artist");
        spotify.duration_secs = Some(200.0);
        spotify.stream_url = Some("https://p.scdn.co/mp3-preview/abc".to_owned());
        let sources = sources_for(&spotify);
        assert!(
            matches!(sources[0], Source::Search(ref prefix) if prefix.starts_with("ytsearch1:"))
        );
        assert_eq!(sources.len(), 1);

        let mut netease = Track::new(Platform::Netease, "netease:1", "Song", "Artist");
        netease.page_url = Some("https://music.163.com/song?id=1".to_owned());
        let sources = sources_for(&netease);
        assert!(matches!(sources[0], Source::Url(_)));
        assert!(matches!(sources[1], Source::Search(_)));
    }

    #[test]
    fn only_public_preview_cdns_are_fetched_directly() {
        let mut apple = Track::new(Platform::AppleMusic, "apple:1", "Song", "Artist");
        apple.stream_url = Some("https://audio-ssl.itunes.apple.com/preview.m4a".to_owned());
        assert!(preview_url(&apple).is_some());
        assert_eq!(preview_extension(&apple), "m4a");

        let mut spotify = Track::new(Platform::Spotify, "spotify:1", "Song", "Artist");
        spotify.stream_url = Some("https://p.scdn.co/mp3-preview/hash".to_owned());
        assert!(preview_url(&spotify).is_some());
        assert_eq!(preview_extension(&spotify), "mp3");

        // The HARD RULE: yt-dlp resolved media URLs (IP signed CDNs) must never be
        // handed to Http, even if they end up in `stream_url`.
        let mut evil = Track::new(Platform::YouTube, "yt:1", "Song", "Artist");
        evil.stream_url = Some("https://rr1.googlevideo.com/videoplayback?expire=1".to_owned());
        assert!(preview_url(&evil).is_none());

        let mut sneaky = Track::new(Platform::AppleMusic, "apple:2", "Song", "Artist");
        sneaky.stream_url = Some("https://rr1.googlevideo.com/videoplayback".to_owned());
        assert!(preview_url(&sneaky).is_none());

        let mut plain = Track::new(Platform::AppleMusic, "apple:3", "Song", "Artist");
        plain.stream_url = Some("http://127.0.0.1/evil.mp3".to_owned());
        assert!(preview_url(&plain).is_none());
    }

    #[test]
    fn flat_search_json_parses_into_tracks() {
        let json = r#"{
            "_type": "playlist",
            "entries": [
                {
                    "_type": "url",
                    "id": "dQw4w9WgXcQ",
                    "title": "Never Gonna Give You Up",
                    "url": "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
                    "duration": 213.0,
                    "channel": "Rick Astley",
                    "thumbnails": [{"url": "https://i.ytimg.com/vi/dQw4w9WgXcQ/hq.jpg"}]
                },
                {
                    "_type": "url",
                    "id": "second",
                    "title": "Second Song",
                    "url": "https://www.youtube.com/watch?v=second",
                    "uploader": "Some Channel"
                }
            ]
        }"#;
        let parsed: FlatSearch = serde_json::from_str(json).expect("parse");
        let entries = parsed.entries.expect("entries");
        assert_eq!(entries.len(), 2);
        let first = &entries[0];
        assert_eq!(first.title().as_deref(), Some("Never Gonna Give You Up"));
        assert_eq!(first.artist(), "Rick Astley");
        assert_eq!(first.duration_secs(), Some(213.0));
        assert_eq!(
            first.page_url(Platform::YouTube).as_deref(),
            Some("https://www.youtube.com/watch?v=dQw4w9WgXcQ")
        );
        assert!(first.thumbnail_url().is_some());
        assert_eq!(entries[1].artist(), "Some Channel");
    }

    #[test]
    fn cached_file_is_reused_without_ytdlp() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config = ResolveConfig {
            ytdlp: None,
            ffmpeg: None,
            cache_dir: temp.path().to_path_buf(),
        };
        let track = youtube_track();
        let cached = cache_path(&config, &track, "mp3");
        std::fs::write(&cached, vec![0u8; (MIN_CACHE_BYTES + 1) as usize]).expect("write cache");
        let resolved = resolve(&config, &track).expect("cached resolution");
        assert_eq!(resolved.path, cached);
        assert!(resolved.temporary);
    }

    #[test]
    fn too_small_cache_entries_are_ignored() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config = ResolveConfig {
            // Bogus explicit path: the test must never discover a real yt-dlp or
            // touch the network.
            ytdlp: Some(PathBuf::from("/nonexistent/echoport-yt-dlp")),
            ffmpeg: None,
            cache_dir: temp.path().to_path_buf(),
        };
        let track = youtube_track();
        let cached = cache_path(&config, &track, "mp3");
        std::fs::write(&cached, vec![0u8; 16]).expect("write stub");
        // The truncated cache file must not be served; the bogus tool fails.
        assert!(resolve(&config, &track).is_err());
    }

    #[test]
    fn local_track_resolves_to_its_own_path() {
        let temp = tempfile::tempdir().expect("tempdir");
        let file = temp.path().join("song.mp3");
        std::fs::write(&file, b"not really an mp3").expect("write");
        let track = Track::from_local_path(file.clone());
        let config = ResolveConfig {
            ytdlp: None,
            ffmpeg: None,
            cache_dir: temp.path().join("cache"),
        };
        let resolved = resolve(&config, &track).expect("local resolution");
        assert_eq!(resolved.path, file);
        assert!(!resolved.temporary);
    }

    #[test]
    fn process_timeout_is_enforced() {
        let mut command = Command::new("sleep");
        command.arg("5");
        let started = Instant::now();
        let output = run_process(command, Duration::from_millis(200)).expect("spawn sleep");
        assert!(output.timed_out);
        assert!(!output.success());
        assert!(started.elapsed() < Duration::from_secs(4));
    }

    #[test]
    fn classify_failure_buckets_errors() {
        let network = ProcessOutput {
            status: None,
            stdout: String::new(),
            stderr: "ERROR: Unable to download webpage: getaddrinfo failed".to_owned(),
            timed_out: false,
        };
        assert!(matches!(
            classify_failure(&network),
            ResolveError::Network(_)
        ));
        let media = ProcessOutput {
            status: None,
            stdout: String::new(),
            stderr: "ERROR: Requested format is not available".to_owned(),
            timed_out: false,
        };
        assert!(matches!(classify_failure(&media), ResolveError::Media(_)));
    }

    #[test]
    fn truncation_keeps_chars() {
        assert_eq!(truncate("abcdef", 10), "abcdef");
        assert_eq!(truncate("abcdef", 4), "abc…");
    }

    // -- playback priority / full-vs-preview ---------------------------------

    fn apple_track(duration: Option<f64>, preview: bool) -> Track {
        let mut track = Track::new(Platform::AppleMusic, "apple:track:1", "Song", "Artist");
        track.duration_secs = duration;
        if preview {
            track.stream_url = Some("https://audio-ssl.itunes.apple.com/preview.m4a".to_owned());
        }
        track
    }

    fn spotify_track(duration: Option<f64>) -> Track {
        let mut track = Track::new(Platform::Spotify, "spotify:track:1", "Song", "Artist");
        track.duration_secs = duration;
        track.stream_url = Some("https://p.scdn.co/mp3-preview/hash".to_owned());
        track
    }

    #[test]
    fn streaming_platforms_try_full_audio_before_preview() {
        // Apple Music and Spotify must prefer the full yt-dlp match; the preview
        // CDN is only a fallback (this is the user reported 30 s bug).
        assert_eq!(
            playback_priority(&apple_track(Some(200.0), true)),
            PlaybackPriority::FullThenPreview
        );
        assert_eq!(
            playback_priority(&spotify_track(Some(200.0))),
            PlaybackPriority::FullThenPreview
        );
        assert_eq!(
            playback_priority(&youtube_track()),
            PlaybackPriority::FullOnly
        );
        assert_eq!(
            playback_priority(&Track::new(Platform::Netease, "n:1", "Song", "Artist")),
            PlaybackPriority::FullOnly
        );

        // The sources used for streaming tracks are full-audio searches, never a
        // preview URL, and resolve() reorders around this list.
        let sources = sources_for(&spotify_track(Some(200.0)));
        assert_eq!(sources.len(), 1);
        assert!(matches!(&sources[0], Source::Search(query) if query.starts_with("ytsearch1:")));
        assert!(!matches!(&sources[0], Source::Url(_)));
    }

    #[test]
    fn match_classification_prefers_full_songs_over_previews() {
        use MatchLength::*;
        // Within the CONTRACT tolerance.
        assert_eq!(classify_match(Some(200.0), Some(200.0)), Matched);
        assert_eq!(classify_match(Some(205.0), Some(200.0)), Matched);
        assert_eq!(classify_match(Some(195.0), Some(200.0)), Matched);
        // Unknown durations can never reject a match.
        assert_eq!(classify_match(None, Some(200.0)), Matched);
        assert_eq!(classify_match(Some(200.0), None), Matched);
        assert_eq!(classify_match(None, None), Matched);
        // A 30 s clip of a long streaming track is the reported bug: reject it.
        assert_eq!(classify_match(Some(30.0), Some(200.0)), PreviewLength);
        assert_eq!(classify_match(Some(29.5), Some(369.6)), PreviewLength);
        // The YouTube radio edit of a long album track is full playback: accept.
        assert_eq!(classify_match(Some(249.0), Some(369.6)), FullButDifferent);
        assert_eq!(classify_match(Some(240.0), Some(200.0)), FullButDifferent);
        // A genuinely short track is not a preview of itself.
        assert_eq!(classify_match(Some(30.0), Some(30.0)), Matched);
    }

    #[test]
    fn preview_duration_heuristic_detects_legacy_previews() {
        // 30 s clip of a 3+ minute streaming track → legacy preview.
        assert!(preview_duration_mismatch(Some(30.0), Some(200.0)));
        assert!(preview_duration_mismatch(Some(30.0), None));
        // A genuinely short track is not a preview.
        assert!(!preview_duration_mismatch(Some(30.0), Some(30.0)));
        // Full-length files are never previews.
        assert!(!preview_duration_mismatch(Some(200.0), Some(200.0)));
        assert!(!preview_duration_mismatch(Some(90.0), Some(90.0)));
        assert!(!preview_duration_mismatch(None, Some(200.0)));
    }

    // -- preview cache naming / shadowing ------------------------------------

    #[test]
    fn preview_cache_paths_are_explicitly_marked() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config = ResolveConfig {
            ytdlp: None,
            ffmpeg: None,
            cache_dir: temp.path().to_path_buf(),
        };
        let track = apple_track(Some(200.0), true);
        let preview = preview_cache_path(&config, &track);
        let full = cache_path(&config, &track, preview_extension(&track));
        assert_ne!(preview, full);
        assert!(
            preview
                .file_name()
                .unwrap()
                .to_string_lossy()
                .contains(".preview."),
            "preview cache file must carry the marker: {preview:?}"
        );
        assert!(is_preview_marker(&preview));
        assert!(!is_preview_marker(&full));
    }

    /// Minimal 8 kHz mono 16-bit PCM WAV writer used to give cache files a real
    /// probed duration (symphonia's wav support is enabled).
    fn write_test_wav(path: &Path, seconds: f32) -> std::io::Result<()> {
        let sample_rate = 8_000u32;
        let frames = (sample_rate as f32 * seconds) as u32;
        let data_len = frames * 2;
        let mut bytes = Vec::with_capacity((44 + data_len) as usize);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&sample_rate.to_le_bytes());
        bytes.extend_from_slice(&(sample_rate * 2).to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        for frame in 0..frames {
            let sample = ((frame as f32 * 440.0 * std::f32::consts::TAU / sample_rate as f32).sin()
                * 1000.0) as i16;
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        std::fs::write(path, bytes)
    }

    #[test]
    fn legacy_preview_cache_cannot_shadow_full_audio() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config = ResolveConfig {
            ytdlp: Some(PathBuf::from("/nonexistent/echoport-yt-dlp")),
            ffmpeg: None,
            cache_dir: temp.path().to_path_buf(),
        };
        let track = apple_track(Some(200.0), true);

        // A legacy cache entry from the buggy version: 30 s of audio, no marker.
        let legacy = cache_path(&config, &track, "wav");
        write_test_wav(&legacy, 30.0).expect("write wav");

        // It is recognised as a preview and is not served as full playback...
        assert!(find_cached(&config, &track).is_none());
        assert_eq!(
            find_cached_preview(&config, &track).as_deref(),
            Some(legacy.as_path())
        );

        // ...but it is still usable as the fallback preview (no network needed:
        // the cached legacy preview is reused) because the bogus yt-dlp fails.
        let resolved = resolve(&config, &track).expect("preview fallback");
        assert_eq!(resolved.path, legacy);
        assert!(resolved.temporary);
    }

    #[test]
    fn full_cache_wins_over_marked_preview() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config = ResolveConfig {
            ytdlp: Some(PathBuf::from("/nonexistent/echoport-yt-dlp")),
            ffmpeg: None,
            cache_dir: temp.path().to_path_buf(),
        };
        let track = spotify_track(Some(200.0));
        let full = cache_path(&config, &track, "m4a");
        std::fs::write(&full, vec![1u8; (MIN_CACHE_BYTES + 1) as usize]).expect("full cache");
        let preview = preview_cache_path(&config, &track);
        std::fs::write(&preview, vec![2u8; (MIN_CACHE_BYTES + 1) as usize]).expect("preview");

        // A previous preview must not shadow the full file.
        assert_eq!(
            find_cached(&config, &track).as_deref(),
            Some(full.as_path())
        );
        let resolved = resolve(&config, &track).expect("full cache resolution");
        assert_eq!(resolved.path, full);
    }

    #[test]
    fn move_into_cache_replaces_legacy_preview_at_destination() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config = ResolveConfig {
            ytdlp: None,
            ffmpeg: None,
            cache_dir: temp.path().to_path_buf(),
        };
        let track = apple_track(Some(200.0), true);
        let destination = cache_path(&config, &track, "wav");
        write_test_wav(&destination, 30.0).expect("legacy preview at destination");

        let fresh = temp.path().join("fresh-download.wav");
        std::fs::write(&fresh, vec![9u8; (MIN_CACHE_BYTES + 2) as usize]).expect("fresh");
        let final_path = move_into_cache(&config, &track, &fresh, "wav").expect("move");
        assert_eq!(final_path, destination);
        let size = std::fs::metadata(&destination).expect("metadata").len();
        assert_eq!(
            size,
            MIN_CACHE_BYTES + 2,
            "the stale 30 s preview must be replaced by the full download"
        );
    }

    // -- cache_stats / prune_cache -------------------------------------------

    fn write_sized(path: &Path, size: usize) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("parent");
        }
        std::fs::write(path, vec![0u8; size]).expect("write sized");
    }

    /// `File::set_times` needs no extra crate and lets the pruning tests age files.
    fn age_file(path: &Path, seconds: u64) {
        let file = std::fs::File::options()
            .write(true)
            .open(path)
            .expect("open for ageing");
        let modified = SystemTime::now() - Duration::from_secs(seconds);
        file.set_times(std::fs::FileTimes::new().set_modified(modified))
            .expect("set modified time");
    }

    #[test]
    fn cache_stats_counts_files_recursively() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_sized(&temp.path().join("a.m4a"), 100);
        write_sized(&temp.path().join("sub").join("b.mp3"), 250);
        let stats = cache_stats(temp.path());
        assert_eq!(
            stats,
            CacheStats {
                bytes: 350,
                files: 2
            }
        );
        // Missing directory is simply empty.
        assert_eq!(
            cache_stats(&temp.path().join("missing")),
            CacheStats::default()
        );
    }

    #[test]
    fn prune_cache_protects_recent_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_sized(&temp.path().join("fresh.m4a"), 500);
        assert_eq!(prune_cache(temp.path(), 0), 0);
        assert!(temp.path().join("fresh.m4a").is_file());
    }

    #[test]
    fn prune_cache_removes_oldest_files_until_under_limit() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_sized(&temp.path().join("oldest.m4a"), 400);
        write_sized(&temp.path().join("middle.m4a"), 400);
        write_sized(&temp.path().join("newest.m4a"), 400);
        write_sized(&temp.path().join("recent.m4a"), 400);
        age_file(&temp.path().join("oldest.m4a"), 900);
        age_file(&temp.path().join("middle.m4a"), 600);
        age_file(&temp.path().join("newest.m4a"), 300);

        // 1600 total, limit 800: the two oldest files are removed.
        let freed = prune_cache(temp.path(), 800);
        assert_eq!(freed, 800);
        assert!(!temp.path().join("oldest.m4a").exists());
        assert!(!temp.path().join("middle.m4a").exists());
        assert!(temp.path().join("newest.m4a").is_file());
        assert!(temp.path().join("recent.m4a").is_file());
        assert!(cache_stats(temp.path()).bytes <= 800);
    }

    #[test]
    fn prune_cache_skips_in_flight_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_sized(&temp.path().join("partial.part"), 500);
        write_sized(&temp.path().join("fragment.part-abc"), 500);
        write_sized(&temp.path().join("done.m4a"), 500);
        age_file(&temp.path().join("partial.part"), 900);
        age_file(&temp.path().join("fragment.part-abc"), 900);
        age_file(&temp.path().join("done.m4a"), 900);

        let freed = prune_cache(temp.path(), 0);
        assert_eq!(freed, 500, "only the completed file may be pruned");
        assert!(temp.path().join("partial.part").exists());
        assert!(temp.path().join("fragment.part-abc").exists());
        assert!(!temp.path().join("done.m4a").exists());
    }

    #[test]
    fn prune_cache_is_noop_when_under_limit() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_sized(&temp.path().join("song.m4a"), 100);
        age_file(&temp.path().join("song.m4a"), 900);
        assert_eq!(prune_cache(temp.path(), 1_000), 0);
        assert!(temp.path().join("song.m4a").is_file());
    }

    /// End-to-end regression test for the user reported bug: an Apple Music track
    /// with a 30 s preview URL must resolve to the FULL song through yt-dlp, not
    /// to the preview. Network + yt-dlp required, hence ignored.
    #[test]
    #[ignore = "network + yt-dlp required"]
    fn net_apple_track_resolves_full_audio_not_preview() {
        if detect_ytdlp().is_none() {
            eprintln!("skipping: yt-dlp not available");
            return;
        }
        let http = crate::net::Http::new();
        let search_url =
            "https://itunes.apple.com/search?term=daft+punk+get+lucky&entity=song&limit=1";
        let json: serde_json::Value = match http.get_json(search_url, &[]) {
            Ok(json) => json,
            Err(err) => {
                eprintln!("skipping: iTunes API unavailable: {err}");
                return;
            }
        };
        let Some(item) = json["results"].get(0) else {
            eprintln!("skipping: iTunes API returned no results");
            return;
        };
        let preview = item["previewUrl"].as_str().unwrap_or_default().to_owned();
        let catalogue_secs = item["trackTimeMillis"].as_f64().unwrap_or(0.0) / 1000.0;
        if preview.is_empty() || catalogue_secs < 120.0 {
            eprintln!("skipping: unexpected iTunes payload");
            return;
        }
        let mut track = Track::new(
            Platform::AppleMusic,
            "apple:net:get-lucky",
            item["trackName"].as_str().unwrap_or("Get Lucky"),
            item["artistName"].as_str().unwrap_or("Daft Punk"),
        );
        track.duration_secs = Some(catalogue_secs);
        track.stream_url = Some(preview);

        let cache = tempfile::tempdir().expect("tempdir");
        let config = ResolveConfig {
            ytdlp: None,
            ffmpeg: None,
            cache_dir: cache.path().to_path_buf(),
        };
        // Simulate a user upgrading from the buggy version: only a legacy 30 s
        // preview is cached. It must not shadow full playback any more.
        let legacy_preview = cache_path(&config, &track, "wav");
        write_test_wav(&legacy_preview, 30.0).expect("legacy preview");
        assert!(find_cached(&config, &track).is_none());

        let resolved = resolve(&config, &track).expect("resolve Apple track");
        assert_ne!(
            resolved.path, legacy_preview,
            "stale 30 s preview cache must be bypassed for full playback"
        );
        let actual = crate::audio::probe_audio(&resolved.path)
            .expect("probe resolved audio")
            .duration_secs
            .unwrap_or(0.0);
        assert!(
            actual > PREVIEW_MAX_SECS,
            "resolved only preview-length audio ({actual:.1}s, catalogue {catalogue_secs:.1}s) — \
             full yt-dlp match must take priority"
        );
        // The first YouTube match may be a radio edit / live version, which is
        // still full playback (e.g. 4:08 edit of a 6:09 album track). Assert it
        // is not a preview, not that it is byte-for-byte the catalogue length.
        assert!(
            actual >= catalogue_secs * 0.5,
            "resolved audio too short to be the full song: {actual:.1}s vs catalogue {catalogue_secs:.1}s"
        );
        assert!(
            !is_preview_marker(&resolved.path) && !looks_like_preview(&resolved.path, &track),
            "resolved path must not be a preview cache entry: {:?}",
            resolved.path
        );
    }
}
