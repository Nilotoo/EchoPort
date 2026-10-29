//! Producer/artist card helpers shared by the artist search and artist page.
//!
//! Platforms with a public artist catalogue (Netease, iTunes) implement their
//! own search in `netease.rs` / `apple.rs`. YouTube, SoundCloud and Bilibili
//! have no keyless artist search, so their creator cards are derived from the
//! regular track search: channel/uploader names that match the query become
//! artists. Their artist page is then loaded by searching that name again.

use crate::model::{Artist, Platform, Track};
use crate::providers::util;
use crate::resolver::{self, ResolveConfig, ResolveError};

/// Human search page used as the "artist page" on platforms without an artist
/// endpoint. The query is percent-encoded.
pub fn creator_search_url(platform: Platform, name: &str) -> Option<String> {
    let query = util::pct(name.trim());
    match platform {
        Platform::YouTube => Some(format!(
            "https://www.youtube.com/results?search_query={query}"
        )),
        Platform::SoundCloud => Some(format!("https://soundcloud.com/search?q={query}")),
        Platform::Bilibili => Some(format!("https://search.bilibili.com/all?keyword={query}")),
        _ => None,
    }
}

/// Channel names are often suffixed with YouTube's automatic "- Topic" tag;
/// strip it so the same creator is not listed twice.
pub fn clean_creator_name(raw: &str) -> Option<String> {
    let name = util::clean(raw)?;
    let mut cleaned = name.as_str();
    for suffix in [" - Topic", "- Topic", " - topic", "- topic"] {
        if let Some(stripped) = cleaned.strip_suffix(suffix) {
            cleaned = stripped;
            break;
        }
    }
    let cleaned = cleaned.trim();
    (!cleaned.is_empty()).then(|| cleaned.to_owned())
}

/// Derive creator cards from track results (YouTube / SoundCloud / Bilibili).
///
/// Only channel names matching the query become cards; the result is ranked by
/// relevance and capped at `limit` (the contract asks for at most 2 derived
/// creators per platform).
pub fn derive_creators(
    platform: Platform,
    query: &str,
    tracks: &[Track],
    limit: usize,
) -> Vec<Artist> {
    let mut artists: Vec<Artist> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for track in tracks {
        let Some(name) = clean_creator_name(&track.artist) else {
            continue;
        };
        if util::name_relevance(query, &name) == 0 {
            continue;
        }
        let key = util::normalize_match(&name);
        if seen.iter().any(|kept| kept == &key) {
            continue;
        }
        seen.push(key);
        // No artist API: use the (cleaned) channel name as source id, so the
        // artist page provider can search it again.
        let mut artist = Artist::new(platform, &name, &name);
        if let Some(page) = creator_search_url(platform, &name) {
            artist = artist.with_page(page);
        }
        artists.push(artist);
    }
    let mut ranked =
        util::rank_by_score(artists, |artist| util::name_relevance(query, &artist.name));
    ranked.truncate(limit);
    ranked
}

/// Hot tracks for a resolver-backed platform: search by the artist name and
/// keep only rows actually credited to that artist.
pub fn resolver_hot_tracks(
    config: &ResolveConfig,
    artist: &Artist,
    limit: usize,
) -> Result<Vec<Track>, ResolveError> {
    let search_limit = limit.saturating_mul(2).max(10);
    let tracks = resolver::ytdlp_search(config, artist.platform, &artist.name, search_limit)?;
    Ok(filter_hot_tracks(tracks, &artist.name, limit))
}

