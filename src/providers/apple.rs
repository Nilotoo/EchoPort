//! Apple Music / iTunes catalogue provider.
//!
//! Apple Music itself is FairPlay protected, but the public iTunes Search API
//! is keyless and returns full metadata plus 30 s preview clips. It is used as
//! both a searchable catalogue and the metadata source for `music.apple.com`
//! link imports.

use serde::Deserialize;

use crate::model::{Album as ModelAlbum, Artist as ModelArtist, Platform, Track};
use crate::net::Http;

use super::util;

const SEARCH_URL: &str = "https://itunes.apple.com/search";
const LOOKUP_URL: &str = "https://itunes.apple.com/lookup";
/// Used when a link does not name a storefront. The US store has the broadest
/// catalogue and predictable English metadata.
pub const DEFAULT_STOREFRONT: &str = "US";

/// A parsed `music.apple.com` link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppleRef {
    /// A song (`/song/.../<id>` or an album link with `?i=<track>`).
    Track { id: u64, country: String },
    /// An album (`/album/.../<id>` without `?i=`).
    Album { id: u64, country: String },
}

#[derive(Debug, Default, Deserialize)]
struct ITunesResponse {
    #[serde(default)]
    results: Vec<ITunesItem>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct ITunesItem {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default, rename = "trackId")]
    track_id: Option<u64>,
    #[serde(default, rename = "collectionId")]
    collection_id: Option<u64>,
    #[serde(default, rename = "trackName")]
    track_name: Option<String>,
    #[serde(default, rename = "artistName")]
    artist_name: Option<String>,
    #[serde(default, rename = "artistId")]
    artist_id: Option<u64>,
    #[serde(default, rename = "collectionName")]
    collection_name: Option<String>,
    #[serde(default, rename = "trackViewUrl")]
    track_view_url: Option<String>,
    #[serde(default, rename = "collectionViewUrl")]
    collection_view_url: Option<String>,
    #[serde(default, rename = "previewUrl")]
    preview_url: Option<String>,
    #[serde(default, rename = "artworkUrl100")]
    artwork_url100: Option<String>,
    #[serde(default, rename = "artworkUrl60")]
    artwork_url60: Option<String>,
    #[serde(default, rename = "releaseDate")]
    release_date: Option<String>,
    #[serde(default, rename = "trackTimeMillis")]
    track_time_millis: Option<f64>,
    #[serde(default, rename = "trackCount")]
    track_count: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
struct ITunesArtistResponse {
    #[serde(default)]
    results: Vec<ITunesArtist>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct ITunesArtist {
    #[serde(default, rename = "artistId")]
    artist_id: Option<u64>,
    #[serde(default, rename = "artistName")]
    artist_name: Option<String>,
    #[serde(default, rename = "artistLinkUrl")]
    artist_link_url: Option<String>,
}

impl ITunesItem {
    fn is_song(&self) -> bool {
        self.track_id.is_some()
            && (self.kind.as_deref() == Some("song") || self.track_name.is_some())
    }
}

// ---------------------------------------------------------------------------
// Public provider API
// ---------------------------------------------------------------------------

/// Search the US iTunes store for songs.
pub fn search(http: &Http, query: &str, limit: usize) -> Result<Vec<Track>, String> {
    search_in(http, query, limit, DEFAULT_STOREFRONT)
}

fn search_in(
    http: &Http,
    query: &str,
    limit: usize,
    storefront: &str,
) -> Result<Vec<Track>, String> {
    let limit = limit.clamp(1, 50);
    let url = format!(
        "{SEARCH_URL}?term={}&entity=song&media=music&limit={limit}&country={}",
        util::pct(query.trim()),
        storefront
    );
    let response: ITunesResponse = http
        .get_json(&url, &[])
        .map_err(|err| format!("Apple Music search failed: {err}"))?;
    let mut tracks = Vec::new();
    let mut seen: Vec<u64> = Vec::new();
    for item in response.results.iter().filter(|item| item.is_song()) {
        let Some(id) = item.track_id else {
            continue;
        };
        if seen.contains(&id) {
            continue;
        }
        seen.push(id);
        if let Some(track) = build_track(item) {
            tracks.push(track);
            if tracks.len() >= limit {
                break;
            }
        }
    }
    Ok(tracks)
}

/// Import an Apple Music link (song or album).
pub fn import(http: &Http, reference: &AppleRef) -> Result<Track, String> {
    match reference {
        AppleRef::Track { id, country } => lookup_track(http, *id, country),
        AppleRef::Album { id, country } => lookup_album_first_track(http, *id, country),
    }
}

/// Parse `music.apple.com` / `itunes.apple.com` URLs into ids.
pub fn parse_apple_url(url: &str) -> Result<AppleRef, String> {
    let parsed = url::Url::parse(url)
        .map_err(|_| "That does not look like a valid Apple Music link.".to_owned())?;
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host).to_owned();
    if !(host == "music.apple.com"
        || host.ends_with(".music.apple.com")
        || host == "itunes.apple.com"
        || host.ends_with(".itunes.apple.com"))
    {
        return Err("That is not an Apple Music link.".to_owned());
    }

