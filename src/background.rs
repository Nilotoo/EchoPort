//! Song background: artist, album and song context plus interviews
//! (owned by the providers workstream).
//!
//! Everything here uses keyless public endpoints:
//! * Wikipedia REST summary (artist biography, album and song articles);
//! * MusicBrainz release-group lookup as a factual album fallback;
//! * a Genius search link (no scraping — we only open the search page);
//! * YouTube interview / explanation links via `resolver::search_links`.
//!
//! **Strict relevance.** The panel used to show loosely related material
//! ("信息不多还很杂"). Every article, link and video is now gated on tokens of
//! *this* track: a general item needs the artist **and** (album **or** title),
//! a song-specific item needs the artist **and** the title. If nothing passes
//! the gate we return nothing rather than unrelated context.
//!
//! Every URL shown to the user must be a human-readable page. Network API
//! endpoints (Wikipedia `api.php`, the REST summary API, MusicBrainz `ws/2`)
//! are only used internally and are filtered from the result.
//!
//! The function blocks on a worker thread and keeps a hard 8 s budget: calls
//! are skipped once the deadline is close, so slow networks degrade to partial
//! data instead of blocking the UI.

use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::model::{format_duration, ArtistBio, BackgroundLink, LinkKind, SongBackground, Track};
use crate::net::Http;
use crate::providers::util;
use crate::resolver::{self, ResolveConfig, SearchLink};

/// Whole-call time budget from `docs/CONTRACT.md` §8.
const TOTAL_BUDGET: Duration = Duration::from_secs(8);
/// Do not start the second yt-dlp video search after this much time.
const SECOND_VIDEO_SEARCH_AFTER: Duration = Duration::from_millis(4_500);
/// Do not start the MusicBrainz album lookup after this much time.
const MUSICBRAINZ_LOOKUP_AFTER: Duration = Duration::from_millis(6_000);
/// At most this many interview/explanation videos are shown.
const MAX_VIDEOS: usize = 5;
/// How many search-API titles are inspected per Wikipedia query.
const WIKI_SEARCH_TITLES: usize = 4;
/// MusicBrainz asks clients for at most one request per second.
const MUSICBRAINZ_MIN_INTERVAL: Duration = Duration::from_millis(1_100);

/// Process-wide MusicBrainz rate limiter: serialises requests from concurrent
/// background workers and keeps them ≥1.1 s apart (the API returns 503 when
/// hammered, which would silently drop the album fallback).
static MUSICBRAINZ_LIMITER: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();

/// Wikipedia REST summary endpoint (network API — never a user-facing link).
const WIKI_SUMMARY_API: &str = "https://en.wikipedia.org/api/rest_v1/page/summary/";
/// MediaWiki action API, used only to find page titles (never a link).
const WIKI_SEARCH_API: &str = "https://en.wikipedia.org/w/api.php";
/// Human Wikipedia search page — safe to open in a browser.
const WIKI_SEARCH_PAGE: &str = "https://en.wikipedia.org/w/index.php";
/// MusicBrainz artist lookup (network API — never a user-facing link).
const MUSICBRAINZ_ARTIST_API: &str = "https://musicbrainz.org/ws/2/artist/";
/// MusicBrainz release-group lookup (network API — never a link).
const MUSICBRAINZ_RELEASE_GROUP_API: &str = "https://musicbrainz.org/ws/2/release-group/";
/// Human MusicBrainz artist search page.
const MUSICBRAINZ_SEARCH_PAGE: &str = "https://musicbrainz.org/search";
/// Human MusicBrainz release-group page prefix.
const MUSICBRAINZ_RELEASE_GROUP_PAGE: &str = "https://musicbrainz.org/release-group/";
/// Human YouTube search results page.
const YOUTUBE_RESULTS_PAGE: &str = "https://www.youtube.com/results";
const GENIUS_SEARCH: &str = "https://genius.com/search";

/// Words that carry no identity on their own. They are ignored when building
/// the relevance token sets so that e.g. "Nurture (Deluxe Edition)" only
/// requires "nurture".
const STOP_WORDS: &[&str] = &[
    "the",
    "a",
    "an",
    "and",
    "or",
    "of",
    "for",
    "with",
    "vs",
    "feat",
    "featuring",
    "ft",
    "prod",
    "remaster",
    "remastered",
    "deluxe",
    "edition",
    "expanded",
    "anniversary",
    "bonus",
    "album",
    "song",
    "single",
    "ep",
    "official",
    "video",
    "audio",
    "visualizer",
    "lyrics",
    "lyric",
    "music",
    "track",
    "version",
    "mix",
    "live",
    "mono",
    "stereo",
    "radio",
    "edit",
    "original",
    "topic",
    "hd",
    "hq",
    "4k",
    "full",
];

/// Fetch context for a track. Always returns whatever passed the relevance
/// gate; never panics and never blocks longer than the individual network calls.
pub fn fetch(http: &Http, config: &ResolveConfig, track: &Track) -> SongBackground {
    let started = Instant::now();
    // Without an artist nothing can be matched reliably — show nothing rather
    // than unrelated content.
    let Some(context) = TrackContext::from_track(track) else {
        return SongBackground::default();
    };
    let mut background = SongBackground::default();

    // ---- Artist bio -------------------------------------------------------
    if started.elapsed() < TOTAL_BUDGET {
        background.bio = artist_background(http, &context, started);
    }

    // ---- Album background (Wikipedia, MusicBrainz fallback) --------------
    if context.album.is_some() && started.elapsed() < TOTAL_BUDGET {
        background.album = album_background(http, &context, started);
    }

    // ---- Song background (Wikipedia) -------------------------------------
    if !context.title.is_empty() && started.elapsed() < TOTAL_BUDGET {
        background.song = song_background(http, &context, started);
    }
    // Never show the same article twice (e.g. a one-track album page).
    if let (Some(album), Some(song)) = (&background.album, &background.song) {
        if album.url == song.url {
            background.song = None;
        }
    }
    if let (Some(bio), Some(song)) = (&background.bio, &background.song) {
        if bio.url == song.url {
            background.song = None;
        }
    }

    // ---- Further reading links -------------------------------------------
    let mut links: Vec<BackgroundLink> = Vec::new();
    if let Some(bio) = &background.bio {
        links.push(bio_link(bio, "Artist biography"));
    }
    if let Some(album) = &background.album {
        links.push(bio_link(album, "Album background"));
    }
    if let Some(song) = &background.song {
        links.push(bio_link(song, "Song background"));
    }
    if !context.artist.is_empty() {
        links.push(genius_link(&context));
    }
    if let Some(page) = track
        .page_url
        .as_deref()
        .filter(|url| url.starts_with("https://") || url.starts_with("http://"))
    {
        links.push(BackgroundLink {
            kind: LinkKind::Source,
            title: format!("Open the {} source page", track.platform.label()),
            url: page.to_owned(),
            subtitle: Some(track.platform.label().to_owned()),
        });
    }
    // Artist fallbacks only when no biography matched: still about the artist,
    // never random material.
    if background.bio.is_none() {
        links.push(wikipedia_search_link(&context.artist));
        let musicbrainz = if started.elapsed() < MUSICBRAINZ_LOOKUP_AFTER {
            musicbrainz_artist(http, &context.artist)
        } else {
            None
        };
        links.push(musicbrainz_artist_link(
            &context.artist,
            musicbrainz.as_ref(),
        ));
    }

    // ---- Interview / explanation videos ----------------------------------
    let mut videos: Vec<BackgroundLink> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    let queries = video_queries(&context);
    for (index, query) in queries.iter().enumerate() {
        if videos.len() >= MAX_VIDEOS || started.elapsed() >= TOTAL_BUDGET {
            break;
        }
        if index > 0 && started.elapsed() > SECOND_VIDEO_SEARCH_AFTER {
            break;
        }
        match resolver::search_links(config, query, 4) {
            Ok(found) => collect_videos(&context, &mut videos, found, &mut seen),
            Err(err) => log::debug!("background video search skipped: {err}"),
        }
    }
    if videos.is_empty() {
        // A correctly targeted search page is still useful; the query carries
        // artist + album + title so it cannot lead to unrelated content.
        links.push(youtube_search_link(&context));
    }

    background.links = sanitize_links(links);
    background.videos = videos;
    background
}

