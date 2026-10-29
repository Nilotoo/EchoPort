//! Integration tests: local-first persistence (`ProfileStore` / `AppPaths`).
//!
//! Everything runs against a temporary directory via `AppPaths::portable`, so
//! the real user profile under `$XDG_DATA_HOME/echoport` is never touched.

use std::path::PathBuf;

use echoport::i18n::Language;
use echoport::model::{DownloadFormat, Platform, Playlist, Profile, Settings, Track};
use echoport::store::{
    self, dir_size, human_size, sanitize_name, AppPaths, ProfileStore, DEFAULT_PROFILE,
};
use echoport::theme::ThemeMode;

fn store_in(dir: &tempfile::TempDir) -> ProfileStore {
    ProfileStore::new(AppPaths::portable(dir.path()))
}

#[test]
fn portable_paths_keep_everything_under_one_root() {
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = AppPaths::portable(dir.path());
    assert_eq!(paths.data_dir, dir.path());
    assert_eq!(paths.cache_dir, dir.path());
    assert_eq!(paths.profiles_dir, dir.path().join("profiles"));
    assert_eq!(paths.covers_dir, dir.path().join("covers"));
    assert_eq!(paths.audio_cache_dir, dir.path().join("audio"));
    // Lua scripting config + plugins live beside the profile data in portable mode.
    assert_eq!(paths.config_dir, dir.path());
    assert_eq!(paths.config_file, dir.path().join("config.lua"));
    assert_eq!(paths.plugins_dir, dir.path().join("plugins"));

    paths.ensure().expect("ensure creates the directory tree");
    assert!(paths.profiles_dir.is_dir());
    assert!(paths.covers_dir.is_dir());
    assert!(paths.audio_cache_dir.is_dir());
    assert!(paths.config_dir.is_dir());
    assert!(paths.plugins_dir.is_dir());

    // Profile names are sanitised into plain file names, never paths.
    let file = paths.profile_file("../../etc/passwd");
    assert_eq!(file.parent(), Some(paths.profiles_dir.as_path()));
    assert_eq!(file.extension().and_then(|e| e.to_str()), Some("json"));
}

#[test]
fn resolved_paths_are_clean_absolute_locations() {
    let paths = AppPaths::resolve();
    assert!(
        paths.data_dir.is_absolute(),
        "data dir should be absolute: {:?}",
        paths.data_dir
    );
    assert!(
        paths.cache_dir.is_absolute(),
        "cache dir should be absolute: {:?}",
        paths.cache_dir
    );
    assert_eq!(
        paths.data_dir.file_name().and_then(|n| n.to_str()),
        Some("echoport")
    );
    assert_eq!(
        paths.cache_dir.file_name().and_then(|n| n.to_str()),
        Some("echoport")
    );
    assert_eq!(paths.profiles_dir, paths.data_dir.join("profiles"));
}

#[test]
fn profile_roundtrip_keeps_every_field() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = store_in(&dir);

    let mut settings = Settings::default();
    settings.language = Language::Zh;
    settings.theme_mode = ThemeMode::Ink;
    settings.follow_cover_accent = false;
    settings.download_format = DownloadFormat::Flac;
    settings.download_dir = PathBuf::from("/tmp/echoport-downloads");
    settings.music_dirs = vec![PathBuf::from("/music/a"), PathBuf::from("/music/b")];
    settings.volume = 0.33;
    settings.results_per_platform = 7;

    let mut profile = Profile::new("Round Trip", settings);
    let local = {
        let mut track = Track::new(Platform::Local, "/music/a/song.mp3", "本地歌曲", "");
        track.local_path = Some(PathBuf::from("/music/a/song.mp3"));
        track.with_album("本地专辑").with_duration(123.5)
    };
    let spotify = Track::new(
        Platform::Spotify,
        "4cOdK2wGLETKBW3PvgPWqT",
        "Spotify Track",
        "Artist",
    )
    .with_duration(213.573)
    .with_page("https://open.spotify.com/track/4cOdK2wGLETKBW3PvgPWqT");
    let netease = Track::new(Platform::Netease, "347230", "海阔天空", "Beyond")
        .with_duration(326.0)
        .with_page("https://music.163.com/song?id=347230");

    let playlist_id = profile.create_playlist("混合歌单 / Mixed");
    assert!(profile.add_to_playlist(&playlist_id, &local));
    assert!(profile.add_to_playlist(&playlist_id, &spotify));
    assert!(profile.add_to_playlist(&playlist_id, &netease));
    profile.toggle_favorite(&netease);
    profile.add_history(&spotify.id);
    profile.playlist_mut(&playlist_id).unwrap().description = "from every source".to_owned();

    store.save(&profile).expect("save succeeds");
    assert_eq!(store.list(), vec!["Round Trip".to_owned()]);

    let loaded = store.load("Round Trip").expect("load succeeds");
    assert_eq!(loaded.schema, profile.schema);
    assert_eq!(loaded.name, "Round Trip");
    assert_eq!(loaded.created_at, profile.created_at);
    assert_eq!(loaded.settings.language, Language::Zh);
    assert_eq!(loaded.settings.theme_mode, ThemeMode::Ink);
    assert!(
        !loaded.settings.follow_cover_accent,
        "follow_cover_accent roundtrips"
    );
    assert_eq!(loaded.settings.download_format, DownloadFormat::Flac);
    assert_eq!(
        loaded.settings.download_dir,
        PathBuf::from("/tmp/echoport-downloads")
    );
    assert_eq!(loaded.settings.music_dirs.len(), 2);
    assert_eq!(loaded.settings.volume, 0.33);
    assert_eq!(loaded.settings.results_per_platform, 7);

    assert_eq!(loaded.playlists.len(), 1);
    assert_eq!(loaded.playlists[0].name, "混合歌单 / Mixed");
    assert_eq!(loaded.playlists[0].description, "from every source");
    assert_eq!(loaded.playlist_tracks(&playlist_id).len(), 3);
    assert_eq!(loaded.playlist_tracks(&playlist_id)[2].title, "海阔天空");
    assert!(loaded.is_favorite(&netease.id));
    assert_eq!(loaded.history.len(), 1);
    assert_eq!(
        loaded.recent_tracks(10).first().map(|t| t.id.as_str()),
        Some(spotify.id.as_str())
    );
}

