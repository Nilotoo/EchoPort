//! Catalogue providers: search metadata on each platform and gather background
//! material for a song (owned by the providers workstream).
//!
//! Providers only deal with metadata. Playback is always handled by
//! `resolver`, which may match a track to a public audio source via yt-dlp.
//!
//! Design rules (see `docs/CONTRACT.md` §5/§8):
//! * zero accounts, zero DRM: Spotify/Apple are metadata + 30 s previews only;
//! * one platform failing must never break the others;
//! * all functions are blocking and panic free — run them on a worker thread;
//! * yt-dlp work is delegated to `resolver` (which caps global concurrency).

use crate::model::{
    Artist, ArtistDetail, Platform, PlatformAlbumResults, PlatformArtistResults, PlatformResults,
    SongBackground, Track,
};
use crate::net::Http;
use crate::resolver::{self, ResolveConfig, ResolveError};

pub mod apple;
pub mod archive;
pub(crate) mod artists;
pub mod netease;
pub mod spotify;
pub(crate) mod util;
pub mod ytdlp;

/// Short hint shown instead of Spotify search results. Anonymous Spotify
/// search no longer exists, so a link is the only way in.
pub const SPOTIFY_SEARCH_HINT: &str =
    "Spotify search needs a link — paste a track link to import it.";

const UNSUPPORTED_LINK: &str = "Unsupported link — paste a Spotify, Apple Music, YouTube, \
     SoundCloud, Bilibili or Netease track link.";

/// Shown instead of the raw yt-dlp error when Bilibili's anti-bot layer blocks
/// the search (HTTP 412 / 567). Bilingual on purpose: the platform error string
/// is displayed verbatim by the UI, which does not translate provider errors.
const BILIBILI_BLOCKED_HINT: &str = "Bilibili 当前网络被反爬拦截（HTTP 412），请更换网络或使用其它音源后重试。 \
     / Bilibili is blocking this network (HTTP 412 anti-bot) — try again on another network or source.";

/// Search every requested platform. One result group per platform, in the
/// order requested. Implementations never panic and report per platform errors
/// via `PlatformResults::error` instead of failing the whole search.
pub fn search(
    http: &Http,
    config: &ResolveConfig,
    query: &str,
    platforms: &[Platform],
    limit: usize,
) -> Vec<PlatformResults> {
    let query = query.trim();
    let limit = limit.clamp(1, 50);
    let mut selected: Vec<Platform> = Vec::new();
    for platform in platforms {
        if !selected.contains(platform) {
            selected.push(*platform);
        }
    }
    if selected.is_empty() {
        return Vec::new();
    }
    if query.is_empty() {
        return selected
            .into_iter()
            .map(|platform| PlatformResults {
                platform,
                tracks: Vec::new(),
                error: Some("Empty search — type an artist or song name.".to_owned()),
            })
            .collect();
    }

    // Scoped threads let the HTTP catalogues run concurrently while borrowing
    // `http`/`config`. yt-dlp based platforms may serialise inside `resolver`;
    // that is the resolver's job and each platform still reports separately.
    std::thread::scope(|scope| {
        let handles: Vec<_> = selected
            .iter()
            .map(|platform| {
                let platform = *platform;
                let handle =
                    scope.spawn(move || guarded_search(http, config, query, platform, limit));
                (platform, handle)
            })
            .collect();
        handles
            .into_iter()
            .map(|(platform, handle)| {
                handle.join().unwrap_or_else(|_| PlatformResults {
                    platform,
                    tracks: Vec::new(),
                    error: Some("This platform failed unexpectedly.".to_owned()),
                })
            })
            .collect()
    })
}

fn guarded_search(
    http: &Http,
    config: &ResolveConfig,
    query: &str,
    platform: Platform,
    limit: usize,
) -> PlatformResults {
    let guarded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        search_platform(http, config, query, platform, limit)
    }));
    guarded.unwrap_or_else(|_| PlatformResults {
        platform,
        tracks: Vec::new(),
        error: Some("This platform failed unexpectedly.".to_owned()),
    })
}

fn search_platform(
    http: &Http,
    config: &ResolveConfig,
    query: &str,
    platform: Platform,
    limit: usize,
) -> PlatformResults {
    let outcome: Result<Vec<Track>, String> = match platform {
        Platform::Local => Err("Local files are searched from the Library, not online.".to_owned()),
        Platform::Spotify => {
            return PlatformResults {
                platform,
                tracks: Vec::new(),
                error: Some(SPOTIFY_SEARCH_HINT.to_owned()),
            };
        }
        Platform::AppleMusic => apple::search(http, query, limit),
        Platform::Netease => netease::search(http, query, limit),
        Platform::YouTube | Platform::SoundCloud | Platform::Bilibili => {
            resolver_search(config, platform, query, limit)
        }
        Platform::InternetArchive => archive::search(http, query, limit),
    };
    match outcome {
        Ok(tracks) => {
            // Re-rank by relevance to the query so the UI's "by relevance"
            // lists put exact title/artist hits first.
            let ranked = util::rank_by_score(tracks, |track| {
                util::track_relevance(query, &track.title, &track.artist)
            });
            PlatformResults {
                platform,
                tracks: sanitize(ranked, limit),
                error: None,
            }
        }
        Err(error) => PlatformResults {
            platform,
            tracks: Vec::new(),
            error: Some(error),
        },
    }
}

fn resolver_search(
    config: &ResolveConfig,
    platform: Platform,
    query: &str,
    limit: usize,
) -> Result<Vec<Track>, String> {
    resolver::ytdlp_search(config, platform, query, limit)
        .map_err(|err| friendly_resolver_error(platform, &err))
}

/// User-facing search error. One failing platform stays local to its result
/// group; Bilibili's anti-bot responses are replaced with a bilingual hint so
/// the raw yt-dlp text (HTTP 412 / 567 Precondition Failed) is not shown.
fn friendly_resolver_error(platform: Platform, error: &ResolveError) -> String {
    if matches!(error, ResolveError::YtDlpMissing) {
        return "yt-dlp is not installed — install it to search YouTube, SoundCloud and Bilibili."
            .to_owned();
    }
    let text = error.to_string();
    if platform == Platform::Bilibili && is_bilibili_anti_bot(&text) {
        return BILIBILI_BLOCKED_HINT.to_owned();
    }
    text
}