/// Fetch only the Wikipedia artist biography (used by the artist/producer page
/// provider). Same strict relevance gate as `fetch`, but no album/song/video
/// work, and its own 8 s budget.
pub(crate) fn artist_bio_for(http: &Http, artist_name: &str) -> Option<ArtistBio> {
    let started = Instant::now();
    let context = TrackContext::from_artist(artist_name)?;
    artist_background(http, &context, started)
}

// ---------------------------------------------------------------------------
// Track context + relevance tokens
// ---------------------------------------------------------------------------

/// Artist / album / title plus the significant token sets used to decide
/// whether a piece of content is about this track.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct TrackContext {
    artist: String,
    album: Option<String>,
    title: String,
    artist_tokens: Vec<String>,
    album_tokens: Vec<String>,
    title_tokens: Vec<String>,
}

impl TrackContext {
    /// Artist-only context, used by the artist/producer page provider.
    fn from_artist(artist: &str) -> Option<Self> {
        let artist = primary_artist(artist)?;
        Some(Self {
            artist_tokens: significant_tokens(&artist),
            artist,
            album: None,
            title: String::new(),
            album_tokens: Vec::new(),
            title_tokens: Vec::new(),
        })
    }

    fn from_track(track: &Track) -> Option<Self> {
        let artist = primary_artist(&track.artist)?;
        let album = track.album.as_deref().and_then(util::clean);
        let title = util::clean(&track.title).unwrap_or_default();
        Some(Self {
            artist_tokens: significant_tokens(&artist),
            album_tokens: album.as_deref().map(significant_tokens).unwrap_or_default(),
            title_tokens: significant_tokens(&title),
            artist,
            album,
            title,
        })
    }

    /// Text contains every significant artist token.
    fn artist_matches(&self, text: &str) -> bool {
        all_tokens_present(&normalize_match(text), &self.artist_tokens)
    }

    /// Text contains every significant album token (false when album unknown).
    fn album_matches(&self, text: &str) -> bool {
        all_tokens_present(&normalize_match(text), &self.album_tokens)
    }

    /// Text contains every significant title token (false when title unknown).
    fn title_matches(&self, text: &str) -> bool {
        all_tokens_present(&normalize_match(text), &self.title_tokens)
    }

    /// General relevance: artist AND (album OR title).
    fn general_matches(&self, text: &str) -> bool {
        self.artist_matches(text) && (self.album_matches(text) || self.title_matches(text))
    }

    /// An artist biography page must be titled after the artist.
    fn artist_bio_ok(&self, bio: &ArtistBio) -> bool {
        all_tokens_present(&normalize_match(&bio.title), &self.artist_tokens)
    }

    /// An album article must mention the artist and be about the album.
    fn album_article_ok(&self, bio: &ArtistBio) -> bool {
        let text = format!("{} {}", bio.title, bio.extract);
        self.artist_matches(&text) && self.album_matches(&text)
    }

    /// A song article must mention the artist and be about the title.
    fn song_article_ok(&self, bio: &ArtistBio) -> bool {
        let text = format!("{} {}", bio.title, bio.extract);
        self.artist_matches(&text) && self.title_matches(&text)
    }
}

/// Case-fold, replace punctuation with spaces and pad with spaces so that
/// `" token "` matching cannot match inside another word.
fn normalize_match(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push(' ');
    let mut in_gap = true;
    for ch in text.chars().flat_map(char::to_lowercase) {
        if ch.is_alphanumeric() {
            out.push(ch);
            in_gap = false;
        } else if !in_gap {
            out.push(' ');
            in_gap = true;
        }
    }
    if !in_gap {
        out.push(' ');
    }
    out
}

/// Identity-bearing tokens: normalized words without stop words, single
/// letters and pure numbers (year/edition suffixes).
fn significant_tokens(text: &str) -> Vec<String> {
    let normalized = normalize_match(text);
    let mut tokens: Vec<String> = Vec::new();
    for token in normalized.split_whitespace() {
        if token.chars().count() <= 1 || STOP_WORDS.contains(&token) {
            continue;
        }
        if token.chars().all(|ch| ch.is_ascii_digit()) {
            continue;
        }
        if !tokens.iter().any(|kept| kept == token) {
            tokens.push(token.to_owned());
        }
    }
    if tokens.is_empty() {
        // e.g. "M.I.A." → "m i a": fall back to the full normalized phrase so
        // the artist still has *something* to match on.
        let phrase = normalized.split_whitespace().collect::<Vec<_>>().join(" ");
        if !phrase.is_empty() {
            tokens.push(phrase);
        }
    }
    tokens
}

fn all_tokens_present(haystack: &str, tokens: &[String]) -> bool {
    if tokens.is_empty() || haystack.trim().is_empty() {
        return false;
    }
    tokens.iter().all(|token| contains_token(haystack, token))
}

fn contains_token(haystack: &str, token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    if token.contains(' ') {
        // Normalized multi-word fallback phrase.
        return haystack.contains(token);
    }
    if !token.chars().any(|ch| ch.is_ascii_alphanumeric()) {
        // CJK and similar scripts are not space separated: substring match.
        return haystack.contains(token);
    }
    haystack.contains(&format!(" {token} "))
}

fn lead_artist(artist: &str) -> String {
    let mut name = artist.trim().to_owned();
    let lower = name.to_lowercase();
    for marker in [" feat. ", " feat ", " ft. ", " ft ", " featuring ", " x "] {
        if let Some(position) = lower.find(marker) {
            name.truncate(position);
            break;
        }
    }
    name.trim().to_owned()
}

/// Used as the displayed artist for background searches: drops `feat.` parts.
fn primary_artist(artist: &str) -> Option<String> {
    let lead = lead_artist(artist);
    (!lead.is_empty()).then_some(lead)
}

// ---------------------------------------------------------------------------
// Query construction (every query carries the artist)
// ---------------------------------------------------------------------------

