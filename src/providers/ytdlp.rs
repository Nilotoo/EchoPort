//! Minimal `yt-dlp` metadata helper used for link imports.
//!
//! The resolver owns playback and searches; this module only reads the public
//! `-J` JSON for a pasted page URL (YouTube, SoundCloud, Bilibili, Netease…)
//! and turns it into a `Track`. It spawns the process directly because the
//! resolver API does not expose a per-URL metadata call.

use std::io::Read;
use std::path::Path;
use std::process::Stdio;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::process;

use crate::model::{Platform, Track};
use crate::resolver::ResolveConfig;

use super::util;

/// Hard cap for one metadata extraction; normal runs take 1–4 s.
const METADATA_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Default)]
pub struct Metadata {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub duration_secs: Option<f64>,
    pub thumbnail: Option<String>,
    pub webpage_url: Option<String>,
    /// Best-effort release/upload date as `YYYY-MM-DD`.
    pub release_date: Option<String>,
}

/// Import a pasted page URL as a `Track` via `yt-dlp -J`.
pub fn import(config: &ResolveConfig, url: &str, platform: Platform) -> Result<Track, String> {
    let metadata = fetch(config, url)?;
    Ok(track_from_metadata(platform, &metadata, url))
}

/// Run `yt-dlp -J --no-playlist <url>` and parse the metadata.
pub fn fetch(config: &ResolveConfig, url: &str) -> Result<Metadata, String> {
    let tool = config.ytdlp().ok_or_else(|| {
        "yt-dlp is not installed or could not be found — install yt-dlp (or set its path in \
         Settings) to import this link."
            .to_owned()
    })?;
    let json = run_json(&tool, url)?;
    metadata_from_json(&json)
}

fn run_json(tool: &Path, url: &str) -> Result<String, String> {
    let mut child = process::quiet(tool)
        .args(["-J", "--no-playlist", "--no-warnings", "--no-progress"])
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("Could not start yt-dlp: {err}"))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Could not read yt-dlp output.".to_owned())?;
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut stdout = stdout;
        let mut buffer = Vec::new();
        let result = stdout.read_to_end(&mut buffer).map(|_| buffer);
        let _ = tx.send(result);
    });

    let deadline = Instant::now() + METADATA_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = reader.join();
                    return Err(
                        "yt-dlp took too long to answer — check the link or try again.".to_owned(),
                    );
                }
                std::thread::sleep(Duration::from_millis(40));
            }
            Err(err) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err(format!("yt-dlp failed: {err}"));
            }
        }
    };

    // The process has exited, so reading the (small, warning-free) stderr is safe.
    let mut stderr = String::new();
    if let Some(mut handle) = child.stderr.take() {
        let _ = handle.read_to_string(&mut stderr);
    }
    let stdout = match rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(err)) => return Err(format!("Could not read yt-dlp output: {err}")),
        Err(_) => return Err("yt-dlp produced no output.".to_owned()),
    };
    let _ = reader.join();
    let text = String::from_utf8_lossy(&stdout).into_owned();

    if !status.success() || text.trim().is_empty() {
        let detail = stderr
            .lines()
            .find(|line| line.contains("ERROR"))
            .unwrap_or_default()
            .trim()
            .trim_start_matches("ERROR:")
            .trim()
            .to_owned();
        if detail.is_empty() {
            return Err(format!(
                "yt-dlp could not read this link (exit code {}).",
                status.code().unwrap_or(-1)
            ));
        }
        return Err(format!("yt-dlp could not read this link: {detail}"));
    }
    Ok(text)
}

