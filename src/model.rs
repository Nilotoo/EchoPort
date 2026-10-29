//! Core domain types shared by every layer.
//!
//! This module deliberately contains no UI, network or process code so the
//! persistence layer and the tests can use it in isolation.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::i18n::Language;
use crate::theme::ThemeMode;

fn default_true() -> bool {
    true
}

fn default_ui_scale() -> f32 {
    1.0
}

fn default_cache_limit_mb() -> u32 {
    1024
}

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

static UNIQUE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Monotonic token used to keep ids unique even when two objects are created
/// within the same second (or even nanosecond).
fn unique_token() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let counter = UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{nanos}-{counter}-{}", std::process::id())
}

/// Stable, platform independent 64-bit FNV-1a hash rendered as hex.
///
/// Used to derive ids and cache file names. `std`'s hashers are explicitly
/// allowed to change between releases, so we implement the hash ourselves.
pub fn stable_hash(input: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

// ---------------------------------------------------------------------------
// Platform
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Local,
    Spotify,
    AppleMusic,
    Netease,
    SoundCloud,
    Bilibili,
    YouTube,
}

impl Platform {
    pub const ALL: [Self; 7] = [
        Self::Local,
        Self::Spotify,
        Self::AppleMusic,
        Self::Netease,
        Self::SoundCloud,
        Self::Bilibili,
        Self::YouTube,
    ];

    /// Platforms that can be searched over the network.
    pub const REMOTE: [Self; 6] = [
        Self::Spotify,
        Self::AppleMusic,
        Self::Netease,
        Self::SoundCloud,
        Self::Bilibili,
        Self::YouTube,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Spotify => "spotify",
            Self::AppleMusic => "apple_music",
            Self::Netease => "netease",
            Self::SoundCloud => "soundcloud",
            Self::Bilibili => "bilibili",
            Self::YouTube => "youtube",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.key() == key)
    }

    /// Brand label. Platform names are never translated.
    pub fn label(self) -> &'static str {
        match self {
            Self::Local => "Local",
            Self::Spotify => "Spotify",
            Self::AppleMusic => "Apple Music",
            Self::Netease => "Netease",
            Self::SoundCloud => "SoundCloud",
            Self::Bilibili => "Bilibili",
            Self::YouTube => "YouTube",
        }
    }

    pub fn is_streaming(self) -> bool {
        self != Self::Local
    }
}

// ---------------------------------------------------------------------------
// Track
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub platform: Platform,
    pub title: String,
    pub artist: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    /// Duration in seconds when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<f64>,
    /// Remote cover art URL (never a local path; see `art` module for the cache).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub art_url: Option<String>,
    /// Human friendly page on the source platform.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_url: Option<String>,
    /// Direct, publicly reachable audio URL when the provider knows one
    /// (e.g. Apple Music preview clips). Normally playback goes through yt-dlp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_url: Option<String>,
    /// Set for local files only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub year: Option<u16>,
    /// Release date as reported by the platform (`YYYY-MM-DD` when known).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_date: Option<String>,
}

impl Track {
    /// Build a track with an id derived from `id_source` (usually a platform id
    /// or URL). Never contains user specific data.
    pub fn new(
        platform: Platform,
        id_source: &str,
        title: impl Into<String>,
        artist: impl Into<String>,
    ) -> Self {
        Self {
            id: format!("{}:{}", platform.key(), stable_hash(id_source)),
            platform,
            title: title.into(),
            artist: artist.into(),
            album: None,
            duration_secs: None,
            art_url: None,
            page_url: None,
            stream_url: None,
            local_path: None,
            year: None,
            release_date: None,
        }
    }

    pub fn from_local_path(path: PathBuf) -> Self {
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        let id_source = path.to_string_lossy();
        let mut track = Self::new(Platform::Local, &id_source, stem, "");
        track.local_path = Some(path);
        track
    }

    pub fn with_album(mut self, album: impl Into<String>) -> Self {
        self.album = Some(album.into());
        self
    }

    pub fn with_duration(mut self, secs: f64) -> Self {
        if secs.is_finite() && secs > 0.0 {
            self.duration_secs = Some(secs);
        }
        self
    }

    pub fn with_art(mut self, url: impl Into<String>) -> Self {
        let url = url.into();
        if !url.is_empty() {
            self.art_url = Some(url);
        }
        self
    }