/// yt-dlp reports Bilibili blocks as `HTTP Error 412: Precondition Failed`,
/// `HTTP Error 567` or "risk control" messages, sometimes wrapped in other
/// extractor errors. Match those markers instead of the exact wording.
fn is_bilibili_anti_bot(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("412")
        || lower.contains("567")
        || lower.contains("precondition")
        || lower.contains("risk control")
        || lower.contains("anti-bot")
        || lower.contains("too many requests")
}

/// Drop empty/duplicate entries and never exceed the requested limit.
fn sanitize(tracks: Vec<Track>, limit: usize) -> Vec<Track> {
    let mut seen: Vec<String> = Vec::new();
    let mut kept = Vec::new();
    for track in tracks {
        if track.title.trim().is_empty() || seen.contains(&track.id) {
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

// ---------------------------------------------------------------------------
// Link import
// ---------------------------------------------------------------------------

/// What a pasted URL points at.
#[derive(Clone, Debug, PartialEq, Eq)]
enum LinkTarget {
    Spotify(spotify::SpotifyRef),
    Apple(apple::AppleRef),
    YouTube,
    SoundCloud,
    Bilibili,
    Netease(Option<u64>),
}

/// Search producers / artists on each requested platform, ranked by relevance.
/// Search every requested platform for producer/artist cards. One result
/// group per platform, ranked by relevance; per-platform failures stay local.
///
/// Netease and Apple Music have real artist catalogues; YouTube, SoundCloud
/// and Bilibili derive creator cards from their track search (channel names
/// matching the query). Spotify has no anonymous search, so its artist group
/// is empty (the track group already carries the "paste a link" hint).
pub fn search_artists(
    http: &Http,
    config: &ResolveConfig,
    query: &str,
    platforms: &[Platform],
    limit: usize,
) -> Vec<PlatformArtistResults> {
    let query = query.trim();
    let limit = limit.clamp(1, 50);
    let mut selected: Vec<Platform> = Vec::new();
    for platform in platforms {
        if !selected.contains(platform) {
            selected.push(*platform);
        }
    }
    if selected.is_empty() {
        return Vec::new();
    }
    if query.is_empty() {
        return selected
            .into_iter()
            .map(|platform| PlatformArtistResults {
                platform,
                artists: Vec::new(),
                error: Some("Empty search — type an artist or producer name.".to_owned()),
            })
            .collect();
    }

    std::thread::scope(|scope| {
        let handles: Vec<_> = selected
            .iter()
            .map(|platform| {
                let platform = *platform;
                let handle = scope
                    .spawn(move || guarded_artist_search(http, config, query, platform, limit));
                (platform, handle)
            })
            .collect();
        handles
            .into_iter()
            .map(|(platform, handle)| {
                handle.join().unwrap_or_else(|_| PlatformArtistResults {
                    platform,
                    artists: Vec::new(),
                    error: Some("This platform failed unexpectedly.".to_owned()),
                })
            })
            .collect()
    })
}

fn guarded_artist_search(
    http: &Http,
    config: &ResolveConfig,
    query: &str,
    platform: Platform,
    limit: usize,
) -> PlatformArtistResults {
    let guarded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        artist_search_platform(http, config, query, platform, limit)
    }));
    guarded.unwrap_or_else(|_| PlatformArtistResults {
        platform,
        artists: Vec::new(),
        error: Some("This platform failed unexpectedly.".to_owned()),
    })
}

fn artist_search_platform(
    http: &Http,
    config: &ResolveConfig,
    query: &str,
    platform: Platform,
    limit: usize,
) -> PlatformArtistResults {
    let outcome: Result<Vec<Artist>, String> = match platform {
        Platform::Local => Err("Local files have no online artist catalogue.".to_owned()),
        // The Spotify track group already shows the link-import hint; keep the
        // artist group quiet instead of repeating it.
        Platform::Spotify => {
            return PlatformArtistResults {
                platform,
                artists: Vec::new(),
                error: None,
            };
        }
        Platform::Netease => netease::search_artists(http, query, limit),
        Platform::AppleMusic => apple::search_artists(http, query, limit),
        Platform::YouTube | Platform::SoundCloud | Platform::Bilibili => {
            let search_limit = limit.saturating_mul(3).max(8);
            match resolver::ytdlp_search(config, platform, query, search_limit) {
                Ok(tracks) => Ok(artists::derive_creators(platform, query, &tracks, 2)),
                Err(err) => Err(friendly_resolver_error(platform, &err)),
            }
        }
        Platform::InternetArchive => Ok(Vec::new()),
    };
    match outcome {
        Ok(list) => PlatformArtistResults {
            platform,
            artists: artists::finalize_artists(list, query, limit),
            error: None,
        },
        Err(error) => PlatformArtistResults {
            platform,
            artists: Vec::new(),
            error: Some(error),
        },
    }
}

/// Best-effort artist portrait for platforms whose search response has no
/// portrait (currently Apple Music; Netease returns one directly).
pub fn artist_portrait(http: &Http, artist: &crate::model::Artist) -> Option<String> {
    match artist.platform {
        Platform::AppleMusic => apple::artist_portrait(http, artist),
        _ => None,
    }
}

/// Derive creator cards from already-fetched track results for platforms
/// without a public artist API (YouTube / SoundCloud / Bilibili). Reusing the
/// track search avoids a second, duplicate yt-dlp search.
pub fn artists_from_tracks(
    query: &str,
    results: &[PlatformResults],
    limit: usize,
) -> Vec<PlatformArtistResults> {
    results
        .iter()
        .filter(|group| {
            matches!(
                group.platform,
                Platform::YouTube | Platform::SoundCloud | Platform::Bilibili
            )
        })
        .map(|group| {
            let derived = artists::derive_creators(group.platform, query, &group.tracks, 2);
            let mut cards = artists::finalize_artists(derived, query, limit);
            // yt-dlp search rows do not expose channel avatars; use the first
            // matching track thumbnail as a useful, cacheable fallback.
            for card in &mut cards {
                if card.image_url.is_none() {
                    if let Some(track) = group.tracks.iter().find(|track| {
                        artists::clean_creator_name(&track.artist).as_deref()
                            == Some(card.name.as_str())
                    }) {
                        card.image_url = track.art_url.clone();
                    }
                }
            }
            PlatformArtistResults {
                platform: group.platform,
                artists: cards,
                error: None,
            }
        })
        .collect()
}

