//! Netease Cloud Music catalogue provider.
//!
//! Search and song details both come from the public, unauthenticated mobile
//! web API (`music.163.com/api/...`). It is region dependent and legally grey:
//! the app treats it as an optional catalogue and playback still goes through
//! yt-dlp. Requests only carry a `Referer`, no account cookies or tokens.

use std::collections::HashMap;

use serde::Deserialize;

use crate::model::{Album as ModelAlbum, Artist as ModelArtist, Platform, Track};
use crate::net::Http;
use crate::resolver::ResolveConfig;

use super::util;

const SEARCH_URL: &str = "https://music.163.com/api/search/get";
const DETAIL_URL: &str = "https://music.163.com/api/song/detail/";
const SONG_PAGE: &str = "https://music.163.com/song?id=";
const ARTIST_PAGE: &str = "https://music.163.com/#/artist?id=";
const ARTIST_DETAIL_URL: &str = "https://music.163.com/api/artist/";
const ALBUM_DETAIL_URL: &str = "https://music.163.com/api/album/";
const ALBUM_PAGE: &str = "https://music.163.com/#/album?id=";
const REFERER: &str = "https://music.163.com/";

#[derive(Debug, Default, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    code: Option<i64>,
    #[serde(default)]
    result: Option<SearchResult>,
}

#[derive(Debug, Default, Deserialize)]
struct SearchResult {
    #[serde(default)]
    songs: Vec<Song>,
}

#[derive(Debug, Default, Deserialize)]
struct DetailResponse {
    #[serde(default)]
    code: Option<i64>,
    #[serde(default)]
    songs: Vec<Song>,
}

#[derive(Debug, Default, Deserialize)]
struct ArtistSearchResponse {
    #[serde(default)]
    code: Option<i64>,
    #[serde(default)]
    result: Option<ArtistSearchResult>,
}

#[derive(Debug, Default, Deserialize)]
struct ArtistSearchResult {
    #[serde(default)]
    artists: Vec<ApiArtist>,
}

#[derive(Debug, Default, Deserialize)]
struct AlbumSearchResponse {
    #[serde(default)]
    code: Option<i64>,
    #[serde(default)]
    result: Option<AlbumSearchResult>,
}

