//! Download pipeline built on yt-dlp (owned by the audio-core workstream).
//!
//! Downloads always go through yt-dlp itself (never through a resolved media
//! URL, see `resolver`), which can also convert the audio to the user selected
//! format with ffmpeg. Progress lines (`[download] 42.3%`) are parsed into
//! `DownloadProgress` updates while the job runs on a worker thread.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::model::{DownloadFormat, DownloadProgress, Track};
use crate::process;
use crate::resolver::{self, ResolveConfig};

/// Upper bound for a single download (large FLAC tracks / slow connections).
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Clone, Debug)]
pub struct DownloadOptions {
    pub dir: PathBuf,
    pub format: DownloadFormat,
    /// Embed cover art / tags when the container supports it.
    pub embed_metadata: bool,
}

/// Download `track` into `options.dir` as `options.format`, reporting progress.
///
/// Returns the final on-disk path. Blocking: call from a worker thread.
pub fn download_track(
    track: &Track,
    config: &ResolveConfig,
    options: &DownloadOptions,
    progress: &mut dyn FnMut(DownloadProgress),
) -> Result<PathBuf, String> {
    if track.local_path.is_some() {
        return Err("local tracks are already on disk".to_owned());
    }
    let ytdlp = config.ytdlp().ok_or_else(|| {
        "yt-dlp is not available; install yt-dlp or set its path in Settings".to_owned()
    })?;
    if config.ffmpeg().is_none() {
        return Err(
            "ffmpeg is not available; it is required to convert downloaded audio".to_owned(),
        );
    }
    let sources = resolver::sources_for(track);
    if sources.is_empty() {
        return Err(format!("no download source known for {}", track.title));
    }
    std::fs::create_dir_all(&options.dir)
        .map_err(|err| format!("cannot create download folder: {err}"))?;

    // Keep the process wide yt-dlp limit shared with playback resolution.
    let _slot = resolver::YtdlpSlot::acquire();
    let mut last_error = String::new();
    for source in &sources {
        progress(DownloadProgress {
            fraction: Some(0.0),
            stage: "starting".to_owned(),
        });
        match run_download(&ytdlp, track, config, options, source.argument(), progress) {
            Ok(path) => {
                progress(DownloadProgress {
                    fraction: Some(1.0),
                    stage: "done".to_owned(),
                });
                return Ok(path);
            }
            Err(err) => {
                log::warn!(
                    "download attempt failed for {} ({}): {err}",
                    track.title,
                    source.argument()
                );
                last_error = err;
            }
        }
    }
    Err(last_error)
}

/// Build the yt-dlp argument list (without the binary and the source argument).
fn download_args(
    format: DownloadFormat,
    embed_metadata: bool,
    output_template: &Path,
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "-x".to_owned(),
        "--audio-format".to_owned(),
        format.key().to_owned(),
        "--audio-quality".to_owned(),
        "0".to_owned(),
        "--newline".to_owned(),
        "--no-playlist".to_owned(),
        "--no-warnings".to_owned(),
        "--no-mtime".to_owned(),
        "--retries".to_owned(),
        "3".to_owned(),
        "--fragment-retries".to_owned(),
        "3".to_owned(),
    ];
    if embed_metadata {
        args.push("--embed-metadata".to_owned());
        args.push("--embed-thumbnail".to_owned());
    }
    args.push("-o".to_owned());
    args.push(output_template.to_string_lossy().into_owned());
    args
}

