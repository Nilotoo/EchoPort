//! Integration tests: mixed-platform playlists, favourites and history.
//!
//! EchoPort playlists deliberately mix local files with every web platform, so
//! these tests assert ordering, de-duplication and persistence across all of
//! `Platform::ALL` (a playlist is a list of track ids plus a metadata map).

use echoport::model::{Platform, Playlist, Profile, RepeatMode, Settings, Track};
use echoport::store::{AppPaths, ProfileStore};

fn track(platform: Platform, source: &str, title: &str, artist: &str) -> Track {
    Track::new(platform, source, title, artist)
}

fn all_platform_tracks() -> Vec<Track> {
    vec![
        {
            let mut t = track(Platform::Local, "/music/local.flac", "本地曲目", "本地歌手");
            t.local_path = Some("/music/local.flac".into());
            t.with_duration(180.0)
        },
        track(
            Platform::Spotify,
            "4cOdK2wGLETKBW3PvgPWqT",
            "Never Gonna Give You Up",
            "Rick Astley",
        )
        .with_duration(213.573)
        .with_page("https://open.spotify.com/track/4cOdK2wGLETKBW3PvgPWqT"),
        track(
            Platform::AppleMusic,
            "1559885421",
            "Never Gonna Give You Up",
            "Rick Astley",
        )
        .with_duration(213.0),
        track(Platform::Netease, "347230", "海阔天空", "Beyond").with_duration(326.0),
        track(Platform::SoundCloud, "sc-1", "Lofi Beat", "Some Artist").with_duration(154.2),
        track(Platform::Bilibili, "BV1GJ411x7h7", "音乐现场", "UP主").with_duration(240.0),
        track(
            Platform::YouTube,
            "dQw4w9WgXcQ",
            "Official Video",
            "Rick Astley",
        )
        .with_duration(213.0),
    ]
}

#[test]
fn mixed_platform_playlist_keeps_order_and_metadata() {
    let mut profile = Profile::new("Mixer", Settings::default());
    let playlist_id = profile.create_playlist("Everything / 全部");
    let tracks = all_platform_tracks();

    assert_eq!(
        profile.add_tracks_to_playlist(&playlist_id, &tracks),
        tracks.len()
    );
    let playlist = profile.playlist(&playlist_id).expect("playlist exists");
    assert_eq!(playlist.track_ids.len(), 7);
    assert_eq!(
        playlist.track_ids,
        tracks.iter().map(|t| t.id.clone()).collect::<Vec<_>>()
    );

    let stored = profile.playlist_tracks(&playlist_id);
    assert_eq!(stored.len(), 7);
    for (expected, actual) in tracks.iter().zip(stored.iter()) {
        assert_eq!(
            expected, actual,
            "track metadata should round-trip through the profile map"
        );
        assert_eq!(expected.platform, actual.platform);
    }
    // Platforms are all distinct and every one is used.
    let mut platforms: Vec<Platform> = stored.iter().map(|t| t.platform).collect();
    platforms.dedup();
    assert_eq!(platforms.len(), 7);
    assert!(stored.iter().any(|t| t.is_local()));
}

#[test]
fn duplicate_tracks_are_ignored_and_removal_keeps_order() {
    let mut profile = Profile::new("Dedup", Settings::default());
    let playlist_id = profile.create_playlist("Dedup list");
    let a = track(Platform::YouTube, "a", "A", "Artist");
    let b = track(Platform::Netease, "b", "B", "Artist");
    let c = track(Platform::Local, "c", "C", "Artist");

    assert!(profile.add_to_playlist(&playlist_id, &a));
    assert!(
        !profile.add_to_playlist(&playlist_id, &a),
        "same track twice is a no-op"
    );
    assert!(profile.add_to_playlist(&playlist_id, &b));
    assert!(profile.add_to_playlist(&playlist_id, &c));
    assert_eq!(profile.playlist(&playlist_id).unwrap().len(), 3);

    assert!(profile.remove_from_playlist(&playlist_id, &b.id));
    let remaining: Vec<_> = profile
        .playlist_tracks(&playlist_id)
        .into_iter()
        .map(|t| t.title)
        .collect();
    assert_eq!(remaining, vec!["A".to_owned(), "C".to_owned()]);
    assert!(
        !profile.remove_from_playlist(&playlist_id, &b.id),
        "already removed"
    );
    assert!(!profile.remove_from_playlist("no-such-playlist", &a.id));
}

#[test]
fn playlist_create_rename_delete_lifecycle() {
    let mut profile = Profile::new("Life", Settings::default());
    assert!(profile.playlists.is_empty());

    let one = profile.create_playlist("First");
    let two = profile.create_playlist("Second");
    assert_eq!(profile.playlists.len(), 2);
    // Newest playlist is first, names and ids are unique.
    assert_eq!(profile.playlists[0].id, two);
    assert_ne!(one, two);

    profile.playlist_mut(&one).unwrap().name = "Renamed".to_owned();
    profile.playlist_mut(&one).unwrap().touch();
    assert_eq!(profile.playlist(&one).unwrap().name, "Renamed");

    assert!(profile.delete_playlist(&two));
    assert!(!profile.delete_playlist(&two));
    assert_eq!(profile.playlists.len(), 1);
    assert!(profile.playlist(&two).is_none());
    // Deleting a playlist does not delete the tracks' metadata.
    profile.remember(&track(Platform::Spotify, "kept", "Kept", "Artist"));
    assert!(profile.tracks.values().any(|t| t.title == "Kept"));
}