    let segments: Vec<&str> = parsed
        .path_segments()
        .map(|parts| parts.filter(|part| !part.is_empty()).collect())
        .unwrap_or_default();
    let country = segments
        .first()
        .filter(|part| part.len() == 2 && part.chars().all(|ch| ch.is_ascii_alphabetic()))
        .map(|part| part.to_ascii_uppercase())
        .unwrap_or_else(|| DEFAULT_STOREFRONT.to_owned());

    if segments
        .iter()
        .any(|part| part.eq_ignore_ascii_case("artist"))
    {
        return Err(
            "Apple Music artist links cannot be imported — paste a song or album link instead."
                .to_owned(),
        );
    }
    if segments
        .iter()
        .any(|part| part.eq_ignore_ascii_case("playlist"))
    {
        return Err(
            "Apple Music playlist links cannot be imported — paste a song or album link instead."
                .to_owned(),
        );
    }

    // `?i=<track id>` identifies a song inside an album.
    if let Some(id) = parsed
        .query_pairs()
        .find(|(key, _)| key == "i")
        .and_then(|(_, value)| value.parse::<u64>().ok())
    {
        return Ok(AppleRef::Track { id, country });
    }

    let last_id = segments
        .iter()
        .rev()
        .find_map(|part| part.parse::<u64>().ok());
    let Some(id) = last_id else {
        return Err(
            "Could not find an Apple Music song or album id in this link — open the song in \
             Apple Music and copy its link again."
                .to_owned(),
        );
    };
    if segments
        .iter()
        .any(|part| part.eq_ignore_ascii_case("song"))
    {
        Ok(AppleRef::Track { id, country })
    } else if segments
        .iter()
        .any(|part| part.eq_ignore_ascii_case("album"))
    {
        Ok(AppleRef::Album { id, country })
    } else {
        Err("This Apple Music link is not a song or album link.".to_owned())
    }
}

/// Load a single track by iTunes track id.
pub fn lookup_track(http: &Http, id: u64, country: &str) -> Result<Track, String> {
    let url = format!("{LOOKUP_URL}?id={id}&country={}", storefront(country));
    let response: ITunesResponse = http
        .get_json(&url, &[])
        .map_err(|err| format!("Apple Music lookup failed: {err}"))?;
    response
        .results
        .iter()
        .find(|item| item.track_id == Some(id) && item.is_song())
        .or_else(|| response.results.iter().find(|item| item.is_song()))
        .and_then(build_track)
        .ok_or_else(|| format!("Apple Music track {id} was not found in this storefront."))
}