/// Search albums across the requested platforms (Apple / Netease only).
pub fn search_albums(
    http: &Http,
    query: &str,
    platforms: &[Platform],
    limit: usize,
) -> Vec<PlatformAlbumResults> {
    let query = query.trim();
    let limit = limit.clamp(1, 50);
    let mut selected = Vec::new();
    for platform in platforms {
        if !selected.contains(platform) {
            selected.push(*platform);
        }
    }
    if query.is_empty() {
        return selected
            .into_iter()
            .map(|platform| PlatformAlbumResults {
                platform,
                albums: Vec::new(),
                error: None,
            })
            .collect();
    }
    std::thread::scope(|scope| {
        let handles: Vec<_> = selected
            .iter()
            .map(|platform| {
                let platform = *platform;
                let handle = scope.spawn(move || {
                    let outcome: Result<Vec<crate::model::Album>, String> = match platform {
                        Platform::AppleMusic => apple::search_albums(http, query, limit),
                        Platform::Netease => netease::search_albums(http, query, limit),
                        _ => Ok(Vec::new()),
                    };
                    PlatformAlbumResults {
                        platform,
                        albums: outcome.unwrap_or_default(),
                        error: None,
                    }
                });
                (platform, handle)
            })
            .collect();
        handles
            .into_iter()
            .map(|(platform, handle)| {
                handle.join().unwrap_or_else(|_| PlatformAlbumResults {
                    platform,
                    albums: Vec::new(),
                    error: Some("This platform failed unexpectedly.".to_owned()),
                })
            })
            .collect()
    })
}

/// Load an album page and its same-artist follow-up albums. Track, series
/// and Wikipedia/background work run concurrently so the page does not wait
/// for the slowest source sequentially.
pub fn album_detail(http: &Http, album: &crate::model::Album) -> crate::model::AlbumDetail {
    let (bio, (fetched_album, tracks), series) = std::thread::scope(|scope| {
        let bio_handle =
            scope.spawn(|| crate::background::album_bio_for(http, &album.artist, &album.title));
        let tracks_handle = scope.spawn(|| -> (Option<crate::model::Album>, Vec<Track>) {
            match album.platform {
                Platform::AppleMusic => {
                    (None, apple::album_tracks(http, album).unwrap_or_default())
                }
                Platform::Netease => match netease::album_detail(http, album) {
                    Ok((meta, tracks)) => (Some(meta), tracks),
                    Err(_) => (None, Vec::new()),
                },
                _ => (None, Vec::new()),
            }
        });
        let series_handle = scope.spawn(|| match album.platform {
            Platform::AppleMusic => {
                apple::artist_albums(http, &album.artist, 12).unwrap_or_default()
            }
            Platform::Netease => {
                netease::artist_albums(http, &album.artist, 12).unwrap_or_default()
            }
            _ => Vec::new(),
        });
        (
            bio_handle.join().unwrap_or(None),
            tracks_handle.join().unwrap_or_default(),
            series_handle.join().unwrap_or_default(),
        )
    });
    let mut detail = crate::model::AlbumDetail {
        album: Some(fetched_album.unwrap_or_else(|| album.clone())),
        tracks,
        bio,
        series,
    };
    detail.series.retain(|candidate| candidate.id != album.id);
    detail
}

/// Load an artist/producer page: Wikipedia bio (strict relevance) plus hot
/// tracks from the artist's own platform. No accounts, no DRM.
pub fn artist_detail(http: &Http, config: &ResolveConfig, artist: &Artist) -> ArtistDetail {
    const HOT_TRACK_LIMIT: usize = 12;
    let mut detail = ArtistDetail {
        artist: Some(artist.clone()),
        bio: None,
        hot_tracks: Vec::new(),
    };
    detail.bio = crate::background::artist_bio_for(http, &artist.name);
    let tracks: Vec<Track> = match artist.platform {
        Platform::Netease => {
            netease::artist_hot_tracks(http, artist, HOT_TRACK_LIMIT).unwrap_or_default()
        }
        Platform::AppleMusic => {
            apple::artist_hot_tracks(http, artist, HOT_TRACK_LIMIT).unwrap_or_default()
        }
        Platform::YouTube | Platform::SoundCloud | Platform::Bilibili => {
            match artists::resolver_hot_tracks(config, artist, HOT_TRACK_LIMIT) {
                Ok(tracks) => tracks,
                Err(err) => {
                    log::debug!("artist hot tracks skipped: {err}");
                    Vec::new()
                }
            }
        }
        Platform::Spotify | Platform::Local | Platform::InternetArchive => Vec::new(),
    };
    detail.hot_tracks = finalize_hot_tracks(tracks, artist, HOT_TRACK_LIMIT);
    // Platforms without an artist avatar endpoint can still use the first
    // hot track's cover as a useful fallback.
    if detail
        .artist
        .as_ref()
        .map(|artist| artist.image_url.is_none())
        .unwrap_or(false)
    {
        if let Some(url) = detail
            .hot_tracks
            .first()
            .and_then(|track| track.art_url.clone())
        {
            if let Some(artist) = detail.artist.as_mut() {
                artist.image_url = Some(url);
            }
        }
    }
    detail
}

/// Defensive final pass: only tracks credited to this artist, deduped, capped.
fn finalize_hot_tracks(tracks: Vec<Track>, artist: &Artist, limit: usize) -> Vec<Track> {
    let mut kept: Vec<Track> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for track in tracks {
        if track.title.trim().is_empty() || util::name_relevance(&artist.name, &track.artist) == 0 {
            continue;
        }
        let key = util::normalize_match(&format!("{} {}", track.title, track.artist));
        if seen.iter().any(|existing| existing == &key) {
            continue;
        }
        seen.push(key);
        kept.push(track);
        if kept.len() >= limit {
            break;
        }
    }
    kept
}

