//! Spotify link import.
//!
//! Anonymous Spotify search does not exist (the token endpoints answer 403 and
//! the web search page is client rendered), and full playback is DRM
//! protected. What does work without an account is the public embed page: it
//! ships a `__NEXT_DATA__` JSON blob with title, artists, duration, cover art
//! and a 30 s preview clip. EchoPort imports that metadata and later matches
//! the track to public audio through yt-dlp.

use serde_json::Value;

use crate::model::{Platform, Track};
use crate::net::Http;

use super::util;

/// A parsed `open.spotify.com` or `spotify:` link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpotifyRef {
    Track(String),
    Album(String),
    Playlist(String),
    Episode(String),
}

impl SpotifyRef {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Track(_) => "track",
            Self::Album(_) => "album",
            Self::Playlist(_) => "playlist",
            Self::Episode(_) => "episode",
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Self::Track(id) | Self::Album(id) | Self::Playlist(id) | Self::Episode(id) => id,
        }
    }
}

// ---------------------------------------------------------------------------
// Public provider API
// ---------------------------------------------------------------------------

/// Import metadata for a Spotify link.
pub fn import(http: &Http, reference: &SpotifyRef) -> Result<Track, String> {
    if matches!(reference, SpotifyRef::Episode(_)) {
        return Err(
            "Spotify podcast episodes are not supported — paste a track, album or playlist link."
                .to_owned(),
        );
    }
    fetch(http, reference.kind(), reference.id())
}

/// Parse the different Spotify URL shapes into an id + kind.
pub fn parse_spotify_url(url: &str) -> Result<SpotifyRef, String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err("Paste a Spotify link first.".to_owned());
    }
    // `spotify:track:<id>` style URIs are accepted as a convenience.
    if let Some(rest) = trimmed.strip_prefix("spotify:") {
        let mut parts = rest.split(':');
        let kind = parts.next().unwrap_or_default();
        let id = parts.next().unwrap_or_default();
        return reference_from_parts(kind, id);
    }

    let parsed = url::Url::parse(trimmed)
        .map_err(|_| "That does not look like a valid Spotify link.".to_owned())?;
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host).to_owned();
    if host == "spotify.link" || host == "spotify.app.link" {
        return Err(
            "Short Spotify links are not supported — open the link in a browser and paste the \
             full open.spotify.com URL."
                .to_owned(),
        );
    }
    if !(host == "open.spotify.com" || host.ends_with(".spotify.com")) {
        return Err("That is not a Spotify link.".to_owned());
    }

    let mut segments: Vec<&str> = parsed
        .path_segments()
        .map(|parts| parts.filter(|part| !part.is_empty()).collect())
        .unwrap_or_default();
    if segments.is_empty() {
        return Err("This Spotify link does not point at a track, album or playlist.".to_owned());
    }
    // `open.spotify.com/intl-de/track/<id>` and `/embed/track/<id>` variants.
    if segments[0].starts_with("intl-") || segments[0].eq_ignore_ascii_case("embed") {
        segments.remove(0);
    }
    if segments.len() < 2 {
        return Err(
            "This Spotify link does not point at a track, album or playlist — open the item in \
             Spotify and copy its link."
                .to_owned(),
        );
    }
    reference_from_parts(segments[0], segments[1])
}

fn reference_from_parts(kind: &str, id: &str) -> Result<SpotifyRef, String> {
    let kind = kind.trim().to_ascii_lowercase();
    let id = id.trim();
    if id.is_empty() || !id.chars().all(|ch| ch.is_ascii_alphanumeric()) {
        return Err("Could not read the Spotify id from this link.".to_owned());
    }
    match kind.as_str() {
        "track" => Ok(SpotifyRef::Track(id.to_owned())),
        "album" => Ok(SpotifyRef::Album(id.to_owned())),
        "playlist" => Ok(SpotifyRef::Playlist(id.to_owned())),
        "episode" => Ok(SpotifyRef::Episode(id.to_owned())),
        "artist" | "show" | "user" => Err(format!(
            "Spotify {kind} links cannot be imported — paste a track, album or playlist link instead."
        )),
        _ => Err("This Spotify link is not a track, album or playlist.".to_owned()),
    }
}