/// For album links (no single track id) import the album's first song as a
/// best effort seed for playback matching.
pub fn lookup_album_first_track(
    http: &Http,
    collection_id: u64,
    country: &str,
) -> Result<Track, String> {
    let url = format!(
        "{LOOKUP_URL}?id={collection_id}&entity=song&limit=50&country={}",
        storefront(country)
    );
    let response: ITunesResponse = http
        .get_json(&url, &[])
        .map_err(|err| format!("Apple Music album lookup failed: {err}"))?;
    // The first result is the album wrapper; songs follow.
    let mut track = response
        .results
        .iter()
        .find(|item| item.is_song() && item.collection_id == Some(collection_id))
        .or_else(|| response.results.iter().find(|item| item.is_song()))
        .and_then(build_track)
        .ok_or_else(|| {
            "This Apple Music album has no songs that can be imported (it may be region locked)."
                .to_owned()
        })?;
    if track.album.is_none() {
        // Keep the album context when the wrapper song lacks it.
        track.album = response
            .results
            .iter()
            .find(|item| item.collection_id == Some(collection_id) && !item.is_song())
            .and_then(|item| item.collection_name.as_deref())
            .and_then(util::clean);
    }
    Ok(track)
}

// ---------------------------------------------------------------------------
// Artists (producer search + artist page)
// ---------------------------------------------------------------------------

/// Search iTunes artists (`entity=musicArtist`).
pub fn search_artists(http: &Http, query: &str, limit: usize) -> Result<Vec<ModelArtist>, String> {
    let limit = limit.clamp(1, 50);
    let url = format!(
        "{SEARCH_URL}?term={}&entity=musicArtist&limit={limit}&country={DEFAULT_STOREFRONT}",
        util::pct(query.trim())
    );
    let response: ITunesArtistResponse = http
        .get_json(&url, &[])
        .map_err(|err| format!("Apple Music artist search failed: {err}"))?;
    Ok(build_artists(&response.results, limit))
}

// ---------------------------------------------------------------------------
// Albums (search / detail / artist discography)
// ---------------------------------------------------------------------------

fn build_album(item: &ITunesItem) -> Option<ModelAlbum> {
    let id = item.collection_id?;
    let title = item.collection_name.as_deref().and_then(util::clean)?;
    let artist = util::clean(item.artist_name.as_deref().unwrap_or_default()).unwrap_or_default();
    let mut album = ModelAlbum::new(Platform::AppleMusic, &id.to_string(), title, artist);
    if let Some(art) = item
        .artwork_url100
        .as_deref()
        .or(item.artwork_url60.as_deref())
        .and_then(upgrade_artwork)
    {
        album = album.with_art(art);
    }
    if let Some(year) = item.release_date.as_deref().and_then(util::year_from_iso) {
        album = album.with_year(year);
    }
    if let Some(count) = item.track_count {
        album = album.with_track_count(count);
    }
    if let Some(page) = item
        .collection_view_url
        .as_deref()
        .and_then(util::https_url)
    {
        album = album.with_page(page);
    }
    Some(album)
}

/// Search albums through the public iTunes Search API (`entity=album`).
pub fn search_albums(http: &Http, query: &str, limit: usize) -> Result<Vec<ModelAlbum>, String> {
    let limit = limit.clamp(1, 50);
    let url = format!(
        "{SEARCH_URL}?term={}&entity=album&limit={limit}&country={DEFAULT_STOREFRONT}",
        util::pct(query.trim())
    );
    let response: ITunesResponse = http
        .get_json(&url, &[])
        .map_err(|err| format!("Apple Music album search failed: {err}"))?;
    let mut albums = Vec::new();
    let mut seen = Vec::new();
    for item in &response.results {
        let Some(id) = item.collection_id else {
            continue;
        };
        if seen.contains(&id) {
            continue;
        }
        if let Some(album) = build_album(item) {
            seen.push(id);
            albums.push(album);
            if albums.len() >= limit {
                break;
            }
        }
    }
    Ok(albums)
}

/// Fetch the songs of an Apple Music album, preserving the album order.
pub fn album_tracks(http: &Http, album: &ModelAlbum) -> Result<Vec<Track>, String> {
    let id = album
        .source_id
        .trim()
        .parse::<u64>()
        .map_err(|_| "This Apple Music album has an invalid source id.".to_owned())?;
    let url = format!("{LOOKUP_URL}?id={id}&entity=song&limit=200&country={DEFAULT_STOREFRONT}");
    let response: ITunesResponse = http
        .get_json(&url, &[])
        .map_err(|err| format!("Apple Music album lookup failed: {err}"))?;
    let mut tracks = Vec::new();
    for item in &response.results {
        if !item.is_song() {
            continue;
        }
        if item.collection_id.is_some() && item.collection_id != Some(id) {
            continue;
        }
        if let Some(mut track) = build_track(item) {
            if track.album.is_none() {
                track.album = Some(album.title.clone());
            }
            if track.art_url.is_none() {
                track.art_url = album.art_url.clone();
            }
            tracks.push(track);
        }
    }
    if tracks.is_empty() {
        return Err(
            "This Apple Music album has no playable songs in the selected store.".to_owned(),
        );
    }
    Ok(tracks)
}