#[derive(Debug, Default, Deserialize)]
struct AlbumSearchResult {
    #[serde(default)]
    albums: Vec<ApiAlbum>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiAlbum {
    #[serde(default)]
    id: Option<u64>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    pic_url: Option<String>,
    #[serde(default)]
    artist: Option<Artist>,
    #[serde(default)]
    artists: Vec<Artist>,
    #[serde(default)]
    size: Option<u32>,
    #[serde(default)]
    publish_time: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
struct AlbumDetailResponse {
    #[serde(default)]
    code: Option<i64>,
    #[serde(default)]
    album: Option<AlbumDetail>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AlbumDetail {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    pic_url: Option<String>,
    #[serde(default)]
    size: Option<u32>,
    #[serde(default)]
    publish_time: Option<i64>,
    #[serde(default)]
    songs: Vec<Song>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiArtist {
    #[serde(default)]
    id: Option<u64>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    pic_url: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArtistDetailResponse {
    #[serde(default)]
    code: Option<i64>,
    #[serde(default)]
    hot_songs: Vec<Song>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Song {
    #[serde(default)]
    id: Option<u64>,
    #[serde(default)]
    name: Option<String>,
    /// Milliseconds.
    #[serde(default)]
    duration: Option<f64>,
    #[serde(default)]
    artists: Vec<Artist>,
    #[serde(default)]
    album: Option<Album>,
    /// Song-level publish timestamp (ms or seconds), when the endpoint has it.
    #[serde(default)]
    publish_time: Option<i64>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct Artist {
    #[serde(default)]
    id: Option<u64>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Album {
    #[serde(default)]
    name: Option<String>,
    /// Only present on the `song/detail` endpoint, not on search results.
    #[serde(default)]
    pic_url: Option<String>,
    /// Milliseconds since the epoch.
    #[serde(default)]
    publish_time: Option<i64>,
    #[serde(default)]
    artist: Option<Artist>,
    #[serde(default)]
    artists: Vec<Artist>,
}

// ---------------------------------------------------------------------------
// Public provider API
// ---------------------------------------------------------------------------

/// Search Netease and return parsed tracks (no playback URLs, only metadata).
pub fn search(http: &Http, query: &str, limit: usize) -> Result<Vec<Track>, String> {
    let limit = limit.clamp(1, 50);
    let url = format!(
        "{SEARCH_URL}?s={}&type=1&limit={limit}",
        util::pct(query.trim())
    );
    let response: SearchResponse = http
        .get_json(&url, &[("Referer", REFERER)])
        .map_err(|err| format!("Netease search failed: {err}"))?;
    if let Some(code) = response.code {
        if code != 200 {
            return Err(format!("Netease API error (code {code})"));
        }
    }
    let songs = response
        .result
        .map(|result| result.songs)
        .unwrap_or_default();
    if songs.is_empty() {
        return Ok(Vec::new());
    }
    // Search results omit `album.picUrl`; one batched detail call enriches the
    // whole page with cover art, album artist and publish year.
    let ids: Vec<u64> = songs.iter().filter_map(|song| song.id).collect();
    let details = fetch_details(http, &ids).unwrap_or_default();
    Ok(build_tracks(&songs, &details, limit))
}

/// Import a Netease song from its numeric id. Falls back to `yt-dlp` metadata
/// for links where only a page URL is known.
pub fn import(
    http: &Http,
    config: &ResolveConfig,
    url: &str,
    song_id: Option<u64>,
) -> Result<Track, String> {
    let Some(id) = song_id else {
        return super::ytdlp::import(config, url, Platform::Netease);
    };
    match track_from_id(http, id) {
        Ok(track) => Ok(track),
        Err(api_error) => {
            // The public API sometimes hides region-blocked songs; yt-dlp may
            // still know the title, so keep that as the fallback.
            super::ytdlp::import(config, url, Platform::Netease).map_err(|_| api_error)
        }
    }
}

/// Resolve a single Netease song id to a `Track` via the detail endpoint.
pub fn track_from_id(http: &Http, id: u64) -> Result<Track, String> {
    let details = fetch_details(http, &[id])
        .map_err(|err| format!("Netease song {id} could not be loaded: {err}"))?;
    details
        .get(&id)
        .and_then(build_track)
        .ok_or_else(|| format!("Netease song {id} was not found (removed or region locked)."))
}

// ---------------------------------------------------------------------------
// Albums
// ---------------------------------------------------------------------------

fn album_artists(album: &ApiAlbum) -> String {
    let names = util::join_names(
        album
            .artists
            .iter()
            .filter_map(|artist| artist.name.as_deref()),
    );
    if names.is_empty() {
        album
            .artist
            .as_ref()
            .and_then(|artist| artist.name.as_deref())
            .and_then(util::clean)
            .unwrap_or_default()
    } else {
        names
    }
}

fn api_album_to_model(album: &ApiAlbum) -> Option<ModelAlbum> {
    let id = album.id?;
    let title = album.name.as_deref().and_then(util::clean)?;
    let mut out = ModelAlbum::new(
        Platform::Netease,
        &id.to_string(),
        title,
        album_artists(album),
    );
    if let Some(art) = album.pic_url.as_deref().and_then(util::https_url) {
        out = out.with_art(art);
    }
    if let Some(year) = album
        .publish_time
        .map(|ms| ms.saturating_add(8 * 3_600_000))
        .and_then(util::year_from_unix_ms)
    {
        out = out.with_year(year);
    }
    if let Some(size) = album.size {
        out = out.with_track_count(size);
    }
    out = out.with_page(format!("{ALBUM_PAGE}{id}"));
    Some(out)
}

/// Search Netease albums (`type=10`).
pub fn search_albums(http: &Http, query: &str, limit: usize) -> Result<Vec<ModelAlbum>, String> {
    let limit = limit.clamp(1, 50);
    let url = format!(
        "{SEARCH_URL}?s={}&type=10&limit={limit}",
        util::pct(query.trim())
    );
    let response: AlbumSearchResponse = http
        .get_json(&url, &[("Referer", REFERER)])
        .map_err(|err| format!("Netease album search failed: {err}"))?;
    if let Some(code) = response.code {
        if code != 200 {
            return Err(format!("Netease album API error (code {code})"));
        }
    }
    let albums = response
        .result
        .map(|result| result.albums)
        .unwrap_or_default();
    Ok(albums
        .iter()
        .filter_map(api_album_to_model)
        .take(limit)
        .collect())
}

/// Fetch album metadata + songs for the album page.
pub fn album_detail(http: &Http, album: &ModelAlbum) -> Result<(ModelAlbum, Vec<Track>), String> {
    let id = album
        .source_id
        .trim()
        .parse::<u64>()
        .map_err(|_| "This Netease album has an invalid source id.".to_owned())?;
    let url = format!("{ALBUM_DETAIL_URL}{id}");
    let raw = http
        .get_text(&url, &[("Referer", REFERER)])
        .map_err(|err| format!("Netease album lookup failed: {err}"))?;
    let response: AlbumDetailResponse =
        serde_json::from_str(&raw).map_err(|err| format!("Netease album parse failed: {err}"))?;
    if let Some(code) = response.code {
        if code != 200 {
            return Err(format!("Netease album API error (code {code})"));
        }
    }
    let detail = response.album.ok_or_else(|| {
        "This Netease album is not available (removed or region locked).".to_owned()
    })?;
    let mut model = ModelAlbum::new(
        Platform::Netease,
        &id.to_string(),
        detail
            .name
            .as_deref()
            .and_then(util::clean)
            .unwrap_or_else(|| album.title.clone()),
        album.artist.clone(),
    );
    if let Some(art) = detail.pic_url.as_deref().and_then(util::https_url) {
        model = model.with_art(art);
    }
    if let Some(year) = detail
        .publish_time
        .map(|ms| ms.saturating_add(8 * 3_600_000))
        .and_then(util::year_from_unix_ms)
    {
        model = model.with_year(year);
    }
    if let Some(size) = detail.size {
        model = model.with_track_count(size);
    }
    model = model.with_page(format!("{ALBUM_PAGE}{id}"));
    let mut tracks = Vec::new();
    for song in &detail.songs {
        if let Some(mut track) = build_track(song) {
            if track.album.is_none() {
                track.album = Some(model.title.clone());
            }
            if track.art_url.is_none() {
                track.art_url = model.art_url.clone();
            }
            tracks.push(track);
        }
    }
    model.page_url = Some(format!("{ALBUM_PAGE}{id}"));
    Ok((model, tracks))
}

/// Other albums by a Netease artist (album series/discography).
pub fn artist_albums(
    http: &Http,
    artist_name: &str,
    limit: usize,
) -> Result<Vec<ModelAlbum>, String> {
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

// ---------------------------------------------------------------------------
// Artists (producer search + artist page)
// ---------------------------------------------------------------------------

/// Search Netease artists (`type=100`).
pub fn search_artists(http: &Http, query: &str, limit: usize) -> Result<Vec<ModelArtist>, String> {
    let limit = limit.clamp(1, 50);
    let url = format!(
        "{SEARCH_URL}?s={}&type=100&limit={limit}",
        util::pct(query.trim())
    );
    let response: ArtistSearchResponse = http
        .get_json(&url, &[("Referer", REFERER)])
        .map_err(|err| format!("Netease artist search failed: {err}"))?;
    if let Some(code) = response.code {
        if code != 200 {
            return Err(format!("Netease artist API error (code {code})"));
        }
    }
    let items = response
        .result
        .map(|result| result.artists)
        .unwrap_or_default();
    Ok(build_artists(&items, limit))
}

/// Hot songs for a Netease artist: `/api/artist/{id}` first, then a filtered
/// song search by the artist name as fallback.
pub fn artist_hot_tracks(
    http: &Http,
    artist: &ModelArtist,
    limit: usize,
) -> Result<Vec<Track>, String> {
    let limit = limit.clamp(1, 30);
    let artist_id = artist.source_id.trim().parse::<u64>().ok();
    let mut first_error = None;
    if let Some(id) = artist_id {
        match fetch_artist_detail(http, id) {
            Ok(mut response) => {
                enrich_hot_song_dates(http, &mut response.hot_songs);
                let tracks =
                    hot_songs_to_tracks(&response.hot_songs, Some(id), &artist.name, limit);
                if !tracks.is_empty() {
                    return Ok(tracks);
                }
            }
            Err(err) => first_error = Some(err),
        }
    }
    // Fallback: search songs by the artist name and keep only that artist's rows.
    let searched = search(http, &artist.name, limit.saturating_mul(3))?;
    let filtered = filter_artist_tracks(searched, &artist.name, limit);
    if filtered.is_empty() {
        return Err(first_error.unwrap_or_else(|| {
            format!("Netease has no hot songs for {} right now.", artist.name)
        }));
    }
    Ok(filtered)
}

fn fetch_artist_detail(http: &Http, id: u64) -> Result<ArtistDetailResponse, String> {
    let url = format!("{ARTIST_DETAIL_URL}{id}");
    let response: ArtistDetailResponse = http
        .get_json(&url, &[("Referer", REFERER)])
        .map_err(|err| err.to_string())?;
    if let Some(code) = response.code {
        if code != 200 {
            return Err(format!("code {code}"));
        }
    }
    Ok(response)
}

fn build_artists(items: &[ApiArtist], limit: usize) -> Vec<ModelArtist> {
    let mut artists: Vec<ModelArtist> = Vec::new();
    let mut seen: Vec<u64> = Vec::new();
    for item in items {
        let Some(id) = item.id else {
            continue;
        };
        if seen.contains(&id) {
            continue;
        }
        let Some(name) = item.name.as_deref().and_then(util::clean) else {
            continue;
        };
        seen.push(id);
        let mut artist = ModelArtist::new(Platform::Netease, &id.to_string(), name);
        if let Some(image) = item.pic_url.as_deref().and_then(util::https_url) {
            artist = artist.with_image(image);
        }
        artist = artist.with_page(format!("{ARTIST_PAGE}{id}"));
        artists.push(artist);
        if artists.len() >= limit {
            break;
        }
    }
    artists
}

/// Keep only hot songs that actually credit this artist (primary or feature).
fn hot_songs_to_tracks(
    songs: &[Song],
    artist_id: Option<u64>,
    artist_name: &str,
    limit: usize,
) -> Vec<Track> {
    let mut tracks = Vec::new();
    let mut seen: Vec<u64> = Vec::new();
    for song in songs {
        let Some(id) = song.id else {
            continue;
        };
        if seen.contains(&id) {
            continue;
        }
        let credit = song_artist_names(song);
        let relevant = match artist_id {
            Some(id) => {
                song.artists.iter().any(|artist| artist.id == Some(id)) && !credit.is_empty()
            }
            None => false,
        };
        if !relevant && util::name_relevance(artist_name, &credit) == 0 {
            continue;
        }
        let Some(track) = build_track_pair(song, None) else {
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

/// Keep only search results that credit this artist (fallback for endpoints).
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

/// Best publish timestamp on a song: song level first, then its album.
fn song_publish_time(song: &Song) -> Option<i64> {
    song.publish_time
        .or_else(|| song.album.as_ref().and_then(|album| album.publish_time))
}

/// Hot-song payloads sometimes omit `publishTime`; one batched detail request
/// fills the dates in for the rows that need it (best effort, errors ignored).
fn enrich_hot_song_dates(http: &Http, songs: &mut [Song]) {
    let ids: Vec<u64> = songs
        .iter()
        .filter(|song| song_publish_time(song).is_none())
        .filter_map(|song| song.id)
        .take(30)
        .collect();
    if ids.is_empty() {
        return;
    }
    let Ok(details) = fetch_details(http, &ids) else {
        return;
    };
    for song in songs.iter_mut() {
        if song_publish_time(song).is_some() {
            continue;
        }
        let Some(id) = song.id else {
            continue;
        };
        let Some(detail) = details.get(&id) else {
            continue;
        };
        let publish_time = song_publish_time(detail);
        if let Some(value) = publish_time {
            song.publish_time = Some(value);
        }
    }
}

fn song_artist_names(song: &Song) -> String {
    util::join_names(
        song.artists
            .iter()
            .filter_map(|artist| artist.name.as_deref()),
    )
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// One request for up to a full results page. Errors are non fatal: the caller
/// keeps the un-enriched search tracks.
fn fetch_details(http: &Http, ids: &[u64]) -> Result<HashMap<u64, Song>, String> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let list: Vec<String> = ids.iter().map(u64::to_string).collect();
    let encoded = util::pct(&format!("[{}]", list.join(",")));
    let url = format!("{DETAIL_URL}?id={}&ids={encoded}", ids[0]);
    let response: DetailResponse = http
        .get_json(&url, &[("Referer", REFERER)])
        .map_err(|err| err.to_string())?;
    if let Some(code) = response.code {
        if code != 200 {
            return Err(format!("code {code}"));
        }
    }
    let mut by_id: HashMap<u64, Song> = HashMap::new();
    for song in response.songs {
        if let Some(id) = song.id {
            by_id.insert(id, song);
        }
    }
    Ok(by_id)
}

fn build_tracks(search: &[Song], details: &HashMap<u64, Song>, limit: usize) -> Vec<Track> {
    let mut tracks = Vec::new();
    let mut seen = Vec::new();
    for song in search {
        let Some(id) = song.id else {
            continue;
        };
        if seen.contains(&id) {
            continue;
        }
        seen.push(id);
        let enriched = details.get(&id);
        if let Some(track) = build_track_pair(song, enriched) {
            tracks.push(track);
            if tracks.len() >= limit {
                break;
            }
        }
    }
    tracks
}

fn build_track(song: &Song) -> Option<Track> {
    build_track_pair(song, None)
}

fn build_track_pair(search: &Song, detail: Option<&Song>) -> Option<Track> {
    let id = search.id.or_else(|| detail.and_then(|song| song.id))?;
    let title = search.name.as_deref().and_then(util::clean).or_else(|| {
        detail
            .and_then(|song| song.name.as_deref())
            .and_then(util::clean)
    })?;
    let merged_album = detail
        .and_then(|song| song.album.as_ref())
        .or(search.album.as_ref());
    let artist = {
        let mut artist = artist_names(detail.unwrap_or(search));
        if artist.is_empty() {
            artist = artist_names(search);
        }
        if artist.is_empty() {
            artist = merged_album.map(album_artist_names).unwrap_or_default();
        }
        artist
    };
    let duration = detail
        .and_then(|song| song.duration)
        .or(search.duration)
        .and_then(util::seconds_from_ms);
    let album = merged_album
        .and_then(|album| album.name.as_deref())
        .and_then(util::clean);
    let art = merged_album
        .and_then(|album| album.pic_url.as_deref())
        .and_then(util::https_url);
    // Netease publishes `publishTime` as local (UTC+8) midnight. Prefer the
    // song-level value, then the album value; both may be ms or seconds.
    let publish_time = detail
        .and_then(song_publish_time)
        .or_else(|| song_publish_time(search))
        .or_else(|| merged_album.and_then(|album| album.publish_time));
    let year = publish_time
        .map(|ms| ms.saturating_add(8 * 3_600_000))
        .and_then(util::year_from_unix_ms);
    let release_date = publish_time.and_then(|value| util::date_from_epoch_value(value, 8 * 3_600));

    let mut track = Track::new(Platform::Netease, &id.to_string(), title, artist);
    if let Some(album) = album {
        track = track.with_album(album);
    }
    if let Some(duration) = duration {
        track = track.with_duration(duration);
    }
    if let Some(art) = art {
        track = track.with_art(art);
    }
    track = track.with_page(format!("{SONG_PAGE}{id}"));
    if let Some(year) = year {
        track = track.with_year(year);
    }
    if let Some(date) = release_date {
        track = track.with_release_date(date);
    }
    Some(track)
}

fn artist_names(song: &Song) -> String {
    util::join_names(
        song.artists
            .iter()
            .filter_map(|artist| artist.name.as_deref()),
    )
}

fn album_artist_names(album: &Album) -> String {
    let from_list = util::join_names(
        album
            .artists
            .iter()
            .filter_map(|artist| artist.name.as_deref()),
    );
    if from_list.is_empty() {
        album
            .artist
            .as_ref()
            .and_then(|artist| artist.name.as_deref())
            .and_then(util::clean)
            .unwrap_or_default()
    } else {
        from_list
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH_FIXTURE: &str = r#"{
        "result": {
            "songs": [
                {
                    "id": 347230,
                    "name": "海阔天空",
                    "duration": 326000,
                    "artists": [{ "name": "Beyond" }, { "name": "黄家驹" }],
                    "album": { "id": 3170026, "name": "海阔天空", "picId": 7954966627607511 }
                },
                {
                    "id": 999,
                    "name": "Song Without Album",
                    "duration": 0,
                    "artists": [],
                    "album": null,
                    "publishTime": 1470931200000
                },
                {
                    "name": "No Id Is Skipped",
                    "duration": 1000
                }
            ],
            "songCount": 3
        },
        "code": 200
    }"#;

    const DETAIL_FIXTURE: &str = r#"{
        "songs": [
            {
                "id": 347230,
                "name": "海阔天空",
                "duration": 324986,
                "artists": [{ "name": "Beyond" }],
                "album": {
                    "id": 3170026,
                    "name": "海阔天空",
                    "picUrl": "http://p1.music.126.net/K_p-jhEvCpP7I4Sqd_QfFw==/7954966627607511.jpg",
                    "publishTime": 788889600004
                }
            }
        ],
        "code": 200
    }"#;

    fn parse_search(json: &str) -> Vec<Song> {
        serde_json::from_str::<SearchResponse>(json)
            .expect("search fixture parses")
            .result
            .expect("result")
            .songs
    }

    fn parse_details(json: &str) -> HashMap<u64, Song> {
        let response: DetailResponse = serde_json::from_str(json).expect("detail fixture parses");
        response
            .songs
            .into_iter()
            .filter_map(|song| song.id.map(|id| (id, song)))
            .collect()
    }

    #[test]
    fn search_tracks_are_built_without_details() {
        let tracks = build_tracks(&parse_search(SEARCH_FIXTURE), &HashMap::new(), 10);
        assert_eq!(tracks.len(), 2, "song without id is skipped");
        let first = &tracks[0];
        assert_eq!(first.title, "海阔天空");
        assert_eq!(first.artist, "Beyond, 黄家驹");
        assert_eq!(first.album.as_deref(), Some("海阔天空"));
        assert_eq!(first.duration_secs, Some(326.0));
        assert_eq!(first.platform, Platform::Netease);
        assert_eq!(
            first.page_url.as_deref(),
            Some("https://music.163.com/song?id=347230")
        );
        assert!(first.art_url.is_none(), "search payload has no picUrl");

        let second = &tracks[1];
        assert_eq!(second.title, "Song Without Album");
        assert_eq!(second.artist, "");
        assert!(second.album.is_none());
        assert!(second.duration_secs.is_none());
        assert!(second.art_url.is_none());
        assert_eq!(
            second.release_date.as_deref(),
            Some("2016-08-12"),
            "song-level publishTime (ms) becomes a date"
        );
    }

    #[test]
    fn details_enrich_cover_album_and_year() {
        let details = parse_details(DETAIL_FIXTURE);
        let tracks = build_tracks(&parse_search(SEARCH_FIXTURE), &details, 10);
        let first = &tracks[0];
        assert_eq!(first.duration_secs, Some(324.986));
        assert_eq!(
            first.art_url.as_deref(),
            Some("https://p1.music.126.net/K_p-jhEvCpP7I4Sqd_QfFw==/7954966627607511.jpg")
        );
        assert_eq!(first.year, Some(1995));
        assert_eq!(
            first.release_date.as_deref(),
            Some("1995-01-01"),
            "album publishTime is local (UTC+8) midnight"
        );
        assert_eq!(first.artist, "Beyond");
    }

    #[test]
    fn limit_caps_the_result_list() {
        let tracks = build_tracks(&parse_search(SEARCH_FIXTURE), &HashMap::new(), 1);
        assert_eq!(tracks.len(), 1);
    }

    #[test]
    fn bad_json_is_rejected() {
        assert!(serde_json::from_str::<SearchResponse>("{\"result\": 42}").is_err());
    }
    const ARTIST_SEARCH_FIXTURE: &str = r#"{
        "result": {
            "artists": [
                {
                    "id": 185871,
                    "name": "Porter Robinson",
                    "picUrl": "http://p1.music.126.net/porter.jpg",
                    "albumSize": 69,
                    "musicSize": 515
                },
                {
                    "id": 950526031,
                    "name": "Daniel Porter Robinson",
                    "picUrl": null
                },
                { "name": "No Id Artist" }
            ],
            "artistCount": 3
        },
        "code": 200
    }"#;

    const ARTIST_DETAIL_FIXTURE: &str = r#"{
        "artist": { "id": 185871, "name": "Porter Robinson" },
        "hotSongs": [
            {
                "id": 425280053,
                "name": "Shelter",
                "duration": 218971,
                "artists": [
                    { "id": 185871, "name": "Porter Robinson" },
                    { "id": 40280, "name": "Madeon" }
                ],
                "album": {
                    "id": 1,
                    "name": "Shelter",
                    "picUrl": "http://p1.music.126.net/shelter.jpg",
                    "publishTime": 1470931200007
                }
            },
            {
                "id": 2,
                "name": "Not Porter",
                "duration": 100000,
                "artists": [{ "id": 999, "name": "Somebody Else" }],
                "album": { "id": 2, "name": "Other" }
            }
        ],
        "code": 200
    }"#;

    fn parse_artist_search(json: &str) -> Vec<ApiArtist> {
        let response: ArtistSearchResponse = serde_json::from_str(json).expect("artist fixture");
        response.result.expect("result").artists
    }

    #[test]
    fn artist_search_is_parsed_and_built() {
        let artists = build_artists(&parse_artist_search(ARTIST_SEARCH_FIXTURE), 10);
        assert_eq!(artists.len(), 2, "artist without id is skipped");
        let first = &artists[0];
        assert_eq!(first.name, "Porter Robinson");
        assert_eq!(first.platform, Platform::Netease);
        assert_eq!(first.source_id, "185871");
        assert_eq!(
            first.image_url.as_deref(),
            Some("https://p1.music.126.net/porter.jpg"),
            "http image is upgraded to https"
        );
        assert_eq!(
            first.page_url.as_deref(),
            Some("https://music.163.com/#/artist?id=185871")
        );
        let second = &artists[1];
        assert!(second.image_url.is_none(), "null picUrl is tolerated");
    }

    #[test]
    fn artist_limit_caps_the_list() {
        assert_eq!(
            build_artists(&parse_artist_search(ARTIST_SEARCH_FIXTURE), 1).len(),
            1
        );
    }

    #[test]
    fn hot_songs_are_filtered_to_the_artist() {
        let response: ArtistDetailResponse =
            serde_json::from_str(ARTIST_DETAIL_FIXTURE).expect("detail fixture");
        let tracks = hot_songs_to_tracks(&response.hot_songs, Some(185_871), "Porter Robinson", 10);
        assert_eq!(tracks.len(), 1, "only songs crediting the artist survive");
        let track = &tracks[0];
        assert_eq!(track.title, "Shelter");
        assert_eq!(track.artist, "Porter Robinson, Madeon");
        assert_eq!(track.duration_secs, Some(218.971));
        assert_eq!(
            track.art_url.as_deref(),
            Some("https://p1.music.126.net/shelter.jpg")
        );
        assert_eq!(
            track.page_url.as_deref(),
            Some("https://music.163.com/song?id=425280053")
        );
        assert_eq!(track.year, Some(2016));
        assert_eq!(track.release_date.as_deref(), Some("2016-08-12"));
    }

    #[test]
    fn fallback_track_filter_requires_the_artist_name() {
        let tracks = vec![
            Track::new(Platform::Netease, "1", "Shelter", "Porter Robinson, Madeon"),
            Track::new(Platform::Netease, "2", "Song", "Somebody Else"),
            Track::new(Platform::Netease, "3", "Sad Machine", "Porter Robinson"),
        ];
        let filtered = filter_artist_tracks(tracks, "Porter Robinson", 10);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].title, "Shelter");
        assert_eq!(filtered[1].title, "Sad Machine");
    }
}