#[test]
fn profile_schema_is_hand_editable_and_forward_compatible() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = store_in(&dir);
    store.paths.ensure().expect("dirs");

    // An old/minimal profile: only the required fields. All the newer fields
    // must default instead of failing the whole load.
    let path = store.paths.profile_file("Legacy");
    std::fs::write(&path, r#"{"schema":1,"name":"Legacy","created_at":1}"#).expect("write");
    let loaded = store.load("Legacy").expect("legacy profile loads");
    assert_eq!(loaded.name, "Legacy");
    assert_eq!(loaded.created_at, 1);
    assert!(loaded.playlists.is_empty());
    assert!(loaded.favorites.is_empty());
    assert_eq!(
        loaded.settings.results_per_platform,
        Settings::default().results_per_platform
    );

    // Unknown keys are ignored; a newer version may have written them.
    std::fs::write(
        &path,
        r#"{"schema":1,"name":"Legacy","created_at":1,"future_field":{"x":1}}"#,
    )
    .expect("write");
    assert!(store.load("Legacy").is_some());
}

#[test]
fn corrupt_profiles_are_reported_as_missing_not_panics() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = store_in(&dir);
    store.paths.ensure().expect("dirs");
    std::fs::write(store.paths.profile_file("Broken"), b"{ not json").expect("write");

    assert!(store.load("Broken").is_none());
    // load_or_create recovers by recreating the named profile with defaults
    // instead of panicking on the unreadable file.
    let profile = store.load_or_create(Some("Broken"));
    assert_eq!(profile.name, "Broken");
    assert!(
        store.load("Broken").is_some(),
        "corrupt profile is replaced by a healthy default"
    );
}

#[test]
fn save_writes_valid_json_and_leaves_no_temp_files() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = store_in(&dir);
    store.paths.ensure().expect("dirs");
    let profile = Profile::new("Atomic", Settings::default());

    store.save(&profile).expect("first save");
    store.save(&profile).expect("second save overwrites");

    let path = store.paths.profile_file("Atomic");
    let text = std::fs::read_to_string(&path).expect("profile file readable");
    let parsed: serde_json::Value = serde_json::from_str(&text).expect("valid json");
    assert_eq!(parsed["name"], "Atomic");
    assert_eq!(parsed["schema"], 1);

    let leftovers: Vec<_> = std::fs::read_dir(&store.paths.profiles_dir)
        .expect("read profiles dir")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".tmp"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "atomic write left temp files: {leftovers:?}"
    );
}

#[test]
fn load_or_create_prefers_the_named_profile_then_falls_back() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = store_in(&dir);

    let first = store.load_or_create(None);
    assert_eq!(first.name, DEFAULT_PROFILE);
    assert!(
        store.paths.profile_file(DEFAULT_PROFILE).is_file(),
        "default profile persisted"
    );

    let mut second = Profile::new("Work", Settings::default());
    second.toggle_favorite(&Track::new(Platform::YouTube, "abc", "t", "a"));
    store.save(&second).expect("save second");

    // Known profile wins.
    assert_eq!(store.load_or_create(Some("Work")).name, "Work");
    // An explicit, unknown preference is recreated under the requested name
    // (explicit user intent wins over silently switching profiles).
    let recovered = store.load_or_create(Some("Does Not Exist"));
    assert_eq!(recovered.name, "Does Not Exist");
    assert!(
        store.load("Does Not Exist").is_some(),
        "explicit profile is persisted"
    );
    // Startup (no preference) prefers the default profile and never crashes.
    assert_eq!(store.load_or_create(None).name, DEFAULT_PROFILE);
    // Without a healthy default profile, startup adopts any loadable profile.
    let dir2 = tempfile::tempdir().expect("tempdir");
    let store2 = store_in(&dir2);
    store2
        .save(&Profile::new("Only", Settings::default()))
        .expect("save");
    assert_eq!(store2.load_or_create(None).name, "Only");
}