/// Parse the JSON produced by `yt-dlp -J`. Pure: used by unit tests.
pub fn metadata_from_json(json: &str) -> Result<Metadata, String> {
    let root: Value = serde_json::from_str(json)
        .map_err(|_| "yt-dlp returned unreadable metadata.".to_owned())?;
    // `--no-playlist` normally yields a video object; tolerate a one-entry
    // playlist just in case an extractor returns one anyway.
    let entry = if root.get("entries").is_some() && root.get("id").is_none() {
        root.get("entries")
            .and_then(Value::as_array)
            .and_then(|entries| entries.iter().find(|entry| entry.is_object()))
            .ok_or_else(|| "yt-dlp returned an empty playlist for this link.".to_owned())?
    } else {
        &root
    };

    let id = util::get_str(entry, "/id").unwrap_or_default().to_owned();
    let title = util::get_str(entry, "/title")
        .or_else(|| util::get_str(entry, "/fulltitle"))
        .map(util::tidy_text)
        .filter(|title| !title.is_empty());
    let Some(title) = title else {
        return Err("yt-dlp returned metadata without a title for this link.".to_owned());
    };
    let artist = util::get_str(entry, "/artist")
        .or_else(|| util::get_str(entry, "/uploader"))
        .or_else(|| util::get_str(entry, "/channel"))
        .or_else(|| util::get_str(entry, "/creator"))
        .unwrap_or_default()
        .to_owned();
    let duration_secs = util::get_f64(entry, "/duration")
        .filter(|secs| secs.is_finite() && (0.5..=4.0 * 3600.0).contains(secs));
    let thumbnail = util::get_str(entry, "/thumbnail")
        .and_then(util::https_url)
        .or_else(|| {
            let thumbs = entry.get("thumbnails")?.as_array()?;
            thumbs
                .iter()
                .rev()
                .filter_map(|thumb| util::get_str(thumb, "/url"))
                .find_map(util::https_url)
        });
    let webpage_url = util::get_str(entry, "/webpage_url")
        .or_else(|| util::get_str(entry, "/original_url"))
        .and_then(util::https_url);
    // Prefer a real release date over the upload date; both are best effort.
    let release_date = util::get_str(entry, "/release_date")
        .and_then(util::date_from_iso)
        .or_else(|| {
            util::get_i64(entry, "/release_timestamp")
                .and_then(|secs| util::date_from_unix(secs, 0))
        })
        .or_else(|| util::get_str(entry, "/upload_date").and_then(util::date_from_iso))
        .or_else(|| {
            util::get_i64(entry, "/timestamp").and_then(|secs| util::date_from_unix(secs, 0))
        });

    Ok(Metadata {
        id,
        title,
        artist,
        duration_secs,
        thumbnail,
        webpage_url,
        release_date,
    })
}