/// Candidate Wikipedia page titles for an album, most specific first.
fn album_candidates(context: &TrackContext) -> Vec<String> {
    let Some(album) = context.album.as_deref() else {
        return Vec::new();
    };
    vec![
        format!("{album} (album)"),
        format!("{} {album}", context.artist),
        album.to_owned(),
    ]
}

fn album_search_query(context: &TrackContext) -> Option<String> {
    let album = context.album.as_deref()?;
    Some(format!("{} {album} album", context.artist))
}

fn song_candidates(context: &TrackContext) -> Vec<String> {
    if context.title.is_empty() {
        return Vec::new();
    }
    vec![
        format!("{} (song)", context.title),
        format!("{} {}", context.artist, context.title),
        context.title.clone(),
    ]
}

fn song_search_query(context: &TrackContext) -> Option<String> {
    if context.title.is_empty() {
        return None;
    }
    Some(format!("{} {} song", context.artist, context.title))
}

/// Interview / explanation searches. Every query contains the artist; when the
/// album is known it contains artist + album + title.
fn video_queries(context: &TrackContext) -> Vec<String> {
    match context.album.as_deref() {
        Some(album) if !context.title.is_empty() => vec![
            format!("{} {album} {} interview", context.artist, context.title),
            format!("{} {album} {} explained", context.artist, context.title),
        ],
        _ if !context.title.is_empty() => vec![
            format!("{} {} interview", context.artist, context.title),
            format!("{} {} explained", context.artist, context.title),
        ],
        Some(album) => vec![format!("{} {album} interview", context.artist)],
        None => Vec::new(),
    }
}

fn youtube_search_query(context: &TrackContext) -> String {
    match context.album.as_deref() {
        Some(album) if !context.title.is_empty() => {
            format!("{} {album} {} interview", context.artist, context.title)
        }
        _ => format!("{} {} interview", context.artist, context.title),
    }
}

// ---------------------------------------------------------------------------
// Wikipedia
// ---------------------------------------------------------------------------

fn artist_background(http: &Http, context: &TrackContext, started: Instant) -> Option<ArtistBio> {
    for candidate in wiki_artist_candidates(&context.artist) {
        if started.elapsed() >= TOTAL_BUDGET {
            return None;
        }
        if let Some(bio) = wiki_summary(http, &candidate) {
            if context.artist_bio_ok(&bio) {
                return Some(bio);
            }
        }
    }
    // Unusual stage names: use the search API to find the page title, then
    // validate the summary before accepting it.
    if started.elapsed() < TOTAL_BUDGET {
        for title in wiki_search_titles(http, &context.artist) {
            if started.elapsed() >= TOTAL_BUDGET {
                return None;
            }
            if title.eq_ignore_ascii_case(&context.artist) {
                continue; // already tried above
            }
            if let Some(bio) = wiki_summary(http, &title) {
                if context.artist_bio_ok(&bio) {
                    return Some(bio);
                }
            }
        }
    }
    None
}

/// Ordered page titles to try: the full artist string first (covers band names
/// with `&` or `,`), then the lead artist for `feat.` / collaboration strings.
fn wiki_artist_candidates(artist: &str) -> Vec<String> {
    let full = artist.trim();
    if full.is_empty() {
        return Vec::new();
    }
    let lead = lead_artist(full);
    let mut out: Vec<String> = Vec::new();
    push_unique(&mut out, full.to_owned());
    push_unique(&mut out, lead.clone());
    push_unique(&mut out, format!("{full} (musician)"));
    push_unique(&mut out, format!("{lead} (musician)"));
    push_unique(&mut out, format!("{full} (band)"));
    out
}

fn push_unique(out: &mut Vec<String>, name: String) {
    let name = name.trim().to_owned();
    if name.is_empty() {
        return;
    }
    if !out.iter().any(|kept| kept.eq_ignore_ascii_case(&name)) {
        out.push(name);
    }
}

fn wiki_summary(http: &Http, page_title: &str) -> Option<ArtistBio> {
    let slug = page_title.trim().replace(' ', "_");
    if slug.is_empty() {
        return None;
    }
    let url = format!("{WIKI_SUMMARY_API}{}", util::pct(&slug));
    let value: Value = http
        .get_json(&url, &[("Accept", "application/json")])
        .ok()?;
    parse_wiki_summary(&value)
}

fn parse_wiki_summary(value: &Value) -> Option<ArtistBio> {
    let kind = util::get_str(value, "/type")
        .unwrap_or("standard")
        .to_ascii_lowercase();
    // Disambiguation pages explain nothing useful; fall through instead.
    if kind == "disambiguation" || kind == "maintenance" {
        return None;
    }
    let title = util::get_str(value, "/title")?;
    let extract = util::get_str(value, "/extract")?;
    let url = util::get_str(value, "/content_urls/desktop/page")?;
    if !is_human_url(url) {
        return None;
    }
    let thumbnail = util::get_str(value, "/thumbnail/source").and_then(util::https_url);
    Some(ArtistBio {
        title: title.to_owned(),
        extract: extract.to_owned(),
        url: url.to_owned(),
        thumbnail,
    })
}