/// Other albums by the same Apple Music artist (album series / discography).
pub fn artist_albums(
    http: &Http,
    artist_name: &str,
    limit: usize,
) -> Result<Vec<ModelAlbum>, String> {
    let limit = limit.clamp(1, 30);
    // The artist lookup requires an artist id, which album search results do
    // not carry; a focused album search by artist name is the keyless route.
    let mut albums = search_albums(http, artist_name, limit.saturating_mul(2))?;
    albums.retain(|album| {
        album
            .artist
            .to_lowercase()
            .contains(&artist_name.to_lowercase())
            || artist_name
                .to_lowercase()
                .contains(&album.artist.to_lowercase())
    });
    albums.truncate(limit);
    Ok(albums)
}

/// Best-effort portrait for an iTunes artist: the artwork of their first
/// album (iTunes artist search itself does not return portraits). Called from
/// a background prefetch worker, never on the UI thread.
pub fn artist_portrait(http: &Http, artist: &ModelArtist) -> Option<String> {
    let id = artist.source_id.trim().parse::<u64>().ok()?;
    let url = format!("{LOOKUP_URL}?id={id}&entity=album&limit=1&country={DEFAULT_STOREFRONT}");
    let response: ITunesResponse = http.get_json(&url, &[]).ok()?;
    response
        .results
        .iter()
        .find_map(|item| {
            item.artwork_url100
                .clone()
                .or_else(|| item.artwork_url60.clone())
        })
        .and_then(|url| util::https_url(&url))
}

/// Hot songs for an iTunes artist (lookup by artistId + `entity=song`).
pub fn artist_hot_tracks(
    http: &Http,
    artist: &ModelArtist,
    limit: usize,
) -> Result<Vec<Track>, String> {
    let limit = limit.clamp(1, 30);
    let artist_id = artist.source_id.trim().parse::<u64>().ok();
    if let Some(id) = artist_id {
        let url = format!(
            "{LOOKUP_URL}?id={id}&entity=song&limit={}&country={DEFAULT_STOREFRONT}",
            limit.saturating_mul(2).max(12)
        );
        let response: ITunesResponse = http
            .get_json(&url, &[])
            .map_err(|err| format!("Apple Music artist lookup failed: {err}"))?;
        let tracks = items_to_artist_tracks(&response.results, Some(id), &artist.name, limit);
        if !tracks.is_empty() {
            return Ok(tracks);
        }
    }
    // Fallback when the source id is unknown/stale: search songs by name and
    // keep only rows credited to this artist.
    let searched = search(http, &artist.name, limit.saturating_mul(3))?;
    let filtered = filter_artist_tracks(searched, &artist.name, limit);
    if filtered.is_empty() {
        return Err(format!(
            "Apple Music has no hot songs for this artist in the {} store.",
            DEFAULT_STOREFRONT
        ));
    }
    Ok(filtered)
}

fn build_artists(items: &[ITunesArtist], limit: usize) -> Vec<ModelArtist> {
    let mut artists: Vec<ModelArtist> = Vec::new();
    let mut seen: Vec<u64> = Vec::new();
    for item in items {
        let Some(id) = item.artist_id else {
            continue;
        };
        if seen.contains(&id) {
            continue;
        }
        let Some(name) = item.artist_name.as_deref().and_then(util::clean) else {
            continue;
        };
        seen.push(id);
        let mut artist = ModelArtist::new(Platform::AppleMusic, &id.to_string(), name);
        if let Some(page) = item.artist_link_url.as_deref().and_then(human_artist_link) {
            artist = artist.with_page(page);
        }
        artists.push(artist);
        if artists.len() >= limit {
            break;
        }
    }
    artists
}

