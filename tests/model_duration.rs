//! Integration tests: model helpers — duration parsing/formatting, cache keys,
//! stable hashes, track builders and platform/download enums.

use std::path::PathBuf;

use echoport::model::{
    ellipsize, format_duration, stable_hash, DownloadFormat, Platform, Track, TrackId,
};

#[test]
fn duration_formatting_covers_hours_minutes_seconds() {
    assert_eq!(format_duration(0.0), "--:--");
    assert_eq!(format_duration(-1.0), "--:--");
    assert_eq!(format_duration(f64::NAN), "--:--");
    assert_eq!(format_duration(f64::INFINITY), "--:--");
    assert_eq!(format_duration(f64::NEG_INFINITY), "--:--");
    assert_eq!(
        format_duration(0.4),
        "0:00",
        "sub-second positive durations round to zero"
    );
    assert_eq!(format_duration(1.0), "0:01");
    assert_eq!(format_duration(59.0), "0:59");
    assert_eq!(
        format_duration(59.9),
        "1:00",
        "rounds to the nearest second"
    );
    assert_eq!(format_duration(60.0), "1:00");
    assert_eq!(format_duration(65.0), "1:05");
    assert_eq!(
        format_duration(213.573),
        "3:34",
        "Spotify's duration for Never Gonna Give You Up"
    );
    assert_eq!(format_duration(326.0), "5:26");
    assert_eq!(format_duration(3599.0), "59:59");
    assert_eq!(format_duration(3600.0), "1:00:00");
    assert_eq!(format_duration(3725.0), "1:02:05");
    assert_eq!(format_duration(86_399.0), "23:59:59");
}

#[test]
fn duration_formatting_is_bilingual_neutral() {
    // Digits and separators only — never localized, so both UI languages share it.
    let zh = format_duration(65.0);
    assert_eq!(zh, "1:05");
    assert!(zh.chars().all(|c| c.is_ascii_digit() || c == ':'));
}

#[test]
fn ellipsize_counts_characters_not_bytes() {
    assert_eq!(ellipsize("short", 10), "short");
    assert_eq!(ellipsize("exactly10!", 10), "exactly10!");
    assert_eq!(ellipsize("This is a very long title", 10), "This is a…");
    // CJK titles are measured in characters (3 bytes each in UTF-8) and never
    // get cut mid-codepoint.
    let zh = "海阔天空现场版超长歌名";
    let cut = ellipsize(zh, 5);
    assert_eq!(cut.chars().count(), 5);
    assert_eq!(cut.chars().last(), Some('…'));
    assert!(cut.starts_with("海阔天"));
    assert_eq!(ellipsize("", 5), "");
}