fn run_download(
    ytdlp: &Path,
    track: &Track,
    _config: &ResolveConfig,
    options: &DownloadOptions,
    source: &str,
    progress: &mut dyn FnMut(DownloadProgress),
) -> Result<PathBuf, String> {
    let temp_dir = options.dir.join(format!(
        ".echoport-tmp-{}-{}",
        track.cache_key(),
        unique_suffix()
    ));
    std::fs::create_dir_all(&temp_dir)
        .map_err(|err| format!("cannot create temporary download folder: {err}"))?;
    let guard = TempDirGuard::new(temp_dir.clone());

    // The [%(id)s] component keeps different tracks from overwriting each other;
    // yt-dlp sanitises filesystem-unsafe characters while keeping non-ASCII
    // titles intact.
    let output_template = temp_dir.join("%(title)s [%(id)s].%(ext)s");
    let mut command = process::quiet(ytdlp);
    command.args(download_args(
        options.format,
        options.embed_metadata,
        &output_template,
    ));
    command.arg(source);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|err| format!("cannot start yt-dlp: {err}"))?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    enum Line {
        Out(String),
        Err(String),
    }
    let (line_tx, line_rx) = mpsc::channel::<Line>();
    if let Some(stdout) = stdout {
        let sender = line_tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if sender.send(Line::Out(line)).is_err() {
                    break;
                }
            }
        });
    }
    if let Some(stderr) = stderr {
        let sender = line_tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if sender.send(Line::Err(line)).is_err() {
                    break;
                }
            }
        });
    }
    drop(line_tx);

    let deadline = Instant::now() + DOWNLOAD_TIMEOUT;
    let mut timed_out = false;
    let mut stderr_tail: Vec<String> = Vec::new();
    let mut stage = String::from("downloading");
    let status = loop {
        // Drain whatever yt-dlp printed since the last tick.
        loop {
            match line_rx.try_recv() {
                Ok(Line::Out(line)) => {
                    if let Some(next_stage) = stage_for_line(&line) {
                        stage = next_stage.to_owned();
                    }
                    if let Some(fraction) = parse_progress_line(&line) {
                        progress(DownloadProgress {
                            fraction: Some(fraction),
                            stage: stage.clone(),
                        });
                    } else if let Some(next_stage) = stage_for_line(&line) {
                        progress(DownloadProgress {
                            fraction: None,
                            stage: next_stage.to_owned(),
                        });
                    }
                }
                Ok(Line::Err(line)) => {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        stderr_tail.push(trimmed.to_owned());
                        if stderr_tail.len() > 12 {
                            stderr_tail.remove(0);
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => break,
            }
        }

        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {}
            Err(err) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("cannot wait for yt-dlp: {err}"));
            }
        }
        if Instant::now() >= deadline {
            timed_out = true;
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(50));
    };

    // Drain the remaining lines so the reader threads can finish.
    while let Ok(line) = line_rx.recv_timeout(Duration::from_millis(250)) {
        if let Line::Err(line) = line {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                stderr_tail.push(trimmed.to_owned());
                if stderr_tail.len() > 12 {
                    stderr_tail.remove(0);
                }
            }
        }
    }

    if timed_out {
        return Err("download timed out".to_owned());
    }
    if !status.map(|status| status.success()).unwrap_or(false) {
        let detail = stderr_tail
            .last()
            .cloned()
            .unwrap_or_else(|| "yt-dlp failed".to_owned());
        return Err(format!("download failed: {detail}"));
    }

    let downloaded = resolver::find_downloaded_file(&temp_dir)
        .ok_or_else(|| "yt-dlp finished but produced no audio file".to_owned())?;
    let final_path = unique_destination(&options.dir, &downloaded)?;
    move_file(&downloaded, &final_path)?;
    drop(guard);
    Ok(final_path)
}

/// Parse a yt-dlp progress line like `[download]  42.3% of 3.50MiB ...`.
fn parse_progress_line(line: &str) -> Option<f32> {
    let rest = line.trim_start().strip_prefix("[download]")?;
    let percent_index = rest.find('%')?;
    let number = rest[..percent_index].trim().split_whitespace().last()?;
    let value: f32 = number.parse().ok()?;
    if !value.is_finite() {
        return None;
    }
    Some((value / 100.0).clamp(0.0, 1.0))
}

