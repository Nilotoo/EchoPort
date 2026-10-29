//! Timed lyrics: local `.lrc` sidecars first, then the public LRCLIB API.
//!
//! No account, no scraping: LRCLIB is a public lyrics database with a JSON
//! API. Requests are only made when lyrics are enabled for the current track.

use serde::Deserialize;

use crate::model::Track;
use crate::net::Http;

const LRCLIB_GET: &str = "https://lrclib.net/api/get";
const LRCLIB_SEARCH: &str = "https://lrclib.net/api/search";

#[derive(Clone, Debug, PartialEq)]
pub struct LyricLine {
    pub time_secs: f64,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LyricsData {
    pub lines: Vec<LyricLine>,
    pub plain: Vec<String>,
    pub source: String,
}

impl LyricsData {
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty() && self.plain.is_empty()
    }

    /// Index of the line active at `position`, if any.
    pub fn active_line(&self, position: f64) -> Option<usize> {
        if self.lines.is_empty() {
            return None;
        }
        let mut active = None;
        for (index, line) in self.lines.iter().enumerate() {
            if line.time_secs <= position + 0.05 {
                active = Some(index);
            } else {
                break;
            }
        }
        active
    }
}

#[derive(Debug, Deserialize)]
struct LrclibResponse {
    #[serde(default, rename = "syncedLyrics")]
    synced_lyrics: Option<String>,
    #[serde(default, rename = "plainLyrics")]
    plain_lyrics: Option<String>,
    #[serde(default)]
    instrumental: bool,
}

/// Fetch lyrics for a track on a worker thread. Local `.lrc` sidecars take
/// priority; otherwise LRCLIB is queried anonymously.
pub fn fetch(http: &Http, track: &Track) -> Result<LyricsData, String> {
    if let Some(path) = track.local_path.as_deref() {
        let sidecar = path.with_extension("lrc");
        if sidecar.is_file() {
            if let Ok(text) = std::fs::read_to_string(&sidecar) {
                let data = parse_lrc(&text);
                if !data.is_empty() {
                    let mut data = data;
                    data.source = "local .lrc".to_owned();
                    return Ok(data);
                }
            }
        }
    }
    fetch_lrclib(http, track)
}

fn fetch_lrclib(http: &Http, track: &Track) -> Result<LyricsData, String> {
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    query.append_pair("artist_name", track.display_artist());
    query.append_pair("track_name", &track.title);
    if let Some(album) = track.album.as_deref() {
        query.append_pair("album_name", album);
    }
    if let Some(duration) = track.duration_secs.filter(|duration| *duration > 0.0) {
        query.append_pair("duration", &format!("{duration:.0}"));
    }
    let get_url = format!("{LRCLIB_GET}?{}", query.finish());
    if let Ok(response) =
        http.get_json::<LrclibResponse>(&get_url, &[("Accept", "application/json")])
    {
        if let Some(data) = records_to_data(std::slice::from_ref(&response)) {
            if data.lines.len() >= 3 || data.plain.len() >= 5 {
                return Ok(data);
            }
        }
    }
    // Exact lookup can return a stub (e.g. a one-line "probe"). Fall back to
    // search and choose the record with the most timed lines.
    let search_url = format!(
        "{LRCLIB_SEARCH}?track_name={}&artist_name={}",
        url::form_urlencoded::byte_serialize(track.title.as_bytes()).collect::<String>(),
        url::form_urlencoded::byte_serialize(track.display_artist().as_bytes()).collect::<String>(),
    );
    let records: Vec<LrclibResponse> = http
        .get_json(&search_url, &[("Accept", "application/json")])
        .map_err(|err| format!("lyrics service: {err}"))?;
    records_to_data(&records).ok_or_else(|| "lyrics service: no matching lyrics".to_owned())
}

fn records_to_data(records: &[LrclibResponse]) -> Option<LyricsData> {
    let mut best: Option<LyricsData> = None;
    for record in records {
        if record.instrumental {
            continue;
        }
        let mut data = record
            .synced_lyrics
            .as_deref()
            .map(parse_lrc)
            .unwrap_or_default();
        if data.lines.is_empty() {
            data.plain = record
                .plain_lyrics
                .as_deref()
                .unwrap_or_default()
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_owned)
                .collect();
        }
        if data.lines.is_empty() && data.plain.is_empty() {
            continue;
        }
        data.source = "LRCLIB".to_owned();
        let replace = match &best {
            None => true,
            Some(current) => {
                data.lines.len() > current.lines.len()
                    || (data.lines.len() == current.lines.len()
                        && data.plain.len() > current.plain.len())
            }
        };
        if replace {
            best = Some(data);
        }
    }
    best
}

/// Parse LRC timestamps. Accepts `[mm:ss]`, `[mm:ss.xx]` and multiple tags per
/// line; metadata tags (`[ar:...]`, `[ti:...]`) are ignored.
pub fn parse_lrc(text: &str) -> LyricsData {
    let mut lines = Vec::new();
    for raw in text.lines() {
        let mut rest = raw.trim();
        let mut times = Vec::new();
        while rest.starts_with('[') {
            let Some(end) = rest.find(']') else { break };
            let tag = &rest[1..end];
            if let Some(seconds) = parse_timestamp(tag) {
                times.push(seconds);
            }
            rest = rest[end + 1..].trim_start();
        }
        if times.is_empty() || rest.is_empty() {
            continue;
        }
        for time in times {
            lines.push(LyricLine {
                time_secs: time,
                text: rest.to_owned(),
            });
        }
    }
    lines.sort_by(|a, b| a.time_secs.total_cmp(&b.time_secs));
    lines.dedup_by(|a, b| (a.time_secs - b.time_secs).abs() < 0.001 && a.text == b.text);
    LyricsData {
        lines,
        plain: Vec::new(),
        source: String::new(),
    }
}

fn parse_timestamp(tag: &str) -> Option<f64> {
    let mut parts = tag.split(':');
    let minutes: f64 = parts.next()?.trim().parse().ok()?;
    let seconds: f64 = parts.next()?.trim().parse().ok()?;
    if parts.next().is_some() || !minutes.is_finite() || !seconds.is_finite() {
        return None;
    }
    Some(minutes * 60.0 + seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_timestamps_and_skips_metadata() {
        let text = "[ar:Rick Astley]\n[00:01.20]Never gonna give you up\n[00:05]Never gonna let you down\n";
        let data = parse_lrc(text);
        assert_eq!(data.lines.len(), 2);
        assert!((data.lines[0].time_secs - 1.2).abs() < 0.01);
        assert_eq!(data.lines[1].text, "Never gonna let you down");
    }

    #[test]
    fn active_line_tracks_position() {
        let data = LyricsData {
            lines: vec![
                LyricLine {
                    time_secs: 0.0,
                    text: "a".to_owned(),
                },
                LyricLine {
                    time_secs: 5.0,
                    text: "b".to_owned(),
                },
            ],
            ..Default::default()
        };
        assert_eq!(data.active_line(0.0), Some(0));
        assert_eq!(data.active_line(4.9), Some(0));
        assert_eq!(data.active_line(5.0), Some(1));
    }
}