/// Import metadata from a public platform URL (used for Spotify / Apple Music
/// where anonymous search is not available, and for any pasted link).
pub fn track_from_url(http: &Http, config: &ResolveConfig, url: &str) -> Result<Track, String> {
    let trimmed = url.trim();
    let target = resolve_link(trimmed)?;
    let normalized = normalize_url(trimmed);
    match target {
        LinkTarget::Spotify(reference) => spotify::import(http, &reference),
        LinkTarget::Apple(reference) => apple::import(http, &reference),
        LinkTarget::YouTube => ytdlp::import(config, &normalized, Platform::YouTube),
        LinkTarget::SoundCloud => ytdlp::import(config, &normalized, Platform::SoundCloud),
        LinkTarget::Bilibili => ytdlp::import(config, &normalized, Platform::Bilibili),
        LinkTarget::Netease(id) => netease::import(http, config, &normalized, id),
    }
}

/// Detect the platform behind a URL, returning a helpful error otherwise.
fn resolve_link(input: &str) -> Result<LinkTarget, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Paste a link first.".to_owned());
    }
    if trimmed.starts_with("spotify:") {
        return spotify::parse_spotify_url(trimmed).map(LinkTarget::Spotify);
    }
    let parsed = parse_web_url(trimmed).ok_or_else(|| UNSUPPORTED_LINK.to_owned())?;
    let scheme = parsed.scheme().to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return Err(UNSUPPORTED_LINK.to_owned());
    }
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host).to_owned();

    // Spotify: open.spotify.com, play.spotify.com, short links (clear error).
    if host == "open.spotify.com"
        || host.ends_with(".spotify.com")
        || host == "spotify.com"
        || host == "spotify.link"
        || host == "spotify.app.link"
    {
        return spotify::parse_spotify_url(trimmed).map(LinkTarget::Spotify);
    }
    if host == "music.apple.com"
        || host.ends_with(".music.apple.com")
        || host == "itunes.apple.com"
        || host.ends_with(".itunes.apple.com")
    {
        return apple::parse_apple_url(trimmed).map(LinkTarget::Apple);
    }
    if host == "youtube.com"
        || host.ends_with(".youtube.com")
        || host == "youtu.be"
        || host == "youtube-nocookie.com"
        || host.ends_with(".youtube-nocookie.com")
    {
        return Ok(LinkTarget::YouTube);
    }
    if host == "soundcloud.com"
        || host.ends_with(".soundcloud.com")
        || host == "snd.sc"
        || host == "on.soundcloud.com"
    {
        return Ok(LinkTarget::SoundCloud);
    }
    if host == "bilibili.com"
        || host.ends_with(".bilibili.com")
        || host == "b23.tv"
        || host.ends_with(".b23.tv")
    {
        return Ok(LinkTarget::Bilibili);
    }
    if host == "music.163.com"
        || host.ends_with(".music.163.com")
        || host == "163cn.tv"
        || host.ends_with(".163cn.tv")
        || host == "y.music.163.com"
    {
        return Ok(LinkTarget::Netease(parse_netease_id(&parsed)));
    }
    Err(UNSUPPORTED_LINK.to_owned())
}

fn parse_web_url(input: &str) -> Option<url::Url> {
    if input.contains("://") {
        return url::Url::parse(input).ok();
    }
    // Accept scheme-less pastes (`open.spotify.com/track/...`) when the input
    // at least looks like a domain path.
    let first = input.split('/').next().unwrap_or_default();
    if first.contains('.') && !first.contains(char::is_whitespace) {
        return url::Url::parse(&format!("https://{input}")).ok();
    }
    None
}

fn normalize_url(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.contains("://") {
        trimmed.to_owned()
    } else {
        format!("https://{trimmed}")
    }
}

/// Pull `id=<number>` out of a Netease *song* link (query, hash route or
/// `/song/<id>`). Album/playlist links return `None` so they fall back to
/// yt-dlp metadata.
fn parse_netease_id(url: &url::Url) -> Option<u64> {
    let segments: Vec<&str> = url
        .path_segments()
        .map(|parts| parts.collect())
        .unwrap_or_default();
    let fragment = url.fragment().unwrap_or_default();
    let fragment_lower = fragment.to_ascii_lowercase();
    let is_song = segments
        .iter()
        .any(|part| part.eq_ignore_ascii_case("song"))
        || fragment_lower.contains("song");
    if !is_song {
        return None;
    }
    if let Some(id) = url
        .query_pairs()
        .find(|(key, _)| key == "id")
        .and_then(|(_, value)| value.parse::<u64>().ok())
    {
        return Some(id);
    }
    if let Some((_, query)) = fragment.split_once('?') {
        if let Some(id) = url::form_urlencoded::parse(query.as_bytes())
            .find(|(key, _)| key == "id")
            .and_then(|(_, value)| value.parse::<u64>().ok())
        {
            return Some(id);
        }
    }
    segments
        .iter()
        .position(|part| part.eq_ignore_ascii_case("song"))
        .and_then(|index| segments.get(index + 1))
        .and_then(|part| part.parse::<u64>().ok())
        .or_else(|| {
            fragment
                .split(['/', '?', '&'])
                .filter_map(|part| part.parse::<u64>().ok())
                .next()
        })
}

// ---------------------------------------------------------------------------
// Background
// ---------------------------------------------------------------------------

/// Fetch Wikipedia biography, YouTube interviews/explanations and useful links.
pub fn background(http: &Http, config: &ResolveConfig, track: &Track) -> SongBackground {
    crate::background::fetch(http, config, track)
}