/// Fetch and parse a public Spotify embed page.
fn fetch(http: &Http, kind: &str, id: &str) -> Result<Track, String> {
    let url = format!("https://open.spotify.com/embed/{kind}/{id}");
    let html = http
        .get_text(&url, &[("Accept", "text/html")])
        .map_err(|err| format!("Could not reach Spotify: {err}"))?;
    parse_embed(&html, kind, id)
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Extract the JSON inside `<script id="__NEXT_DATA__">`.
fn extract_next_data(html: &str) -> Option<&str> {
    let marker = "id=\"__NEXT_DATA__\"";
    let start = html.find(marker)?;
    let rest = &html[start + marker.len()..];
    let content_start = rest.find('>')? + 1;
    let content = &rest[content_start..];
    let end = content.find("</script>")?;
    Some(content[..end].trim())
}

/// Parse a Spotify embed page into a `Track`.
///
/// Track links yield the track itself; album and playlist links yield their
/// first importable track (EchoPort imports one track per link). A page without
/// a track list is reported as a clear error.
pub fn parse_embed(html: &str, requested_kind: &str, requested_id: &str) -> Result<Track, String> {
    let Some(raw) = extract_next_data(html) else {
        return Err(
            "Spotify returned a page without track data (private or age-gated?).".to_owned(),
        );
    };
    let json: Value = serde_json::from_str(raw).map_err(|_| {
        "Could not read the Spotify page data (the page format changed?).".to_owned()
    })?;

    if let Some(status) = util::get_i64(&json, "/props/pageProps/status") {
        if status >= 400 {
            return Err(
                "Spotify says this link was not found (removed, private, or invalid).".to_owned(),
            );
        }
    }
    let entity = json
        .pointer("/props/pageProps/state/data/entity")
        .filter(|entity| entity.is_object())
        .ok_or_else(|| {
            "This Spotify link has no readable track data (it may be private or region locked)."
                .to_owned()
        })?;

    let kind = util::get_str(entity, "/type").unwrap_or(requested_kind);
    match kind {
        "track" => build_track_entity(entity, requested_id),
        "album" | "playlist" => build_track_from_list(entity, kind, requested_id),
        "episode" => Err(
            "Spotify podcast episodes are not supported — paste a track, album or playlist link."
                .to_owned(),
        ),
        "artist" => Err(
            "Spotify artist links cannot be imported — paste a track, album or playlist link."
                .to_owned(),
        ),
        other => Err(format!(
            "Spotify {other} links cannot be imported — paste a track, album or playlist link."
        )),
    }
}

fn build_track_entity(entity: &Value, fallback_id: &str) -> Result<Track, String> {
    let id = util::get_str(entity, "/id")
        .map(str::to_owned)
        .or_else(|| uri_tail(entity, ":track:"))
        .unwrap_or_else(|| fallback_id.to_owned());
    let title = util::get_str(entity, "/title")
        .or_else(|| util::get_str(entity, "/name"))
        .ok_or_else(|| "Spotify track data has no title.".to_owned())?
        .to_owned();
    let artist = artists_from(entity, "/artists")
        .or_else(|| util::get_str(entity, "/subtitle").map(str::to_owned))
        .unwrap_or_default();
    let mut track = Track::new(Platform::Spotify, &id, title, artist);
    if let Some(album) = util::get_str(entity, "/album/name").and_then(util::clean) {
        track = track.with_album(album);
    }
    if let Some(secs) = util::get_f64(entity, "/duration").and_then(util::seconds_from_ms) {
        track = track.with_duration(secs);
    }
    if let Some(art) = entity_cover(entity) {
        track = track.with_art(art);
    }
    if let Some(preview) = util::get_str(entity, "/audioPreview/url").and_then(util::https_url) {
        track = track.with_stream(preview);
    }
    track = track.with_page(format!("https://open.spotify.com/track/{id}"));
    if let Some(year) =
        util::get_str(entity, "/releaseDate/isoString").and_then(util::year_from_iso)
    {
        track = track.with_year(year);
    }
    if let Some(date) = release_date_of(entity) {
        track = track.with_release_date(date);
    }
    Ok(track)
}

fn build_track_from_list(entity: &Value, kind: &str, fallback_id: &str) -> Result<Track, String> {
    let entity_id = util::get_str(entity, "/id").unwrap_or(fallback_id);
    let entity_name = util::get_str(entity, "/title")
        .or_else(|| util::get_str(entity, "/name"))
        .map(str::to_owned);
    let entries = entity
        .pointer("/trackList")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let entry = entries
        .iter()
        .find(|entry| {
            util::get_str(entry, "/uri")
                .map(|uri| uri.contains(":track:"))
                .unwrap_or(false)
                || util::get_str(entry, "/entityType") == Some("track")
        })
        .ok_or_else(|| {
            format!(
                "This Spotify {kind} has no importable tracks (empty, private or region \
                 restricted)."
            )
        })?;
    let id = uri_tail(entry, ":track:")
        .or_else(|| util::get_str(entry, "/id").map(str::to_owned))
        .ok_or_else(|| format!("Could not read the first track id of this Spotify {kind}."))?;
    let title = util::get_str(entry, "/title")
        .or_else(|| util::get_str(entry, "/name"))
        .ok_or_else(|| "Spotify track data has no title.".to_owned())?
        .to_owned();
    let artist = util::get_str(entry, "/subtitle")
        .or_else(|| util::get_str(entry, "/artists/0/name"))
        .unwrap_or_default()
        .to_owned();
    let mut track = Track::new(Platform::Spotify, &id, title, artist);
    if kind == "album" {
        if let Some(album) = entity_name.clone() {
            track = track.with_album(album);
        }
    }
    if let Some(secs) = util::get_f64(entry, "/duration").and_then(util::seconds_from_ms) {
        track = track.with_duration(secs);
    }
    if let Some(art) = entity_cover(entity).or_else(|| entity_cover(entry)) {
        track = track.with_art(art);
    }
    if let Some(preview) = util::get_str(entry, "/audioPreview/url").and_then(util::https_url) {
        track = track.with_stream(preview);
    }
    track = track.with_page(format!("https://open.spotify.com/track/{id}"));
    if let Some(year) =
        util::get_str(entity, "/releaseDate/isoString").and_then(util::year_from_iso)
    {
        track = track.with_year(year);
    }
    // Track-list entries rarely carry a date; fall back to the album/playlist
    // wrapper when it does.
    if let Some(date) = release_date_of(entry).or_else(|| release_date_of(entity)) {
        track = track.with_release_date(date);
    }
    let _ = entity_id;
    Ok(track)
}

/// Spotify embed release date (`YYYY-MM-DD` from the ISO timestamp).
fn release_date_of(entity: &Value) -> Option<String> {
    util::get_str(entity, "/releaseDate/isoString").and_then(util::date_from_iso)
}

fn artists_from(entity: &Value, pointer: &str) -> Option<String> {
    let array = entity.pointer(pointer)?.as_array()?;
    let names = array
        .iter()
        .filter_map(|artist| util::get_str(artist, "/name"));
    let joined = util::join_names(names);
    (!joined.is_empty()).then_some(joined)
}

fn uri_tail(entity: &Value, needle: &str) -> Option<String> {
    let uri = util::get_str(entity, "/uri")?;
    let tail = uri.split_once(needle)?.1;
    let tail = tail.split('/').next_back().unwrap_or(tail).trim();
    (!tail.is_empty()).then(|| tail.to_owned())
}

/// Pick the largest image Spotify advertises for an entity.
fn entity_cover(entity: &Value) -> Option<String> {
    if let Some(images) = entity
        .pointer("/visualIdentity/image")
        .and_then(Value::as_array)
    {
        let mut best: Option<(i64, String)> = None;
        for image in images {
            let Some(url) = util::get_str(image, "/url").and_then(util::https_url) else {
                continue;
            };
            let size = util::get_i64(image, "/maxWidth")
                .or_else(|| util::get_i64(image, "/maxHeight"))
                .or_else(|| util::get_i64(image, "/width"))
                .unwrap_or(0);
            if best.as_ref().map(|(kept, _)| size > *kept).unwrap_or(true) {
                best = Some((size, url));
            }
        }
        if let Some((_, url)) = best {
            return Some(url);
        }
    }
    if let Some(sources) = entity
        .pointer("/coverArt/sources")
        .and_then(Value::as_array)
    {
        let mut best: Option<(i64, String)> = None;
        for source in sources {
            let Some(url) = util::get_str(source, "/url").and_then(util::https_url) else {
                continue;
            };
            let size = util::get_i64(source, "/width")
                .or_else(|| util::get_i64(source, "/height"))
                .unwrap_or(0);
            if best.as_ref().map(|(kept, _)| size >= *kept).unwrap_or(true) {
                best = Some((size, url));
            }
        }
        if let Some((_, url)) = best {
            return Some(url);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRACK_PAGE: &str = r#"<!DOCTYPE html><html><head><title>Spotify Embed</title></head><body><script id="__NEXT_DATA__" type="application/json">{"props": {"pageProps": {"state": {"data": {"entity": {"type": "track", "id": "4cOdK2wGLETKBW3PvgPWqT", "name": "Never Gonna Give You Up", "title": "Never Gonna Give You Up", "uri": "spotify:track:4cOdK2wGLETKBW3PvgPWqT", "artists": [{"name": "Rick Astley", "uri": "spotify:artist:0gxyHStUsqpMadRV0Di1Qt"}], "releaseDate": {"isoString": "1987-11-12T00:00:00Z"}, "duration": 213573, "isPlayable": true, "audioPreview": {"url": "https://p.scdn.co/mp3-preview/40a780b231e5ed130483bec7917b6c9a4f8a4708"}, "visualIdentity": {"image": [{"url": "https://image-cdn-ak.spotifycdn.com/image/ab67616d00001e02baf89eb11ec7c657805d2da0", "maxHeight": 300, "maxWidth": 300}, {"url": "https://image-cdn-ak.spotifycdn.com/image/ab67616d00004851baf89eb11ec7c657805d2da0", "maxHeight": 64, "maxWidth": 64}, {"url": "https://image-cdn-ak.spotifycdn.com/image/ab67616d0000b273baf89eb11ec7c657805d2da0", "maxHeight": 640, "maxWidth": 640}]}}}, "config": {}}}}}</script><div id="__next"></div></body></html>"#;

    const PLAYLIST_PAGE: &str = r#"<!DOCTYPE html><html><head><title>Spotify Embed</title></head><body><script id="__NEXT_DATA__" type="application/json">{"props": {"pageProps": {"state": {"data": {"entity": {"type": "playlist", "id": "37i9dQZF1DXcBWIGoYBM5M", "name": "Today's Top Hits", "title": "Today's Top Hits", "subtitle": "Spotify", "duration": 0, "coverArt": {"sources": [{"url": "https://i.scdn.co/image/ab67706f00000002e8ccdd567fff6a597b87586b"}]}, "trackList": [{"uri": "spotify:track:2sFepQNyl1WjpROruP8bJ0", "title": "Housewife", "subtitle": "Tate McRae", "duration": 208810, "isPlayable": true, "audioPreview": {"url": "https://p.scdn.co/mp3-preview/724b097d9dc53e0541de27d199adfe3f610944ba"}, "entityType": "track"}]}}, "config": {}}}}}</script><div id="__next"></div></body></html>"#;

    const ALBUM_PAGE: &str = r#"<!DOCTYPE html><html><head><title>Spotify Embed</title></head><body><script id="__NEXT_DATA__" type="application/json">{"props": {"pageProps": {"state": {"data": {"entity": {"type": "album", "id": "2N4vVTdKNVHn6T5rYRJnIS", "name": "The Best of Me", "title": "The Best of Me", "subtitle": "Rick Astley", "visualIdentity": {"image": [{"url": "https://image-cdn-ak.spotifycdn.com/image/large", "maxWidth": 640}]}, "trackList": [{"uri": "spotify:track:32rOY8Hg6yjLCZ8XtahdxX", "title": "Every One of Us", "subtitle": "Rick Astley", "duration": 186586, "isPlayable": true, "audioPreview": {"url": "https://p.scdn.co/mp3-preview/aee5d0fb9f54aaa00fecdef10d681ba82c8cfa41"}, "entityType": "track"}]}}, "config": {}}}}}</script><div id="__next"></div></body></html>"#;

    const NOT_FOUND_PAGE: &str = r#"<!DOCTYPE html><html><head><title>Not found</title></head><body><script id="__NEXT_DATA__" type="application/json">{"props": {"pageProps": {"status": 404, "title": "Page not found", "description": "We can't find that page.", "links": []}}}</script><div id="__next"></div></body></html>"#;

    const NO_PREVIEW_PAGE: &str = r#"<!DOCTYPE html><html><head><title>Spotify Embed</title></head><body><script id="__NEXT_DATA__" type="application/json">{"props": {"pageProps": {"state": {"data": {"entity": {"type": "track", "id": "abc123def456ghi789jkl0", "title": "Hidden Gem", "duration": 120000, "artists": [{"name": "Someone"}], "isPlayable": false}}, "config": {}}}}}</script><div id="__next"></div></body></html>"#;

    #[test]
    fn track_embed_is_parsed() {
        let track = parse_embed(TRACK_PAGE, "track", "4cOdK2wGLETKBW3PvgPWqT").expect("track");
        assert_eq!(track.title, "Never Gonna Give You Up");
        assert_eq!(track.artist, "Rick Astley");
        assert_eq!(track.duration_secs, Some(213.573));
        assert_eq!(track.year, Some(1987));
        assert_eq!(track.release_date.as_deref(), Some("1987-11-12"));
        assert_eq!(track.platform, Platform::Spotify);
        assert_eq!(
            track.art_url.as_deref(),
            Some("https://image-cdn-ak.spotifycdn.com/image/ab67616d0000b273baf89eb11ec7c657805d2da0"),
            "largest advertised image wins"
        );
        assert_eq!(
            track.stream_url.as_deref(),
            Some("https://p.scdn.co/mp3-preview/40a780b231e5ed130483bec7917b6c9a4f8a4708")
        );
        assert_eq!(
            track.page_url.as_deref(),
            Some("https://open.spotify.com/track/4cOdK2wGLETKBW3PvgPWqT")
        );
    }

    #[test]
    fn playlist_embed_yields_first_track() {
        let track =
            parse_embed(PLAYLIST_PAGE, "playlist", "37i9dQZF1DXcBWIGoYBM5M").expect("track");
        assert_eq!(track.title, "Housewife");
        assert_eq!(track.artist, "Tate McRae");
        assert_eq!(track.duration_secs, Some(208.81));
        assert_eq!(
            track.page_url.as_deref(),
            Some("https://open.spotify.com/track/2sFepQNyl1WjpROruP8bJ0")
        );
        assert_eq!(
            track.art_url.as_deref(),
            Some("https://i.scdn.co/image/ab67706f00000002e8ccdd567fff6a597b87586b")
        );
        assert!(track.album.is_none(), "playlists are not albums");
    }

    #[test]
    fn album_embed_sets_album_name() {
        let track = parse_embed(ALBUM_PAGE, "album", "2N4vVTdKNVHn6T5rYRJnIS").expect("track");
        assert_eq!(track.title, "Every One of Us");
        assert_eq!(track.album.as_deref(), Some("The Best of Me"));
        assert_eq!(
            track.art_url.as_deref(),
            Some("https://image-cdn-ak.spotifycdn.com/image/large")
        );
    }

    #[test]
    fn missing_page_data_is_a_clear_error() {
        let error = parse_embed(NOT_FOUND_PAGE, "track", "4cOdK2wGLETKBW3PvgPWqT").unwrap_err();
        assert!(error.contains("not found"), "got: {error}");
        let error = parse_embed("<html><body>nothing</body></html>", "track", "x").unwrap_err();
        assert!(error.to_lowercase().contains("spotify"), "got: {error}");
    }

    #[test]
    fn tracks_without_previews_still_import() {
        let track = parse_embed(NO_PREVIEW_PAGE, "track", "abc123def456ghi789jkl0").expect("track");
        assert_eq!(track.title, "Hidden Gem");
        assert!(track.stream_url.is_none());
        assert!(track.art_url.is_none());
    }

    #[test]
    fn spotify_links_are_parsed() {
        assert_eq!(
            parse_spotify_url("https://open.spotify.com/track/4cOdK2wGLETKBW3PvgPWqT"),
            Ok(SpotifyRef::Track("4cOdK2wGLETKBW3PvgPWqT".to_owned()))
        );
        assert_eq!(
            parse_spotify_url("https://open.spotify.com/intl-de/track/4cOdK2wGLETKBW3PvgPWqT?si=x"),
            Ok(SpotifyRef::Track("4cOdK2wGLETKBW3PvgPWqT".to_owned()))
        );
        assert_eq!(
            parse_spotify_url("https://open.spotify.com/embed/track/4cOdK2wGLETKBW3PvgPWqT"),
            Ok(SpotifyRef::Track("4cOdK2wGLETKBW3PvgPWqT".to_owned()))
        );
        assert_eq!(
            parse_spotify_url("https://open.spotify.com/album/2N4vVTdKNVHn6T5rYRJnIS"),
            Ok(SpotifyRef::Album("2N4vVTdKNVHn6T5rYRJnIS".to_owned()))
        );
        assert_eq!(
            parse_spotify_url("spotify:playlist:37i9dQZF1DXcBWIGoYBM5M"),
            Ok(SpotifyRef::Playlist("37i9dQZF1DXcBWIGoYBM5M".to_owned()))
        );
        assert!(
            parse_spotify_url("https://open.spotify.com/artist/0gxyHStUsqpMadRV0Di1Qt").is_err()
        );
        assert!(parse_spotify_url("https://spotify.link/abc").is_err());
        assert!(parse_spotify_url("https://example.com/track/abc").is_err());
        assert!(parse_spotify_url("").is_err());
    }
}