/// Strip the iTunes tracking parameter; keep a human `music.apple.com` page.
fn human_artist_link(raw: &str) -> Option<String> {
    let url = util::https_url(raw)?;
    let page = url.split("?uo=").next().unwrap_or(&url).to_owned();
    Some(page)
}

fn items_to_artist_tracks(
    items: &[ITunesItem],
    artist_id: Option<u64>,
    artist_name: &str,
    limit: usize,
) -> Vec<Track> {
    let mut tracks = Vec::new();
    let mut seen: Vec<u64> = Vec::new();
    for item in items.iter().filter(|item| item.is_song()) {
        let Some(id) = item.track_id else {
            continue;
        };
        if seen.contains(&id) {
            continue;
        }
        // The native artistId is authoritative when present; only fall back
        // to name matching for old rows that do not carry one.
        let credited = match artist_id {
            Some(artist_id) => item.artist_id == Some(artist_id),
            None => {
                util::name_relevance(artist_name, item.artist_name.as_deref().unwrap_or_default())
                    > 0
            }
        };
        if !credited {
            continue;
        }
        let Some(track) = build_track(item) else {
            continue;
        };
        seen.push(id);
        tracks.push(track);
        if tracks.len() >= limit {
            break;
        }
    }
    tracks
}

fn filter_artist_tracks(tracks: Vec<Track>, artist_name: &str, limit: usize) -> Vec<Track> {
    let mut kept = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for track in tracks {
        if util::name_relevance(artist_name, &track.artist) == 0 {
            continue;
        }
        if seen.iter().any(|id| id == &track.id) {
            continue;
        }
        seen.push(track.id.clone());
        kept.push(track);
        if kept.len() >= limit {
            break;
        }
    }
    kept
}