/// Search the MediaWiki action API for page titles (internal use only).
fn wiki_search_titles(http: &Http, query: &str) -> Vec<String> {
    let url = format!(
        "{WIKI_SEARCH_API}?action=query&list=search&format=json&srlimit={WIKI_SEARCH_TITLES}&srsearch={}",
        util::pct(query.trim())
    );
    let Ok(value) = http.get_json::<Value>(&url, &[("Accept", "application/json")]) else {
        return Vec::new();
    };
    value
        .pointer("/query/search")
        .and_then(Value::as_array)
        .map(|results| {
            results
                .iter()
                .filter_map(|result| util::get_str(result, "/title").map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn album_background(http: &Http, context: &TrackContext, started: Instant) -> Option<ArtistBio> {
    // Exact page-name candidates.
    for candidate in album_candidates(context) {
        if started.elapsed() >= TOTAL_BUDGET {
            return None;
        }
        if let Some(bio) = wiki_summary(http, &candidate) {
            if context.album_article_ok(&bio) {
                return Some(bio);
            }
        }
    }
    // Wikipedia search fallback.
    if let Some(query) = album_search_query(context) {
        for title in wiki_search_titles(http, &query) {
            if started.elapsed() >= TOTAL_BUDGET {
                return None;
            }
            if let Some(bio) = wiki_summary(http, &title) {
                if context.album_article_ok(&bio) {
                    return Some(bio);
                }
            }
        }
    }
    // Factual MusicBrainz fallback when Wikipedia lacks an article.
    if started.elapsed() < MUSICBRAINZ_LOOKUP_AFTER {
        if let Some(bio) = musicbrainz_album(http, context) {
            return Some(bio);
        }
    }
    None
}

fn song_background(http: &Http, context: &TrackContext, started: Instant) -> Option<ArtistBio> {
    for candidate in song_candidates(context) {
        if started.elapsed() >= TOTAL_BUDGET {
            return None;
        }
        if let Some(bio) = wiki_summary(http, &candidate) {
            if context.song_article_ok(&bio) {
                return Some(bio);
            }
        }
    }
    if let Some(query) = song_search_query(context) {
        for title in wiki_search_titles(http, &query) {
            if started.elapsed() >= TOTAL_BUDGET {
                return None;
            }
            if let Some(bio) = wiki_summary(http, &title) {
                if context.song_article_ok(&bio) {
                    return Some(bio);
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// MusicBrainz (factual fallbacks)
// ---------------------------------------------------------------------------

/// Block until another MusicBrainz request is allowed and keep the guard for
/// the duration of the request so concurrent workers cannot overlap.
fn musicbrainz_rate_limit() -> MutexGuard<'static, Option<Instant>> {
    let mutex = MUSICBRAINZ_LIMITER.get_or_init(|| Mutex::new(None));
    let mut guard = match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(last) = *guard {
        let elapsed = last.elapsed();
        if elapsed < MUSICBRAINZ_MIN_INTERVAL {
            std::thread::sleep(MUSICBRAINZ_MIN_INTERVAL - elapsed);
        }
    }
    *guard = Some(Instant::now());
    guard
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MusicBrainzArtist {
    name: String,
    /// MusicBrainz artist MBID (server side UUID).
    id: String,
}

/// Look up an artist on MusicBrainz. Returns `None` on any error — the caller
/// then falls back to the human search page.
fn musicbrainz_artist(http: &Http, artist: &str) -> Option<MusicBrainzArtist> {
    let _rate_limit = musicbrainz_rate_limit();
    let query = format!("artist:\"{}\"", lucene_quote(artist));
    let url = format!(
        "{MUSICBRAINZ_ARTIST_API}?query={}&fmt=json&limit=3",
        util::pct(&query)
    );
    let value: Value = http
        .get_json(&url, &[("Accept", "application/json")])
        .ok()?;
    parse_musicbrainz_artist(&value)
}

fn parse_musicbrainz_artist(value: &Value) -> Option<MusicBrainzArtist> {
    let artists = value.pointer("/artists")?.as_array()?;
    for artist in artists {
        let Some(id) = util::get_str(artist, "/id") else {
            continue;
        };
        if !looks_like_mbid(id) {
            continue;
        }
        let name = util::get_str(artist, "/name")
            .or_else(|| util::get_str(artist, "/sort-name"))
            .unwrap_or_default()
            .to_owned();
        return Some(MusicBrainzArtist {
            name,
            id: id.to_owned(),
        });
    }
    None
}

/// Look up the album as a MusicBrainz release-group and build a short factual
/// description ("Album by X, released YYYY") with a human MusicBrainz URL.
fn musicbrainz_album(http: &Http, context: &TrackContext) -> Option<ArtistBio> {
    let album = context.album.as_deref()?;
    let _rate_limit = musicbrainz_rate_limit();
    let query = format!(
        "releasegroup:\"{}\" AND artist:\"{}\"",
        lucene_quote(album),
        lucene_quote(&context.artist)
    );
    let url = format!(
        "{MUSICBRAINZ_RELEASE_GROUP_API}?query={}&fmt=json&limit=5",
        util::pct(&query)
    );
    let value: Value = http
        .get_json(&url, &[("Accept", "application/json")])
        .ok()?;
    parse_musicbrainz_album(&value, context)
}

/// Pure parser for the MusicBrainz release-group response.
fn parse_musicbrainz_album(value: &Value, context: &TrackContext) -> Option<ArtistBio> {
    let groups = value.pointer("/release-groups")?.as_array()?;
    for group in groups {
        let Some(title) = util::get_str(group, "/title") else {
            continue;
        };
        let Some(id) = util::get_str(group, "/id") else {
            continue;
        };
        if !looks_like_mbid(id) || !context.album_matches(title) {
            continue;
        }
        let credit = artist_credit(group);
        if credit.is_empty() || !context.artist_matches(&credit) {
            continue;
        }
        let primary_type = util::get_str(group, "/primary-type");
        let kind = match primary_type.map(str::to_ascii_lowercase).as_deref() {
            Some("album") => "Album",
            Some("single") => "Single",
            Some("ep") => "EP",
            _ => "Release",
        };
        let year = util::get_str(group, "/first-release-date").and_then(util::year_from_iso);
        let extract = match year {
            Some(year) => format!("{kind} by {credit}, released {year}."),
            None => format!("{kind} by {credit}."),
        };
        return Some(ArtistBio {
            title: title.to_owned(),
            extract,
            url: format!("{MUSICBRAINZ_RELEASE_GROUP_PAGE}{id}"),
            thumbnail: None,
        });
    }
    None
}

fn artist_credit(group: &Value) -> String {
    let Some(credits) = group.pointer("/artist-credit").and_then(Value::as_array) else {
        return String::new();
    };
    let names: Vec<&str> = credits
        .iter()
        .filter_map(|credit| {
            util::get_str(credit, "/name").or_else(|| util::get_str(credit, "/artist/name"))
        })
        .collect();
    util::join_names(names)
}

/// Strip Lucene syntax characters from a user supplied value before it is
/// wrapped in quotes for a MusicBrainz query.
fn lucene_quote(value: &str) -> String {
    value.replace(['"', '\\'], " ").trim().to_owned()
}

fn looks_like_mbid(id: &str) -> bool {
    id.len() == 36
        && id.chars().enumerate().all(|(index, ch)| match index {
            8 | 13 | 18 | 23 => ch == '-',
            _ => ch.is_ascii_hexdigit(),
        })
}

// ---------------------------------------------------------------------------
// Videos
// ---------------------------------------------------------------------------

/// Accept only videos about this artist *and* the album or title, then dedupe
/// and cap the list.
fn collect_videos(
    context: &TrackContext,
    videos: &mut Vec<BackgroundLink>,
    links: Vec<SearchLink>,
    seen: &mut Vec<String>,
) {
    for link in links {
        if videos.len() >= MAX_VIDEOS {
            return;
        }
        let url = link.url.trim().to_owned();
        if url.is_empty() || !url.starts_with("http") || seen.iter().any(|kept| kept == &url) {
            continue;
        }
        let display = format!(
            "{} {}",
            link.title.trim(),
            link.channel.as_deref().unwrap_or_default()
        );
        if !context.general_matches(&display) {
            continue;
        }
        let title = util::clean(&link.title).unwrap_or_else(|| "YouTube video".to_owned());
        let mut parts: Vec<String> = Vec::new();
        if let Some(channel) = link.channel.as_deref().and_then(util::clean) {
            parts.push(channel);
        }
        if let Some(secs) = link
            .duration_secs
            .filter(|secs| secs.is_finite() && *secs > 0.0)
        {
            parts.push(format_duration(secs));
        }
        seen.push(url.clone());
        videos.push(BackgroundLink {
            kind: LinkKind::YouTube,
            title,
            url,
            subtitle: (!parts.is_empty()).then(|| parts.join(" · ")),
        });
    }
}

// ---------------------------------------------------------------------------
// Human-readable links (never API endpoints)
// ---------------------------------------------------------------------------

/// True for URLs that point at a machine API rather than a page a person can
/// read. Used as a final defensive filter.
fn is_api_endpoint(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.contains("/api.php")
        || lower.contains("/api/rest_v1/")
        || lower.contains("musicbrainz.org/ws/")
        || lower.contains("action=query")
}

fn is_human_url(url: &str) -> bool {
    (url.starts_with("https://") || url.starts_with("http://")) && !is_api_endpoint(url)
}

/// Drop API endpoints, non-http junk and duplicate URLs.
fn sanitize_links(links: Vec<BackgroundLink>) -> Vec<BackgroundLink> {
    let mut seen: Vec<String> = Vec::new();
    links
        .into_iter()
        .filter(|link| {
            if !is_human_url(&link.url) {
                return false;
            }
            if seen.iter().any(|kept| kept == &link.url) {
                return false;
            }
            seen.push(link.url.clone());
            true
        })
        .collect()
}

fn bio_link(bio: &ArtistBio, subtitle: &str) -> BackgroundLink {
    let (kind, label) = if bio.url.contains("musicbrainz.org") {
        (LinkKind::Other, "MusicBrainz")
    } else {
        (LinkKind::Wikipedia, "Wikipedia")
    };
    BackgroundLink {
        kind,
        title: format!("{label} — {}", bio.title),
        url: bio.url.clone(),
        subtitle: Some(subtitle.to_owned()),
    }
}

fn wikipedia_search_link(artist: &str) -> BackgroundLink {
    BackgroundLink {
        kind: LinkKind::Wikipedia,
        title: format!("Wikipedia — search for {artist}"),
        // Human search page (`/w/index.php`), never `api.php`.
        url: format!(
            "{WIKI_SEARCH_PAGE}?search={}&ns0=1",
            util::pct(artist.trim())
        ),
        subtitle: Some("Artist search on Wikipedia".to_owned()),
    }
}

fn musicbrainz_artist_link(artist: &str, found: Option<&MusicBrainzArtist>) -> BackgroundLink {
    let Some(found) = found else {
        return BackgroundLink {
            kind: LinkKind::Other,
            title: format!("MusicBrainz — search for {artist}"),
            url: format!(
                "{MUSICBRAINZ_SEARCH_PAGE}?query={}&type=artist&method=indexed",
                util::pct(artist.trim())
            ),
            subtitle: Some("Artist search on MusicBrainz".to_owned()),
        };
    };
    let name = util::clean(&found.name).unwrap_or_else(|| artist.trim().to_owned());
    BackgroundLink {
        kind: LinkKind::Other,
        title: format!("MusicBrainz — {name}"),
        url: format!("https://musicbrainz.org/artist/{}", util::pct(&found.id)),
        subtitle: Some("Artist page on MusicBrainz".to_owned()),
    }
}

fn youtube_search_link(context: &TrackContext) -> BackgroundLink {
    let query = youtube_search_query(context);
    BackgroundLink {
        kind: LinkKind::YouTube,
        title: format!("YouTube — search for {query}"),
        url: format!(
            "{YOUTUBE_RESULTS_PAGE}?search_query={}",
            util::pct(query.trim())
        ),
        subtitle: Some("Interview search".to_owned()),
    }
}

fn genius_link(context: &TrackContext) -> BackgroundLink {
    let mut query = context.artist.clone();
    if !context.title.is_empty() {
        query.push(' ');
        query.push_str(&context.title);
    }
    BackgroundLink {
        kind: LinkKind::Genius,
        title: "Genius — lyrics, samples & annotations".to_owned(),
        url: format!("{GENIUS_SEARCH}?q={}", util::pct(query.trim())),
        subtitle: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const WIKI_FIXTURE: &str = r#"{
        "type": "standard",
        "title": "Rick Astley",
        "extract": "Richard Paul Astley is an English singer and songwriter.",
        "thumbnail": { "source": "https://upload.wikimedia.org/wikipedia/commons/thumb/x.jpg" },
        "content_urls": { "desktop": { "page": "https://en.wikipedia.org/wiki/Rick_Astley" } }
    }"#;

    fn context(artist: &str, album: Option<&str>, title: &str) -> TrackContext {
        let mut track = Track::new(crate::model::Platform::AppleMusic, "test", title, artist);
        if let Some(album) = album {
            track = track.with_album(album);
        }
        TrackContext::from_track(&track).expect("context")
    }

    fn porter() -> TrackContext {
        context("Porter Robinson", Some("Nurture"), "Get Your Wish")
    }

    fn bio(title: &str, extract: &str) -> ArtistBio {
        ArtistBio {
            title: title.to_owned(),
            extract: extract.to_owned(),
            url: format!("https://en.wikipedia.org/wiki/{}", title.replace(' ', "_")),
            thumbnail: None,
        }
    }

    // -----------------------------------------------------------------------
    // Relevance gate
    // -----------------------------------------------------------------------

    #[test]
    fn tokens_are_normalized_and_stripped_of_noise() {
        assert_eq!(
            significant_tokens("Porter Robinson"),
            ["porter", "robinson"]
        );
        assert_eq!(
            significant_tokens("Nurture (Deluxe Edition)"),
            ["nurture"],
            "brackets and edition words are ignored"
        );
        assert_eq!(
            significant_tokens("Get Your Wish - Remastered 2011"),
            ["get", "your", "wish"],
            "year and remaster suffix are ignored"
        );
        assert_eq!(significant_tokens("周杰伦"), ["周杰伦"]);
        assert_eq!(significant_tokens("M.I.A."), ["m i a"]);
        assert_eq!(
            context("The Weeknd", None, "Blinding Lights").artist_tokens,
            ["weeknd"],
            "stop words are dropped"
        );
    }

    #[test]
    fn relevance_gate_accepts_only_this_artist_and_album_or_title() {
        let ctx = porter();
        let album_page = bio(
            "Nurture (album)",
            "Nurture is the second studio album by American singer Porter Robinson.",
        );
        assert!(ctx.album_article_ok(&album_page));
        let song_page = bio(
            "Get Your Wish",
            "Get Your Wish is a song by American record producer Porter Robinson.",
        );
        assert!(ctx.song_article_ok(&song_page));
        // Same album name by another artist is not relevant.
        let wrong_artist = bio(
            "Nurture (album)",
            "Nurture is an album by the band Somebody Else.",
        );
        assert!(!ctx.album_article_ok(&wrong_artist));
        // Same artist, unrelated album.
        let wrong_album = bio(
            "Random Access Memories",
            "Random Access Memories is the fourth studio album by Daft Punk.",
        );
        assert!(!ctx.album_article_ok(&wrong_album));
        // Song gate needs the title, not just the artist.
        let wrong_song = bio(
            "Something Comforting",
            "Something Comforting is a song by Porter Robinson.",
        );
        assert!(!ctx.song_article_ok(&wrong_song));
        // The artist bio must be titled after the artist.
        assert!(ctx.artist_bio_ok(&bio(
            "Porter Robinson",
            "Porter Robinson is an American DJ."
        )));
        assert!(!ctx.artist_bio_ok(&bio("Madeon", "Madeon is a French musician.")));
        // The artist bio may carry the disambiguation suffix.
        assert!(ctx.artist_bio_ok(&bio("Porter Robinson (DJ)", "…")));
    }

    #[test]
    fn video_relevance_requires_artist_and_album_or_title() {
        let ctx = porter();
        let mut videos = Vec::new();
        let mut seen = Vec::new();
        collect_videos(
            &ctx,
            &mut videos,
            vec![
                SearchLink {
                    title: "Porter Robinson - Get Your Wish (Official Video)".to_owned(),
                    url: "https://youtube.com/watch?v=relevant1".to_owned(),
                    channel: Some("Porter Robinson".to_owned()),
                    duration_secs: Some(200.0),
                    thumbnail: None,
                },
                SearchLink {
                    title: "Porter Robinson - Nurture (Full Album)".to_owned(),
                    url: "https://youtube.com/watch?v=relevant2".to_owned(),
                    channel: Some("Porter Robinson".to_owned()),
                    duration_secs: None,
                    thumbnail: None,
                },
                SearchLink {
                    title: "Daft Punk - Get Lucky (Official Video)".to_owned(),
                    url: "https://youtube.com/watch?v=unrelated".to_owned(),
                    channel: Some("Daft Punk".to_owned()),
                    duration_secs: Some(369.0),
                    thumbnail: None,
                },
                SearchLink {
                    title: "Top 10 EDM tracks 2021".to_owned(),
                    url: "https://youtube.com/watch?v=noise".to_owned(),
                    channel: Some("Music Weekly".to_owned()),
                    duration_secs: None,
                    thumbnail: None,
                },
                SearchLink {
                    title: "Get Your Wish".to_owned(),
                    url: "https://youtube.com/watch?v=relevant1".to_owned(),
                    channel: None,
                    duration_secs: None,
                    thumbnail: None,
                },
            ],
            &mut seen,
        );
        assert_eq!(videos.len(), 2, "only relevant, deduped videos survive");
        assert_eq!(videos[0].url, "https://youtube.com/watch?v=relevant1");
        assert_eq!(
            videos[0].subtitle.as_deref(),
            Some("Porter Robinson · 3:20")
        );
        assert_eq!(videos[1].url, "https://youtube.com/watch?v=relevant2");
    }

    #[test]
    fn videos_are_capped_at_five() {
        let ctx = porter();
        let mut videos = Vec::new();
        let mut seen = Vec::new();
        let links: Vec<SearchLink> = (0..8)
            .map(|index| SearchLink {
                title: format!("Porter Robinson Nurture Get Your Wish interview {index}"),
                url: format!("https://youtube.com/watch?v=v{index}"),
                channel: Some("Porter Robinson".to_owned()),
                duration_secs: Some(60.0),
                thumbnail: None,
            })
            .collect();
        collect_videos(&ctx, &mut videos, links, &mut seen);
        assert_eq!(videos.len(), MAX_VIDEOS);
    }

    // -----------------------------------------------------------------------
    // Query construction
    // -----------------------------------------------------------------------

    #[test]
    fn queries_always_include_the_artist_and_are_specific() {
        let ctx = porter();
        let album_queries = album_candidates(&ctx);
        assert_eq!(album_queries[0], "Nurture (album)");
        assert!(album_queries
            .iter()
            .any(|query| query.contains("Porter Robinson")));
        let album_search = album_search_query(&ctx).expect("album query");
        assert!(album_search.contains("Porter Robinson"));
        assert!(album_search.contains("Nurture"));

        let song_queries = song_candidates(&ctx);
        assert_eq!(song_queries[0], "Get Your Wish (song)");
        assert!(song_queries
            .iter()
            .any(|query| query.contains("Porter Robinson")));
        let song_search = song_search_query(&ctx).expect("song query");
        assert!(song_search.contains("Porter Robinson"));
        assert!(song_search.contains("Get Your Wish"));

        let videos = video_queries(&ctx);
        assert_eq!(videos.len(), 2);
        for query in &videos {
            assert!(query.contains("Porter Robinson"), "query: {query}");
            assert!(query.contains("Nurture"), "query: {query}");
            assert!(query.contains("Get Your Wish"), "query: {query}");
            assert!(query.contains("interview") || query.contains("explained"));
        }
        // Without album information the artist + title are still required.
        let no_album = context("Porter Robinson", None, "Get Your Wish");
        for query in video_queries(&no_album) {
            assert!(query.contains("Porter Robinson"));
            assert!(query.contains("Get Your Wish"));
        }
        assert!(video_queries(&context("Artist", None, "")).is_empty());
    }

    // -----------------------------------------------------------------------
    // Wikipedia parsing
    // -----------------------------------------------------------------------

    #[test]
    fn wiki_summary_is_parsed() {
        let value: Value = serde_json::from_str(WIKI_FIXTURE).expect("fixture");
        let bio = parse_wiki_summary(&value).expect("bio");
        assert_eq!(bio.title, "Rick Astley");
        assert!(bio.extract.contains("English singer"));
        assert_eq!(bio.url, "https://en.wikipedia.org/wiki/Rick_Astley");
        assert!(bio.thumbnail.is_some());
    }

    #[test]
    fn disambiguation_pages_are_skipped() {
        let value = json!({
            "type": "disambiguation",
            "title": "Nurture",
            "extract": "Nurture may refer to:",
            "content_urls": { "desktop": { "page": "https://en.wikipedia.org/wiki/Nurture" } }
        });
        assert!(parse_wiki_summary(&value).is_none());
        let value = json!({ "type": "standard", "title": "No extract" });
        assert!(parse_wiki_summary(&value).is_none());
    }

    #[test]
    fn api_urls_are_never_accepted_as_articles() {
        let value = json!({
            "type": "standard",
            "title": "Nurture (album)",
            "extract": "Nurture is an album by Porter Robinson.",
            "content_urls": { "desktop": { "page": "https://en.wikipedia.org/w/api.php?action=query" } }
        });
        assert!(parse_wiki_summary(&value).is_none());
    }

    // -----------------------------------------------------------------------
    // MusicBrainz album fallback
    // -----------------------------------------------------------------------

    const MB_ALBUM_FIXTURE: &str = r#"{
        "release-groups": [
            {
                "id": "2b9e7a0d-1c3f-4c9a-9d2e-5f1a6b7c8d90",
                "title": "Nurture",
                "primary-type": "Album",
                "first-release-date": "2021-04-23",
                "artist-credit": [
                    { "name": "Porter Robinson", "artist": { "id": "a", "name": "Porter Robinson" } }
                ]
            },
            {
                "id": "9f0e1d2c-3b4a-4958-8675-a4b3c2d1e0f9",
                "title": "Nurture (Deluxe)",
                "primary-type": "Album",
                "first-release-date": "2021-09-24",
                "artist-credit": [
                    { "name": "Somebody Else", "artist": { "id": "b", "name": "Somebody Else" } }
                ]
            }
        ]
    }"#;

    #[test]
    fn musicbrainz_album_fallback_is_factual_and_relevant() {
        let ctx = porter();
        let value: Value = serde_json::from_str(MB_ALBUM_FIXTURE).expect("fixture");
        let album = parse_musicbrainz_album(&value, &ctx).expect("relevant album");
        assert_eq!(album.title, "Nurture");
        assert_eq!(album.extract, "Album by Porter Robinson, released 2021.");
        assert!(album.thumbnail.is_none());
        assert_eq!(
            album.url,
            "https://musicbrainz.org/release-group/2b9e7a0d-1c3f-4c9a-9d2e-5f1a6b7c8d90"
        );
        assert!(!album.url.contains("ws/2"));
        // A different album name yields nothing.
        let other = context("Porter Robinson", Some("Worlds"), "Get Your Wish");
        assert!(parse_musicbrainz_album(&value, &other).is_none());
    }

    #[test]
    fn musicbrainz_album_rejects_wrong_artist() {
        let ctx = porter();
        let value = json!({
            "release-groups": [{
                "id": "9f0e1d2c-3b4a-4958-8675-a4b3c2d1e0f9",
                "title": "Nurture",
                "primary-type": "Album",
                "first-release-date": "2021",
                "artist-credit": [{ "name": "Somebody Else" }]
            }]
        });
        assert!(parse_musicbrainz_album(&value, &ctx).is_none());
        assert!(parse_musicbrainz_album(&json!({}), &ctx).is_none());
    }

    #[test]
    fn musicbrainz_artist_fixture_is_parsed() {
        let fixture = json!({
            "count": 2,
            "artists": [
                { "id": "not-an-mbid", "name": "Wrong" },
                {
                    "id": "8f1a5c3e-9d42-4b7a-8c61-2e5f9a0d3b47",
                    "name": "Drumboii",
                    "score": 100
                }
            ]
        });
        let artist = parse_musicbrainz_artist(&fixture).expect("artist parsed");
        assert_eq!(artist.name, "Drumboii");
        assert_eq!(artist.id, "8f1a5c3e-9d42-4b7a-8c61-2e5f9a0d3b47");
        assert!(parse_musicbrainz_artist(&json!({ "artists": [] })).is_none());
        assert!(parse_musicbrainz_artist(&json!({})).is_none());
    }

    #[test]
    fn lucene_values_are_escaped() {
        assert_eq!(lucene_quote("AC/DC \"Live\""), "AC/DC  Live");
        assert_eq!(lucene_quote("Nurture"), "Nurture");
    }

    // -----------------------------------------------------------------------
    // Links
    // -----------------------------------------------------------------------

    #[test]
    fn wikipedia_fallback_uses_the_human_search_page_and_is_encoded() {
        let link = wikipedia_search_link("Drumboii");
        assert_eq!(
            link.url,
            "https://en.wikipedia.org/w/index.php?search=Drumboii&ns0=1"
        );
        assert!(!link.url.contains("/api.php"));
        assert_eq!(
            wikipedia_search_link("Rick Astley").url,
            "https://en.wikipedia.org/w/index.php?search=Rick%20Astley&ns0=1"
        );
        let slash = wikipedia_search_link("AC/DC");
        assert!(slash.url.contains("search=AC%2FDC"), "got: {}", slash.url);
        let cjk = wikipedia_search_link("周杰伦");
        assert!(cjk.url.contains("%E5%91%A8%E6%9D%B0%E4%BC%A6"));
    }

    #[test]
    fn musicbrainz_links_are_human_pages_and_encoded() {
        let search = musicbrainz_artist_link("Earth, Wind & Fire", None);
        assert!(search
            .url
            .starts_with("https://musicbrainz.org/search?query="));
        assert!(search.url.contains("type=artist"));
        assert!(search.url.contains("method=indexed"));
        assert!(search.url.contains("%2C"), "got: {}", search.url);
        assert!(!search.url.contains("ws/2"));
        let found = MusicBrainzArtist {
            name: "Drumboii".to_owned(),
            id: "8f1a5c3e-9d42-4b7a-8c61-2e5f9a0d3b47".to_owned(),
        };
        let page = musicbrainz_artist_link("Drumboii", Some(&found));
        assert_eq!(
            page.url,
            "https://musicbrainz.org/artist/8f1a5c3e-9d42-4b7a-8c61-2e5f9a0d3b47"
        );
    }

    #[test]
    fn youtube_fallback_is_a_human_search_page() {
        let ctx = porter();
        let link = youtube_search_link(&ctx);
        assert!(link
            .url
            .starts_with("https://www.youtube.com/results?search_query="));
        assert!(link.url.contains("Porter%20Robinson"));
        assert!(link.url.contains("Nurture"));
        assert!(link.url.contains("Get%20Your%20Wish"));
        assert_eq!(link.kind, LinkKind::YouTube);
    }

    #[test]
    fn genius_query_contains_artist_and_title() {
        let ctx = porter();
        let link = genius_link(&ctx);
        assert_eq!(link.kind, LinkKind::Genius);
        assert_eq!(
            link.url,
            "https://genius.com/search?q=Porter%20Robinson%20Get%20Your%20Wish"
        );
    }

    #[test]
    fn api_endpoints_are_detected_and_filtered_from_user_links() {
        assert!(is_api_endpoint(
            "https://en.wikipedia.org/w/api.php?search=drumboii&ns0=1"
        ));
        assert!(is_api_endpoint(
            "https://en.wikipedia.org/api/rest_v1/page/summary/X"
        ));
        assert!(is_api_endpoint(
            "https://musicbrainz.org/ws/2/artist/?query=x&fmt=json"
        ));
        assert!(!is_api_endpoint(
            "https://en.wikipedia.org/wiki/Rick_Astley"
        ));
        assert!(!is_api_endpoint(
            "https://musicbrainz.org/release-group/2b9e7a0d-1c3f-4c9a-9d2e-5f1a6b7c8d90"
        ));
        let links = sanitize_links(vec![
            bio_link(
                &bio("Nurture (album)", "Nurture is an album by Porter Robinson."),
                "Album background",
            ),
            BackgroundLink {
                kind: LinkKind::Other,
                title: "API leak".to_owned(),
                url: "https://en.wikipedia.org/w/api.php?action=query".to_owned(),
                subtitle: None,
            },
            BackgroundLink {
                kind: LinkKind::Other,
                title: "Duplicate".to_owned(),
                url: "https://en.wikipedia.org/wiki/Nurture_(album)".to_owned(),
                subtitle: None,
            },
        ]);
        assert_eq!(links.len(), 1);
        assert!(links.iter().all(|link| !is_api_endpoint(&link.url)));
    }

    #[test]
    fn bio_links_use_the_right_human_page_kind() {
        let wiki = bio_link(
            &bio("Nurture (album)", "Nurture is an album by Porter Robinson."),
            "Album background",
        );
        assert_eq!(wiki.kind, LinkKind::Wikipedia);
        assert_eq!(wiki.url, "https://en.wikipedia.org/wiki/Nurture_(album)");
        let mb = bio_link(
            &ArtistBio {
                title: "Nurture".to_owned(),
                extract: "Album by Porter Robinson, released 2021.".to_owned(),
                url: "https://musicbrainz.org/release-group/2b9e7a0d".to_owned(),
                thumbnail: None,
            },
            "Album background",
        );
        assert_eq!(mb.kind, LinkKind::Other);
        assert!(mb.title.contains("MusicBrainz"));
    }

    // -----------------------------------------------------------------------
    // End-to-end shape (offline)
    // -----------------------------------------------------------------------

    #[test]
    fn background_works_offline_for_a_local_track() {
        // No artist metadata → show nothing rather than random context.
        let http = Http::new();
        let config = ResolveConfig {
            ytdlp: None,
            ffmpeg: None,
            cache_dir: std::env::temp_dir().join("echoport-bg-test"),
        };
        let track = Track::from_local_path(std::path::PathBuf::from("/tmp/song.mp3"));
        let background = fetch(&http, &config, &track);
        assert!(background.bio.is_none());
        assert!(background.album.is_none());
        assert!(background.song.is_none());
        assert!(background.links.is_empty());
        assert!(background.videos.is_empty());
    }

    #[test]
    fn artist_only_context_and_bio_helper_require_an_artist() {
        let context = TrackContext::from_artist("Porter Robinson").expect("context");
        assert_eq!(context.artist, "Porter Robinson");
        assert_eq!(context.artist_tokens, ["porter", "robinson"]);
        assert!(context.album.is_none());
        assert!(context.title.is_empty());
        assert!(TrackContext::from_artist("  ").is_none());
        // Empty names are rejected before any network access.
        let http = Http::new();
        assert!(artist_bio_for(&http, "").is_none());
        assert!(artist_bio_for(&http, "   ").is_none());
    }

    #[test]
    fn context_requires_an_artist() {
        let mut track = Track::new(crate::model::Platform::AppleMusic, "x", "Get Your Wish", "");
        track = track.with_album("Nurture");
        assert!(TrackContext::from_track(&track).is_none());
    }

    // -----------------------------------------------------------------------
    // Network smoke tests. Run with:
    //   cargo test --lib background:: -- --ignored --nocapture
    // -----------------------------------------------------------------------

    fn live_config() -> ResolveConfig {
        ResolveConfig {
            ytdlp: crate::resolver::detect_ytdlp(),
            ffmpeg: None,
            cache_dir: std::env::temp_dir().join("echoport-bg-test"),
        }
    }

    fn assert_all_links_human(background: &SongBackground) {
        for link in background.links.iter().chain(background.videos.iter()) {
            assert!(!link.url.contains("/api.php"), "got: {}", link.url);
            assert!(!is_api_endpoint(&link.url), "got: {}", link.url);
            assert!(link.url.starts_with("https://"), "got: {}", link.url);
            assert!(!link.url.contains(' '), "unencoded url: {}", link.url);
        }
    }

    #[test]
    #[ignore = "network smoke test"]
    fn net_background_fetch() {
        let http = Http::new();
        let config = live_config();
        let mut track = Track::new(
            crate::model::Platform::YouTube,
            "dQw4w9WgXcQ",
            "Never Gonna Give You Up",
            "Rick Astley",
        );
        track = track.with_page("https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        let started = Instant::now();
        let background = fetch(&http, &config, &track);
        println!("background in {:?}: {background:#?}", started.elapsed());
        assert!(background.bio.is_some());
        assert!(background.bio.as_ref().unwrap().extract.len() > 40);
        assert_all_links_human(&background);
        assert!(started.elapsed() < Duration::from_secs(12));
    }

    #[test]
    #[ignore = "network smoke test"]
    fn net_porter_robinson_nurture_album_and_single() {
        let http = Http::new();
        let config = live_config();
        // "Get Your Wish" is a single from Porter Robinson's album "Nurture".
        let mut track = Track::new(
            crate::model::Platform::AppleMusic,
            "nurture-get-your-wish",
            "Get Your Wish",
            "Porter Robinson",
        );
        track = track.with_album("Nurture");
        let started = Instant::now();
        let background = fetch(&http, &config, &track);
        println!(
            "Porter Robinson background in {:?}: {background:#?}",
            started.elapsed()
        );
        assert_all_links_human(&background);
        let album = background
            .album
            .as_ref()
            .expect("Nurture album background must be found");
        assert!(
            album.extract.contains("Porter Robinson") || album.extract.contains("Nurture"),
            "album extract: {}",
            album.extract
        );
        if let Some(song) = &background.song {
            assert!(song.title.contains("Get Your Wish"), "song: {song:?}");
            assert!(song.extract.contains("Porter Robinson"), "song: {song:?}");
        }
        // Every video must mention the artist and the album or title.
        let context = TrackContext::from_track(&track).expect("context");
        assert!(background.videos.len() <= MAX_VIDEOS);
        for video in &background.videos {
            let display = format!(
                "{} {}",
                video.title,
                video.subtitle.as_deref().unwrap_or_default()
            );
            assert!(
                context.general_matches(&display),
                "unrelated video leaked: {video:?}"
            );
        }
        assert!(started.elapsed() < Duration::from_secs(12));
    }

    #[test]
    #[ignore = "network smoke test"]
    fn net_musicbrainz_album_fallback() {
        // Directly exercise the factual release-group fallback (the Wikipedia
        // album article is not involved here).
        let http = Http::new();
        let ctx = porter();
        // MusicBrainz returns 503 "server busy" occasionally; retry briefly.
        let mut album = None;
        for attempt in 0..3 {
            album = musicbrainz_album(&http, &ctx);
            if album.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(1_200));
            log::debug!("MusicBrainz release-group retry {attempt}");
        }
        let album = album.expect("MusicBrainz release-group (after retries)");
        println!("MusicBrainz album: {album:?}");
        assert_eq!(album.title, "Nurture");
        assert!(
            album.extract.contains("Porter Robinson"),
            "extract: {}",
            album.extract
        );
        assert!(album.extract.contains("2021"), "extract: {}", album.extract);
        assert!(album
            .url
            .starts_with("https://musicbrainz.org/release-group/"));
        assert!(!is_api_endpoint(&album.url));
    }

    #[test]
    #[ignore = "network smoke test"]
    fn net_background_niche_artist_gets_fallbacks() {
        // Regression scenario from the older bug report: "Drumboii" has no
        // Wikipedia article, so the tab must fall back to human search /
        // MusicBrainz pages instead of linking the MediaWiki API help page.
        let http = Http::new();
        let config = live_config();
        let track = Track::new(
            crate::model::Platform::YouTube,
            "drumboii-test",
            "Test Song",
            "Drumboii",
        );
        let started = Instant::now();
        let background = fetch(&http, &config, &track);
        println!(
            "niche background in {:?}: {background:#?}",
            started.elapsed()
        );
        assert_all_links_human(&background);
        // Unrelated content must be empty.
        assert!(background.album.is_none());
        assert!(
            background.song.is_none(),
            "unrelated song article leaked: {:?}",
            background.song
        );
        let context = TrackContext::from_track(&track).expect("context");
        for video in &background.videos {
            let display = format!(
                "{} {}",
                video.title,
                video.subtitle.as_deref().unwrap_or_default()
            );
            assert!(
                context.general_matches(&display),
                "unrelated video leaked: {video:?}"
            );
        }
        if background.bio.is_none() {
            assert!(background.links.iter().any(|link| link
                .url
                .starts_with("https://en.wikipedia.org/w/index.php?search=")));
            assert!(background
                .links
                .iter()
                .any(|link| link.url.contains("musicbrainz.org")));
        }
        assert!(
            started.elapsed() < Duration::from_secs(12),
            "background exceeded the time budget: {:?}",
            started.elapsed()
        );
    }
}