// ---------------------------------------------------------------------------
// Tests (all offline; network smoke tests are #[ignore]d at the bottom)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn config() -> ResolveConfig {
        ResolveConfig {
            ytdlp: None,
            ffmpeg: None,
            cache_dir: PathBuf::from("/tmp/echoport-test-cache"),
        }
    }

    #[test]
    fn spotify_links_are_recognised() {
        assert_eq!(
            resolve_link("https://open.spotify.com/track/4cOdK2wGLETKBW3PvgPWqT"),
            Ok(LinkTarget::Spotify(spotify::SpotifyRef::Track(
                "4cOdK2wGLETKBW3PvgPWqT".to_owned()
            )))
        );
        assert_eq!(
            resolve_link("https://open.spotify.com/intl-zh/track/4cOdK2wGLETKBW3PvgPWqT?si=abc"),
            Ok(LinkTarget::Spotify(spotify::SpotifyRef::Track(
                "4cOdK2wGLETKBW3PvgPWqT".to_owned()
            )))
        );
        assert_eq!(
            resolve_link("spotify:album:2N4vVTdKNVHn6T5rYRJnIS"),
            Ok(LinkTarget::Spotify(spotify::SpotifyRef::Album(
                "2N4vVTdKNVHn6T5rYRJnIS".to_owned()
            )))
        );
    }

    #[test]
    fn apple_links_are_recognised() {
        assert_eq!(
            resolve_link("https://music.apple.com/us/album/x/1559885420?i=1559885421"),
            Ok(LinkTarget::Apple(apple::AppleRef::Track {
                id: 1_559_885_421,
                country: "US".to_owned()
            }))
        );
        assert_eq!(
            resolve_link("https://music.apple.com/gb/album/x/1559885420"),
            Ok(LinkTarget::Apple(apple::AppleRef::Album {
                id: 1_559_885_420,
                country: "GB".to_owned()
            }))
        );
    }

    #[test]
    fn streaming_platform_links_are_recognised() {
        for url in [
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://youtu.be/dQw4w9WgXcQ",
            "https://music.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://www.youtube.com/shorts/dQw4w9WgXcQ",
        ] {
            assert_eq!(resolve_link(url), Ok(LinkTarget::YouTube), "url: {url}");
        }
        assert_eq!(
            resolve_link("https://soundcloud.com/rick-astley-official/never-gonna-give-you-up"),
            Ok(LinkTarget::SoundCloud)
        );
        assert_eq!(
            resolve_link("https://www.bilibili.com/video/BV1GJ411x7h7"),
            Ok(LinkTarget::Bilibili)
        );
        assert_eq!(
            resolve_link("https://b23.tv/abc123"),
            Ok(LinkTarget::Bilibili)
        );
    }

    #[test]
    fn netease_links_and_ids_are_recognised() {
        assert_eq!(
            resolve_link("https://music.163.com/song?id=347230"),
            Ok(LinkTarget::Netease(Some(347_230)))
        );
        assert_eq!(
            resolve_link("https://music.163.com/#/song?id=347230"),
            Ok(LinkTarget::Netease(Some(347_230)))
        );
        assert_eq!(
            resolve_link("https://music.163.com/song/347230"),
            Ok(LinkTarget::Netease(Some(347_230)))
        );
        assert_eq!(
            resolve_link("https://music.163.com/album?id=3170026"),
            Ok(LinkTarget::Netease(None)),
            "album links fall back to yt-dlp metadata"
        );
        assert_eq!(
            resolve_link("https://163cn.tv/abc"),
            Ok(LinkTarget::Netease(None))
        );
    }

    #[test]
    fn unsupported_links_are_rejected_with_a_helpful_error() {
        for url in [
            "https://example.com/track/1",
            "hello",
            "",
            "https://open.spotify.com/artist/0gxyHStUsqpMadRV0Di1Qt",
            "https://music.apple.com/us/artist/rick-astley/669771",
        ] {
            let error = resolve_link(url).unwrap_err();
            assert!(!error.is_empty(), "url: {url:?}");
        }
        assert!(resolve_link("https://example.com/track/1")
            .unwrap_err()
            .contains("Unsupported link"));
        assert!(resolve_link("https://open.spotify.com/artist/x")
            .unwrap_err()
            .to_lowercase()
            .contains("artist"));
    }

    #[test]
    fn sanitize_dedupes_caps_and_drops_untitled() {
        let mut a = Track::new(Platform::YouTube, "a", "Song A", "Artist");
        a.id = "youtube:a".to_owned();
        let mut b = Track::new(Platform::YouTube, "b", "Song B", "Artist");
        b.id = "youtube:b".to_owned();
        let mut untitled = Track::new(Platform::YouTube, "c", "   ", "Artist");
        untitled.id = "youtube:c".to_owned();
        let tracks = sanitize(vec![a.clone(), a, b, untitled], 5);
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].title, "Song A");
        assert_eq!(
            sanitize(vec![tracks[0].clone(), tracks[1].clone()], 1).len(),
            1
        );
    }

    #[test]
    fn search_returns_one_result_group_per_platform_without_network() {
        // Spotify and Local never touch the network, so this exercises the
        // orchestration, thread scoping and per-platform error isolation.
        let http = Http::new();
        let results = search(
            &http,
            &config(),
            "test query",
            &[Platform::Spotify, Platform::Local, Platform::Spotify],
            3,
        );
        assert_eq!(results.len(), 2, "duplicate platforms are collapsed");
        assert_eq!(results[0].platform, Platform::Spotify);
        assert!(results[0].tracks.is_empty());
        assert!(results[0]
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("link"));
        assert_eq!(results[1].platform, Platform::Local);
        assert!(results[1].error.is_some());
    }

    #[test]
    fn bilibili_anti_bot_errors_become_a_friendly_bilingual_hint() {
        // The exact shape yt-dlp reports when Bilibili blocks the network.
        for message in [
            "network error: HTTP Error 412: Precondition Failed",
            "[BiliBili] 113911642789603: HTTP Error 567",
            "ERROR: Precondition failed",
        ] {
            let mapped = friendly_resolver_error(
                Platform::Bilibili,
                &ResolveError::Other(message.to_owned()),
            );
            assert!(mapped.contains("Bilibili"), "got: {mapped}");
            assert!(mapped.contains("412"), "got: {mapped}");
            assert!(mapped.contains("反爬拦截"), "got: {mapped}");
            assert!(mapped.contains("anti-bot"), "got: {mapped}");
            assert!(
                !mapped.contains("ERROR:"),
                "raw yt-dlp error leaked: {mapped}"
            );
            assert!(
                !mapped.contains("Precondition"),
                "raw yt-dlp error leaked: {mapped}"
            );
        }
        // The same HTTP error on another platform is not mislabeled.
        let generic = friendly_resolver_error(
            Platform::YouTube,
            &ResolveError::Network("HTTP Error 412: Precondition Failed".to_owned()),
        );
        assert!(generic.contains("412"));
        assert!(!generic.contains("反爬拦截"));
        // Ordinary Bilibili errors (geo-blocked, deleted) pass through as-is.
        let geo = friendly_resolver_error(
            Platform::Bilibili,
            &ResolveError::Other("this video is deleted or geo-restricted".to_owned()),
        );
        assert!(geo.contains("geo-restricted"));
        assert!(!geo.contains("反爬拦截"));
        // Missing yt-dlp has its own actionable hint.
        let missing = friendly_resolver_error(Platform::Bilibili, &ResolveError::YtDlpMissing);
        assert!(missing.contains("yt-dlp"));
    }

    #[test]
    fn empty_search_returns_errors_without_network() {
        let http = Http::new();
        let results = search(&http, &config(), "   ", &[Platform::Netease], 5);
        assert_eq!(results.len(), 1);
        assert!(results[0].error.is_some());
    }

    #[test]
    fn spotify_artist_search_returns_a_quiet_empty_group() {
        // Spotify and Local never touch the network here, so this covers the
        // offline orchestration and per-platform isolation.
        let http = Http::new();
        let results = search_artists(
            &http,
            &config(),
            "Porter Robinson",
            &[Platform::Spotify, Platform::Local],
            6,
        );
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].platform, Platform::Spotify);
        assert!(results[0].artists.is_empty());
        assert!(
            results[0].error.is_none(),
            "the track group already hints about link import"
        );
        assert_eq!(results[1].platform, Platform::Local);
        assert!(results[1].error.is_some());
    }

    #[test]
    fn empty_artist_search_returns_errors_without_network() {
        let http = Http::new();
        let results = search_artists(&http, &config(), "   ", &[Platform::Netease], 6);
        assert_eq!(results.len(), 1);
        assert!(results[0].artists.is_empty());
        assert!(results[0].error.is_some());
    }

    #[test]
    fn artist_detail_without_a_name_does_no_network_work() {
        let http = Http::new();
        let artist = Artist::new(Platform::Spotify, "spotify-artist", "");
        let detail = artist_detail(&http, &config(), &artist);
        assert_eq!(detail.artist.as_ref().map(|a| a.name.as_str()), Some(""));
        assert!(detail.bio.is_none());
        assert!(detail.hot_tracks.is_empty());
    }

    #[test]
    fn hot_tracks_finalize_filters_dedupes_and_caps() {
        let artist = Artist::new(Platform::Netease, "185871", "Porter Robinson");
        let mut tracks = Vec::new();
        for index in 0..20 {
            tracks.push(Track::new(
                Platform::Netease,
                &format!("{index}"),
                &format!("Song {index}"),
                "Porter Robinson",
            ));
        }
        tracks.push(Track::new(
            Platform::Netease,
            "unrelated",
            "Get Lucky",
            "Daft Punk",
        ));
        tracks.push(Track::new(
            Platform::Netease,
            "0",
            "Song 0",
            "Porter Robinson",
        ));
        let hot = finalize_hot_tracks(tracks, &artist, 12);
        assert_eq!(hot.len(), 12, "capped");
        assert!(hot
            .iter()
            .all(|track| track.artist.contains("Porter Robinson")));
        // Duplicate id/title + artist rows are collapsed.
        assert_eq!(
            hot.iter().filter(|track| track.title == "Song 0").count(),
            1
        );
    }

    // -----------------------------------------------------------------------
    // Network smoke tests. Run explicitly with:
    //   cargo test --lib providers:: -- --ignored --nocapture
    // -----------------------------------------------------------------------

    #[test]
    #[ignore = "network smoke test"]
    fn net_netease_and_apple_search() {
        let http = Http::new();
        let query = "Never Gonna Give You Up Rick Astley";
        let netease = netease::search(&http, query, 5);
        let apple = apple::search(&http, query, 5);
        println!("netease: {netease:#?}");
        println!("apple: {apple:#?}");
        let netease = netease.expect("netease search");
        let apple = apple.expect("apple search");
        assert!(!netease.is_empty());
        assert!(!apple.is_empty());
        assert_eq!(netease[0].platform, Platform::Netease);
        assert_eq!(apple[0].platform, Platform::AppleMusic);
        assert!(netease.iter().any(|track| track.art_url.is_some()));
        assert!(apple.iter().any(|track| track.art_url.is_some()));
        assert!(apple.iter().any(|track| track.stream_url.is_some()));
    }

    #[test]
    #[ignore = "network smoke test"]
    fn net_link_imports() {
        let http = Http::new();
        let config = config();
        let spotify = track_from_url(
            &http,
            &config,
            "https://open.spotify.com/track/4cOdK2wGLETKBW3PvgPWqT",
        )
        .expect("spotify import");
        println!("spotify: {spotify:#?}");
        assert_eq!(spotify.platform, Platform::Spotify);
        assert_eq!(spotify.title, "Never Gonna Give You Up");
        assert!(spotify.stream_url.is_some());
        assert!(spotify.art_url.is_some());

        let apple = track_from_url(
            &http,
            &config,
            "https://music.apple.com/us/album/never-gonna-give-you-up/1559885420?i=1559885421",
        )
        .expect("apple import");
        println!("apple: {apple:#?}");
        assert_eq!(apple.platform, Platform::AppleMusic);
        assert!(apple.title.contains("Never Gonna Give You Up"));
        assert!(apple.art_url.is_some());

        let netease = track_from_url(&http, &config, "https://music.163.com/song?id=347230")
            .expect("netease import");
        println!("netease: {netease:#?}");
        assert_eq!(netease.platform, Platform::Netease);
        assert!(netease.duration_secs.is_some());

        let youtube =
            track_from_url(&http, &config, "https://youtu.be/dQw4w9WgXcQ").expect("youtube import");
        println!("youtube: {youtube:#?}");
        assert_eq!(youtube.platform, Platform::YouTube);
        assert!(youtube.title.contains("Never Gonna Give You Up"));

        let unknown = track_from_url(&http, &config, "https://example.com/track/1");
        assert!(unknown.is_err());
    }

    #[test]
    #[ignore = "network smoke test"]
    fn net_full_catalogue_search() {
        let http = Http::new();
        let config = ResolveConfig {
            ytdlp: crate::resolver::detect_ytdlp(),
            ffmpeg: None,
            cache_dir: std::env::temp_dir().join("echoport-search-test"),
        };
        let results = search(
            &http,
            &config,
            "Rick Astley Never Gonna Give You Up",
            &Platform::REMOTE,
            5,
        );
        for result in &results {
            println!(
                "{}: {} tracks, error: {:?}",
                result.platform.label(),
                result.tracks.len(),
                result.error
            );
        }
        assert_eq!(results.len(), Platform::REMOTE.len());
        let by_platform = |platform: Platform| {
            results
                .iter()
                .find(|result| result.platform == platform)
                .expect("every requested platform has a result group")
        };
        assert!(!by_platform(Platform::YouTube).tracks.is_empty());
        assert!(!by_platform(Platform::Netease).tracks.is_empty());
        assert!(!by_platform(Platform::AppleMusic).tracks.is_empty());
        assert!(
            by_platform(Platform::Spotify).error.is_some(),
            "Spotify search must stay a graceful hint"
        );
        println!("soundcloud: {:?}", by_platform(Platform::SoundCloud).error);
        println!("bilibili: {:?}", by_platform(Platform::Bilibili).error);
    }

    #[test]
    #[ignore = "network smoke test"]
    fn net_album_and_playlist_imports() {
        let http = Http::new();
        let config = config();
        let playlist = track_from_url(
            &http,
            &config,
            "https://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M",
        )
        .expect("spotify playlist import");
        println!("spotify playlist first track: {playlist:#?}");
        assert_eq!(playlist.platform, Platform::Spotify);
        assert!(!playlist.title.is_empty());

        let album = track_from_url(
            &http,
            &config,
            "https://open.spotify.com/album/2N4vVTdKNVHn6T5rYRJnIS",
        )
        .expect("spotify album import");
        println!("spotify album first track: {album:#?}");
        assert_eq!(album.album.as_deref(), Some("The Best of Me"));

        let apple_album = track_from_url(
            &http,
            &config,
            "https://music.apple.com/us/album/never-gonna-give-you-up/1559885420",
        )
        .expect("apple album import");
        println!("apple album first track: {apple_album:#?}");
        assert_eq!(apple_album.platform, Platform::AppleMusic);
        assert!(apple_album.duration_secs.is_some());
    }

    #[test]
    #[ignore = "network smoke test"]
    fn net_artist_search_porter_robinson() {
        let http = Http::new();
        let config = config();
        let results = search_artists(&http, &config, "Porter Robinson", &Platform::REMOTE, 6);
        for result in &results {
            println!(
                "{}: {} artists, error: {:?}, first: {:?}",
                result.platform.label(),
                result.artists.len(),
                result.error,
                result.artists.first().map(|artist| &artist.name)
            );
        }
        let by_platform = |platform: Platform| {
            results
                .iter()
                .find(|result| result.platform == platform)
                .expect("every requested platform has a result group")
        };
        let netease = by_platform(Platform::Netease);
        assert!(!netease.artists.is_empty(), "Netease artist search empty");
        assert_eq!(
            netease.artists[0].name, "Porter Robinson",
            "exact match must rank first"
        );
        assert!(netease.artists[0]
            .page_url
            .as_deref()
            .unwrap_or_default()
            .contains("music.163.com"));

        let apple = by_platform(Platform::AppleMusic);
        assert!(!apple.artists.is_empty(), "Apple artist search empty");
        assert_eq!(apple.artists[0].name, "Porter Robinson");
        assert_eq!(apple.artists[0].source_id, "282330711");
        assert!(apple.artists[0]
            .page_url
            .as_deref()
            .unwrap_or_default()
            .starts_with("https://music.apple.com/"));

        let youtube = by_platform(Platform::YouTube);
        assert!(
            youtube
                .artists
                .iter()
                .any(|artist| artist.name.contains("Porter Robinson")),
            "derived YouTube creator expected: {:?}",
            youtube.artists
        );
        assert!(
            youtube.artists.len() <= 2,
            "derived creators are capped at 2"
        );
        for artist in &youtube.artists {
            assert!(artist
                .page_url
                .as_deref()
                .unwrap_or_default()
                .starts_with("https://www.youtube.com/results?search_query="));
        }
        assert!(by_platform(Platform::Spotify).artists.is_empty());
    }

    #[test]
    #[ignore = "network smoke test"]
    fn net_artist_detail_porter_robinson() {
        let http = Http::new();
        let config = config();
        // Netease artist page (hot songs endpoint).
        let netease_artists =
            netease::search_artists(&http, "Porter Robinson", 5).expect("Netease artist search");
        let netease_artist = netease_artists
            .iter()
            .find(|artist| artist.name == "Porter Robinson")
            .expect("Porter Robinson on Netease");
        let started = std::time::Instant::now();
        let detail = artist_detail(&http, &config, netease_artist);
        println!(
            "Netease artist detail in {:?}: {} hot tracks, bio: {:?}",
            started.elapsed(),
            detail.hot_tracks.len(),
            detail.bio.as_ref().map(|bio| &bio.title)
        );
        let bio = detail.bio.as_ref().expect("Wikipedia bio expected");
        assert!(
            util::name_relevance("Porter Robinson", &bio.extract) > 0,
            "unrelated bio: {bio:?}"
        );
        assert!(!detail.hot_tracks.is_empty(), "Netease hot tracks expected");
        assert!(detail.hot_tracks.len() <= 12);
        for track in &detail.hot_tracks {
            assert!(
                util::name_relevance("Porter Robinson", &track.artist) > 0,
                "unrelated hot track: {track:?}"
            );
        }
        assert!(
            detail.hot_tracks.iter().any(|track| track
                .release_date
                .as_deref()
                .and_then(util::date_from_iso)
                .is_some()),
            "Netease hot tracks should carry release dates: {:?}",
            detail
                .hot_tracks
                .iter()
                .map(|track| &track.release_date)
                .collect::<Vec<_>>()
        );

        // YouTube artist page (resolver search filtered to the channel).
        let youtube_artist = Artist::new(Platform::YouTube, "Porter Robinson", "Porter Robinson");
        let yt_detail = artist_detail(&http, &config, &youtube_artist);
        println!(
            "YouTube artist detail: {} hot tracks, bio: {:?}",
            yt_detail.hot_tracks.len(),
            yt_detail.bio.as_ref().map(|bio| &bio.title)
        );
        assert!(yt_detail.bio.is_some());
        assert!(
            !yt_detail.hot_tracks.is_empty(),
            "YouTube hot tracks expected"
        );
        for track in &yt_detail.hot_tracks {
            assert!(util::name_relevance("Porter Robinson", &track.artist) > 0);
        }
    }

    #[test]
    #[ignore = "network smoke test"]
    fn net_artist_detail_apple_id_and_netease_fallback() {
        let http = Http::new();
        let config = config();
        // Apple artistId lookup path.
        let apple_artists =
            apple::search_artists(&http, "Porter Robinson", 5).expect("Apple artist search");
        let apple_artist = apple_artists
            .iter()
            .find(|artist| artist.name == "Porter Robinson")
            .expect("Porter Robinson on Apple Music");
        assert_eq!(apple_artist.source_id, "282330711");
        let apple_detail = artist_detail(&http, &config, apple_artist);
        println!(
            "Apple artist detail: {} hot tracks, bio: {:?}",
            apple_detail.hot_tracks.len(),
            apple_detail.bio.as_ref().map(|bio| &bio.title)
        );
        assert!(apple_detail.bio.is_some());
        assert!(
            !apple_detail.hot_tracks.is_empty(),
            "Apple hot tracks expected"
        );
        assert!(apple_detail.hot_tracks.len() <= 12);
        for track in &apple_detail.hot_tracks {
            assert!(util::name_relevance("Porter Robinson", &track.artist) > 0);
            assert!(!track.title.is_empty());
        }
        assert!(
            apple_detail
                .hot_tracks
                .iter()
                .any(|track| track.release_date.is_some()),
            "Apple hot tracks should carry release dates"
        );

        // Non-numeric Netease source id must fall back to a filtered search.
        let legacy = Artist::new(Platform::Netease, "legacy-artist-page", "Porter Robinson");
        let fallback = artist_detail(&http, &config, &legacy);
        println!(
            "Netease fallback detail: {} hot tracks",
            fallback.hot_tracks.len()
        );
        assert!(
            !fallback.hot_tracks.is_empty(),
            "Netease search fallback expected"
        );
        for track in &fallback.hot_tracks {
            assert!(util::name_relevance("Porter Robinson", &track.artist) > 0);
        }
    }

    #[test]
    #[ignore = "network smoke test"]
    fn net_release_dates() {
        fn assert_valid_date(track: &Track, label: &str) {
            let date = track
                .release_date
                .as_deref()
                .unwrap_or_else(|| panic!("{label}: release_date missing on {track:?}"));
            assert_eq!(
                util::date_from_iso(date).as_deref(),
                Some(date),
                "{label}: invalid date {date:?}"
            );
        }

        let http = Http::new();
        let config = ResolveConfig {
            ytdlp: crate::resolver::detect_ytdlp(),
            ffmpeg: None,
            cache_dir: std::env::temp_dir().join("echoport-dates-test"),
        };

        // Apple + Netease search tracks carry full dates.
        let apple =
            apple::search(&http, "Never Gonna Give You Up Rick Astley", 3).expect("apple search");
        assert_valid_date(&apple[0], "apple search");
        let netease = netease::search(&http, "Never Gonna Give You Up Rick Astley", 3)
            .expect("netease search");
        assert_valid_date(&netease[0], "netease search");

        // Link imports: Spotify embed releaseDate, Apple lookup releaseDate,
        // yt-dlp upload_date, Netease detail publishTime.
        let spotify = track_from_url(
            &http,
            &config,
            "https://open.spotify.com/track/4cOdK2wGLETKBW3PvgPWqT",
        )
        .expect("spotify import");
        assert_eq!(spotify.release_date.as_deref(), Some("1987-11-12"));
        let apple_link = track_from_url(
            &http,
            &config,
            "https://music.apple.com/us/album/never-gonna-give-you-up/1559885420?i=1559885421",
        )
        .expect("apple import");
        assert_eq!(apple_link.release_date.as_deref(), Some("1987-07-27"));
        let netease_link = track_from_url(&http, &config, "https://music.163.com/song?id=347230")
            .expect("netease import");
        assert_valid_date(&netease_link, "netease import");
        let youtube =
            track_from_url(&http, &config, "https://youtu.be/dQw4w9WgXcQ").expect("youtube import");
        println!(
            "release dates: apple={:?} netease={:?} spotify={:?} apple-link={:?} netease-link={:?} youtube={:?}",
            apple[0].release_date,
            netease[0].release_date,
            spotify.release_date,
            apple_link.release_date,
            netease_link.release_date,
            youtube.release_date
        );
        assert_valid_date(&youtube, "youtube import");
        assert_eq!(youtube.release_date.as_deref(), Some("2009-10-25"));
    }

    #[test]
    #[ignore = "network smoke test"]
    fn net_background() {
        let http = Http::new();
        let mut track = Track::new(
            Platform::YouTube,
            "dQw4w9WgXcQ",
            "Never Gonna Give You Up",
            "Rick Astley",
        );
        track = track.with_page("https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        let started = std::time::Instant::now();
        let background = background(&http, &config(), &track);
        println!("background in {:?}: {background:#?}", started.elapsed());
        assert!(background.bio.is_some(), "Wikipedia biography expected");
        assert!(!background.links.is_empty());
    }
}