    pub fn with_page(mut self, url: impl Into<String>) -> Self {
        let url = url.into();
        if !url.is_empty() {
            self.page_url = Some(url);
        }
        self
    }

    pub fn with_stream(mut self, url: impl Into<String>) -> Self {
        let url = url.into();
        if !url.is_empty() {
            self.stream_url = Some(url);
        }
        self
    }

    pub fn with_year(mut self, year: u16) -> Self {
        self.year = Some(year);
        self
    }

    pub fn with_release_date(mut self, date: impl Into<String>) -> Self {
        let date = date.into();
        if !date.trim().is_empty() {
            self.release_date = Some(date);
        }
        self
    }

    /// Release label for the UI: full date when known, otherwise year.
    pub fn release_label(&self) -> Option<String> {
        self.release_date
            .clone()
            .or_else(|| self.year.map(|year| year.to_string()))
    }

    pub fn is_local(&self) -> bool {
        self.local_path.is_some() || self.platform == Platform::Local
    }

    pub fn display_artist(&self) -> &str {
        if self.artist.trim().is_empty() {
            "—"
        } else {
            &self.artist
        }
    }

    /// Text used when matching this track on streaming services (yt-dlp).
    pub fn search_text(&self) -> String {
        if self.artist.trim().is_empty() {
            self.title.clone()
        } else {
            format!("{} {}", self.artist, self.title)
        }
    }

    /// Stable key for cached media files.
    pub fn cache_key(&self) -> String {
        if let Some(path) = &self.local_path {
            return format!("local-{}", stable_hash(&path.to_string_lossy()));
        }
        stable_hash(&self.id)
    }
}

// ---------------------------------------------------------------------------
// Playlists / profile data
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub track_ids: Vec<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

impl Playlist {
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        let now = unix_now();
        Self {
            id: format!(
                "pl-{}",
                stable_hash(&format!("{}:{now}:{}", name.trim(), unique_token()))
            ),
            name,
            description: String::new(),
            track_ids: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_name(name: impl Into<String>) -> Self {
        Self::new(name)
    }

    pub fn touch(&mut self) {
        self.updated_at = unix_now();
    }

    pub fn len(&self) -> usize {
        self.track_ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.track_ids.is_empty()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub track_id: String,
    pub played_at: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadFormat {
    Mp3,
    M4a,
    Opus,
    Flac,
}

impl DownloadFormat {
    pub const ALL: [Self; 4] = [Self::Mp3, Self::M4a, Self::Opus, Self::Flac];

    pub fn key(self) -> &'static str {
        match self {
            Self::Mp3 => "mp3",
            Self::M4a => "m4a",
            Self::Opus => "opus",
            Self::Flac => "flac",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Mp3 => "MP3",
            Self::M4a => "M4A (AAC)",
            Self::Opus => "Opus",
            Self::Flac => "FLAC",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.key() == key)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub language: Language,
    pub theme_mode: ThemeMode,
    /// When true the accent / highlight / macaron colours follow the cover art
    /// of the current track (background and text stay theme-fixed).
    #[serde(default = "default_true")]
    pub follow_cover_accent: bool,
    pub download_format: DownloadFormat,
    pub download_dir: PathBuf,
    pub music_dirs: Vec<PathBuf>,
    /// Explicit yt-dlp binary; `None` means auto-detect.
    #[serde(default)]
    pub ytdlp_path: Option<PathBuf>,
    pub volume: f32,
    /// UI zoom factor (1.0 = default). Users on HiDPI / large monitors can
    /// scale the whole interface from Settings.
    #[serde(default = "default_ui_scale")]
    pub ui_scale: f32,
    /// Streamed-audio cache cap in MiB. `0` = do not keep played tracks on disk
    /// (the current track is deleted as soon as the next one starts).
    #[serde(default = "default_cache_limit_mb")]
    pub cache_limit_mb: u32,
    /// How many search results to request per platform.
    pub results_per_platform: usize,
}

impl Default for Settings {
    fn default() -> Self {
        let music_dir = dirs::audio_dir().unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("Music")
        });
        Self {
            language: Language::detect(),
            theme_mode: ThemeMode::default(),
            follow_cover_accent: true,
            download_format: DownloadFormat::Mp3,
            download_dir: music_dir.join("EchoPort"),
            music_dirs: vec![music_dir],
            ytdlp_path: None,
            volume: 0.72,
            ui_scale: 1.0,
            cache_limit_mb: 1024,
            results_per_platform: 12,
        }
    }
}

impl Settings {
    pub fn music_dir(&self) -> PathBuf {
        self.music_dirs
            .first()
            .cloned()
            .unwrap_or_else(|| PathBuf::from("."))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    pub schema: u32,
    pub name: String,
    #[serde(default)]
    pub bio: String,
    /// Data URL (`data:image/png;base64,…`) of the profile avatar, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    pub created_at: u64,
    #[serde(default)]
    pub settings: Settings,
    /// Metadata for every track referenced by playlists / favourites / history.
    #[serde(default)]
    pub tracks: BTreeMap<String, Track>,
    #[serde(default)]
    pub playlists: Vec<Playlist>,
    #[serde(default)]
    pub favorites: Vec<String>,
    #[serde(default)]
    pub history: Vec<HistoryEntry>,
}

pub const PROFILE_SCHEMA: u32 = 1;

impl Profile {
    pub fn new(name: impl Into<String>, settings: Settings) -> Self {
        Self {
            schema: PROFILE_SCHEMA,
            name: name.into(),
            bio: String::new(),
            avatar: None,
            created_at: unix_now(),
            settings,
            tracks: BTreeMap::new(),
            playlists: Vec::new(),
            favorites: Vec::new(),
            history: Vec::new(),
        }
    }