#[test]
fn stable_hash_is_stable_and_matches_fnv1a_reference() {
    // Golden values: changing these would change cache keys of existing users.
    assert_eq!(stable_hash("hello"), "a430d84680aabd0b");
    assert_eq!(stable_hash("world"), "4f59ff5e730c8af3");
    assert_eq!(stable_hash("echoport"), "1a5239bcd607978f");
    assert_eq!(stable_hash(""), "cbf29ce484222325");
    assert_eq!(stable_hash("hello").len(), 16);
    assert!(stable_hash("hello").chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(stable_hash("hello"), stable_hash("hello"));
    assert_ne!(stable_hash("hello"), stable_hash("hello "));
    // Unicode input is hashed as UTF-8 bytes.
    assert_eq!(stable_hash("海阔天空"), stable_hash("海阔天空"));
    assert_ne!(stable_hash("海阔天空"), stable_hash("海闊天空"));
}

#[test]
fn remote_cache_keys_are_platform_namespaced() {
    let spotify_a = Track::new(Platform::Spotify, "4cOdK2wGLETKBW3PvgPWqT", "T", "A");
    let spotify_b = Track::new(
        Platform::Spotify,
        "4cOdK2wGLETKBW3PvgPWqT",
        "Different title",
        "B",
    );
    let youtube = Track::new(Platform::YouTube, "4cOdK2wGLETKBW3PvgPWqT", "T", "A");

    assert_eq!(
        spotify_a.cache_key(),
        spotify_b.cache_key(),
        "cache key is by track id only"
    );
    assert_ne!(
        spotify_a.cache_key(),
        youtube.cache_key(),
        "same source id, different platform"
    );
    assert_eq!(
        spotify_a.cache_key(),
        spotify_a.cache_key(),
        "stable across calls"
    );
    assert_ne!(
        spotify_a.id, youtube.id,
        "track ids are namespaced by platform"
    );

    // Local files use a distinct, path-based cache key namespace.
    let local = Track::from_local_path(PathBuf::from("/music/a.flac"));
    assert!(local.cache_key().starts_with("local-"));
    assert_ne!(local.cache_key(), spotify_a.cache_key());
    let same_local = Track::from_local_path(PathBuf::from("/music/a.flac"));
    assert_eq!(local.cache_key(), same_local.cache_key());
    let other_local = Track::from_local_path(PathBuf::from("/music/b.flac"));
    assert_ne!(local.cache_key(), other_local.cache_key());
}

#[test]
fn track_builders_reject_invalid_durations_and_empties() {
    let mut track = Track::new(Platform::Netease, "347230", "海阔天空", "Beyond");
    assert_eq!(track.duration_secs, None);
    track = track.with_duration(0.0);
    assert_eq!(track.duration_secs, None, "zero duration is not stored");
    track = track.with_duration(-5.0);
    assert_eq!(track.duration_secs, None);
    track = track.with_duration(f64::NAN);
    assert_eq!(track.duration_secs, None);
    track = track.with_duration(326.0);
    assert_eq!(track.duration_secs, Some(326.0));

    track = track
        .with_album("乐与怒")
        .with_art("https://example.invalid/cover.jpg")
        .with_page("https://music.163.com/song?id=347230")
        .with_stream("")
        .with_year(1993)
        .with_duration(326.0);
    assert_eq!(track.album.as_deref(), Some("乐与怒"));
    assert_eq!(track.year, Some(1993));
    assert!(track.art_url.is_some());
    assert!(track.page_url.is_some());
    assert_eq!(track.stream_url, None, "empty stream URLs are ignored");

    // Empty metadata is dropped rather than stored as "".
    let empty = Track::new(Platform::SoundCloud, "sc", "T", "A")
        .with_art("")
        .with_page("")
        .with_stream("");
    assert_eq!(empty.art_url, None);
    assert_eq!(empty.page_url, None);
    assert_eq!(empty.stream_url, None);
}

#[test]
fn track_display_and_search_helpers_handle_missing_metadata() {
    let mut track = Track::new(
        Platform::YouTube,
        "dQw4w9WgXcQ",
        "Never Gonna Give You Up",
        "",
    );
    assert_eq!(
        track.display_artist(),
        "—",
        "unknown artist is rendered as an em dash"
    );
    assert_eq!(track.search_text(), "Never Gonna Give You Up");
    assert!(!track.is_local());

    track.artist = "Rick Astley".to_owned();
    assert_eq!(track.display_artist(), "Rick Astley");
    assert_eq!(track.search_text(), "Rick Astley Never Gonna Give You Up");

    let local = Track::from_local_path(PathBuf::from("/music/样本歌曲.mp3"));
    assert!(local.is_local());
    assert_eq!(local.platform, Platform::Local);
    assert_eq!(local.title, "样本歌曲");
    assert_eq!(local.local_path, Some(PathBuf::from("/music/样本歌曲.mp3")));
    assert_eq!(local.artist, "");
}

#[test]
fn platform_keys_and_labels_roundtrip() {
    assert_eq!(Platform::ALL.len(), 7);
    assert_eq!(Platform::REMOTE.len(), 6);
    assert!(!Platform::REMOTE.contains(&Platform::Local));
    for platform in Platform::ALL {
        assert_eq!(Platform::from_key(platform.key()), Some(platform));
        assert!(!platform.label().is_empty());
        assert_eq!(platform.is_streaming(), platform != Platform::Local);
    }
    assert_eq!(Platform::AppleMusic.label(), "Apple Music");
    assert_eq!(Platform::AppleMusic.key(), "apple_music");
    assert_eq!(Platform::from_key("unsupported"), None);
    // Brand names are never translated (the UI shows these as-is).
    assert_eq!(Platform::Netease.label(), "Netease");
}

#[test]
fn download_formats_roundtrip() {
    assert_eq!(DownloadFormat::ALL.len(), 4);
    for format in DownloadFormat::ALL {
        assert_eq!(DownloadFormat::from_key(format.key()), Some(format));
        assert!(!format.label().is_empty());
    }
    assert_eq!(DownloadFormat::from_key("mp3"), Some(DownloadFormat::Mp3));
    assert_eq!(DownloadFormat::from_key("M4A"), None, "keys are lowercase");
    assert_eq!(DownloadFormat::Mp3.key(), "mp3");
}

#[test]
fn track_id_alias_is_usable() {
    let id: TrackId = Track::new(Platform::Spotify, "x", "T", "A").id;
    assert!(id.starts_with("spotify:"));
    assert_eq!(id.split(':').count(), 2);
}