fn track_from_metadata(platform: Platform, metadata: &Metadata, requested_url: &str) -> Track {
    let id_source = metadata
        .webpage_url
        .clone()
        .filter(|url| !url.is_empty())
        .or_else(|| (!metadata.id.is_empty()).then(|| metadata.id.clone()))
        .unwrap_or_else(|| requested_url.to_owned());
    let mut track = Track::new(platform, &id_source, &metadata.title, &metadata.artist);
    if let Some(secs) = metadata.duration_secs {
        track = track.with_duration(secs);
    }
    if let Some(art) = &metadata.thumbnail {
        track = track.with_art(art.clone());
    }
    let page = metadata
        .webpage_url
        .clone()
        .unwrap_or_else(|| requested_url.to_owned());
    track = track.with_page(page);
    if let Some(date) = &metadata.release_date {
        track = track.with_release_date(date.clone());
    }
    track
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIDEO_JSON: &str = r#"{
        "id": "dQw4w9WgXcQ",
        "title": "Rick Astley - Never Gonna Give You Up (Official Video)",
        "uploader": "Rick Astley",
        "channel": "Rick Astley",
        "duration": 213.0,
        "thumbnail": "https://i.ytimg.com/vi_webp/dQw4w9WgXcQ/maxresdefault.webp",
        "webpage_url": "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
        "upload_date": "20091025",
        "extractor": "youtube",
        "_type": "video"
    }"#;

    const THUMBNAILS_JSON: &str = r#"{
        "id": "abc",
        "title": "  Song &amp; Dance  ",
        "artist": null,
        "uploader": null,
        "channel": "Some Channel",
        "duration": "180.5",
        "thumbnails": [
            { "url": "http://example.com/small.jpg", "width": 120 },
            { "url": "http://example.com/large.jpg", "width": 1280 }
        ]
    }"#;

    const PLAYLIST_JSON: &str = r#"{
        "_type": "playlist",
        "entries": [
            { "id": "one", "title": "First", "duration": 60.0, "webpage_url": "https://example.com/one" },
            { "id": "two", "title": "Second" }
        ]
    }"#;

    #[test]
    fn video_metadata_is_parsed_and_builds_a_track() {
        let metadata = metadata_from_json(VIDEO_JSON).expect("metadata");
        assert_eq!(
            metadata.title,
            "Rick Astley - Never Gonna Give You Up (Official Video)"
        );
        assert_eq!(metadata.artist, "Rick Astley");
        assert_eq!(metadata.duration_secs, Some(213.0));
        assert_eq!(
            metadata.webpage_url.as_deref(),
            Some("https://www.youtube.com/watch?v=dQw4w9WgXcQ")
        );
        let track =
            track_from_metadata(Platform::YouTube, &metadata, "https://youtu.be/dQw4w9WgXcQ");
        assert_eq!(track.platform, Platform::YouTube);
        assert_eq!(track.title, metadata.title);
        assert_eq!(track.artist, "Rick Astley");
        assert_eq!(track.duration_secs, Some(213.0));
        assert_eq!(
            track.page_url.as_deref(),
            Some("https://www.youtube.com/watch?v=dQw4w9WgXcQ")
        );
        assert_eq!(track.release_date.as_deref(), Some("2009-10-25"));
        assert!(
            track.id.starts_with("youtube:"),
            "ids are platform prefixed: {}",
            track.id
        );
    }

    #[test]
    fn thumbnails_array_fallback_and_text_tidying() {
        let metadata = metadata_from_json(THUMBNAILS_JSON).expect("metadata");
        assert_eq!(metadata.title, "Song & Dance");
        assert_eq!(metadata.artist, "Some Channel");
        assert_eq!(metadata.duration_secs, Some(180.5));
        assert_eq!(
            metadata.thumbnail.as_deref(),
            Some("https://example.com/large.jpg")
        );
    }

    #[test]
    fn playlist_json_uses_the_first_entry() {
        let metadata = metadata_from_json(PLAYLIST_JSON).expect("metadata");
        assert_eq!(metadata.id, "one");
        assert_eq!(metadata.title, "First");
        assert_eq!(metadata.duration_secs, Some(60.0));
    }

    #[test]
    fn release_dates_prefer_release_over_upload_and_accept_timestamps() {
        let with_release = r#"{
            "id": "x",
            "title": "Song",
            "release_date": "20210210",
            "upload_date": "20210212"
        }"#;
        assert_eq!(
            metadata_from_json(with_release)
                .expect("metadata")
                .release_date
                .as_deref(),
            Some("2021-02-10"),
            "release_date wins over upload_date"
        );
        let with_release_ts = r#"{ "id": "x", "title": "Song", "release_timestamp": 1612915200 }"#;
        assert_eq!(
            metadata_from_json(with_release_ts)
                .expect("metadata")
                .release_date
                .as_deref(),
            Some("2021-02-10")
        );
        let with_upload_ts = r#"{ "id": "x", "title": "Song", "timestamp": 1612915200 }"#;
        assert_eq!(
            metadata_from_json(with_upload_ts)
                .expect("metadata")
                .release_date
                .as_deref(),
            Some("2021-02-10")
        );
        let without_date = r#"{ "id": "x", "title": "Song" }"#;
        assert!(metadata_from_json(without_date)
            .expect("metadata")
            .release_date
            .is_none());
    }

    #[test]
    fn missing_title_is_an_error() {
        assert!(metadata_from_json(r#"{ "id": "x" }"#).is_err());
        assert!(metadata_from_json("not json").is_err());
        assert!(metadata_from_json(r#"{ "_type": "playlist", "entries": [] }"#).is_err());
    }
}
