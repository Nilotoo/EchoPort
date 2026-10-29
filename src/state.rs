//! UI facing application state plus the command / event vocabulary shared by
//! the interface, the player worker and the network workers.

use std::collections::HashSet;
use std::path::PathBuf;

use crate::i18n::{t, Key, Language};
use crate::model::{
    Artist, ArtistDetail, DownloadFormat, DownloadJob, JobState, Platform, PlaybackStatus,
    Playlist, Profile, RepeatMode, SearchResults, SongBackground, Track,
};
use crate::scripting::{BindingAction, KeyBinding, ScriptActionInfo};
use crate::theme::{Palette, Theme, ThemeMode};
use crate::update::UpdateInfo;

// ---------------------------------------------------------------------------
// Navigation / view state
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Home,
    Search,
    Library,
    Playlists,
    Downloads,
    NowPlaying,
    Artist,
    Album,
    Profile,
    Settings,
}

impl Page {
    pub const PRIMARY: [Self; 10] = [
        Self::Home,
        Self::Search,
        Self::Library,
        Self::Playlists,
        Self::Downloads,
        Self::NowPlaying,
        Self::Artist,
        Self::Album,
        Self::Profile,
        Self::Settings,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Search => "search",
            Self::Library => "library",
            Self::Playlists => "playlists",
            Self::Downloads => "downloads",
            Self::NowPlaying => "now_playing",
            Self::Artist => "artist",
            Self::Album => "album",
            Self::Profile => "profile",
            Self::Settings => "settings",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NowPlayingTab {
    Track,
    Background,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Error,
}

#[derive(Clone, Debug)]
pub struct Toast {
    pub text: String,
    pub kind: ToastKind,
    /// Seconds on the app clock; the UI fades the toast out after a while.
    pub created_at: f64,
}

/// Snapshot of the Lua configuration / plugin state shown in Settings.
#[derive(Clone, Debug, Default)]
pub struct ScriptStatus {
    pub loaded: bool,
    pub config_file: PathBuf,
    pub plugins_dir: PathBuf,
    pub plugins: Vec<String>,
    pub actions: Vec<ScriptActionInfo>,
    pub warnings: Vec<String>,
    pub bindings: Vec<KeyBinding>,
}

#[derive(Clone, Debug, Default)]
pub struct ToolingStatus {
    pub ytdlp_path: Option<PathBuf>,
    pub ytdlp_version: Option<String>,
    pub ffmpeg_path: Option<PathBuf>,
    pub checking: bool,
}

impl ToolingStatus {
    pub fn ytdlp_ready(&self) -> bool {
        self.ytdlp_path.is_some()
    }

    pub fn ffmpeg_ready(&self) -> bool {
        self.ffmpeg_path.is_some()
    }
}

/// Search result category shown by the search page.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SearchTab {
    #[default]
    All,
    Artists,
    Albums,
}

#[derive(Clone, Debug)]
pub struct SearchView {
    /// Text currently in the search box.
    pub input: String,
    /// Query that produced `results`.
    pub query: String,
    pub platforms: HashSet<Platform>,
    pub loading: bool,
    /// Platform currently loading "show more" results.
    pub loading_more: Option<Platform>,
    pub results: Option<SearchResults>,
    pub error: Option<String>,
    pub tab: SearchTab,
}

impl Default for SearchView {
    fn default() -> Self {
        Self {
            input: String::new(),
            query: String::new(),
            platforms: Platform::REMOTE.into_iter().collect(),
            loading: false,
            loading_more: None,
            results: None,
            error: None,
            tab: SearchTab::All,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct LibraryView {
    pub tracks: Vec<Track>,
    pub scanning: bool,
    pub scanned_once: bool,
    pub folder_input: String,
}

#[derive(Clone, Debug, Default)]
pub struct DownloadsView {
    pub jobs: Vec<DownloadJob>,
}

impl DownloadsView {
    pub fn track_job_mut(&mut self, track_id: &str) -> Option<&mut DownloadJob> {
        self.jobs.iter_mut().find(|job| job.track.id == track_id)
    }

    pub fn active(&self) -> impl Iterator<Item = &DownloadJob> {
        self.jobs
            .iter()
            .filter(|job| matches!(job.state, JobState::Queued | JobState::Running))
    }

    pub fn completed(&self) -> impl Iterator<Item = &DownloadJob> {
        self.jobs
            .iter()
            .filter(|job| matches!(job.state, JobState::Done | JobState::Failed))
    }
}

#[derive(Clone, Debug, Default)]
pub struct UpdateView {
    pub checking: bool,
    /// A user-initiated check (show a result toast even when up to date).
    pub manual_check: bool,
    pub available: Option<UpdateInfo>,
    pub dismissed: bool,
    pub last_checked: Option<std::time::Instant>,
    pub error: Option<String>,
    /// Downloading/installing the available update.
    pub downloading: bool,
    pub progress: Option<f32>,
}

#[derive(Clone, Debug, Default)]
pub struct AlbumPageView {
    pub album: Option<crate::model::Album>,
    pub tracks: Vec<crate::model::Track>,
    pub bio: Option<crate::model::ArtistBio>,
    pub series: Vec<crate::model::Album>,
    pub loading: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ArtistPageView {
    pub artist: Option<Artist>,
    pub bio: Option<crate::model::ArtistBio>,
    pub hot_tracks: Vec<Track>,
    /// The user's own listening history for this artist (local profile data).
    pub local_tracks: Vec<Track>,
    pub loading: bool,
    pub error: Option<String>,
    pub platform: Option<Platform>,
}

#[derive(Clone, Debug, Default)]
pub struct BackgroundView {
    pub track_id: Option<String>,
    pub loading: bool,
    pub data: Option<SongBackground>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct CoverView {
    pub track_id: Option<String>,
    pub path: Option<PathBuf>,
    pub palette: Option<Palette>,
    pub loading: bool,
}

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

pub struct AppState {
    pub lang: Language,
    pub page: Page,
    pub profile: Profile,
    pub profiles: Vec<String>,

    pub theme_mode: ThemeMode,
    pub theme: Theme,
    pub theme_target: Theme,
    /// Optional accent overrides from Lua config (`accent`, `highlight`,
    /// `macaron`). They are applied on top of the selected base theme.
    pub accent_override: Option<egui::Color32>,
    pub highlight_override: Option<egui::Color32>,
    pub macaron_override: Option<[egui::Color32; 5]>,

    pub search: SearchView,
    pub library: LibraryView,
    pub downloads: DownloadsView,
    pub background: BackgroundView,
    pub artist_page: ArtistPageView,
    pub album_page: AlbumPageView,
    pub updates: UpdateView,
    pub cover: CoverView,
    pub tooling: ToolingStatus,
    pub script: ScriptStatus,

    /// Accent colours currently follow cover art (user toggle).
    pub follow_cover_accent: bool,
    /// Keyboard step sizes (overridable from Lua config later).
    pub seek_step: f64,
    pub volume_step: f32,
    /// Applied egui zoom factor.
    pub ui_scale: f32,
    /// True while the main window is unfocused/minimized: UI-only effects and
    /// repaint cadence are throttled (audio playback is unaffected).
    pub ui_backgrounded: bool,

    pub now_playing: Option<Track>,
    pub playback: PlaybackStatus,
    pub queue: Vec<Track>,
    pub queue_pos: usize,
    pub repeat: RepeatMode,
    pub shuffle: bool,

    pub selected_playlist: Option<String>,
    pub now_playing_tab: NowPlayingTab,
    pub queue_panel_open: bool,
    pub toasts: Vec<Toast>,
    /// Tracks dropped onto the window; the UI asks whether to save them.
    pub drop_prompt: Option<Vec<Track>>,

    /// True when running with the offline demo data (used for screenshots).
    pub demo: bool,
}

impl AppState {
    pub fn new(profile: Profile, profiles: Vec<String>) -> Self {
        let theme_mode = profile.settings.theme_mode;
        let volume = profile.settings.volume;
        let follow_cover_accent = profile.settings.follow_cover_accent;
        let ui_scale = profile.settings.ui_scale;
        let mut playback = PlaybackStatus::default();
        playback.volume = volume;
        let theme = Theme::for_mode(theme_mode, None);
        Self {
            lang: profile.settings.language,
            page: Page::Home,
            profile,
            profiles,
            theme_mode,
            theme,
            theme_target: theme,
            accent_override: None,
            highlight_override: None,
            macaron_override: None,
            search: SearchView::default(),
            library: LibraryView::default(),
            downloads: DownloadsView::default(),
            background: BackgroundView::default(),
            artist_page: ArtistPageView::default(),
            album_page: AlbumPageView::default(),
            updates: UpdateView::default(),
            cover: CoverView::default(),
            tooling: ToolingStatus::default(),
            script: ScriptStatus::default(),
            follow_cover_accent,
            seek_step: 5.0,
            volume_step: 0.05,
            ui_scale: ui_scale.clamp(0.8, 2.0),
            ui_backgrounded: false,
            now_playing: None,
            playback,
            queue: Vec::new(),
            queue_pos: 0,
            repeat: RepeatMode::Off,
            shuffle: false,
            selected_playlist: None,
            now_playing_tab: NowPlayingTab::Track,
            queue_panel_open: false,
            toasts: Vec::new(),
            drop_prompt: None,
            demo: false,
        }
    }

    pub fn tr(&self, key: Key) -> &'static str {
        t(self.lang, key)
    }

    pub fn set_language(&mut self, lang: Language) {
        self.lang = lang;
        self.profile.settings.language = lang;
    }

    pub fn set_theme_mode(&mut self, mode: ThemeMode) {
        self.theme_mode = mode;
        self.profile.settings.theme_mode = mode;
        self.refresh_theme_target();
    }

    /// Recompute the target theme after a new cover palette arrives.
    pub fn refresh_theme_target(&mut self) {
        let palette = if self.follow_cover_accent {
            self.cover.palette.as_ref()
        } else {
            None
        };
        let mut target = Theme::for_mode(self.theme_mode, palette);
        if let Some(accent) = self.accent_override {
            target = target.with_accent(accent);
        }
        if let Some(highlight) = self.highlight_override {
            target = target.with_highlight(highlight);
        }
        if let Some(macaron) = self.macaron_override {
            target = target.with_macaron(macaron);
        }
        self.theme_target = target;
    }

    pub fn apply_theme_overrides(
        &mut self,
        accent: Option<egui::Color32>,
        highlight: Option<egui::Color32>,
        macaron: Option<[egui::Color32; 5]>,
    ) {
        self.accent_override = accent;
        self.highlight_override = highlight;
        self.macaron_override = macaron;
        self.refresh_theme_target();
    }

    pub fn set_follow_cover_accent(&mut self, follow: bool) {
        self.follow_cover_accent = follow;
        self.profile.settings.follow_cover_accent = follow;
        self.refresh_theme_target();
    }

    pub fn toast(&mut self, kind: ToastKind, text: impl Into<String>, now: f64) {
        self.toasts.push(Toast {
            text: text.into(),
            kind,
            created_at: now,
        });
        if self.toasts.len() > 4 {
            self.toasts.remove(0);
        }
    }

    pub fn remember_playing(&mut self, track: &Track) {
        self.profile.remember(track);
        self.profile.add_history(&track.id);
    }

    pub fn is_favorite(&self, track_id: &str) -> bool {
        self.profile.is_favorite(track_id)
    }

    pub fn recent_tracks(&self, limit: usize) -> Vec<Track> {
        self.profile.recent_tracks(limit)
    }

    pub fn favorite_tracks(&self) -> Vec<Track> {
        self.profile.favorite_tracks()
    }

    pub fn selected_playlist(&self) -> Option<&Playlist> {
        self.selected_playlist
            .as_deref()
            .and_then(|id| self.profile.playlist(id))
    }

    pub fn selected_playlist_tracks(&self) -> Vec<Track> {
        self.selected_playlist
            .as_deref()
            .map(|id| self.profile.playlist_tracks(id))
            .unwrap_or_default()
    }

    pub fn current_track(&self) -> Option<&Track> {
        self.now_playing.as_ref()
    }

    pub fn toggle_platform_filter(&mut self, platform: Platform) {
        if !self.search.platforms.remove(&platform) {
            self.search.platforms.insert(platform);
        }
    }

    pub fn prune_toasts(&mut self, now: f64) {
        self.toasts.retain(|toast| now - toast.created_at < 4.2);
    }
}

// ---------------------------------------------------------------------------
// Commands (UI -> app) and events (workers -> app)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum Command {
    PlayTrack(Box<Track>),
    PlayTracks(Vec<Track>),
    /// Play from a visible list, starting at `index` (keeps list order as queue).
    PlayTracksFrom {
        tracks: Vec<Track>,
        index: usize,
    },
    PlayPlaylist(String),
    TogglePlayPause,
    Pause,
    Resume,
    Stop,
    Next,
    Previous,
    Seek(f64),
    SeekRelative(f64),
    SetVolume(f32),
    CycleRepeat,
    ToggleShuffle,
    ToggleFavorite(Box<Track>),
    Search {
        query: String,
        platforms: Vec<Platform>,
    },
    /// Paste a public platform link (Spotify / Apple Music / YouTube / Netease
    /// / SoundCloud / Bilibili) and import its metadata.
    ImportLink(String),
    AddToQueue(Vec<Track>),
    PlayNext(Box<Track>),
    ClearQueue,
    RemoveFromQueue(usize),
    /// Open a producer/artist page and load bio + platform tracks.
    OpenArtist(Box<Artist>),
    /// Open an album page and load its tracks + same-artist albums.
    OpenAlbum(Box<crate::model::Album>),
    /// Add/remove an album from the current profile's favourite albums.
    ToggleFavoriteAlbum(Box<crate::model::Album>),
    /// Save drag-and-dropped local files into the library.
    SaveDroppedToLibrary(Vec<Track>),
    /// Return from the artist page to the search results.
    BackToSearch,
    /// Load more results for one platform (keeps the current query).
    SearchMore(Platform),
    ToggleQueuePanel,
    Download(Vec<Track>),
    LoadBackground(Box<Track>),
    OpenUrl(String),
    CopyText(String),
    CreatePlaylist {
        name: String,
        tracks: Vec<Track>,
    },
    RenamePlaylist {
        id: String,
        name: String,
    },
    SetPlaylistDescription {
        id: String,
        description: String,
    },
    SetPlaylistCover {
        id: String,
        source: Option<PathBuf>,
    },
    DeletePlaylist(String),
    AddToPlaylist {
        playlist_id: String,
        tracks: Vec<Track>,
    },
    RemoveFromPlaylist {
        playlist_id: String,
        track_id: String,
    },
    SwitchProfile(String),
    NewProfile(String),
    DeleteProfile(String),
    ScanLibrary,
    AddMusicDir(PathBuf),
    RemoveMusicDir(PathBuf),
    PickMusicFolder,
    PickDownloadFolder,
    SetLanguage(Language),
    SetThemeMode(ThemeMode),
    /// Change UI zoom (1.0 = 100 %).
    SetUiScale(f32),
    SetFollowCoverAccent(bool),
    /// Streamed-audio cache cap in MiB; `0` = delete played tracks immediately.
    SetCacheLimit(u32),
    /// Export a playlist as a metadata-only share file.
    ExportPlaylist(String),
    /// Import a shared playlist file.
    ImportPlaylist,
    /// Set/replace the current local profile's bio.
    SetProfileBio(String),
    /// Pick an image file and set it as the current profile avatar.
    SetProfileAvatar,
    /// Clear the current profile avatar (falls back to a solid-colour avatar).
    ClearProfileAvatar,
    /// Export the current profile as a portable JSON file.
    ExportProfile,
    /// Import a shared profile file as a new local profile and switch to it.
    ImportProfile,
    /// Query GitHub Releases for a newer version.
    CheckForUpdates,
    /// Keep the current version (dismiss the startup update prompt).
    DismissUpdate,
    /// Download, verify and install the available update.
    InstallUpdate,
    /// Open the release page (fallback / release notes).
    OpenUpdatePage,
    /// Open the embedded bilingual plugin-development manual.
    OpenPluginDocs,
    /// Open the embedded bilingual configuration reference.
    OpenConfigDocs,
    /// Re-read `config.lua` and the plugins directory.
    ReloadScripts,
    /// Run a custom action registered by a Lua script/plugin.
    RunScriptAction(String),
    /// Open the user's scripting directory in the file manager.
    OpenScriptsFolder,
    SetDownloadFormat(DownloadFormat),
    SetDownloadDir(PathBuf),
    SetResultsPerPlatform(usize),
    SetYtdlpPath(Option<PathBuf>),
    DetectTools,
    ClearCache,
    OpenDownloadFolder,
    OpenDataFolder,
    Quit,
}

#[derive(Clone, Debug)]
pub enum AppEvent {
    SearchDone(SearchResults),
    /// Extra results for one platform appended to the current search.
    SearchMoreDone(SearchResults),
    /// Artist/producer page data (bio + platform tracks).
    ArtistDetailReady(Box<ArtistDetail>),
    /// Album page data (metadata + ordered tracks + same-artist albums).
    AlbumDetailReady(Box<crate::model::AlbumDetail>),
    /// Best-effort artist portrait URL found by a background lookup.
    ArtistPortrait {
        artist_id: String,
        url: String,
    },
    /// A page cover (home/profile/library/playlist) finished downloading.
    CoverPrefetched,
    /// An artist avatar finished downloading.
    ArtistImageReady,
    /// A low-resolution search cover finished downloading.
    SearchCoverReady {
        generation: u64,
        path: std::path::PathBuf,
    },
    /// Result of the GitHub update check.
    UpdateCheckDone(Result<Option<UpdateInfo>, String>),
    /// Update asset download progress (0..1 when the size is known).
    UpdateProgress(Option<f32>),
    /// A verified update installer/archive is ready to apply.
    UpdateDownloaded(std::path::PathBuf),
    /// Update download/verification/install failed.
    UpdateDownloadFailed(String),
    Resolved {
        track: Track,
        path: PathBuf,
        temporary: bool,
    },
    ResolveFailed {
        track: Track,
        error: String,
    },
    LinkImported {
        track: Track,
        source: String,
    },
    CoverReady {
        track_id: String,
        path: Option<PathBuf>,
        palette: Option<Palette>,
    },
    BackgroundDone {
        track_id: String,
        data: SongBackground,
    },
    BackgroundFailed {
        track_id: String,
        error: String,
    },
    DownloadUpdate {
        track_id: String,
        state: JobState,
        progress: f32,
        stage: String,
        path: Option<PathBuf>,
        error: Option<String>,
    },
    LibraryScanned(Vec<Track>),
    ToolsDetected(ToolingStatus),
    /// A Lua script / plugin asked the app to run a command.
    RunCommand(Command),
    /// A Lua script / plugin asked to show a toast.
    ScriptToast(String),
    /// A Lua script / plugin asked to run a keyboard-style action.
    ScriptBinding(BindingAction),
    PlaybackEnded,
    Toast {
        kind: ToastKind,
        text: String,
    },
}