/// Human readable stage for a yt-dlp output line.
fn stage_for_line(line: &str) -> Option<&'static str> {
    let line = line.trim_start();
    if line.starts_with("[ExtractAudio]") {
        Some("converting")
    } else if line.starts_with("[Metadata]") {
        Some("tagging")
    } else if line.starts_with("[EmbedThumbnail]") {
        Some("embedding cover")
    } else if line.starts_with("[download] Destination:") {
        Some("downloading")
    } else if line.starts_with("[download]") && line.contains('%') {
        Some("downloading")
    } else {
        None
    }
}

/// Keep Unicode titles readable but replace path separators / control chars.
fn sanitize_filename(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|character| {
            if character.is_control() || matches!(character, '/' | '\\' | ':' | '\0') {
                '_'
            } else {
                character
            }
        })
        .collect();
    out = out.trim().trim_end_matches('.').trim().to_owned();
    if out.is_empty() {
        out = "track".to_owned();
    }
    if out.chars().count() > 120 {
        out = out.chars().take(120).collect();
    }
    out
}

/// Final path in the download folder, avoiding overwrites (` (2)`, ` (3)`, ...).
fn unique_destination(dir: &Path, downloaded: &Path) -> Result<PathBuf, String> {
    let stem = downloaded
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "track".to_owned());
    let extension = downloaded
        .extension()
        .map(|extension| extension.to_string_lossy().into_owned())
        .unwrap_or_else(|| "mp3".to_owned());
    let stem = sanitize_filename(&stem);
    let mut candidate = dir.join(format!("{stem}.{extension}"));
    let mut counter = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{stem} ({counter}).{extension}"));
        counter += 1;
        if counter > 1_000 {
            return Err("cannot find a free download file name".to_owned());
        }
    }
    Ok(candidate)
}

fn move_file(source: &Path, destination: &Path) -> Result<(), String> {
    match std::fs::rename(source, destination) {
        Ok(()) => Ok(()),
        Err(rename_error) => {
            std::fs::copy(source, destination).map_err(|copy_error| {
                format!(
                    "cannot move download into place: {rename_error}; copy fallback: {copy_error}"
                )
            })?;
            let _ = std::fs::remove_file(source);
            Ok(())
        }
    }
}

struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
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
// Tests (no network, no tools required)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Platform;

    fn options(dir: &Path, format: DownloadFormat) -> DownloadOptions {
        DownloadOptions {
            dir: dir.to_path_buf(),
            format,
            embed_metadata: true,
        }
    }

    #[test]
    fn parses_download_progress() {
        let fraction = parse_progress_line("[download]  42.3% of  3.50MiB at 1.00MiB/s ETA 00:03")
            .expect("42.3%");
        assert!((fraction - 0.423).abs() < 1e-4, "parsed {fraction}");
        assert_eq!(
            parse_progress_line("[download] 100% of 3.50MiB in 00:03"),
            Some(1.0)
        );
        assert_eq!(
            parse_progress_line("[download]   1.0% of ~   3.5MiB at  Unknown B/s ETA Unknown"),
            Some(0.01)
        );
        assert_eq!(
            parse_progress_line("[download] Destination: /tmp/a.m4a"),
            None
        );
        assert_eq!(
            parse_progress_line("[ExtractAudio] Destination: /tmp/a.mp3"),
            None
        );
        assert_eq!(parse_progress_line("random line"), None);
    }

    #[test]
    fn recognises_stages() {
        assert_eq!(
            stage_for_line("[ExtractAudio] Destination: /tmp/a.mp3"),
            Some("converting")
        );
        assert_eq!(
            stage_for_line("[Metadata] Adding metadata to \"/tmp/a.mp3\""),
            Some("tagging")
        );
        assert_eq!(
            stage_for_line("[EmbedThumbnail] Adding thumbnail to \"/tmp/a.mp3\""),
            Some("embedding cover")
        );
        assert_eq!(
            stage_for_line("[download] Destination: /tmp/a.m4a"),
            Some("downloading")
        );
        assert_eq!(stage_for_line("noise"), None);
    }

    #[test]
    fn download_arguments_include_expected_flags() {
        let template = Path::new("/tmp/out/%(title)s [%(id)s].%(ext)s");
        let args = download_args(DownloadFormat::Flac, true, template);
        assert!(args.windows(2).any(|pair| pair == ["-x", "--audio-format"]));
        assert!(args.contains(&"flac".to_owned()));
        assert!(args.contains(&"--embed-metadata".to_owned()));
        assert!(args.contains(&"--embed-thumbnail".to_owned()));
        assert!(args.contains(&"--no-playlist".to_owned()));
        assert!(args.contains(&"--newline".to_owned()));
        assert!(args.contains(&"--retries".to_owned()));
        let template_arg = template.to_string_lossy().into_owned();
        assert!(args.contains(&template_arg));
        assert!(
            template_arg.contains("%(id)s"),
            "output template keeps the id"
        );

        let plain = download_args(DownloadFormat::Mp3, false, template);
        assert!(!plain.contains(&"--embed-metadata".to_owned()));
        assert!(!plain.contains(&"--embed-thumbnail".to_owned()));
        assert!(plain.contains(&"0".to_owned()), "best audio quality");
    }

    #[test]
    fn sanitizes_file_names_but_keeps_unicode() {
        assert_eq!(sanitize_filename("Bohemian Rhapsody"), "Bohemian Rhapsody");
        assert_eq!(sanitize_filename("夜に駆ける / 少女"), "夜に駆ける _ 少女");
        assert_eq!(sanitize_filename("a\\b:c"), "a_b_c");
        assert_eq!(sanitize_filename("  ..  "), "track");
        assert_eq!(sanitize_filename(""), "track");
        assert_eq!(sanitize_filename("line\nbreak"), "line_break");
    }

    #[test]
    fn unique_destination_avoids_overwrites() {
        let temp = tempfile::tempdir().expect("tempdir");
        let download_dir = temp.path().join("downloads");
        let staging_dir = temp.path().join("staging");
        std::fs::create_dir_all(&download_dir).expect("download dir");
        std::fs::create_dir_all(&staging_dir).expect("staging dir");
        let downloaded = staging_dir.join("Song [abc123].mp3");
        std::fs::write(&downloaded, b"x").expect("write downloaded");

        let first = unique_destination(&download_dir, &downloaded).expect("destination");
        assert_eq!(
            first.file_name().unwrap().to_string_lossy(),
            "Song [abc123].mp3"
        );
        std::fs::write(&first, b"x").expect("write first");
        let second = unique_destination(&download_dir, &downloaded).expect("second destination");
        assert_eq!(
            second.file_name().unwrap().to_string_lossy(),
            "Song [abc123] (2).mp3"
        );
    }

    #[test]
    fn download_track_rejects_local_tracks_and_missing_tools() {
        let temp = tempfile::tempdir().expect("tempdir");
        let local = Track::from_local_path(temp.path().join("a.mp3"));
        // Explicit bogus paths so the test never discovers (or runs) a real tool.
        let config = ResolveConfig {
            ytdlp: Some(PathBuf::from("/nonexistent/echoport-yt-dlp")),
            ffmpeg: Some(PathBuf::from("/nonexistent/echoport-ffmpeg")),
            cache_dir: temp.path().join("cache"),
        };
        let mut no_progress = |_progress: DownloadProgress| {};
        let result = download_track(
            &local,
            &config,
            &options(temp.path(), DownloadFormat::Mp3),
            &mut no_progress,
        );
        assert!(result.is_err());

        let remote = Track::new(Platform::YouTube, "abc", "Song", "Artist");
        let result = download_track(
            &remote,
            &config,
            &options(temp.path(), DownloadFormat::Mp3),
            &mut no_progress,
        );
        assert!(result.is_err(), "missing yt-dlp must be reported");
    }
}