/// Keep tracks whose artist/channel matches the artist name, dedupe and cap.
pub fn filter_hot_tracks(tracks: Vec<Track>, artist_name: &str, limit: usize) -> Vec<Track> {
    let mut kept: Vec<Track> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for track in tracks {
        if track.title.trim().is_empty() || util::name_relevance(artist_name, &track.artist) == 0 {
            continue;
        }
        let channel = clean_creator_name(&track.artist).unwrap_or_else(|| track.artist.clone());
        let key = util::normalize_match(&format!("{} {}", track.title, channel));
        if seen.iter().any(|kept| kept == &key) {
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

/// Rank artist cards by relevance to the query, dedupe by platform id/name and
/// cap at `limit`.
pub fn finalize_artists(artists: Vec<Artist>, query: &str, limit: usize) -> Vec<Artist> {
    let mut deduped: Vec<Artist> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for artist in artists {
        if artist.name.trim().is_empty() {
            continue;
        }
        let key = util::normalize_match(&artist.name);
        if seen.iter().any(|kept| kept == &key) {
            continue;
        }
        seen.push(key);
        deduped.push(artist);
    }
    let mut ranked =
        util::rank_by_score(deduped, |artist| util::name_relevance(query, &artist.name));
    ranked.truncate(limit);
    ranked
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(platform: Platform, title: &str, artist: &str) -> Track {
        Track::new(platform, title, title, artist)
    }

    #[test]
    fn creators_are_derived_from_matching_channels_only() {
        let tracks = vec![
            track(Platform::YouTube, "Get Your Wish", "Porter Robinson"),
            track(
                Platform::YouTube,
                "Get Your Wish (Cover)",
                "Some Cover Band",
            ),
            track(Platform::YouTube, "Shelter", "Porter Robinson - Topic"),
            track(Platform::YouTube, "Nurture", "Porter Robinson Tribute Band"),
            track(Platform::YouTube, "Unrelated", ""),
        ];
        let creators = derive_creators(Platform::YouTube, "Porter Robinson", &tracks, 2);
        assert_eq!(creators.len(), 2, "capped at limit");
        assert_eq!(creators[0].name, "Porter Robinson");
        assert_eq!(creators[0].platform, Platform::YouTube);
        assert_eq!(creators[0].source_id, "Porter Robinson");
        assert_eq!(
            creators[0].page_url.as_deref(),
            Some("https://www.youtube.com/results?search_query=Porter%20Robinson")
        );
        // "- Topic" duplicates are merged into the same creator.
        assert!(creators
            .iter()
            .all(|artist| artist.name != "Porter Robinson - Topic"));
    }

    #[test]
    fn creators_are_dropped_when_nothing_matches_the_query() {
        let tracks = vec![
            track(Platform::SoundCloud, "Song", "Daft Punk"),
            track(Platform::SoundCloud, "Song 2", "Somebody Else"),
        ];
        assert!(derive_creators(Platform::SoundCloud, "Porter Robinson", &tracks, 2).is_empty());
    }

    #[test]
    fn creator_search_urls_are_human_and_encoded() {
        assert_eq!(
            creator_search_url(Platform::YouTube, "Porter Robinson").as_deref(),
            Some("https://www.youtube.com/results?search_query=Porter%20Robinson")
        );
        assert_eq!(
            creator_search_url(Platform::SoundCloud, "AC/DC").as_deref(),
            Some("https://soundcloud.com/search?q=AC%2FDC")
        );
        assert_eq!(
            creator_search_url(Platform::Bilibili, "周杰伦").as_deref(),
            Some("https://search.bilibili.com/all?keyword=%E5%91%A8%E6%9D%B0%E4%BC%A6")
        );
        assert!(creator_search_url(Platform::Netease, "X").is_none());
    }

    #[test]
    fn hot_tracks_are_filtered_deduped_and_capped() {
        let tracks = vec![
            track(Platform::YouTube, "Get Your Wish", "Porter Robinson"),
            track(
                Platform::YouTube,
                "Get Your Wish",
                "Porter Robinson - Topic",
            ),
            track(Platform::YouTube, "Get Lucky", "Daft Punk"),
            track(Platform::YouTube, "Shelter", "Porter Robinson, Madeon"),
            track(Platform::YouTube, "Sad Machine", "Porter Robinson"),
        ];
        let hot = filter_hot_tracks(tracks, "Porter Robinson", 2);
        assert_eq!(hot.len(), 2);
        assert_eq!(hot[0].title, "Get Your Wish");
        assert_eq!(hot[1].title, "Shelter");
    }

    #[test]
    fn finalize_artists_ranks_exact_match_first() {
        let artists = vec![
            Artist::new(Platform::Netease, "1", "Porter Robinson Tribute"),
            Artist::new(Platform::Netease, "2", "Porter Robinson"),
            Artist::new(Platform::Netease, "3", "The Porter Robinson Experience"),
            Artist::new(Platform::Netease, "2", "Porter Robinson"),
        ];
        let ranked = finalize_artists(artists, "Porter Robinson", 10);
        assert_eq!(ranked.len(), 3, "duplicate artist is dropped");
        assert_eq!(ranked[0].name, "Porter Robinson");
        assert_eq!(ranked[1].name, "Porter Robinson Tribute");
        assert_eq!(ranked[2].name, "The Porter Robinson Experience");
    }
}