    pub fn track(&self, id: &str) -> Option<&Track> {
        self.tracks.get(id)
    }

    pub fn remember(&mut self, track: &Track) {
        self.tracks.insert(track.id.clone(), track.clone());
        // Keep the metadata map bounded; real libraries are covered by the
        // local library scan which re-registers tracks.
        if self.tracks.len() > 5_000 {
            let keep: Vec<String> = self
                .favorites
                .iter()
                .chain(self.history.iter().map(|h| &h.track_id))
                .chain(self.playlists.iter().flat_map(|p| p.track_ids.iter()))
                .cloned()
                .collect();
            self.tracks.retain(|id, _| keep.iter().any(|k| k == id));
        }
    }

    pub fn remember_many<'a>(&mut self, tracks: impl IntoIterator<Item = &'a Track>) {
        for track in tracks {
            self.remember(track);
        }
    }

    pub fn add_history(&mut self, track_id: &str) {
        self.history.retain(|h| h.track_id != track_id);
        self.history.insert(
            0,
            HistoryEntry {
                track_id: track_id.to_owned(),
                played_at: unix_now(),
            },
        );
        self.history.truncate(300);
    }

    pub fn recent_tracks(&self, limit: usize) -> Vec<Track> {
        self.history
            .iter()
            .filter_map(|h| self.track(&h.track_id).cloned())
            .take(limit)
            .collect()
    }

    pub fn favorite_tracks(&self) -> Vec<Track> {
        self.favorites
            .iter()
            .filter_map(|id| self.track(id).cloned())
            .collect()
    }

    pub fn is_favorite(&self, id: &str) -> bool {
        self.favorites.iter().any(|f| f == id)
    }

    /// Returns `true` when the track is a favourite after the toggle.
    pub fn toggle_favorite(&mut self, track: &Track) -> bool {
        self.remember(track);
        if let Some(pos) = self.favorites.iter().position(|id| id == &track.id) {
            self.favorites.remove(pos);
            false
        } else {
            self.favorites.insert(0, track.id.clone());
            true
        }
    }

    pub fn playlist(&self, id: &str) -> Option<&Playlist> {
        self.playlists.iter().find(|p| p.id == id)
    }

    pub fn playlist_mut(&mut self, id: &str) -> Option<&mut Playlist> {
        self.playlists.iter_mut().find(|p| p.id == id)
    }

    pub fn create_playlist(&mut self, name: impl Into<String>) -> String {
        let mut playlist = Playlist::with_name(name);
        // Defensive: the monotonic token above already makes collisions
        // practically impossible, but a hand-edited profile could contain a
        // duplicate id. Re-roll until it is unique inside this profile.
        while self
            .playlists
            .iter()
            .any(|existing| existing.id == playlist.id)
        {
            playlist = Playlist::with_name(playlist.name.clone());
        }
        let id = playlist.id.clone();
        self.playlists.insert(0, playlist);
        id
    }

    pub fn delete_playlist(&mut self, id: &str) -> bool {
        let before = self.playlists.len();
        self.playlists.retain(|p| p.id != id);
        self.playlists.len() != before
    }