#[test]
fn chinese_profile_names_are_valid_file_stems() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = store_in(&dir);
    let profile = Profile::new("小明", Settings::default());
    store.save(&profile).expect("save chinese profile name");

    let file = store.paths.profile_file("小明");
    assert!(file.is_file(), "profile file should exist at {file:?}");
    assert_eq!(file.file_stem().and_then(|s| s.to_str()), Some("小明"));
    assert_eq!(store.list(), vec!["小明".to_owned()]);
    assert_eq!(store.load("小明").expect("load").name, "小明");
}

#[test]
fn sanitize_name_blocks_traversal_and_empty_names() {
    assert_eq!(sanitize_name("../../etc/passwd"), "etcpasswd");
    assert_eq!(sanitize_name("a/b\\c"), "abc");
    assert_eq!(sanitize_name("   "), DEFAULT_PROFILE);
    assert_eq!(sanitize_name(""), DEFAULT_PROFILE);
    assert_eq!(sanitize_name("  Working Set  "), "Working Set");
    // CJK letters are alphanumeric and stay usable.
    assert_eq!(sanitize_name("深夜写作"), "深夜写作");
}

#[test]
fn delete_removes_only_the_named_profile() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = store_in(&dir);
    store
        .save(&Profile::new("Keep", Settings::default()))
        .expect("save");
    store
        .save(&Profile::new("Drop", Settings::default()))
        .expect("save");

    assert!(store.delete("Drop"));
    assert!(!store.paths.profile_file("Drop").exists());
    assert!(store.paths.profile_file("Keep").exists());
    assert!(!store.delete("Drop"), "deleting twice returns false");
}

#[test]
fn human_size_and_dir_size_reports_are_sane() {
    assert_eq!(human_size(0), "0 B");
    assert_eq!(human_size(512), "512 B");
    assert_eq!(human_size(1024), "1.0 KiB");
    assert_eq!(human_size(1536), "1.5 KiB");
    assert_eq!(human_size(1024 * 1024 * 3 / 2), "1.5 MiB");
    assert_eq!(human_size(1024 * 1024 * 1024), "1.0 GiB");

    let dir = tempfile::tempdir().expect("tempdir");
    let nested = dir.path().join("a/b");
    std::fs::create_dir_all(&nested).expect("nested dirs");
    std::fs::write(dir.path().join("one.bin"), vec![0u8; 100]).expect("write");
    std::fs::write(nested.join("two.bin"), vec![0u8; 50]).expect("write");
    assert_eq!(dir_size(dir.path()), 150);
    assert_eq!(dir_size(&dir.path().join("missing")), 0);
}

#[test]
fn default_settings_are_playable_and_bilingual() {
    let settings = Settings::default();
    assert!(matches!(settings.language, Language::En | Language::Zh));
    assert!(settings.volume > 0.0 && settings.volume <= 1.0);
    assert!(settings.results_per_platform >= 4 && settings.results_per_platform <= 40);
    assert!(!settings.music_dirs.is_empty());
    assert!(settings.download_format.key().len() >= 3);
    assert_eq!(settings.music_dir(), settings.music_dirs[0]);

    // The store's default profile is never anonymous/blank.
    let profile = Profile::new(DEFAULT_PROFILE, settings);
    assert_eq!(profile.schema, echoport::model::PROFILE_SCHEMA);
    // Playlist helper types stay constructible through the public API.
    let playlist = Playlist::with_name("Sanity");
    assert_eq!(playlist.len(), 0);
    assert!(playlist.is_empty());
    let _ = store::DEFAULT_PROFILE;
}

#[test]
fn legacy_theme_and_scripting_settings_json_still_load() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = store_in(&dir);
    store.paths.ensure().expect("dirs");

    // A pre-redesign profile: theme_mode "midnight", no follow_cover_accent.
    let path = store.paths.profile_file("OldTheme");
    std::fs::write(
        &path,
        r#"{"schema":1,"name":"OldTheme","created_at":1,"settings":{"language":"zh","theme_mode":"midnight","download_format":"mp3","download_dir":"/tmp/x","music_dirs":[],"volume":0.5,"results_per_platform":12}}"#,
    )
    .expect("write legacy profile");
    let loaded = store.load("OldTheme").expect("legacy profile loads");
    assert_eq!(
        loaded.settings.theme_mode,
        ThemeMode::Ink,
        "midnight is an Ink alias"
    );
    assert!(
        loaded.settings.follow_cover_accent,
        "missing follow_cover_accent defaults to true"
    );

    // The current theme keys round-trip through JSON unchanged.
    for (key, expected) in [
        ("pulse", ThemeMode::Pulse),
        ("tide", ThemeMode::Tide),
        ("ink", ThemeMode::Ink),
    ] {
        let json = format!(
            r#"{{"language":"en","theme_mode":"{key}","follow_cover_accent":false,"download_format":"mp3","download_dir":"/tmp/x","music_dirs":[],"volume":0.5,"results_per_platform":12}}"#
        );
        let settings: Settings = serde_json::from_str(&json).expect("settings deserialize");
        assert_eq!(settings.theme_mode, expected);
        assert!(!settings.follow_cover_accent);
    }
}