#[test]
fn same_song_on_two_platforms_stays_two_entries() {
    let mut profile = Profile::new("Dup2", Settings::default());
    let playlist_id = profile.create_playlist("Cross-platform");
    let spotify = track(Platform::Spotify, "same-id", "Same Song", "Artist");
    let youtube = track(Platform::YouTube, "same-id", "Same Song", "Artist");
    assert_ne!(
        spotify.id, youtube.id,
        "track ids are namespaced by platform"
    );

    profile.add_to_playlist(&playlist_id, &spotify);
    profile.add_to_playlist(&playlist_id, &youtube);
    assert_eq!(profile.playlist_tracks(&playlist_id).len(), 2);
}

#[test]
fn favourites_and_history_are_bounded_and_deduplicated() {
    let mut profile = Profile::new("History", Settings::default());
    let tracks: Vec<Track> = (0..5)
        .map(|i| {
            track(
                Platform::YouTube,
                &format!("v{i}"),
                &format!("Song {i}"),
                "Artist",
            )
        })
        .collect();

    for t in &tracks {
        assert!(profile.toggle_favorite(t));
    }
    assert_eq!(profile.favorites.len(), 5);
    assert_eq!(profile.favorite_tracks().len(), 5);
    assert!(!profile.toggle_favorite(&tracks[0]));
    assert_eq!(profile.favorite_tracks().len(), 4);

    for t in &tracks {
        profile.add_history(&t.id);
    }
    // History is most-recent-first and removes the previous occurrence.
    assert_eq!(profile.history.len(), 5);
    assert_eq!(profile.recent_tracks(2)[0].id, tracks[4].id);
    profile.add_history(&tracks[4].id);
    assert_eq!(
        profile.history.len(),
        5,
        "already-known track is moved, not duplicated"
    );

    // Very long sessions are truncated instead of growing forever.
    for i in 0..400 {
        let t = track(Platform::SoundCloud, &format!("h{i}"), "Hist", "A");
        profile.add_history(&t.id);
    }
    assert_eq!(profile.history.len(), 300);
}

#[test]
fn mixed_playlist_survives_store_roundtrip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = ProfileStore::new(AppPaths::portable(dir.path()));
    let mut profile = Profile::new("Persisted", Settings::default());
    let playlist_id = profile.create_playlist("Mixed / 混合");
    profile.add_tracks_to_playlist(&playlist_id, &all_platform_tracks());
    store.save(&profile).expect("save");

    let loaded = store.load("Persisted").expect("load");
    assert_eq!(loaded.playlists[0].id, playlist_id);
    let stored = loaded.playlist_tracks(&playlist_id);
    assert_eq!(stored.len(), 7);
    assert_eq!(stored[2].platform, Platform::AppleMusic);
    assert_eq!(stored[3].title, "海阔天空");
    assert_eq!(stored[3].duration_secs, Some(326.0));
}

#[test]
fn serialized_profile_uses_stable_public_keys() {
    let mut profile = Profile::new("Json", Settings::default());
    let list = profile.create_playlist("JSON");
    profile.add_to_playlist(&list, &track(Platform::AppleMusic, "1", "T", "A"));
    let json = serde_json::to_string(&profile).expect("serialize");

    // Data files are user-readable and versioned; these names are the contract.
    for key in [
        "schema",
        "playlists",
        "track_ids",
        "favorites",
        "history",
        "settings",
    ] {
        assert!(
            json.contains(&format!("\"{key}\"")),
            "missing {key} in profile JSON"
        );
    }
    assert!(
        json.contains("apple_music"),
        "platform key should be serialized as apple_music"
    );
    let back: Profile = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.playlists[0].track_ids.len(), 1);
}

#[test]
fn playlist_helper_does_not_collide_for_equal_names() {
    let mut profile = Profile::new("Ids", Settings::default());
    let a = profile.create_playlist("Same Name");
    let b = profile.create_playlist("Same Name");
    assert_ne!(a, b, "playlist ids must be unique even for equal names");
    let playlist = Playlist::with_name("Same Name");
    assert_eq!(playlist.name, "Same Name");
    assert!(playlist.track_ids.is_empty());
}

#[test]
fn repeat_modes_cycle_through_all_states() {
    let mut mode = RepeatMode::Off;
    let mut seen = Vec::new();
    for _ in 0..3 {
        seen.push(mode);
        mode = mode.next();
    }
    assert_eq!(mode, RepeatMode::Off, "cycle returns to the start");
    assert_eq!(
        seen,
        vec![RepeatMode::Off, RepeatMode::All, RepeatMode::One]
    );
}