    pub fn add_to_playlist(&mut self, playlist_id: &str, track: &Track) -> bool {
        self.remember(track);
        let Some(playlist) = self.playlist_mut(playlist_id) else {
            return false;
        };
        if playlist.track_ids.iter().any(|id| id == &track.id) {
            return false;
        }
        playlist.track_ids.push(track.id.clone());
        playlist.touch();
        true
    }

    pub fn add_tracks_to_playlist(&mut self, playlist_id: &str, tracks: &[Track]) -> usize {
        let mut added = 0;
        for track in tracks {
            if self.add_to_playlist(playlist_id, track) {
                added += 1;
            }
        }
        added
    }

    pub fn remove_from_playlist(&mut self, playlist_id: &str, track_id: &str) -> bool {
        let Some(playlist) = self.playlist_mut(playlist_id) else {
            return false;
        };
        let before = playlist.track_ids.len();
        playlist.track_ids.retain(|id| id != track_id);
        playlist.touch();
        playlist.track_ids.len() != before
    }

    pub fn playlist_tracks(&self, playlist_id: &str) -> Vec<Track> {
        self.playlist(playlist_id)
            .map(|playlist| {
                playlist
                    .track_ids
                    .iter()
                    .filter_map(|id| self.track(id).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// Playback
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlaybackState {
    #[default]
    Idle,
    Loading,
    Playing,
    Paused,
    Error,
}

impl PlaybackState {
    pub fn is_active(self) -> bool {
        matches!(self, Self::Loading | Self::Playing | Self::Paused)
    }

    pub fn is_playing(self) -> bool {
        self == Self::Playing
    }
}

#[derive(Clone, Debug)]
pub struct PlaybackStatus {
    pub state: PlaybackState,
    pub position_secs: f64,
    pub duration_secs: Option<f64>,
    pub volume: f32,
    pub error: Option<String>,
    /// Set once when the decoder reaches the end of the current file.
    pub ended: bool,
}

impl Default for PlaybackStatus {
    fn default() -> Self {
        Self {
            state: PlaybackState::Idle,
            position_secs: 0.0,
            duration_secs: None,
            volume: 0.72,
            error: None,
            ended: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}

impl RepeatMode {
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::All,
            Self::All => Self::One,
            Self::One => Self::Off,
        }
    }
}

// ---------------------------------------------------------------------------
// Search / background / downloads
// ---------------------------------------------------------------------------

/// A creator / producer / artist on a platform (search result entity).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artist {
    pub id: String,
    pub platform: Platform,
    pub name: String,
    /// Platform-native id or URL used to fetch the artist page.
    #[serde(default)]
    pub source_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_url: Option<String>,
}

impl Artist {
    pub fn new(platform: Platform, id_source: &str, name: impl Into<String>) -> Self {
        Self {
            id: format!("{}:{}", platform.key(), stable_hash(id_source)),
            platform,
            name: name.into(),
            source_id: id_source.to_owned(),
            image_url: None,
            page_url: None,
        }
    }

    pub fn with_image(mut self, url: impl Into<String>) -> Self {
        let url = url.into();
        if !url.is_empty() {
            self.image_url = Some(url);
        }
        self
    }

    pub fn with_page(mut self, url: impl Into<String>) -> Self {
        let url = url.into();
        if !url.is_empty() {
            self.page_url = Some(url);
        }
        self
    }

    pub fn key(&self) -> String {
        self.id.clone()
    }
}

/// Artist/producer page content: biography from Wikipedia plus platform tracks.
#[derive(Clone, Debug, Default)]
pub struct ArtistDetail {
    pub artist: Option<Artist>,
    pub bio: Option<ArtistBio>,
    pub hot_tracks: Vec<Track>,
}

#[derive(Clone, Debug)]
pub struct PlatformResults {
    pub platform: Platform,
    pub tracks: Vec<Track>,
    pub error: Option<String>,
}

/// Producers / artists found on one platform, best match first.
#[derive(Clone, Debug)]
pub struct PlatformArtistResults {
    pub platform: Platform,
    pub artists: Vec<Artist>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct SearchResults {
    pub query: String,
    pub results: Vec<PlatformResults>,
    /// Producer/artist cards, ranked by relevance.
    pub artists: Vec<PlatformArtistResults>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkKind {
    Wikipedia,
    Genius,
    YouTube,
    WebSearch,
    Source,
    Other,
}

#[derive(Clone, Debug)]
pub struct BackgroundLink {
    pub kind: LinkKind,
    pub title: String,
    pub url: String,
    pub subtitle: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ArtistBio {
    pub title: String,
    pub extract: String,
    pub url: String,
    pub thumbnail: Option<String>,
}

/// Contextual information shown next to the now playing view: who the artist
/// is, the album story, the specific song story, interviews / explanations and
/// further reading links.
#[derive(Clone, Debug, Default)]
pub struct SongBackground {
    pub bio: Option<ArtistBio>,
    /// Background of the album this track belongs to (Wikipedia / MusicBrainz).
    pub album: Option<ArtistBio>,
    /// Background of this specific song / single.
    pub song: Option<ArtistBio>,
    pub links: Vec<BackgroundLink>,
    pub videos: Vec<BackgroundLink>,
}

/// Portable playlist file (offline sharing): metadata only, never audio.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlaylistShare {
    pub format: String,
    pub version: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub creator: String,
    #[serde(default)]
    pub exported_at: u64,
    #[serde(default)]
    pub tracks: Vec<Track>,
}

/// Portable profile file: complete local profile metadata plus avatar/bio.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProfileShare {
    pub format: String,
    pub version: u32,
    pub name: String,
    #[serde(default)]
    pub bio: String,
    #[serde(default)]
    pub avatar: Option<String>,
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub tracks: BTreeMap<String, Track>,
    #[serde(default)]
    pub playlists: Vec<Playlist>,
    #[serde(default)]
    pub favorites: Vec<String>,
    #[serde(default)]
    pub history: Vec<HistoryEntry>,
}

pub const PROFILE_SHARE_FORMAT: &str = "echoport-profile";
pub const PROFILE_SHARE_VERSION: u32 = 1;

impl ProfileShare {
    pub fn from_profile(profile: &Profile) -> Self {
        Self {
            format: PROFILE_SHARE_FORMAT.to_owned(),
            version: PROFILE_SHARE_VERSION,
            name: profile.name.clone(),
            bio: profile.bio.clone(),
            avatar: profile.avatar.clone(),
            settings: profile.settings.clone(),
            tracks: profile.tracks.clone(),
            playlists: profile.playlists.clone(),
            favorites: profile.favorites.clone(),
            history: profile.history.clone(),
        }
    }

    pub fn into_profile(self) -> Profile {
        let mut profile = Profile::new(self.name, self.settings);
        profile.bio = self.bio;
        profile.avatar = self.avatar;
        profile.tracks = self.tracks;
        profile.playlists = self.playlists;
        profile.favorites = self.favorites;
        profile.history = self.history;
        profile
    }
}

pub const PLAYLIST_SHARE_FORMAT: &str = "echoport-playlist";
pub const PLAYLIST_SHARE_VERSION: u32 = 1;

impl PlaylistShare {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        tracks: Vec<Track>,
    ) -> Self {
        Self {
            format: PLAYLIST_SHARE_FORMAT.to_owned(),
            version: PLAYLIST_SHARE_VERSION,
            name: name.into(),
            description: description.into(),
            creator: "nilotoo".to_owned(),
            exported_at: unix_now(),
            tracks,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobState {
    Queued,
    Running,
    Done,
    Failed,
}

#[derive(Clone, Debug)]
pub struct DownloadJob {
    pub track: Track,
    pub state: JobState,
    pub progress: f32,
    pub stage: String,
    pub path: Option<PathBuf>,
    pub error: Option<String>,
}

impl DownloadJob {
    pub fn queued(track: Track) -> Self {
        Self {
            track,
            state: JobState::Queued,
            progress: 0.0,
            stage: String::new(),
            path: None,
            error: None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct DownloadProgress {
    pub fraction: Option<f32>,
    pub stage: String,
}

// ---------------------------------------------------------------------------
// Tiny helpers used across layers
// ---------------------------------------------------------------------------

/// Format seconds as `m:ss` or `h:mm:ss`.
pub fn format_duration(secs: f64) -> String {
    if !secs.is_finite() || secs <= 0.0 {
        return "--:--".to_owned();
    }
    let total = secs.round() as u64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Shorten long strings for compact rows.
pub fn ellipsize(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_are_stable() {
        assert_eq!(stable_hash("hello"), stable_hash("hello"));
        assert_ne!(stable_hash("hello"), stable_hash("world"));
        assert_eq!(stable_hash("hello").len(), 16);
    }

    #[test]
    fn playlist_add_dedupes() {
        let mut profile = Profile::new("test", Settings::default());
        let track = Track::new(Platform::Spotify, "abc", "Song", "Artist");
        let playlist_id = profile.create_playlist("Mix");
        assert!(profile.add_to_playlist(&playlist_id, &track));
        assert!(!profile.add_to_playlist(&playlist_id, &track));
        assert_eq!(profile.playlist(&playlist_id).unwrap().len(), 1);
        assert!(profile.remove_from_playlist(&playlist_id, &track.id));
        assert!(profile.playlist(&playlist_id).unwrap().is_empty());
    }

    #[test]
    fn favourite_toggles() {
        let mut profile = Profile::new("test", Settings::default());
        let track = Track::new(Platform::Netease, "1", "Song", "Artist");
        assert!(profile.toggle_favorite(&track));
        assert!(profile.is_favorite(&track.id));
        assert!(!profile.toggle_favorite(&track));
        assert!(!profile.is_favorite(&track.id));
    }

    #[test]
    fn profile_share_roundtrip_keeps_avatar_bio_and_playlists() {
        let mut profile = Profile::new("nilotoo", Settings::default());
        profile.bio = "late-night listener".to_owned();
        profile.avatar = Some("data:image/png;base64,AAAA".to_owned());
        let playlist_id = profile.create_playlist("Night drive");
        let track = Track::new(
            Platform::AppleMusic,
            "app-1",
            "Look at the Sky",
            "Porter Robinson",
        )
        .with_album("Nurture")
        .with_release_date("2021-02-10");
        profile.add_to_playlist(&playlist_id, &track);
        let share = ProfileShare::from_profile(&profile);
        let json = serde_json::to_string(&share).expect("serialize");
        let parsed: ProfileShare = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.format, PROFILE_SHARE_FORMAT);
        let restored = parsed.into_profile();
        assert_eq!(restored.name, "nilotoo");
        assert_eq!(restored.bio, "late-night listener");
        assert_eq!(
            restored.avatar.as_deref(),
            Some("data:image/png;base64,AAAA")
        );
        assert_eq!(restored.playlists.len(), 1);
        assert_eq!(
            restored.playlist_tracks(&playlist_id)[0]
                .release_label()
                .as_deref(),
            Some("2021-02-10")
        );
    }

    #[test]
    fn track_release_label_prefers_date_then_year() {
        let track = Track::new(Platform::YouTube, "v1", "Song", "Artist").with_year(2019);
        assert_eq!(track.release_label().as_deref(), Some("2019"));
        let track = track.with_release_date("2021-04-23");
        assert_eq!(track.release_label().as_deref(), Some("2021-04-23"));
    }

    #[test]
    fn playlist_share_roundtrip_is_metadata_only() {
        let mut profile = Profile::new("tester", Settings::default());
        let playlist_id = profile.create_playlist("Shared mix");
        let track = Track::new(Platform::YouTube, "abc", "Song", "Artist")
            .with_album("Album")
            .with_duration(200.0)
            .with_page("https://www.youtube.com/watch?v=abc");
        profile.add_to_playlist(&playlist_id, &track);
        let share = PlaylistShare::new(
            "Shared mix",
            "metadata only",
            profile.playlist_tracks(&playlist_id),
        );
        let json = serde_json::to_string(&share).expect("serialize");
        assert!(!json.contains("local_path"));
        let parsed: PlaylistShare = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.format, PLAYLIST_SHARE_FORMAT);
        assert_eq!(parsed.tracks.len(), 1);
        assert_eq!(parsed.tracks[0].title, "Song");
    }

    #[test]
    fn settings_defaults_are_forward_compatible() {
        let mut value = serde_json::to_value(Settings::default()).expect("serialize");
        let object = value.as_object_mut().expect("object");
        object.remove("cache_limit_mb");
        object.remove("ui_scale");
        let settings: Settings =
            serde_json::from_value(value).expect("legacy settings without new fields");
        assert_eq!(settings.cache_limit_mb, 1024);
        assert!((settings.ui_scale - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn duration_formats() {
        assert_eq!(format_duration(0.0), "--:--");
        assert_eq!(format_duration(65.0), "1:05");
        assert_eq!(format_duration(3725.0), "1:02:05");
    }
}

// Re-exported here so callers that only import `model` can still build settings.
pub type TrackId = String;