fn storefront(country: &str) -> String {
    let country = country.trim().to_ascii_uppercase();
    if country.len() == 2 && country.chars().all(|ch| ch.is_ascii_alphabetic()) {
        country
    } else {
        DEFAULT_STOREFRONT.to_owned()
    }
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

fn build_track(item: &ITunesItem) -> Option<Track> {
    let track_id = item.track_id?;
    let title = item
        .track_name
        .as_deref()
        .and_then(util::clean)
        .or_else(|| item.collection_name.as_deref().and_then(util::clean))?;
    let artist = util::clean(item.artist_name.as_deref().unwrap_or_default()).unwrap_or_default();
    let mut track = Track::new(Platform::AppleMusic, &track_id.to_string(), title, artist);
    if let Some(album) = item.collection_name.as_deref().and_then(util::clean) {
        track = track.with_album(album);
    }
    if let Some(secs) = item.track_time_millis.and_then(util::seconds_from_ms) {
        track = track.with_duration(secs);
    }
    if let Some(art) = item
        .artwork_url100
        .as_deref()
        .or(item.artwork_url60.as_deref())
        .and_then(upgrade_artwork)
    {
        track = track.with_art(art);
    }
    if let Some(preview) = item.preview_url.as_deref().and_then(util::https_url) {
        track = track.with_stream(preview);
    }
    if let Some(page) = item
        .track_view_url
        .as_deref()
        .and_then(util::https_url)
        .or_else(|| {
            item.collection_view_url
                .as_deref()
                .and_then(util::https_url)
        })
    {
        track = track.with_page(page);
    }
    if let Some(year) = item.release_date.as_deref().and_then(util::year_from_iso) {
        track = track.with_year(year);
    }
    // Full release date when the API provides it (it does for songs).
    if let Some(date) = item.release_date.as_deref().and_then(util::date_from_iso) {
        track = track.with_release_date(date);
    }
    Some(track)
}

/// iTunes only serves 100 px covers through the search API; the thumbnail
/// service accepts arbitrary sizes, so ask for 600 px instead.
pub fn upgrade_artwork(url: &str) -> Option<String> {
    let upgraded = util::https_url(url)?;
    for (small, large) in [
        ("100x100bb", "600x600bb"),
        ("60x60bb", "600x600bb"),
        ("30x30bb", "600x600bb"),
        ("100x100-75", "600x600bb"),
        ("60x60-75", "600x600bb"),
    ] {
        if upgraded.contains(small) {
            return Some(upgraded.replace(small, large));
        }
    }
    Some(upgraded)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH_FIXTURE: &str = r#"{
        "resultCount": 2,
        "results": [
            {
                "wrapperType": "track",
                "kind": "song",
                "trackId": 1559885421,
                "artistName": "Rick Astley",
                "collectionName": "Whenever You Need Somebody",
                "trackName": "Never Gonna Give You Up",
                "trackViewUrl": "https://music.apple.com/us/album/never-gonna-give-you-up/1559885420?i=1559885421&uo=4",
                "previewUrl": "https://audio-ssl.itunes.apple.com/itunes-assets/AudioPreview221/v4/x.m4a",
                "artworkUrl100": "https://is1-ssl.mzstatic.com/image/thumb/Music124/x/100x100bb.jpg",
                "releaseDate": "1987-07-27T07:00:00Z",
                "trackTimeMillis": 213573
            },
            {
                "wrapperType": "track",
                "kind": "song",
                "trackId": 2,
                "artistName": "Somebody",
                "trackName": "No Preview",
                "trackViewUrl": "https://music.apple.com/us/album/no-preview/2?i=2",
                "artworkUrl60": "https://is1-ssl.mzstatic.com/image/thumb/Music124/y/60x60bb.jpg",
                "trackTimeMillis": 1000
            }
        ]
    }"#;

    const TRACK_LOOKUP_FIXTURE: &str = r#"{
        "resultCount": 1,
        "results": [
            {
                "wrapperType": "track",
                "kind": "song",
                "trackId": 1559885421,
                "artistName": "Rick Astley",
                "collectionName": "Whenever You Need Somebody",
                "trackName": "Never Gonna Give You Up",
                "trackViewUrl": "https://music.apple.com/us/album/x/1559885420?i=1559885421",
                "previewUrl": "https://audio-ssl.itunes.apple.com/itunes-assets/AudioPreview221/v4/x.m4a",
                "artworkUrl100": "https://is1-ssl.mzstatic.com/image/thumb/Music124/x/100x100bb.jpg",
                "releaseDate": "1987-07-27T07:00:00Z",
                "trackTimeMillis": 213573
            }
        ]
    }"#;

    const ALBUM_LOOKUP_FIXTURE: &str = r#"{
        "resultCount": 3,
        "results": [
            {
                "wrapperType": "collection",
                "collectionType": "Album",
                "collectionId": 1559885420,
                "artistName": "Rick Astley",
                "collectionName": "Whenever You Need Somebody"
            },
            {
                "wrapperType": "track",
                "kind": "song",
                "trackId": 1559885421,
                "collectionId": 1559885420,
                "artistName": "Rick Astley",
                "collectionName": "Whenever You Need Somebody",
                "trackName": "Never Gonna Give You Up",
                "previewUrl": "https://audio-ssl.itunes.apple.com/itunes-assets/AudioPreview221/v4/x.m4a",
                "artworkUrl100": "https://is1-ssl.mzstatic.com/image/thumb/Music124/x/100x100bb.jpg",
                "trackTimeMillis": 213573
            },
            {
                "wrapperType": "track",
                "kind": "song",
                "trackId": 1559885422,
                "collectionId": 1559885420,
                "artistName": "Rick Astley",
                "collectionName": "Whenever You Need Somebody",
                "trackName": "Whenever You Need Somebody",
                "trackTimeMillis": 220000
            }
        ]
    }"#;

    fn parse(json: &str) -> ITunesResponse {
        serde_json::from_str(json).expect("itunes fixture parses")
    }

    #[test]
    fn search_results_become_tracks() {
        let response = parse(SEARCH_FIXTURE);
        let tracks: Vec<Track> = response
            .results
            .iter()
            .filter(|item| item.is_song())
            .filter_map(build_track)
            .collect();
        assert_eq!(tracks.len(), 2);
        let first = &tracks[0];
        assert_eq!(first.title, "Never Gonna Give You Up");
        assert_eq!(first.artist, "Rick Astley");
        assert_eq!(first.album.as_deref(), Some("Whenever You Need Somebody"));
        assert_eq!(first.duration_secs, Some(213.573));
        assert_eq!(
            first.art_url.as_deref(),
            Some("https://is1-ssl.mzstatic.com/image/thumb/Music124/x/600x600bb.jpg")
        );
        assert!(first
            .stream_url
            .as_deref()
            .unwrap_or_default()
            .starts_with("https://audio-ssl.itunes.apple.com/"));
        assert_eq!(first.year, Some(1987));
        assert_eq!(first.release_date.as_deref(), Some("1987-07-27"));
        assert_eq!(first.platform, Platform::AppleMusic);

        let second = &tracks[1];
        assert!(second.stream_url.is_none());
        assert_eq!(
            second.art_url.as_deref(),
            Some("https://is1-ssl.mzstatic.com/image/thumb/Music124/y/600x600bb.jpg")
        );
        assert!(second.album.is_none());
    }

    #[test]
    fn track_lookup_uses_the_song_wrapper() {
        let response = parse(TRACK_LOOKUP_FIXTURE);
        let track = response
            .results
            .iter()
            .filter(|item| item.is_song())
            .find_map(build_track)
            .expect("track found");
        assert_eq!(track.title, "Never Gonna Give You Up");
        assert_eq!(track.album.as_deref(), Some("Whenever You Need Somebody"));
    }

    #[test]
    fn album_lookup_skips_the_collection_wrapper() {
        let response = parse(ALBUM_LOOKUP_FIXTURE);
        let track = response
            .results
            .iter()
            .find(|item| item.is_song() && item.collection_id == Some(1559885420))
            .and_then(build_track)
            .expect("first album song");
        assert_eq!(track.title, "Never Gonna Give You Up");
        assert_eq!(track.album.as_deref(), Some("Whenever You Need Somebody"));
    }

    #[test]
    fn artwork_is_upgraded_to_600px() {
        assert_eq!(
            upgrade_artwork("https://is1-ssl.mzstatic.com/a/100x100bb.jpg").as_deref(),
            Some("https://is1-ssl.mzstatic.com/a/600x600bb.jpg")
        );
        assert_eq!(
            upgrade_artwork("https://is1-ssl.mzstatic.com/a/60x60bb.jpg").as_deref(),
            Some("https://is1-ssl.mzstatic.com/a/600x600bb.jpg")
        );
        assert_eq!(
            upgrade_artwork("https://is1-ssl.mzstatic.com/a/original.jpg").as_deref(),
            Some("https://is1-ssl.mzstatic.com/a/original.jpg")
        );
        assert_eq!(
            upgrade_artwork("http://is1-ssl.mzstatic.com/a/100x100bb.jpg").as_deref(),
            Some("https://is1-ssl.mzstatic.com/a/600x600bb.jpg")
        );
    }

    #[test]
    fn apple_links_are_parsed() {
        assert_eq!(
            parse_apple_url("https://music.apple.com/us/album/x/1559885420?i=1559885421"),
            Ok(AppleRef::Track {
                id: 1_559_885_421,
                country: "US".to_owned()
            })
        );
        assert_eq!(
            parse_apple_url("https://music.apple.com/gb/album/x/1559885420"),
            Ok(AppleRef::Album {
                id: 1_559_885_420,
                country: "GB".to_owned()
            })
        );
        assert_eq!(
            parse_apple_url("https://music.apple.com/us/song/x/1559885421"),
            Ok(AppleRef::Track {
                id: 1_559_885_421,
                country: "US".to_owned()
            })
        );
        assert!(parse_apple_url("https://music.apple.com/us/artist/rick-astley/669771").is_err());
        assert!(parse_apple_url("https://music.apple.com/us/playlist/x/pl.abc").is_err());
        assert!(parse_apple_url("https://example.com/album/x/1").is_err());
        assert!(parse_apple_url("not a url").is_err());
    }

    #[test]
    fn storefront_validation() {
        assert_eq!(storefront("us"), "US");
        assert_eq!(storefront("GB"), "GB");
        assert_eq!(storefront(""), "US");
        assert_eq!(storefront("USA"), "US");
    }
    const ARTIST_SEARCH_FIXTURE: &str = r#"{
        "resultCount": 2,
        "results": [
            {
                "wrapperType": "artist",
                "artistType": "Artist",
                "artistName": "Porter Robinson",
                "artistLinkUrl": "https://music.apple.com/us/artist/porter-robinson/282330711?uo=4",
                "artistId": 282330711,
                "primaryGenreName": "Dance"
            },
            {
                "wrapperType": "artist",
                "artistType": "Artist",
                "artistName": "Daniel Porter Robinson",
                "artistLinkUrl": "https://music.apple.com/us/artist/daniel-porter-robinson/950526031?uo=4",
                "artistId": 950526031
            }
        ]
    }"#;

    const ARTIST_LOOKUP_FIXTURE: &str = r#"{
        "resultCount": 3,
        "results": [
            {
                "wrapperType": "artist",
                "artistName": "Porter Robinson",
                "artistId": 282330711
            },
            {
                "wrapperType": "track",
                "kind": "song",
                "trackId": 1440935637,
                "artistId": 282330711,
                "artistName": "Porter Robinson",
                "collectionName": "Nurture",
                "trackName": "Get Your Wish",
                "artworkUrl100": "https://is1-ssl.mzstatic.com/image/thumb/x/100x100bb.jpg",
                "trackTimeMillis": 200000
            },
            {
                "wrapperType": "track",
                "kind": "song",
                "trackId": 1440935999,
                "artistId": 950526031,
                "artistName": "Daniel Porter Robinson",
                "trackName": "Some Classical Piece"
            }
        ]
    }"#;

    fn parse_artist_search(json: &str) -> Vec<ITunesArtist> {
        let response: ITunesArtistResponse = serde_json::from_str(json).expect("artist fixture");
        response.results
    }

    #[test]
    fn itunes_artist_search_is_parsed_and_built() {
        let artists = build_artists(&parse_artist_search(ARTIST_SEARCH_FIXTURE), 10);
        assert_eq!(artists.len(), 2);
        let first = &artists[0];
        assert_eq!(first.name, "Porter Robinson");
        assert_eq!(first.platform, Platform::AppleMusic);
        assert_eq!(first.source_id, "282330711");
        assert_eq!(
            first.page_url.as_deref(),
            Some("https://music.apple.com/us/artist/porter-robinson/282330711"),
            "iTunes tracking parameter is stripped"
        );
        assert!(first.image_url.is_none(), "iTunes artists have no artwork");
        assert_eq!(
            build_artists(&parse_artist_search(ARTIST_SEARCH_FIXTURE), 1).len(),
            1
        );
    }

    #[test]
    fn itunes_artist_lookup_keeps_only_credited_songs() {
        let response: ITunesResponse =
            serde_json::from_str(ARTIST_LOOKUP_FIXTURE).expect("lookup fixture");
        let tracks =
            items_to_artist_tracks(&response.results, Some(282_330_711), "Porter Robinson", 10);
        assert_eq!(tracks.len(), 1);
        let track = &tracks[0];
        assert_eq!(track.title, "Get Your Wish");
        assert_eq!(track.artist, "Porter Robinson");
        assert_eq!(track.album.as_deref(), Some("Nurture"));
        assert_eq!(track.platform, Platform::AppleMusic);
        assert_eq!(
            track.art_url.as_deref(),
            Some("https://is1-ssl.mzstatic.com/image/thumb/x/600x600bb.jpg")
        );
    }

    #[test]
    fn apple_hot_track_fallback_filter_requires_the_artist() {
        let tracks = vec![
            Track::new(
                Platform::AppleMusic,
                "1",
                "Get Your Wish",
                "Porter Robinson",
            ),
            Track::new(Platform::AppleMusic, "2", "Noise", "Daft Punk"),
            Track::new(
                Platform::AppleMusic,
                "3",
                "Shelter",
                "Porter Robinson, Madeon",
            ),
        ];
        let filtered = filter_artist_tracks(tracks, "Porter Robinson", 10);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].title, "Get Your Wish");
        assert_eq!(filtered[1].title, "Shelter");
    }
}
