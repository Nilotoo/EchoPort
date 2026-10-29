//! Internet Archive audio search.
//!
//! Internet Archive is a legal public library with keyless JSON APIs. Search
//! results are metadata; playback resolves a public audio file from the item's
//! metadata endpoint (no account, no DRM, no yt-dlp required).

use serde::Deserialize;

use crate::model::{Platform, Track};
use crate::net::Http;

use super::util;

const SEARCH_URL: &str = "https://archive.org/advancedsearch.php";
const METADATA_URL: &str = "https://archive.org/metadata/";
const DOWNLOAD_URL: &str = "https://archive.org/download/";

#[derive(Debug, Default, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    response: Option<SearchBody>,
}

#[derive(Debug, Default, Deserialize)]
struct SearchBody {
    #[serde(default)]
    docs: Vec<serde_json::Value>,
}

#[derive(Debug, Default, Deserialize)]
struct MetadataResponse {
    #[serde(default)]
    files: Vec<ArchiveFile>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct ArchiveFile {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    source: Option<String>,
}

/// Search public audio items. One HTTP request; no per-result metadata calls.
pub fn search(http: &Http, query: &str, limit: usize) -> Result<Vec<Track>, String> {
    let limit = limit.clamp(1, 50);
    let q = util::pct(&format!("({query}) AND mediatype:(audio)"));
    let url = format!(
        "{SEARCH_URL}?q={q}&fl%5B%5D=identifier&fl%5B%5D=title&fl%5B%5D=creator&fl%5B%5D=year&rows={limit}&page=1&output=json"
    );
    let response: SearchResponse = http
        .get_json(&url, &[])
        .map_err(|err| format!("Internet Archive search failed: {err}"))?;
    let docs = response.response.map(|body| body.docs).unwrap_or_default();
    let mut tracks = Vec::new();
    let mut seen = Vec::new();
    for doc in docs {
        let Some(identifier) = doc.get("identifier").and_then(|value| value.as_str()) else {
            continue;
        };
        if seen.contains(&identifier.to_owned()) {
            continue;
        }
        seen.push(identifier.to_owned());
        let title = doc
            .get("title")
            .and_then(|value| match value {
                serde_json::Value::String(text) => Some(text.as_str()),
                serde_json::Value::Array(items) => items.first().and_then(|item| item.as_str()),
                _ => None,
            })
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .unwrap_or("Internet Archive audio");
        let creator = doc
            .get("creator")
            .and_then(|value| match value {
                serde_json::Value::String(text) => Some(text.as_str()),
                serde_json::Value::Array(items) => items.first().and_then(|item| item.as_str()),
                _ => None,
            })
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .unwrap_or("Internet Archive");
        let mut track = Track::new(Platform::InternetArchive, identifier, title, creator);
        track = track.with_page(format!("https://archive.org/details/{identifier}"));
        track = track.with_art(format!("https://archive.org/services/img/{identifier}"));
        if let Some(year) = doc
            .get("year")
            .and_then(|value| value.as_str())
            .and_then(|value| value.parse::<u16>().ok())
        {
            track = track.with_year(year);
        }
        tracks.push(track);
        if tracks.len() >= limit {
            break;
        }
    }
    Ok(tracks)
}

/// Resolve the best public audio file URL for an Archive item.
pub fn audio_file(http: &Http, track: &Track) -> Result<(String, String), String> {
    let identifier = track
        .page_url
        .as_deref()
        .and_then(|url| url.rsplit('/').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Internet Archive item has no identifier.".to_owned())?;
    let response: MetadataResponse = http
        .get_json(&format!("{METADATA_URL}{identifier}"), &[])
        .map_err(|err| format!("Internet Archive metadata failed: {err}"))?;
    let mut candidates: Vec<(u8, &str, &str)> = Vec::new();
    for file in &response.files {
        let Some(name) = file.name.as_deref() else {
            continue;
        };
        let format = file
            .format
            .as_deref()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if format.contains("metadata") || format.contains("thumbnail") || format.contains("image") {
            continue;
        }
        let rank = if format.contains("vbr mp3") || format == "mp3" {
            0
        } else if format.contains("128kbps mp3") || format.contains("mp3") {
            1
        } else if format.contains("flac") {
            2
        } else if format.contains("ogg vorbis") || format.contains("ogg") {
            3
        } else {
            continue;
        };
        if file.source.as_deref() == Some("derivative") && rank == 0 {
            // Prefer originals only when their format is already MP3.
        }
        if let Some(extension) = name.rsplit('.').next().filter(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "mp3" | "flac" | "ogg" | "m4a"
            )
        }) {
            candidates.push((rank, name, extension));
        }
    }
    candidates.sort_by_key(|(rank, _, _)| *rank);
    let Some((_, name, extension)) = candidates.first() else {
        return Err("Internet Archive item has no public audio file.".to_owned());
    };
    let encoded_name = name.split('/').map(util::pct).collect::<Vec<_>>().join("/");
    Ok((
        format!("{DOWNLOAD_URL}{identifier}/{encoded_name}"),
        extension.to_ascii_lowercase(),
    ))
}
