//! Application shell: owns state, the player and the worker channels.
//!
//! Threading model
//! ---------------
//! * The UI thread (this struct) never blocks on IO.
//! * Every network / decode / download job runs on a detached thread and
//!   reports back through one `AppEvent` channel drained once per frame.
//! * While jobs are in flight we schedule repaints, so the UI stays responsive
//!   without burning CPU when idle.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::{unbounded, Receiver, Sender};
use egui::{Context, ViewportCommand};

use crate::art::ArtCache;
use crate::audio::Player;
use crate::download::{self, DownloadOptions};
use crate::i18n::Key;
use crate::library;
use crate::model::{
    format_duration, DownloadJob, DownloadProgress, JobState, Platform, PlaybackState,
    PlaybackStatus, PlaylistShare, Profile, ProfileShare, RepeatMode, SearchResults, Settings,
    SongBackground, Track, PLAYLIST_SHARE_FORMAT, PROFILE_SHARE_FORMAT,
};
use crate::net::Http;
use crate::providers;
use crate::resolver::{self, ResolveConfig};
use crate::scripting::{
    PluginRegistry, ScriptActionInfo, ScriptConfig, ScriptEvent, ScriptPaths, Scripts,
};
use crate::state::{
    AppEvent, AppState, BackgroundView, Command, CoverView, LibraryView, NowPlayingTab, Page,
    ScriptStatus, SearchView, ToastKind, ToolingStatus,
};
use crate::store::{self, AppPaths, ProfileStore};
use crate::theme::{self, Theme, ThemeMode};
use crate::ui;
use base64::Engine as _;
use sha2::{Digest, Sha256};

struct ShotPlan {
    path: PathBuf,
    at_frame: u64,
    /// Optional wall-clock delay (seconds since start); useful for capturing a
    /// running playback / resolved track instead of the first UI frame.
    after_secs: Option<f64>,
    requested: bool,
}

pub struct EchoPortApp {
    pub state: AppState,
    pub paths: AppPaths,
    pub http: Arc<Http>,
    pub art: ArtCache,
    pub player: Player,
    pub resolve_config: ResolveConfig,
    pub ui: ui::UiState,
    pub store: ProfileStore,
    /// Lua configuration / plugin runtime (UI thread only, `!Send`).
    pub scripts: Scripts,
    /// Compiled-in plugin extension point (empty by default).
    pub plugins: PluginRegistry,

    tx: Sender<AppEvent>,
    rx: Receiver<AppEvent>,
    ctx: Context,

    dirty_since: Option<Instant>,
    last_save_attempt: Instant,
    rng: u64,
    demo: bool,
    frames: u64,
    shot: Option<ShotPlan>,
    applied_scale: f32,
    /// Cache file of the track that is currently loaded (for "no cache" mode).
    current_cache_path: Option<PathBuf>,
    /// Short startup animation / update prompt state.
    pub startup_started: std::time::Instant,
    pub splash_done: bool,
    fake_update_sent: bool,
    quitting: bool,
    /// Low-res cover files fetched for the current search results.
    search_cover_paths: Vec<PathBuf>,
    search_generation: u64,
    last_page: Page,
    /// Track ids whose low-res page covers were already queued.
    prefetched_covers: std::collections::HashSet<String>,
    cover_prefetch_page: Option<Page>,
}

impl EchoPortApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let ctx = cc.egui_ctx.clone();
        crate::fonts::install(&ctx);
        Theme::apply_text_styles(&ctx);

        let paths = AppPaths::resolve();
        if let Err(err) = paths.ensure() {
            log::warn!("could not create data directories: {err}");
        }
        let store = ProfileStore::new(paths.clone());
        let mut profile = store.load_or_create(None);
        let profiles = store.list();

        // Demo mode powers screenshots/UI work. `ECHOPORT_LIVE=1` forces the
        // real network/audio path even when a screenshot was requested.
        let wants_screenshot = std::env::var("ECHOPORT_SHOT").is_ok();
        let demo = (std::env::var("ECHOPORT_DEMO").is_ok() || wants_screenshot)
            && std::env::var("ECHOPORT_LIVE").is_err();

        let (tx, rx) = unbounded();
        // Demo screenshots must be deterministic: never read the user's Lua
        // config. Real runs load config.lua + plugins before building state so
        // startup options (language/theme/volume/page) apply from frame one.
        let scripts = if demo {
            Scripts::empty()
        } else {
            Scripts::load(
                ScriptPaths {
                    config_file: paths.config_file.clone(),
                    plugins_dir: paths.plugins_dir.clone(),
                },
                tx.clone(),
            )
        };
        let script_config = scripts.config().clone();
        if !demo {
            apply_script_config_to_profile(&mut profile, &script_config);
        }
        let settings = profile.settings.clone();
        let mut state = AppState::new(profile, profiles);
        let mut http = Http::new();
        let art = ArtCache::new(paths.covers_dir.clone());
        let _ = art.ensure();
        let _ = &mut http;

        let resolve_config = ResolveConfig {
            ytdlp: settings.ytdlp_path.clone(),
            ffmpeg: resolver::detect_ffmpeg(),
            cache_dir: paths.audio_cache_dir.clone(),
        };

        let player = match Player::new() {
            Ok(player) => player,
            Err(first_err) => {
                // Try once more: a transient device lock must not take the UI down.
                match Player::new() {
                    Ok(player) => player,
                    Err(err) => {
                        log::error!("no audio output available: {first_err} / {err}");
                        state.playback.error = Some(err.to_string());
                        state.playback.state = PlaybackState::Error;
                        let text = state.tr(Key::ErrorAudioDevice).to_owned();
                        state.toast(ToastKind::Error, text, 0.0);
                        // Player::new() is expected to keep working without a
                        // device; callers see the error through the status.
                        Player::new().expect("player must not depend on an output device")
                    }
                }
            }
        };
        if !demo {
            apply_script_config_to_state(&mut state, &script_config, true);
        }
        player.set_volume(state.playback.volume);
        state.script.config_file = paths.config_file.clone();
        state.script.plugins_dir = paths.plugins_dir.clone();

        if let Ok(value) = std::env::var("ECHOPORT_PAGE") {
            state.page = page_from_key(&value).unwrap_or(Page::Home);
        }
        if demo {
            seed_demo(&mut state, &paths);
            state.demo = true;
            if let Ok(value) = std::env::var("ECHOPORT_THEME") {
                if let Some(mode) = ThemeMode::from_key(&value) {
                    state.set_theme_mode(mode);
                }
            }
            if std::env::var("ECHOPORT_FOLLOW_COVER").is_ok() {
                state.set_follow_cover_accent(true);
            }
            state.theme = state.theme_target;
        }

        let ui = ui::UiState::new(cc, &state);
        let at_frame = std::env::var("ECHOPORT_SHOT_FRAMES")
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .unwrap_or(40)
            .max(2);
        let after_secs = std::env::var("ECHOPORT_SHOT_AFTER")
            .ok()
            .and_then(|value| value.trim().parse::<f64>().ok())
            .filter(|secs| secs.is_finite() && *secs > 0.0);
        let shot = std::env::var("ECHOPORT_SHOT").ok().map(|path| ShotPlan {
            path: PathBuf::from(path),
            at_frame,
            after_secs,
            requested: false,
        });

        let plugins = PluginRegistry::with_sender(tx.clone());
        let splash_env = std::env::var("ECHOPORT_SPLASH").is_ok();
        let skip_splash = std::env::var("ECHOPORT_NO_SPLASH").is_ok()
            || (std::env::var("ECHOPORT_SHOT").is_ok() && !splash_env);
        let mut app = Self {
            state,
            paths,
            http: Arc::new(http),
            art,
            player,
            resolve_config,
            ui,
            store,
            scripts,
            plugins,
            tx,
            rx,
            ctx,
            dirty_since: None,
            last_save_attempt: Instant::now(),
            rng: 0x9E37_79B9_7F4A_7C15,
            demo,
            frames: 0,
            shot,
            applied_scale: 1.0,
            current_cache_path: None,
            startup_started: std::time::Instant::now(),
            splash_done: skip_splash,
            fake_update_sent: false,
            quitting: false,
            search_cover_paths: Vec::new(),
            search_generation: 0,
            last_page: Page::Home,
            prefetched_covers: std::collections::HashSet::new(),
            cover_prefetch_page: None,
        };

        app.merge_local_files_into_library();
        if std::env::var("ECHOPORT_PROFILE_EDIT").is_ok() {
            app.ui.profile_editing = true;
            app.ui.profile_bio = app.state.profile.bio.clone();
            app.ui.profile_bio_for = Some(app.state.profile.name.clone());
        }
        // QA hook: start a live search without UI automation.
        if let Ok(query) = std::env::var("ECHOPORT_SEARCH_QUERY") {
            if !app.demo && !query.trim().is_empty() {
                app.state.page = Page::Search;
                app.start_search(query, Platform::REMOTE.to_vec());
            }
        }
        if let Ok(paths) = std::env::var("ECHOPORT_QA_DROP") {
            if !app.demo {
                let files: Vec<egui::DroppedFile> = paths
                    .split(';')
                    .filter(|path| !path.trim().is_empty())
                    .map(|path| egui::DroppedFile {
                        name: String::new(),
                        mime: String::new(),
                        last_modified: None,
                        bytes: None,
                        path: Some(std::path::PathBuf::from(path.trim())),
                    })
                    .collect();
                if !files.is_empty() {
                    app.handle_dropped_files(files);
                }
            }
        }
        if let Ok(query) = std::env::var("ECHOPORT_QA_ALBUM") {
            if !app.demo && !query.trim().is_empty() {
                let http = app.http.clone();
                let albums = providers::search_albums(
                    &http,
                    &query,
                    &[Platform::AppleMusic, Platform::Netease],
                    1,
                );
                if let Some(album) = albums.into_iter().flat_map(|group| group.albums).next() {
                    app.open_album(album);
                    app.state.search.tab = crate::state::SearchTab::Albums;
                }
            }
        }
        if let Ok(lang) = std::env::var("ECHOPORT_LANG") {
            app.state
                .set_language(if lang.trim().eq_ignore_ascii_case("zh") {
                    crate::i18n::Language::Zh
                } else {
                    crate::i18n::Language::En
                });
        }
        if let Ok(tab) = std::env::var("ECHOPORT_SEARCH_TAB") {
            app.state.search.tab = match tab.trim().to_ascii_lowercase().as_str() {
                "artists" | "artist" => crate::state::SearchTab::Artists,
                "albums" | "album" => crate::state::SearchTab::Albums,
                _ => crate::state::SearchTab::All,
            };
        }
        if !app.demo {
            app.refresh_script_status();
            app.spawn_tool_detection();
            app.spawn_library_scan();
            if std::env::var("ECHOPORT_NO_UPDATE_CHECK").is_err() {
                app.spawn_update_check();
            }
            app.fire_script(ScriptEvent::Startup);

            let cache_dir = app.paths.audio_cache_dir.clone();
            let limit = audio_cache_limit_bytes(&app.state.profile.settings);
            std::thread::spawn(move || {
                let freed = resolver::prune_cache(&cache_dir, limit);
                if freed > 0 {
                    log::info!(
                        "audio cache pruned: freed {} MiB (limit {} MiB)",
                        freed / (1024 * 1024),
                        limit / (1024 * 1024)
                    );
                }
            });
        }
        app
    }

    // -----------------------------------------------------------------------
    // Public helpers used by the UI layer
    // -----------------------------------------------------------------------

    pub fn dispatch(&mut self, command: Command) {
        match command {
            Command::PlayTrack(track) => {
                let track = *track;
                if self.same_track_loading(&track) {
                    return;
                }
                self.state.queue = vec![track.clone()];
                self.state.queue_pos = 0;
                self.start_track(track);
            }
            Command::PlayTracks(tracks) => {
                if tracks.is_empty() {
                    return;
                }
                if self.same_track_loading(&tracks[0]) {
                    return;
                }
                self.state.queue = tracks;
                self.state.queue_pos = 0;
                let first = self.state.queue[0].clone();
                self.start_track(first);
            }
            Command::PlayTracksFrom { tracks, index } => {
                if tracks.is_empty() {
                    return;
                }
                let index = index.min(tracks.len() - 1);
                if self.same_track_loading(&tracks[index]) {
                    return;
                }
                self.state.queue = tracks;
                self.state.queue_pos = index;
                let track = self.state.queue[index].clone();
                self.start_track(track);
            }
            Command::PlayPlaylist(id) => {
                let tracks = self.state.profile.playlist_tracks(&id);
                if tracks.is_empty() {
                    self.toast(ToastKind::Info, Key::EmptyPlaylist);
                    return;
                }
                if self.same_track_loading(&tracks[0]) {
                    return;
                }
                self.state.queue = tracks;
                self.state.queue_pos = 0;
                let track = self.state.queue[0].clone();
                self.start_track(track);
            }
            Command::TogglePlayPause => self.toggle_play_pause(),
            Command::Pause => {
                if self.state.playback.state == PlaybackState::Loading {
                    return;
                }
                self.player.pause();
                self.fire_script(ScriptEvent::PlaybackPaused);
            }
            Command::Resume => {
                if self.state.playback.state == PlaybackState::Loading {
                    return;
                }
                self.player.play();
                self.fire_script(ScriptEvent::PlaybackResumed);
            }
            Command::Stop => {
                self.player.stop();
                self.state.playback.state = PlaybackState::Idle;
            }
            Command::Next => self.advance_queue(1, false),
            Command::Previous => {
                if self.state.playback.position_secs > 3.0 {
                    self.player.seek(0.0);
                } else {
                    self.advance_queue(-1, false);
                }
            }
            Command::Seek(position) => {
                self.player.seek(position.max(0.0));
            }
            Command::SeekRelative(delta) => {
                let target = self.state.playback.position_secs + delta;
                self.player.seek(target.max(0.0));
            }
            Command::SetVolume(volume) => {
                let volume = volume.clamp(0.0, 1.0);
                self.player.set_volume(volume);
                self.state.playback.volume = volume;
                self.state.profile.settings.volume = volume;
                self.mark_dirty();
            }
            Command::CycleRepeat => {
                self.state.repeat = self.state.repeat.next();
            }
            Command::ToggleShuffle => {
                self.state.shuffle = !self.state.shuffle;
            }
            Command::ToggleFavorite(track) => {
                let track = *track;
                let favorite = self.state.profile.toggle_favorite(&track);
                let key = if favorite {
                    Key::AddToFavorites
                } else {
                    Key::RemoveFromFavorites
                };
                let text = self.state.tr(key).to_owned();
                self.toast(ToastKind::Success, TextOrKey::Text(text));
                self.mark_dirty();
            }
            Command::Search { query, platforms } => self.start_search(query, platforms),
            Command::ImportLink(url) => {
                let url = url.trim().to_owned();
                if url.is_empty() {
                    return;
                }
                let http = self.http.clone();
                let config = self.resolve_config.clone();
                let tx = self.tx.clone();
                std::thread::spawn(
                    move || match providers::track_from_url(&http, &config, &url) {
                        Ok(track) => {
                            let _ = tx.send(AppEvent::LinkImported { track, source: url });
                        }
                        Err(err) => {
                            let _ = tx.send(AppEvent::Toast {
                                kind: ToastKind::Error,
                                text: format!("{}: {err}", "import failed"),
                            });
                        }
                    },
                );
            }
            Command::AddToQueue(tracks) => {
                if tracks.is_empty() {
                    return;
                }
                self.state.profile.remember_many(&tracks);
                self.state.queue.extend(tracks);
                self.toast(ToastKind::Success, Key::ToastAddedToQueue);
                self.mark_dirty();
            }
            Command::PlayNext(track) => {
                let track = *track;
                self.state.profile.remember(&track);
                let insert_at = (self.state.queue_pos + 1).min(self.state.queue.len());
                self.state.queue.insert(insert_at, track);
                self.toast(ToastKind::Success, Key::ToastAddedToQueue);
                self.mark_dirty();
            }
            Command::RemoveFromQueue(index) => {
                if index < self.state.queue.len() {
                    self.state.queue.remove(index);
                    if self.state.queue.is_empty() {
                        self.player.stop();
                        self.state.playback.state = PlaybackState::Idle;
                        self.state.now_playing = None;
                        self.state.queue_pos = 0;
                    } else if index < self.state.queue_pos {
                        self.state.queue_pos -= 1;
                    } else if index == self.state.queue_pos {
                        self.state.queue_pos = self.state.queue_pos.min(self.state.queue.len() - 1);
                        let track = self.state.queue[self.state.queue_pos].clone();
                        self.start_track(track);
                    }
                }
            }
            Command::ToggleQueuePanel => {
                self.state.queue_panel_open = !self.state.queue_panel_open;
            }
            Command::ClearQueue => {
                self.player.stop();
                self.state.queue.clear();
                self.state.queue_pos = 0;
                self.state.now_playing = None;
                self.state.playback = PlaybackStatus::default();
                self.toast(ToastKind::Info, Key::ToastCleared);
            }
            Command::Download(tracks) => self.start_downloads(tracks),
            Command::LoadBackground(track) => self.start_background(*track),
            Command::OpenUrl(url) => {
                if is_safe_web_url(&url) {
                    if let Err(err) = open::that(&url) {
                        log::warn!("could not open {url}: {err}");
                    }
                } else {
                    log::warn!("refusing to open non-http(s) URL: {url}");
                }
            }
            Command::CopyText(text) => {
                self.ctx.copy_text(text);
                self.toast(ToastKind::Success, Key::ToastCopied);
            }
            Command::CreatePlaylist { name, tracks } => {
                let id = self.state.profile.create_playlist(name);
                self.state.profile.add_tracks_to_playlist(&id, &tracks);
                self.state.selected_playlist = Some(id);
                self.state.page = Page::Playlists;
                self.toast(ToastKind::Success, Key::ToastPlaylistCreated);
                self.mark_dirty();
            }
            Command::RenamePlaylist { id, name } => {
                if let Some(playlist) = self.state.profile.playlist_mut(&id) {
                    playlist.name = name;
                    playlist.touch();
                    self.mark_dirty();
                }
            }
            Command::SetPlaylistDescription { id, description } => {
                if let Some(playlist) = self.state.profile.playlist_mut(&id) {
                    playlist.description = description.trim().to_owned();
                    playlist.touch();
                    self.mark_dirty();
                }
            }
            Command::SetPlaylistCover { id, source } => {
                self.apply_playlist_cover(&id, source);
            }
            Command::DeletePlaylist(id) => {
                if self.state.profile.delete_playlist(&id) {
                    if self.state.selected_playlist.as_deref() == Some(&id) {
                        self.state.selected_playlist = None;
                    }
                    self.toast(ToastKind::Info, Key::PlaylistDeletedToast);
                    self.mark_dirty();
                }
            }
            Command::AddToPlaylist {
                playlist_id,
                tracks,
            } => {
                let added = self
                    .state
                    .profile
                    .add_tracks_to_playlist(&playlist_id, &tracks);
                self.toast(
                    ToastKind::Success,
                    if added > 0 {
                        TextOrKey::Key(Key::PlaylistAddedToast)
                    } else {
                        TextOrKey::Key(Key::ToastAddedToPlaylist)
                    },
                );
                self.mark_dirty();
            }
            Command::RemoveFromPlaylist {
                playlist_id,
                track_id,
            } => {
                if self
                    .state
                    .profile
                    .remove_from_playlist(&playlist_id, &track_id)
                {
                    self.toast(ToastKind::Info, Key::PlaylistRemovedToast);
                    self.mark_dirty();
                }
            }
            Command::SwitchProfile(name) => self.switch_profile(&name),
            Command::NewProfile(name) => {
                let name = name.trim();
                if name.is_empty() {
                    return;
                }
                let mut settings = Settings::default();
                settings.language = self.state.lang;
                let profile = Profile::new(name, settings);
                if let Err(err) = self.store.save(&profile) {
                    log::warn!("could not save new profile: {err}");
                    return;
                }
                self.switch_profile(name);
            }
            Command::DeleteProfile(name) => {
                if self.state.profiles.len() <= 1 {
                    return;
                }
                self.store.delete(&name);
                self.state.profiles = self.store.list();
                let fallback = self.state.profiles.first().cloned();
                if let Some(next) = fallback {
                    self.switch_profile(&next);
                }
            }
            Command::ScanLibrary => self.spawn_library_scan(),
            Command::PickMusicFolder => {
                if let Some(dir) = rfd::FileDialog::new()
                    .set_title(self.state.tr(Key::AddMusicFolder))
                    .pick_folder()
                {
                    self.dispatch(Command::AddMusicDir(dir));
                }
            }
            Command::PickDownloadFolder => {
                if let Some(dir) = rfd::FileDialog::new()
                    .set_title(self.state.tr(Key::ChooseFolder))
                    .pick_folder()
                {
                    self.dispatch(Command::SetDownloadDir(dir));
                }
            }
            Command::AddMusicDir(dir) => {
                if !self.state.profile.settings.music_dirs.contains(&dir) {
                    self.state.profile.settings.music_dirs.push(dir);
                    self.mark_dirty();
                    self.spawn_library_scan();
                }
            }
            Command::RemoveMusicDir(dir) => {
                self.state.profile.settings.music_dirs.retain(|d| d != &dir);
                self.mark_dirty();
                self.spawn_library_scan();
            }
            Command::SetLanguage(lang) => {
                self.state.set_language(lang);
                self.mark_dirty();
            }
            Command::SetThemeMode(mode) => {
                self.state.set_theme_mode(mode);
                self.mark_dirty();
            }
            Command::SetUiScale(scale) => {
                let scale = scale.clamp(0.8, 2.0);
                self.state.ui_scale = scale;
                self.state.profile.settings.ui_scale = scale;
                self.mark_dirty();
            }
            Command::SetFollowCoverAccent(follow) => {
                self.state.set_follow_cover_accent(follow);
                self.mark_dirty();
            }
            Command::CheckForUpdates => {
                self.state.updates.manual_check = true;
                self.toast(ToastKind::Info, Key::CheckingForUpdates);
                self.spawn_update_check();
            }
            Command::DismissUpdate => {
                self.state.updates.dismissed = true;
                self.splash_done = true;
                self.ctx.request_repaint();
            }
            Command::OpenPluginDocs => {
                self.open_embedded_doc("plugins.html", include_str!("../docs/plugins.html"))
            }
            Command::OpenConfigDocs => {
                self.open_embedded_doc("config.html", include_str!("../docs/config.html"))
            }
            Command::InstallUpdate => self.start_update_download(),
            Command::OpenUpdatePage => {
                if let Some(info) = self.state.updates.available.as_ref() {
                    if info.url.starts_with("https://github.com/")
                        && !info
                            .url
                            .chars()
                            .any(|c| c.is_whitespace() || c.is_control())
                    {
                        if let Err(err) = open::that(&info.url) {
                            log::warn!("could not open update page {}: {err}", info.url);
                        }
                    } else {
                        log::warn!("refusing to open untrusted update URL: {}", info.url);
                    }
                }
                self.state.updates.dismissed = true;
                self.splash_done = true;
                self.toast(ToastKind::Info, Key::UpdatePageOpened);
                self.ctx.request_repaint();
            }
            Command::OpenArtist(artist) => self.open_artist(*artist),
            Command::OpenAlbum(album) => self.open_album(*album),
            Command::ToggleFavoriteAlbum(album) => {
                let now_favorite = self.state.profile.toggle_favorite_album(&album);
                self.toast(
                    ToastKind::Success,
                    if now_favorite {
                        TextOrKey::Key(Key::AlbumFavorited)
                    } else {
                        TextOrKey::Key(Key::AlbumUnfavorited)
                    },
                );
                self.mark_dirty();
            }
            Command::SaveDroppedToLibrary(tracks) => {
                self.save_dropped_to_library(tracks);
            }
            Command::BackToSearch => {
                self.state.page = Page::Search;
            }
            Command::SearchMore(platform) => self.search_more(platform),
            Command::SetProfileBio(bio) => {
                self.state.profile.bio = bio.trim().to_owned();
                self.mark_dirty();
            }
            Command::SetProfileAvatar => self.set_profile_avatar(),
            Command::ClearProfileAvatar => {
                self.state.profile.avatar = None;
                self.mark_dirty();
            }
            Command::ExportProfile => self.export_profile(),
            Command::ImportProfile => self.import_profile(),
            Command::SetCacheLimit(mb) => {
                let mb = mb.min(10_240);
                self.state.profile.settings.cache_limit_mb = mb;
                if mb > 0 {
                    let cache_dir = self.paths.audio_cache_dir.clone();
                    let limit = u64::from(mb) * 1024 * 1024;
                    std::thread::spawn(move || {
                        let _ = resolver::prune_cache(&cache_dir, limit);
                    });
                }
                self.toast(ToastKind::Success, Key::CacheLimitUpdated);
                self.mark_dirty();
            }
            Command::ExportPlaylist(id) => self.export_playlist(&id),
            Command::ImportPlaylist => self.import_playlist(),
            Command::ReloadScripts => self.reload_scripts(),
            Command::RunScriptAction(action_id) => self.run_script_action(&action_id),
            Command::OpenScriptsFolder => {
                let dir = self.paths.config_dir.clone();
                let _ = std::fs::create_dir_all(&dir);
                let _ = open::that(&dir);
            }
            Command::SetDownloadFormat(format) => {
                self.state.profile.settings.download_format = format;
                self.mark_dirty();
            }
            Command::SetDownloadDir(dir) => {
                self.state.profile.settings.download_dir = dir;
                self.mark_dirty();
            }
            Command::SetResultsPerPlatform(count) => {
                self.state.profile.settings.results_per_platform = count.clamp(4, 40);
                self.mark_dirty();
            }
            Command::SetYtdlpPath(path) => {
                self.state.profile.settings.ytdlp_path = path.clone();
                self.resolve_config.ytdlp = path;
                self.mark_dirty();
                self.spawn_tool_detection();
            }
            Command::DetectTools => self.spawn_tool_detection(),
            Command::ClearCache => self.clear_cache(),
            Command::OpenDownloadFolder => {
                let dir = self.state.profile.settings.download_dir.clone();
                let _ = std::fs::create_dir_all(&dir);
                let _ = open::that(&dir);
            }
            Command::OpenDataFolder => {
                let _ = open::that(&self.paths.data_dir);
            }
            Command::Quit => {
                self.quitting = true;
                self.save_now();
                self.ctx.send_viewport_cmd(ViewportCommand::Close);
            }
        }
        self.ctx.request_repaint();
    }

    // -----------------------------------------------------------------------
    // Frame loop
    // -----------------------------------------------------------------------

    pub fn update_frame(&mut self, ctx: &Context) {
        self.frames += 1;
        let dropped = ctx.input(|input| input.raw.dropped_files.clone());
        if !dropped.is_empty() {
            self.handle_dropped_files(dropped);
        }
        // User adjustable UI zoom (HiDPI / big monitors).
        let scale = self.state.ui_scale.clamp(0.8, 2.0);
        if (scale - self.applied_scale).abs() > 0.001 {
            ctx.set_zoom_factor(scale);
            self.applied_scale = scale;
        }
        if self.state.search.results.is_none()
            || (self.last_page == Page::Search && self.state.page != Page::Search)
        {
            self.clear_search_covers();
        }
        self.last_page = self.state.page;
        if self.cover_prefetch_page != Some(self.state.page) {
            self.cover_prefetch_page = Some(self.state.page);
            self.prefetch_page_covers();
        }
        let (focused, minimized) = ctx.input(|input| {
            let viewport = input.viewport();
            (
                viewport.focused.unwrap_or(true),
                viewport.minimized.unwrap_or(false),
            )
        });
        self.state.ui_backgrounded = !focused || minimized;
        self.tick(ctx);
        if !self.state.ui_backgrounded {
            self.handle_shortcuts(ctx);
        }
        crate::ui::layout(ctx, self);
        self.handle_screenshot(ctx);
        if self.state.playback.state.is_playing() {
            let interval = if self.state.ui_backgrounded { 1000 } else { 66 };
            ctx.request_repaint_after(Duration::from_millis(interval));
        }
    }

    fn tick(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        // QA hook: navigate away from a live search after N seconds so cover
        // cache eviction can be exercised in a headless run.
        if let Ok(after) = std::env::var("ECHOPORT_SEARCH_CLEAR_AFTER") {
            if let (Ok(secs), Some(_)) = (after.parse::<f32>(), self.state.search.results.as_ref())
            {
                if self.state.page == Page::Search
                    && self.startup_started.elapsed().as_secs_f32() > secs
                {
                    self.state.page = Page::Home;
                }
            }
        }
        // QA/demo hook: show the startup update prompt without network access.
        if !self.fake_update_sent {
            if let Ok(fake) = std::env::var("ECHOPORT_FAKE_UPDATE") {
                if self.startup_started.elapsed() > std::time::Duration::from_millis(300) {
                    let latest = if fake.trim().is_empty() {
                        "9.9.9"
                    } else {
                        fake.trim()
                    };
                    self.state.updates.available = Some(crate::update::UpdateInfo {
                        current: self.version().to_owned(),
                        latest: latest.to_owned(),
                        title: format!("EchoPort {latest}"),
                        url: format!("https://github.com/{}/releases", crate::update::repo()),
                        notes: "Startup update prompt demo.".to_owned(),
                        published_at: None,
                        assets: Vec::new(),
                    });
                    self.state.updates.last_checked = Some(std::time::Instant::now());
                    self.state.updates.dismissed = false;
                    self.fake_update_sent = true;
                    ctx.request_repaint();
                }
            } else {
                self.fake_update_sent = true;
            }
        }
        if !self.demo {
            let due = self
                .state
                .updates
                .last_checked
                .map(|checked| checked.elapsed() >= crate::update::check_interval())
                .unwrap_or(false);
            if due && !self.state.updates.checking {
                self.spawn_update_check();
            }
        }
        while let Ok(event) = self.rx.try_recv() {
            self.handle_event(event, ctx);
        }

        if self.demo {
            self.simulate_demo(now);
        } else {
            let status = self.player.status();
            let ended = status.ended;
            // Keep our own `Loading` indicator while the track is being
            // resolved (engine reports Idle then), but never latch on it: as
            // soon as the engine reports anything real we start syncing again.
            let resolving = self.state.playback.state == PlaybackState::Loading
                && status.state == PlaybackState::Idle;
            if !resolving {
                let volume = self.state.playback.volume;
                self.state.playback = status;
                if self.state.playback.volume == 0.0 {
                    self.state.playback.volume = volume;
                }
            }
            if ended {
                self.player.clear_ended();
                self.advance_queue(1, true);
            }
        }

        // Smooth theme transitions toward the target palette.
        if self.state.theme != self.state.theme_target {
            let dt = ctx.input(|i| i.stable_dt).min(0.1);
            let blend = (dt * 7.5).clamp(0.06, 0.6);
            self.state.theme = self.state.theme.lerp(&self.state.theme_target, blend);
            ctx.request_repaint();
        }
        ctx.set_visuals(self.state.theme.to_visuals());
        self.state.prune_toasts(now);

        if let Some(since) = self.dirty_since {
            if since.elapsed() > Duration::from_millis(1200) {
                self.save_now();
            }
        }

        if ctx.input(|input| input.viewport().close_requested()) {
            // Closing the window exits on every platform (Linux/X11 has no
            // reliable "background app" model). Minimize explicitly if you
            // want playback to continue in the background.
            self.save_now();
        }

        let busy = self.state.playback.state.is_active()
            || self.state.search.loading
            || self.state.background.loading
            || self.state.cover.loading
            || self.state.downloads.active().count() > 0
            || !self.state.toasts.is_empty();
        if busy {
            let interval = if self.state.ui_backgrounded { 1000 } else { 90 };
            ctx.request_repaint_after(Duration::from_millis(interval));
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input(|input| input.modifiers.ctrl && input.key_pressed(egui::Key::Q)) {
            self.dispatch(Command::Quit);
            return;
        }
        if self.state.page == Page::Search
            && self.ui.dialog.is_none()
            && !self.state.search.input.trim().is_empty()
            && ctx.input(|input| input.key_pressed(egui::Key::Enter))
        {
            let query = self.state.search.input.trim().to_owned();
            if crate::ui::looks_like_url(&query) {
                self.dispatch(Command::ImportLink(query));
                return;
            }
            let platforms: Vec<Platform> = Platform::REMOTE
                .into_iter()
                .filter(|platform| self.state.search.platforms.contains(platform))
                .collect();
            self.start_search(
                query,
                if platforms.is_empty() {
                    Platform::REMOTE.to_vec()
                } else {
                    platforms
                },
            );
            return;
        }
        if ctx.memory(|memory| memory.focused().is_some()) {
            return;
        }
        if let Some(action) = self.scripts.poll_binding(ctx) {
            self.run_binding_action(action);
            return;
        }
        let (space, left, right, next, previous) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::Space),
                i.key_pressed(egui::Key::ArrowLeft),
                i.key_pressed(egui::Key::ArrowRight),
                i.key_pressed(egui::Key::N),
                i.key_pressed(egui::Key::B),
            )
        });
        if space {
            self.dispatch(Command::TogglePlayPause);
        }
        if left {
            self.dispatch(Command::SeekRelative(-5.0));
        }
        if right {
            self.dispatch(Command::SeekRelative(5.0));
        }
        if next {
            self.dispatch(Command::Next);
        }
        if previous {
            self.dispatch(Command::Previous);
        }
    }

    fn handle_screenshot(&mut self, ctx: &egui::Context) {
        let Some(plan) = self.shot.as_mut() else {
            return;
        };
        let time_reached = plan
            .after_secs
            .map(|secs| ctx.input(|input| input.time) >= secs)
            .unwrap_or(false);
        let ready = match plan.after_secs {
            Some(_) => time_reached,
            None => self.frames >= plan.at_frame,
        };
        if !plan.requested && ready {
            plan.requested = true;
            ctx.send_viewport_cmd(ViewportCommand::Screenshot(egui::UserData::default()));
            ctx.request_repaint();
            return;
        }
        if !plan.requested {
            ctx.request_repaint();
            return;
        }
        let mut captured = None;
        ctx.input(|input| {
            for event in &input.events {
                if let egui::Event::Screenshot { image, .. } = event {
                    captured = Some(image.clone());
                }
            }
        });
        if let Some(image) = captured {
            if let Some(plan) = self.shot.take() {
                if let Some(parent) = plan.path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                match save_color_image(&image, &plan.path) {
                    Ok(()) => log::info!("screenshot written to {:?}", plan.path),
                    Err(err) => log::error!("failed to write screenshot: {err}"),
                }
            }
            ctx.send_viewport_cmd(ViewportCommand::Close);
        } else {
            ctx.request_repaint();
        }
    }

    // -----------------------------------------------------------------------
    // Event handling
    // -----------------------------------------------------------------------

    fn handle_event(&mut self, event: AppEvent, ctx: &egui::Context) {
        match event {
            AppEvent::UpdateCheckDone(result) => {
                self.state.updates.checking = false;
                self.state.updates.last_checked = Some(std::time::Instant::now());
                match result {
                    Ok(Some(info)) => {
                        let manual = self.state.updates.manual_check;
                        log::info!(
                            "update available: current {} latest {}",
                            info.current,
                            info.latest
                        );
                        self.state.updates.available = Some(info);
                        self.state.updates.dismissed = false;
                        if self.splash_done || manual {
                            self.toast(ToastKind::Info, Key::UpdateAvailable);
                        }
                        if std::env::var("ECHOPORT_AUTO_UPDATE").is_ok() {
                            self.start_update_download();
                        }
                    }
                    Ok(None) => {
                        let manual = self.state.updates.manual_check;
                        self.state.updates.available = None;
                        self.state.updates.error = None;
                        if manual {
                            self.toast(ToastKind::Success, Key::UpToDate);
                        }
                    }
                    Err(err) => {
                        let manual = self.state.updates.manual_check;
                        log::debug!("update check failed: {err}");
                        self.state.updates.error = Some(err);
                        if manual {
                            self.toast(ToastKind::Error, Key::UpdateCheckFailed);
                        }
                    }
                }
                self.state.updates.manual_check = false;
                ctx.request_repaint();
            }
            AppEvent::UpdateProgress(progress) => {
                self.state.updates.progress = progress;
                self.ctx.request_repaint();
            }
            AppEvent::UpdateDownloaded(path) => {
                self.state.updates.downloading = false;
                self.state.updates.progress = Some(1.0);
                self.toast(ToastKind::Info, Key::UpdateInstalling);
                self.apply_update(path);
            }
            AppEvent::UpdateDownloadFailed(err) => {
                log::warn!("automatic update failed: {err}");
                self.state.updates.downloading = false;
                self.state.updates.progress = None;
                self.state.updates.error = Some(err);
                self.toast(ToastKind::Error, Key::UpdateFailed);
                if let Some(info) = self.state.updates.available.as_ref() {
                    let _ = open::that(&info.url);
                }
            }
            AppEvent::SearchDone(results) => {
                self.fire_script(ScriptEvent::SearchDone(results.query.clone()));
                self.state.search.loading = false;
                for platform in &results.results {
                    self.state.profile.remember_many(&platform.tracks);
                }
                self.state.search.query = results.query.clone();
                self.state.search.results = Some(results);
                self.prefetch_search_covers();
                self.prefetch_artist_images();
                self.prefetch_album_covers();
                self.mark_dirty();
            }
            AppEvent::Resolved {
                track,
                path,
                temporary: _,
            } => {
                let is_current = self
                    .state
                    .now_playing
                    .as_ref()
                    .map(|current| current.id == track.id)
                    .unwrap_or(false);
                if !is_current {
                    return;
                }
                self.current_cache_path = Some(path.clone());
                match self.player.load(&path) {
                    Ok(()) => {
                        self.player.play();
                        self.state.playback.state = PlaybackState::Playing;
                        if self.state.playback.duration_secs.is_none() {
                            self.state.playback.duration_secs = track.duration_secs;
                        }
                        self.prefetch_next();
                    }
                    Err(err) => self.fail_playback(format!("{err}")),
                }
            }
            AppEvent::LinkImported { track, source } => {
                let _ = source;
                self.state.profile.remember(&track);
                self.state.search.input = track.page_url.clone().unwrap_or_default();
                self.toast(ToastKind::Success, Key::ImportedTrack);
                let tracks = vec![track.clone()];
                self.state.queue = tracks;
                self.state.queue_pos = 0;
                self.start_track(track);
                self.mark_dirty();
            }
            AppEvent::ResolveFailed { track, error } => {
                let is_current = self
                    .state
                    .now_playing
                    .as_ref()
                    .map(|current| current.id == track.id)
                    .unwrap_or(false);
                if is_current {
                    self.fail_playback(error);
                }
            }
            AppEvent::SearchMoreDone(results) => {
                self.state.search.loading_more = None;
                if let Some(current) = self.state.search.results.as_mut() {
                    for incoming in results.results {
                        if let Some(existing) = current
                            .results
                            .iter_mut()
                            .find(|r| r.platform == incoming.platform)
                        {
                            for track in incoming.tracks {
                                if !existing.tracks.iter().any(|t| t.id == track.id) {
                                    existing.tracks.push(track);
                                }
                            }
                            let tracks = existing.tracks.clone();
                            self.state.profile.remember_many(&tracks);
                        }
                    }
                    for incoming in results.artists {
                        if let Some(existing) = current
                            .artists
                            .iter_mut()
                            .find(|r| r.platform == incoming.platform)
                        {
                            for artist in incoming.artists {
                                if !existing.artists.iter().any(|a| a.id == artist.id) {
                                    existing.artists.push(artist);
                                }
                            }
                        }
                    }
                }
                self.mark_dirty();
            }
            AppEvent::ArtistDetailReady(detail) => {
                let same_artist = self
                    .state
                    .artist_page
                    .artist
                    .as_ref()
                    .zip(detail.artist.as_ref())
                    .map(|(page, incoming)| page.id == incoming.id)
                    .unwrap_or(true);
                if same_artist {
                    self.state.artist_page.loading = false;
                    self.state.artist_page.bio = detail.bio;
                    self.state.artist_page.hot_tracks = detail.hot_tracks;
                    self.state.artist_page.error = None;
                    let tracks = self.state.artist_page.hot_tracks.clone();
                    self.state.profile.remember_many(&tracks);
                    self.prefetch_artist_images();
                    self.mark_dirty();
                }
            }
            AppEvent::ArtistPortrait { artist_id, url } => {
                if let Some(results) = self.state.search.results.as_mut() {
                    for group in &mut results.artists {
                        for artist in &mut group.artists {
                            if artist.id == artist_id && artist.image_url.is_none() {
                                artist.image_url = Some(url.clone());
                            }
                        }
                    }
                }
                if let Some(artist) = self.state.artist_page.artist.as_mut() {
                    if artist.id == artist_id && artist.image_url.is_none() {
                        artist.image_url = Some(url);
                    }
                }
                self.prefetch_artist_images();
                ctx.request_repaint();
            }
            AppEvent::CoverPrefetched => {
                ctx.request_repaint();
            }
            AppEvent::ArtistImageReady => {
                ctx.request_repaint();
            }
            AppEvent::AlbumDetailReady(detail) => {
                let same_album = self
                    .state
                    .album_page
                    .album
                    .as_ref()
                    .zip(detail.album.as_ref())
                    .map(|(page, incoming)| page.id == incoming.id)
                    .unwrap_or(true);
                if same_album {
                    self.state.album_page.loading = false;
                    self.state.album_page.album = detail.album;
                    self.state.album_page.tracks = detail.tracks;
                    self.state.album_page.bio = detail.bio;
                    self.state.album_page.series = detail.series;
                    self.state.album_page.error = None;
                    let tracks = self.state.album_page.tracks.clone();
                    self.state.profile.remember_many(&tracks);
                    self.prefetch_page_covers();
                    self.prefetch_album_covers();
                    self.mark_dirty();
                }
            }
            AppEvent::SearchCoverReady { generation, path } => {
                if generation != self.search_generation {
                    // A stale prefetch from an earlier search/query.
                    let _ = std::fs::remove_file(path);
                } else if !self.search_cover_paths.contains(&path) {
                    self.search_cover_paths.push(path);
                    ctx.request_repaint();
                }
            }
            AppEvent::CoverReady {
                track_id,
                path,
                palette,
            } => {
                if self
                    .state
                    .now_playing
                    .as_ref()
                    .map(|track| track.id == track_id)
                    .unwrap_or(false)
                {
                    self.state.cover = CoverView {
                        track_id: Some(track_id),
                        path,
                        palette,
                        loading: false,
                    };
                    self.state.refresh_theme_target();
                    ctx.request_repaint();
                }
            }
            AppEvent::BackgroundDone { track_id, data } => {
                if self.state.background.track_id.as_deref() == Some(track_id.as_str())
                    || self
                        .state
                        .now_playing
                        .as_ref()
                        .map(|track| track.id == track_id)
                        .unwrap_or(false)
                {
                    self.state.background = BackgroundView {
                        track_id: Some(track_id),
                        loading: false,
                        data: Some(data),
                        error: None,
                    };
                }
            }
            AppEvent::BackgroundFailed { track_id, error } => {
                if self.state.background.track_id.as_deref() == Some(track_id.as_str()) {
                    self.state.background.loading = false;
                    self.state.background.error = Some(error);
                }
            }
            AppEvent::DownloadUpdate {
                track_id,
                state,
                progress,
                stage,
                path,
                error,
            } => {
                let finished = if state == JobState::Done {
                    self.state
                        .downloads
                        .jobs
                        .iter()
                        .find(|job| job.track.id == track_id)
                        .cloned()
                } else {
                    None
                };
                if let Some(job) = self.state.downloads.track_job_mut(&track_id) {
                    job.state = state;
                    job.progress = progress;
                    if !stage.is_empty() {
                        job.stage = stage;
                    }
                    if path.is_some() {
                        job.path = path;
                    }
                    if error.is_some() {
                        job.error = error;
                    }
                    if state == JobState::Done {
                        self.toast(ToastKind::Success, Key::DownloadDone);
                    } else if state == JobState::Failed {
                        self.toast(ToastKind::Error, Key::DownloadFailed);
                    }
                }
                if let Some(job) = finished {
                    if let Some(path) = job.path {
                        self.fire_script(ScriptEvent::DownloadDone(job.track, path));
                    }
                }
            }
            AppEvent::LibraryScanned(tracks) => {
                self.state.library.scanning = false;
                self.state.library.scanned_once = true;
                self.state.library.tracks = tracks;
                self.merge_local_files_into_library();
                let count = self.state.library.tracks.len();
                let text = format!("{} {}", count, self.state.tr(Key::TracksCount));
                self.toast(ToastKind::Info, TextOrKey::Text(text));
                self.fire_script(ScriptEvent::LibraryScanned(count));
                self.mark_dirty();
            }
            AppEvent::ToolsDetected(status) => {
                let ytdlp_missing = status.ytdlp_path.is_none();
                if status.ytdlp_path.is_some() {
                    self.resolve_config.ytdlp = status.ytdlp_path.clone();
                }
                self.state.tooling = status;
                if ytdlp_missing {
                    self.toast(ToastKind::Error, Key::ErrorYtDlpMissing);
                }
            }
            AppEvent::RunCommand(command) => {
                self.dispatch(command);
            }
            AppEvent::ScriptToast(text) => {
                self.toast(ToastKind::Info, TextOrKey::Text(text));
            }
            AppEvent::ScriptBinding(action) => {
                self.run_binding_action(action);
            }
            AppEvent::PlaybackEnded => {
                self.player.clear_ended();
                self.advance_queue(1, true);
            }
            AppEvent::Toast { kind, text } => {
                self.toast(kind, TextOrKey::Text(text));
            }
        }
    }

    // -----------------------------------------------------------------------
    // Playback
    // -----------------------------------------------------------------------

    fn start_track(&mut self, track: Track) {
        // Pressing play repeatedly while a track is still resolving must not
        // spawn another resolve job or restart playback.
        if self.same_track_loading(&track) {
            return;
        }
        if let Some(previous) = self.state.now_playing.clone() {
            if previous.id != track.id {
                self.fire_script(ScriptEvent::TrackEnd(previous));
            }
        }
        // Stop the previous audio right away: a track that is still being
        // resolved must not overlap with the old song (user report: "switching
        // to the next track should pause if the audio has not loaded yet").
        self.player.stop();
        if self.state.profile.settings.cache_limit_mb == 0 {
            if let Some(old_path) = self.current_cache_path.take() {
                let _ = std::fs::remove_file(old_path);
            }
        }
        self.state.now_playing = Some(track.clone());
        self.scripts.set_now_playing(Some(track.clone()));
        self.state.remember_playing(&track);
        self.state.playback = PlaybackStatus {
            state: PlaybackState::Loading,
            position_secs: 0.0,
            duration_secs: track.duration_secs,
            volume: self.state.playback.volume,
            error: None,
            ended: false,
        };
        self.state.now_playing_tab = NowPlayingTab::Track;
        self.state.background = BackgroundView::default();
        self.state.cover = CoverView {
            track_id: Some(track.id.clone()),
            loading: true,
            ..Default::default()
        };
        self.load_cover(track.clone());
        self.spawn_background_if_needed(track.clone());
        self.mark_dirty();
        self.fire_script(ScriptEvent::TrackStart(track.clone()));

        if let Some(path) = track.local_path.clone() {
            match self.player.load(&path) {
                Ok(()) => {
                    let status = self.player.status();
                    self.player.play();
                    self.state.playback.state = PlaybackState::Playing;
                    if status.duration_secs.is_some() {
                        self.state.playback.duration_secs = status.duration_secs;
                    }
                    self.prefetch_next();
                }
                Err(err) => self.fail_playback(format!("{err}")),
            }
            return;
        }

        let config = self.resolve_config.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || match resolver::resolve(&config, &track) {
            Ok(resolved) => {
                let _ = tx.send(AppEvent::Resolved {
                    track,
                    path: resolved.path,
                    temporary: resolved.temporary,
                });
            }
            Err(err) => {
                let _ = tx.send(AppEvent::ResolveFailed {
                    track,
                    error: err.to_string(),
                });
            }
        });
    }

    fn prefetch_next(&mut self) {
        if self.demo || self.state.profile.settings.cache_limit_mb == 0 {
            return;
        }
        let Some(next) = self.peek_next_track() else {
            return;
        };
        if next.local_path.is_some() {
            return;
        }
        let config = self.resolve_config.clone();
        std::thread::spawn(move || {
            // Warm the on-disk cache; errors are fine to ignore here.
            let _ = resolver::resolve(&config, &next);
        });
    }

    fn peek_next_track(&self) -> Option<Track> {
        let queue = &self.state.queue;
        if queue.is_empty() {
            return None;
        }
        if self.state.repeat == RepeatMode::One {
            return queue.get(self.state.queue_pos).cloned();
        }
        let next = self.state.queue_pos + 1;
        if next < queue.len() {
            queue.get(next).cloned()
        } else if self.state.repeat == RepeatMode::All {
            queue.first().cloned()
        } else {
            None
        }
    }

    fn advance_queue(&mut self, direction: i32, automatic: bool) {
        let queue_len = self.state.queue.len();
        if queue_len == 0 {
            self.player.stop();
            self.state.playback.state = PlaybackState::Idle;
            return;
        }
        if automatic && self.state.repeat == RepeatMode::One {
            if let Some(track) = self.state.queue.get(self.state.queue_pos).cloned() {
                self.start_track(track);
            }
            return;
        }
        let current = self.state.queue_pos as i32;
        let mut next = current + direction;
        if direction > 0 && self.state.shuffle {
            next = self.random_index(queue_len, self.state.queue_pos) as i32;
        }
        if next >= queue_len as i32 {
            if self.state.repeat == RepeatMode::Off && automatic {
                self.player.stop();
                self.state.playback.state = PlaybackState::Idle;
                return;
            }
            next = 0;
        } else if next < 0 {
            next = queue_len as i32 - 1;
        }
        self.state.queue_pos = next as usize;
        let track = self.state.queue[next as usize].clone();
        self.start_track(track);
    }

    fn random_index(&mut self, len: usize, avoid: usize) -> usize {
        if len <= 1 {
            return 0;
        }
        // xorshift64 — deterministic, allocation free, good enough for shuffle.
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        let mut index = (x % len as u64) as usize;
        if index == avoid {
            index = (index + 1) % len;
        }
        index
    }

    /// Execute a scriptable / keyboard-driven action. The same handler serves
    /// hard-coded shortcuts, Lua `echoport.command(...)` calls and custom
    /// actions registered by scripts.
    pub fn run_binding_action(&mut self, action: crate::scripting::BindingAction) {
        use crate::scripting::BindingAction as A;
        match action {
            A::PlayPause => self.dispatch(Command::TogglePlayPause),
            A::Next => self.dispatch(Command::Next),
            A::Previous => self.dispatch(Command::Previous),
            A::SeekForward => {
                let step = self.state.seek_step;
                self.dispatch(Command::SeekRelative(step));
            }
            A::SeekBackward => {
                let step = self.state.seek_step;
                self.dispatch(Command::SeekRelative(-step));
            }
            A::VolumeUp => {
                let next = (self.state.playback.volume + self.state.volume_step).min(1.0);
                self.dispatch(Command::SetVolume(next));
            }
            A::VolumeDown => {
                let next = (self.state.playback.volume - self.state.volume_step).max(0.0);
                self.dispatch(Command::SetVolume(next));
            }
            A::ToggleQueue => self.dispatch(Command::ToggleQueuePanel),
            A::ToggleFavorite => {
                if let Some(track) = self.state.now_playing.clone() {
                    self.dispatch(Command::ToggleFavorite(Box::new(track)));
                }
            }
            A::ToggleShuffle => self.dispatch(Command::ToggleShuffle),
            A::CycleRepeat => self.dispatch(Command::CycleRepeat),
            A::OpenSearch => self.set_page(Page::Search),
            A::OpenSettings => self.set_page(Page::Settings),
            A::OpenHome => self.set_page(Page::Home),
            A::Custom(id) => self.run_script_action(&id),
        }
    }

    /// Fire a lifecycle event to Lua handlers and compiled-in plugins.
    fn fire_script(&mut self, event: ScriptEvent) {
        let tx = self.tx.clone();
        self.plugins.fire(&event);
        self.scripts.fire(&event, &tx);
    }

    fn refresh_script_status(&mut self) {
        let config = self.scripts.config();
        self.state.script = ScriptStatus {
            loaded: config.loaded,
            config_file: self.paths.config_file.clone(),
            plugins_dir: self.paths.plugins_dir.clone(),
            plugins: self.scripts.loaded_plugins(),
            actions: config.actions.clone(),
            warnings: config.warnings.clone(),
            bindings: config.bindings.clone(),
        };
    }

    fn reload_scripts(&mut self) {
        self.scripts.reload(self.tx.clone());
        let config = self.scripts.config().clone();
        apply_script_config_to_profile(&mut self.state.profile, &config);
        apply_script_config_to_state(&mut self.state, &config, false);
        self.player.set_volume(self.state.playback.volume);
        self.refresh_script_status();
        let actions = self.state.script.actions.len();
        let warnings = self.state.script.warnings.len();
        let text = if warnings == 0 {
            format!("Loaded {actions} script action(s)")
        } else {
            format!("Loaded {actions} action(s), {warnings} warning(s)")
        };
        self.toast(ToastKind::Info, TextOrKey::Text(text));
        self.mark_dirty();
    }

    fn run_script_action(&mut self, action_id: &str) {
        let tx = self.tx.clone();
        if self.scripts.run_action(action_id, &tx) {
            self.refresh_script_status();
        } else {
            self.toast(
                ToastKind::Error,
                TextOrKey::Text(format!("Unknown script action: {action_id}")),
            );
        }
    }

    fn toggle_play_pause(&mut self) {
        match self.state.playback.state {
            PlaybackState::Playing => {
                self.player.pause();
                self.state.playback.state = PlaybackState::Paused;
                self.fire_script(ScriptEvent::PlaybackPaused);
            }
            // While the resolver/decoder is still working, repeated presses of
            // any play control are ignored — the track starts once when ready.
            PlaybackState::Loading => {}
            PlaybackState::Paused => {
                self.player.play();
                self.state.playback.state = PlaybackState::Playing;
                self.fire_script(ScriptEvent::PlaybackResumed);
            }
            _ => {
                if let Some(track) = self.state.queue.get(self.state.queue_pos).cloned() {
                    self.start_track(track);
                } else if let Some(track) = self.state.now_playing.clone() {
                    self.start_track(track);
                }
            }
        }
    }

    fn fail_playback(&mut self, error: String) {
        log::warn!("playback failed: {error}");
        let lowered = error.to_ascii_lowercase();
        let friendly = if lowered.contains("412")
            || lowered.contains("precondition")
            || lowered.contains("http error 567")
        {
            self.state.tr(Key::ErrorBilibiliBlocked).to_owned()
        } else {
            error.clone()
        };
        self.state.playback.state = PlaybackState::Error;
        self.state.playback.error = Some(friendly);
        self.toast(ToastKind::Error, Key::PlaybackError);
    }

    // -----------------------------------------------------------------------
    // Secondary workers
    // -----------------------------------------------------------------------

    fn start_search(&mut self, query: String, platforms: Vec<Platform>) {
        let query = query.trim().to_owned();
        if query.is_empty() {
            return;
        }
        self.clear_search_covers();
        let search_started = std::time::Instant::now();
        self.state.search.query = query.clone();
        self.state.search.input = query.clone();
        self.state.search.loading = true;
        self.state.search.error = None;
        self.state.search.results = None;
        self.ctx.request_repaint();
        let http = self.http.clone();
        let config = self.resolve_config.clone();
        let tx = self.tx.clone();
        let limit = self.state.profile.settings.results_per_platform;
        let http_platforms: Vec<Platform> = platforms
            .iter()
            .copied()
            .filter(|platform| matches!(platform, Platform::AppleMusic | Platform::Netease))
            .collect();
        std::thread::spawn(move || {
            // Track search and the HTTP artist catalogues run side by side.
            // Creator cards for yt-dlp platforms are derived from the track
            // results, avoiding a duplicated yt-dlp search per platform.
            let (results, mut artists, albums) = std::thread::scope(|scope| {
                let tracks_handle =
                    scope.spawn(|| providers::search(&http, &config, &query, &platforms, limit));
                let artists_handle = scope.spawn(|| {
                    providers::search_artists(&http, &config, &query, &http_platforms, limit)
                });
                let albums_handle =
                    scope.spawn(|| providers::search_albums(&http, &query, &platforms, limit));
                (
                    tracks_handle.join().unwrap_or_default(),
                    artists_handle.join().unwrap_or_default(),
                    albums_handle.join().unwrap_or_default(),
                )
            });
            artists.extend(providers::artists_from_tracks(&query, &results, limit));
            log::info!(
                "search {:?} completed in {:?}: {} groups, {} artist groups",
                query,
                search_started.elapsed(),
                results.len(),
                artists.len()
            );
            let _ = tx.send(AppEvent::SearchDone(SearchResults {
                query,
                results,
                artists,
                albums,
            }));
        });
    }

    fn apply_playlist_cover(&mut self, id: &str, source: Option<std::path::PathBuf>) {
        let Some(playlist) = self.state.profile.playlist_mut(id) else {
            return;
        };
        let old = playlist.cover_path.clone();
        let target = source.and_then(|source| {
            let extension = source
                .extension()
                .and_then(|extension| extension.to_str())
                .unwrap_or("png")
                .to_ascii_lowercase();
            let directory = self.paths.data_dir.join("playlist-covers");
            std::fs::create_dir_all(&directory).ok()?;
            let target = directory.join(format!("{id}.{extension}"));
            std::fs::copy(&source, &target).ok()?;
            Some(target)
        });
        if let Some(playlist) = self.state.profile.playlist_mut(id) {
            playlist.cover_path = target.clone();
            playlist.touch();
        }
        if let Some(old) = old {
            if target.as_deref() != Some(old.as_path())
                && old.starts_with(self.paths.data_dir.join("playlist-covers"))
            {
                let _ = std::fs::remove_file(old);
            }
        }
        self.mark_dirty();
    }

    fn spawn_tool_detection(&self) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let ytdlp = resolver::detect_ytdlp();
            let version = ytdlp.as_deref().and_then(resolver::tool_version);
            let ffmpeg = resolver::detect_ffmpeg();
            let _ = tx.send(AppEvent::ToolsDetected(ToolingStatus {
                ytdlp_path: ytdlp,
                ytdlp_version: version,
                ffmpeg_path: ffmpeg,
                checking: false,
            }));
        });
    }

    fn spawn_library_scan(&self) {
        if self.demo {
            return;
        }
        let dirs = self.state.profile.settings.music_dirs.clone();
        if dirs.is_empty() {
            return;
        }
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let tracks = library::scan_dirs(&dirs);
            let _ = tx.send(AppEvent::LibraryScanned(tracks));
        });
    }

    fn clear_search_covers(&mut self) {
        if self.search_cover_paths.is_empty() {
            return;
        }
        self.search_generation = self.search_generation.wrapping_add(1);
        let paths = std::mem::take(&mut self.search_cover_paths);
        log::info!("clearing {} search cover cache files", paths.len());
        for path in &paths {
            let _ = std::fs::remove_file(path);
        }
        self.ui.covers.forget_paths(&paths);
        self.ctx.request_repaint();
    }

    fn merge_local_files_into_library(&mut self) {
        for path in self.state.profile.local_files.clone() {
            if !path.is_file() {
                continue;
            }
            let track = Track::from_local_path(path);
            if !self
                .state
                .library
                .tracks
                .iter()
                .any(|kept| kept.id == track.id)
            {
                self.state.profile.remember(&track);
                self.state.library.tracks.push(track);
            }
        }
    }

    fn save_dropped_to_library(&mut self, tracks: Vec<Track>) {
        let mut added = 0usize;
        for track in &tracks {
            let Some(path) = track.local_path.clone() else {
                continue;
            };
            if !self
                .state
                .profile
                .local_files
                .iter()
                .any(|kept| kept == &path)
            {
                self.state.profile.local_files.push(path);
                added += 1;
            }
            self.state.profile.remember(track);
            if !self
                .state
                .library
                .tracks
                .iter()
                .any(|kept| kept.id == track.id)
            {
                self.state.library.tracks.push(track.clone());
            }
        }
        self.state.drop_prompt = None;
        self.toast(
            ToastKind::Success,
            TextOrKey::Text(format!("{added} local file(s) saved to Library")),
        );
        self.mark_dirty();
    }

    fn handle_dropped_files(&mut self, files: Vec<egui::DroppedFile>) {
        let mut tracks: Vec<Track> = Vec::new();
        for file in files {
            let Some(path) = file.path else { continue };
            if !is_audio_path(&path) {
                continue;
            }
            let track = Track::from_local_path(path);
            if !tracks.iter().any(|existing| existing.id == track.id) {
                tracks.push(track);
            }
        }
        if tracks.is_empty() {
            self.toast(ToastKind::Error, Key::DropAudioOnly);
            return;
        }
        self.dispatch(Command::PlayTracks(tracks.clone()));
        self.state.drop_prompt = Some(tracks);
        self.ctx.request_repaint();
    }

    fn prefetch_page_covers(&mut self) {
        let mut tracks: Vec<Track> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut push = |track: &Track| {
            if track.art_url.is_some() && seen.insert(track.id.clone()) {
                tracks.push(track.clone());
            }
        };
        match self.state.page {
            Page::Home | Page::Profile => {
                for entry in self.state.profile.history.iter().rev().take(30) {
                    if let Some(track) = self.state.profile.track(&entry.track_id) {
                        push(track);
                    }
                }
                for id in self.state.profile.favorites.iter().take(20) {
                    if let Some(track) = self.state.profile.track(id) {
                        push(track);
                    }
                }
                for playlist in self.state.profile.playlists.iter().take(10) {
                    for track in self.state.profile.playlist_tracks(&playlist.id) {
                        push(&track);
                        break;
                    }
                }
            }
            Page::Playlists => {
                for playlist in self.state.profile.playlists.iter().take(10) {
                    for track in self.state.profile.playlist_tracks(&playlist.id) {
                        push(&track);
                        break;
                    }
                }
            }
            Page::Album => {
                for track in self.state.album_page.tracks.iter().take(60) {
                    push(track);
                }
            }
            _ => {}
        }
        if self.state.page == Page::Profile {
            self.prefetch_album_covers();
        }
        let mut queue = Vec::new();
        for track in tracks {
            if self.prefetched_covers.insert(track.id.clone()) {
                queue.push(track);
            }
        }
        if queue.is_empty() {
            return;
        }
        let art = self.art.clone();
        let http = self.http.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            for track in queue {
                if art.resolve_thumb(&http, &track).is_some() {
                    let _ = tx.send(AppEvent::CoverPrefetched);
                }
            }
        });
    }

    fn prefetch_album_covers(&mut self) {
        let mut albums: Vec<crate::model::Album> = Vec::new();
        if let Some(results) = self.state.search.results.as_ref() {
            for group in &results.albums {
                albums.extend(group.albums.iter().cloned());
            }
        }
        albums.extend(self.state.profile.favorite_albums.iter().cloned());
        if let Some(album) = self.state.album_page.album.clone() {
            albums.push(album);
        }
        albums.extend(self.state.album_page.series.iter().cloned());
        albums.sort_by(|a, b| a.id.cmp(&b.id));
        albums.dedup_by(|a, b| a.id == b.id);
        let art = self.art.clone();
        let http = self.http.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            for album in albums {
                let Some(url) = album.art_url.as_deref() else {
                    continue;
                };
                if art.resolve_url(&http, url).is_some() {
                    let _ = tx.send(AppEvent::CoverPrefetched);
                }
            }
        });
    }

    fn prefetch_artist_images(&mut self) {
        let mut urls: Vec<String> = Vec::new();
        let mut missing: Vec<crate::model::Artist> = Vec::new();
        if let Some(results) = self.state.search.results.as_ref() {
            for group in &results.artists {
                for artist in &group.artists {
                    if let Some(url) = artist.image_url.as_deref() {
                        let url = url.trim();
                        if !url.is_empty() && !urls.iter().any(|kept| kept == url) {
                            urls.push(url.to_owned());
                        }
                    } else if artist.platform == Platform::AppleMusic {
                        missing.push(artist.clone());
                    }
                }
            }
        }
        if let Some(url) = self
            .state
            .artist_page
            .artist
            .as_ref()
            .and_then(|artist| artist.image_url.as_deref())
        {
            let url = url.trim();
            if !url.is_empty() && !urls.iter().any(|kept| kept == url) {
                urls.push(url.to_owned());
            }
        }
        if urls.is_empty() && missing.is_empty() {
            return;
        }
        let art = self.art.clone();
        let http = self.http.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            for artist in missing {
                if let Some(url) = providers::artist_portrait(&http, &artist) {
                    let _ = tx.send(AppEvent::ArtistPortrait {
                        artist_id: artist.id,
                        url,
                    });
                }
            }
            for url in urls {
                if art.resolve_url(&http, &url).is_some() {
                    let _ = tx.send(AppEvent::ArtistImageReady);
                }
            }
        });
    }

    fn prefetch_search_covers(&mut self) {
        let Some(results) = self.state.search.results.as_ref() else {
            return;
        };
        let mut seen = std::collections::HashSet::new();
        let mut tracks: Vec<Track> = Vec::new();
        for group in &results.results {
            for track in group.tracks.iter().take(12) {
                if track.art_url.is_some() && seen.insert(track.id.clone()) {
                    tracks.push(track.clone());
                }
            }
        }
        tracks.truncate(30);
        if tracks.is_empty() {
            return;
        }
        let art = self.art.clone();
        let http = self.http.clone();
        let tx = self.tx.clone();
        let generation = self.search_generation;
        std::thread::spawn(move || {
            for track in tracks {
                if let Some(path) = art.resolve_thumb(&http, &track) {
                    let _ = tx.send(AppEvent::SearchCoverReady { generation, path });
                }
            }
        });
    }

    fn load_cover(&mut self, track: Track) {
        let art = self.art.clone();
        let http = self.http.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let path = if let Some(local) = track.local_path.as_ref() {
                library::embedded_cover(local).and_then(|bytes| {
                    let target = art.embedded_path(local);
                    std::fs::write(&target, bytes).ok()?;
                    Some(target)
                })
            } else {
                art.resolve(&http, &track)
            };
            let palette = path.as_deref().and_then(theme::extract_palette_file);
            let _ = tx.send(AppEvent::CoverReady {
                track_id: track.id.clone(),
                path,
                palette,
            });
        });
    }

    fn spawn_background_if_needed(&mut self, track: Track) {
        self.state.background = BackgroundView {
            track_id: Some(track.id.clone()),
            loading: true,
            data: None,
            error: None,
        };
        let http = self.http.clone();
        let config = self.resolve_config.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let data = crate::background::fetch(&http, &config, &track);
            let _ = tx.send(AppEvent::BackgroundDone {
                track_id: track.id.clone(),
                data,
            });
        });
    }

    fn start_background(&mut self, track: Track) {
        if self.state.background.track_id.as_deref() == Some(track.id.as_str())
            && (self.state.background.loading || self.state.background.data.is_some())
        {
            self.state.now_playing_tab = NowPlayingTab::Background;
            return;
        }
        self.state.now_playing_tab = NowPlayingTab::Background;
        self.spawn_background_if_needed(track);
    }

    fn start_downloads(&mut self, tracks: Vec<Track>) {
        if tracks.is_empty() {
            return;
        }
        for track in tracks {
            if let Some(job) = self
                .state
                .downloads
                .jobs
                .iter()
                .find(|job| job.track.id == track.id)
            {
                if matches!(
                    job.state,
                    JobState::Queued | JobState::Running | JobState::Done
                ) {
                    continue;
                }
            }
            self.state
                .downloads
                .jobs
                .retain(|job| job.track.id != track.id);
            self.state
                .downloads
                .jobs
                .insert(0, DownloadJob::queued(track.clone()));
            self.state.profile.remember(&track);
            let track_id = track.id.clone();
            let config = self.resolve_config.clone();
            let options = DownloadOptions {
                dir: self.state.profile.settings.download_dir.clone(),
                format: self.state.profile.settings.download_format,
                embed_metadata: true,
            };
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let mut progress = |update: DownloadProgress| {
                    let _ = tx.send(AppEvent::DownloadUpdate {
                        track_id: track_id.clone(),
                        state: JobState::Running,
                        progress: update.fraction.unwrap_or(0.0),
                        stage: update.stage,
                        path: None,
                        error: None,
                    });
                };
                match download::download_track(&track, &config, &options, &mut progress) {
                    Ok(path) => {
                        let _ = tx.send(AppEvent::DownloadUpdate {
                            track_id,
                            state: JobState::Done,
                            progress: 1.0,
                            stage: String::new(),
                            path: Some(path),
                            error: None,
                        });
                    }
                    Err(err) => {
                        let _ = tx.send(AppEvent::DownloadUpdate {
                            track_id,
                            state: JobState::Failed,
                            progress: 0.0,
                            stage: String::new(),
                            path: None,
                            error: Some(err),
                        });
                    }
                }
            });
            self.toast(ToastKind::Info, Key::DownloadQueued);
        }
        self.mark_dirty();
    }

    fn clear_cache(&mut self) {
        let _ = self.art.clear();
        if self.paths.audio_cache_dir.exists() {
            let _ = std::fs::remove_dir_all(&self.paths.audio_cache_dir);
        }
        let _ = std::fs::create_dir_all(&self.paths.audio_cache_dir);
        self.toast(ToastKind::Success, Key::ClearCacheDone);
    }

    fn switch_profile(&mut self, name: &str) {
        self.save_now();
        if let Some(profile) = self.store.load(name) {
            self.state.lang = profile.settings.language;
            self.state.profile = profile;
            self.state.profiles = self.store.list();
            self.state.playback = PlaybackStatus {
                volume: self.state.profile.settings.volume,
                ..Default::default()
            };
            self.state.theme_mode = self.state.profile.settings.theme_mode;
            self.state.now_playing = None;
            self.state.queue.clear();
            self.state.queue_pos = 0;
            self.state.selected_playlist = None;
            self.state.page = Page::Profile;
            self.state.cover = CoverView::default();
            self.state.refresh_theme_target();
            self.player.stop();
            self.mark_dirty();
        }
    }

    fn open_embedded_doc(&self, name: &str, html: &str) {
        let dir = self.paths.data_dir.join("docs");
        if let Err(err) = std::fs::create_dir_all(&dir) {
            log::warn!("could not create docs dir: {err}");
            return;
        }
        let path = dir.join(name);
        if let Err(err) = std::fs::write(&path, html) {
            log::warn!("could not write embedded doc {}: {err}", path.display());
            return;
        }
        if let Err(err) = open::that(&path) {
            log::warn!("could not open {}: {err}", path.display());
        }
    }

    fn start_update_download(&mut self) {
        if self.state.updates.downloading {
            return;
        }
        let Some(info) = self.state.updates.available.clone() else {
            return;
        };
        let Some(asset) = crate::update::asset_for_current_platform(&info).cloned() else {
            self.toast(ToastKind::Info, Key::UpdatePageOpened);
            let _ = open::that(&info.url);
            return;
        };
        if !asset.url.starts_with("https://github.com/")
            || asset
                .url
                .chars()
                .any(|c| c.is_whitespace() || c.is_control())
        {
            self.toast(ToastKind::Error, Key::UpdateFailed);
            return;
        }
        let checksum_url = crate::update::checksum_asset(&info, &asset).map(|a| a.url.clone());
        let dir = self.paths.cache_dir.join("updates");
        let dest = dir.join(&asset.name);
        self.state.updates.downloading = true;
        self.state.updates.progress = Some(0.0);
        self.splash_done = true;
        self.toast(ToastKind::Info, Key::UpdateDownloading);
        self.ctx.request_repaint();
        let http = self.http.clone();
        let tx = self.tx.clone();
        let progress_tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<(), String> {
                let _ = std::fs::create_dir_all(&dir);
                http.download(&asset.url, &[], &dest, |written, total| {
                    let _ = progress_tx.send(AppEvent::UpdateProgress(crate::net::percent(
                        written, total,
                    )));
                })
                .map_err(|err| err.to_string())?;
                if let Some(checksum_url) = checksum_url {
                    let expected = http
                        .get_text(&checksum_url, &[])
                        .map_err(|err| err.to_string())?;
                    let expected = expected
                        .split_whitespace()
                        .next()
                        .unwrap_or_default()
                        .to_ascii_lowercase();
                    if expected.len() != 64 || !expected.chars().all(|c| c.is_ascii_hexdigit()) {
                        return Err("invalid published checksum".to_owned());
                    }
                    let mut file = std::fs::File::open(&dest).map_err(|err| err.to_string())?;
                    let mut hasher = Sha256::new();
                    let mut buffer = vec![0u8; 64 * 1024];
                    loop {
                        let read = std::io::Read::read(&mut file, &mut buffer)
                            .map_err(|err| err.to_string())?;
                        if read == 0 {
                            break;
                        }
                        hasher.update(&buffer[..read]);
                    }
                    let actual = hasher
                        .finalize()
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect::<String>();
                    if actual != expected {
                        let _ = std::fs::remove_file(&dest);
                        return Err("download checksum mismatch".to_owned());
                    }
                }
                Ok(())
            })();
            match result {
                Ok(()) => {
                    let _ = tx.send(AppEvent::UpdateDownloaded(dest));
                }
                Err(err) => {
                    let _ = tx.send(AppEvent::UpdateDownloadFailed(err));
                }
            }
        });
    }

    #[cfg(target_os = "windows")]
    fn apply_update(&mut self, path: std::path::PathBuf) {
        let Ok(current) = std::env::current_exe() else {
            let _ = open::that(path);
            return;
        };
        let Some(dir) = current.parent().map(|parent| parent.to_path_buf()) else {
            let _ = open::that(path);
            return;
        };
        let script = self.paths.cache_dir.join("updates/apply-update.ps1");
        let script_text = r#"
param([int]$AppPid, [string]$Installer, [string]$InstallDir)
Start-Sleep -Milliseconds 400
try { Wait-Process -Id $AppPid -Timeout 90 -ErrorAction SilentlyContinue } catch {}
if ($Installer.ToLower().EndsWith('.msi')) {
    Start-Process -FilePath 'msiexec.exe' -ArgumentList '/i', ('"' + $Installer + '"'), '/qn' -Wait
} elseif ($Installer.ToLower().Contains('inno-setup')) {
    Start-Process -FilePath $Installer -ArgumentList ('/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /DIR="' + $InstallDir + '"') -Wait
} else {
    Start-Process -FilePath $Installer -ArgumentList ('/S /D="' + $InstallDir + '"') -Wait
}
Start-Process -FilePath (Join-Path $InstallDir 'echoport.exe')
Remove-Item -LiteralPath $Installer -Force -ErrorAction SilentlyContinue
Remove-Item -LiteralPath $MyInvocation.MyCommand.Path -Force -ErrorAction SilentlyContinue
"#;
        if let Err(err) = std::fs::create_dir_all(self.paths.cache_dir.join("updates")) {
            log::warn!("could not create update dir: {err}");
            let _ = open::that(&path);
            return;
        }
        if std::fs::write(&script, script_text).is_err() {
            let _ = open::that(&path);
            return;
        }
        let pid = std::process::id();
        let mut cmd = crate::process::quiet("powershell.exe");
        cmd.args([
            "-NoProfile",
            "-WindowStyle",
            "Hidden",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(&script)
        .args(["-AppPid", &pid.to_string()])
        .arg("-Installer")
        .arg(&path)
        .arg("-InstallDir")
        .arg(&dir);
        if let Err(err) = cmd.spawn() {
            log::warn!("could not start update helper: {err}");
            let _ = open::that(&path);
            return;
        }
        self.quitting = true;
        self.save_now();
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    #[cfg(target_os = "linux")]
    fn apply_update(&mut self, path: std::path::PathBuf) {
        let Ok(current) = std::env::current_exe() else {
            let _ = open::that(&path);
            return;
        };
        let extract = self.paths.cache_dir.join("updates/extract");
        let _ = std::fs::remove_dir_all(&extract);
        if std::fs::create_dir_all(&extract).is_err() {
            let _ = open::that(&path);
            return;
        }
        let status = std::process::Command::new("tar")
            .args(["-xzf"])
            .arg(&path)
            .arg("-C")
            .arg(&extract)
            .status();
        let Ok(status) = status else {
            let _ = open::that(&path);
            return;
        };
        if !status.success() {
            let _ = open::that(&path);
            return;
        }
        let mut found = None;
        for entry in walkdir::WalkDir::new(&extract).max_depth(3) {
            let Ok(entry) = entry else { continue };
            if entry.file_type().is_file() && entry.file_name() == "echoport" {
                found = Some(entry.into_path());
                break;
            }
        }
        let Some(new_bin) = found else {
            let _ = open::that(&path);
            return;
        };
        let tmp = current.with_extension("update-new");
        if std::fs::copy(&new_bin, &tmp).is_err() {
            let _ = open::that(&path);
            return;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755));
        }
        if std::fs::rename(&tmp, &current).is_err() {
            let _ = std::fs::remove_file(&tmp);
            let _ = open::that(&path);
            return;
        }
        let _ = std::process::Command::new(&current)
            .env_remove("ECHOPORT_AUTO_UPDATE")
            .env_remove("ECHOPORT_CURRENT_VERSION")
            .spawn();
        self.quitting = true;
        self.save_now();
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    fn apply_update(&mut self, path: std::path::PathBuf) {
        let _ = open::that(path);
    }

    fn spawn_update_check(&mut self) {
        if self.state.updates.checking {
            return;
        }
        self.state.updates.checking = true;
        self.state.updates.error = None;
        self.ctx.request_repaint();
        let http = self.http.clone();
        let tx = self.tx.clone();
        let current = std::env::var("ECHOPORT_CURRENT_VERSION")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| self.version().to_owned());
        std::thread::spawn(move || {
            let result = crate::update::check(&http, &current);
            let _ = tx.send(AppEvent::UpdateCheckDone(result));
        });
    }

    // -----------------------------------------------------------------------
    // Artist pages and incremental search
    // -----------------------------------------------------------------------

    fn same_track_loading(&self, track: &Track) -> bool {
        self.state.playback.state == PlaybackState::Loading
            && self
                .state
                .now_playing
                .as_ref()
                .map(|current| current.id == track.id)
                .unwrap_or(false)
    }

    fn open_album(&mut self, album: crate::model::Album) {
        self.state.album_page = crate::state::AlbumPageView {
            album: Some(album.clone()),
            tracks: Vec::new(),
            bio: None,
            series: Vec::new(),
            loading: true,
            error: None,
        };
        self.state.page = Page::Album;
        self.ctx.request_repaint();
        let http = self.http.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let detail = providers::album_detail(&http, &album);
            let _ = tx.send(AppEvent::AlbumDetailReady(Box::new(detail)));
        });
    }

    fn open_artist(&mut self, artist: crate::model::Artist) {
        // Local history for this artist (case-insensitive name match).
        let needle = artist.name.to_lowercase();
        let mut local_tracks: Vec<Track> = self
            .state
            .profile
            .history
            .iter()
            .filter_map(|entry| self.state.profile.track(&entry.track_id).cloned())
            .filter(|track| track.artist.to_lowercase().contains(&needle))
            .collect();
        local_tracks.truncate(20);
        self.state.artist_page = crate::state::ArtistPageView {
            artist: Some(artist.clone()),
            bio: None,
            hot_tracks: Vec::new(),
            local_tracks,
            loading: true,
            error: None,
            platform: Some(artist.platform),
        };
        self.state.page = Page::Artist;
        self.ctx.request_repaint();
        let http = self.http.clone();
        let config = self.resolve_config.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let detail = providers::artist_detail(&http, &config, &artist);
            let _ = tx.send(AppEvent::ArtistDetailReady(Box::new(detail)));
        });
    }

    fn search_more(&mut self, platform: Platform) {
        let query = self.state.search.query.trim().to_owned();
        if query.is_empty() {
            return;
        }
        self.state.search.loading_more = Some(platform);
        self.ctx.request_repaint();
        let current = self
            .state
            .search
            .results
            .as_ref()
            .and_then(|results| results.results.iter().find(|r| r.platform == platform))
            .map(|r| r.tracks.len())
            .unwrap_or(0);
        let limit = current + self.state.profile.settings.results_per_platform;
        let http = self.http.clone();
        let config = self.resolve_config.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let results = providers::search(&http, &config, &query, &[platform], limit);
            let artists = if matches!(platform, Platform::AppleMusic | Platform::Netease) {
                providers::search_artists(&http, &config, &query, &[platform], 6)
            } else {
                providers::artists_from_tracks(&query, &results, 6)
            };
            let albums = providers::search_albums(&http, &query, &[platform], 6);
            let _ = tx.send(AppEvent::SearchMoreDone(SearchResults {
                query,
                results,
                artists,
                albums,
            }));
        });
    }

    // -----------------------------------------------------------------------
    // Profile avatar / bio and portable profile files
    // -----------------------------------------------------------------------

    fn set_profile_avatar(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title(self.state.tr(Key::ChooseAvatar))
            .add_filter("Image", &["png", "jpg", "jpeg", "webp"])
            .pick_file()
        else {
            return;
        };
        match encode_avatar_data_url(&path) {
            Ok(data) => {
                self.state.profile.avatar = Some(data);
                self.mark_dirty();
                self.toast(ToastKind::Success, Key::AvatarUpdated);
            }
            Err(err) => {
                log::warn!("avatar encode failed: {err}");
                self.toast(ToastKind::Error, Key::AvatarFailed);
            }
        }
    }

    fn export_profile(&mut self) {
        let share = ProfileShare::from_profile(&self.state.profile);
        let safe_name: String = self
            .state
            .profile
            .name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || matches!(c, '-' | '_' | ' ') {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let Some(path) = rfd::FileDialog::new()
            .set_title(self.state.tr(Key::ExportProfile))
            .add_filter("EchoPort profile", &["json"])
            .set_file_name(format!("{}.echoport-profile.json", safe_name.trim()))
            .save_file()
        else {
            return;
        };
        let result = serde_json::to_string_pretty(&share)
            .map_err(|err| err.to_string())
            .and_then(|json| std::fs::write(&path, json).map_err(|err| err.to_string()));
        match result {
            Ok(()) => self.toast(ToastKind::Success, Key::ProfileExportDone),
            Err(err) => {
                log::warn!("profile export failed: {err}");
                self.toast(ToastKind::Error, Key::ProfileExportFailed);
            }
        }
    }

    fn import_profile(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title(self.state.tr(Key::ImportProfile))
            .add_filter("EchoPort profile", &["json"])
            .pick_file()
        else {
            return;
        };
        let too_large = std::fs::metadata(&path)
            .map(|meta| meta.len() > 8 * 1024 * 1024)
            .unwrap_or(true);
        if too_large {
            self.toast(ToastKind::Error, Key::ProfileImportFailed);
            return;
        }
        let imported = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<ProfileShare>(&text).ok())
            .filter(|share| share.format == PROFILE_SHARE_FORMAT)
            .map(|mut share| {
                share.name = share.name.chars().take(64).collect();
                share.bio = share.bio.chars().take(500).collect();
                if share
                    .avatar
                    .as_ref()
                    .map(|avatar| avatar.len() > 2 * 1024 * 1024)
                    .unwrap_or(false)
                {
                    share.avatar = None;
                }
                share.playlists.truncate(500);
                share.tracks = share.tracks.into_iter().take(20_000).collect();
                share
            });
        let Some(share) = imported else {
            self.toast(ToastKind::Error, Key::ProfileImportFailed);
            return;
        };
        let base_name = if share.name.trim().is_empty() {
            "Imported profile".to_owned()
        } else {
            share.name.trim().to_owned()
        };
        let mut name = base_name.clone();
        let mut counter = 2;
        while self.state.profiles.iter().any(|existing| existing == &name) {
            name = format!("{base_name} ({counter})");
            counter += 1;
        }
        let mut profile = share.into_profile();
        profile.name = name.clone();
        if let Err(err) = self.store.save(&profile) {
            log::warn!("profile import save failed: {err}");
            self.toast(ToastKind::Error, Key::ProfileImportFailed);
            return;
        }
        self.state.profiles = self.store.list();
        self.switch_profile(&name);
        self.toast(ToastKind::Success, Key::ProfileImportDone);
    }

    // -----------------------------------------------------------------------
    // Playlist sharing (metadata only, no audio)
    // -----------------------------------------------------------------------

    fn export_playlist(&mut self, playlist_id: &str) {
        let Some(playlist) = self.state.profile.playlist(playlist_id).cloned() else {
            self.toast(ToastKind::Error, Key::ErrorPlaylistMissing);
            return;
        };
        let tracks = self.state.profile.playlist_tracks(playlist_id);
        if tracks.is_empty() {
            self.toast(ToastKind::Info, Key::EmptyPlaylist);
            return;
        }
        let share = PlaylistShare::new(playlist.name.clone(), playlist.description.clone(), tracks);
        let safe_name: String = playlist
            .name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || matches!(c, '-' | '_' | ' ') {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let Some(path) = rfd::FileDialog::new()
            .set_title(self.state.tr(Key::ExportPlaylist))
            .add_filter("EchoPort playlist", &["json"])
            .set_file_name(format!("{} .echoport.json", safe_name.trim()))
            .save_file()
        else {
            return;
        };
        let result = serde_json::to_string_pretty(&share)
            .map_err(|err| err.to_string())
            .and_then(|json| std::fs::write(&path, json).map_err(|err| err.to_string()));
        match result {
            Ok(()) => self.toast(ToastKind::Success, Key::PlaylistExportDone),
            Err(err) => {
                log::warn!("playlist export failed: {err}");
                self.toast(ToastKind::Error, Key::PlaylistExportFailed);
            }
        }
    }

    fn import_playlist(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title(self.state.tr(Key::ImportPlaylist))
            .add_filter("EchoPort playlist", &["json"])
            .pick_file()
        else {
            return;
        };
        let too_large = std::fs::metadata(&path)
            .map(|meta| meta.len() > 8 * 1024 * 1024)
            .unwrap_or(true);
        if too_large {
            self.toast(ToastKind::Error, Key::PlaylistImportFailed);
            return;
        }
        let imported = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<PlaylistShare>(&text).ok())
            .filter(|share| share.format == PLAYLIST_SHARE_FORMAT)
            .map(|mut share| {
                share.tracks.truncate(5_000);
                share
            });
        let Some(share) = imported else {
            self.toast(ToastKind::Error, Key::PlaylistImportFailed);
            return;
        };
        self.state.profile.remember_many(&share.tracks);
        let base_name = if share.name.trim().is_empty() {
            "Shared playlist".to_owned()
        } else {
            share.name.trim().to_owned()
        };
        let mut name = base_name.clone();
        let mut counter = 2;
        while self.state.profile.playlists.iter().any(|p| p.name == name) {
            name = format!("{base_name} ({counter})");
            counter += 1;
        }
        let new_id = self.state.profile.create_playlist(name);
        if let Some(playlist) = self.state.profile.playlist_mut(&new_id) {
            playlist.description = share.description.clone();
        }
        let added = self
            .state
            .profile
            .add_tracks_to_playlist(&new_id, &share.tracks);
        self.state.selected_playlist = Some(new_id);
        self.state.page = Page::Playlists;
        self.mark_dirty();
        log::info!("imported playlist with {added} track(s)");
        self.toast(ToastKind::Success, Key::PlaylistImportDone);
    }

    // -----------------------------------------------------------------------
    // Persistence
    // -----------------------------------------------------------------------

    pub fn mark_dirty(&mut self) {
        self.dirty_since = Some(Instant::now());
    }

    pub fn save_now(&mut self) {
        if self.demo {
            // Never persist the synthetic demo catalogue into a real profile.
            self.dirty_since = None;
            return;
        }
        self.dirty_since = None;
        self.last_save_attempt = Instant::now();
        self.state.profile.settings.language = self.state.lang;
        self.state.profile.settings.theme_mode = self.state.theme_mode;
        if let Err(err) = self.store.save(&self.state.profile) {
            log::warn!("failed to save profile: {err}");
        }
    }

    fn toast(&mut self, kind: ToastKind, text: impl IntoToast) {
        let text = text.into_text(&self.state);
        let now = self.ctx.input(|input| input.time as f64);
        self.state.toast(kind, text, now);
        self.ctx.request_repaint();
    }

    fn simulate_demo(&mut self, now: f64) {
        if self.state.playback.state == PlaybackState::Playing {
            let dt = self.ctx.input(|input| input.stable_dt).min(0.1) as f64;
            let duration = self.state.playback.duration_secs.unwrap_or(240.0);
            let position = self.state.playback.position_secs + dt;
            self.state.playback.position_secs = if position >= duration { 0.0 } else { position };
            let _ = now;
            self.ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    /// Set the page (UI convenience, keeps request_repaint logic in one place).
    pub fn set_page(&mut self, page: Page) {
        self.state.page = page;
        self.ctx.request_repaint();
    }

    pub fn cache_size(&self) -> u64 {
        store::dir_size(&self.paths.audio_cache_dir) + store::dir_size(&self.paths.covers_dir)
    }

    pub fn download_dir(&self) -> PathBuf {
        self.state.profile.settings.download_dir.clone()
    }

    pub fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    pub fn now_playing_summary(&self) -> String {
        match &self.state.now_playing {
            Some(track) => format!(
                "{} — {} · {}",
                track.title,
                track.display_artist(),
                format_duration(self.state.playback.position_secs)
            ),
            None => self.state.tr(Key::EmptyStateHint).to_owned(),
        }
    }
}

// ---------------------------------------------------------------------------
// Audio cache housekeeping
// ---------------------------------------------------------------------------

/// Default cap for the streamed-audio cache. Downloads are user files and are
/// never touched by this; prefetch lives in the same cache directory.

fn audio_cache_limit_bytes(settings: &Settings) -> u64 {
    // Environment override is handy for QA/CI; the GUI setting wins otherwise.
    if let Ok(value) = std::env::var("ECHOPORT_CACHE_LIMIT_MB") {
        if let Ok(mb) = value.trim().parse::<u64>() {
            return mb.saturating_mul(1024 * 1024).max(64 * 1024 * 1024);
        }
    }
    let mb = u64::from(settings.cache_limit_mb.min(10_240));
    if mb == 0 {
        0
    } else {
        mb.saturating_mul(1024 * 1024).max(128 * 1024 * 1024)
    }
}

// ---------------------------------------------------------------------------
// Lua configuration application
// ---------------------------------------------------------------------------

/// Apply startup overrides from `config.lua` to the persisted profile.
/// `None` fields keep the stored value; nothing here can fail.
fn apply_script_config_to_profile(profile: &mut Profile, config: &ScriptConfig) {
    if let Some(language) = config.language {
        profile.settings.language = language;
    }
    if let Some(mode) = config.theme_mode {
        profile.settings.theme_mode = mode;
    }
    if let Some(follow) = config.follow_cover_accent {
        profile.settings.follow_cover_accent = follow;
    }
    if let Some(volume) = config.volume {
        profile.settings.volume = volume.clamp(0.0, 1.0);
    }
    if let Some(count) = config.results_per_platform {
        profile.settings.results_per_platform = count.clamp(2, 50);
    }
    if let Some(scale) = config.ui_scale {
        profile.settings.ui_scale = scale.clamp(0.8, 2.0);
    }
    if let Some(mb) = config.cache_limit_mb {
        profile.settings.cache_limit_mb = mb.min(10_240);
    }
    if let Some(format) = config.download_format {
        profile.settings.download_format = format;
    }
    if let Some(dir) = config.download_dir.as_ref() {
        profile.settings.download_dir = dir.clone();
    }
    if let Some(dirs) = config.music_dirs.as_ref() {
        if !dirs.is_empty() {
            profile.settings.music_dirs = dirs.clone();
        }
    }
}

/// Apply script configuration to the live UI state. Called at startup and on
/// `ReloadScripts`; `apply_startup_page` is false for reloads so the user does
/// not get teleported to another page unexpectedly.
fn apply_script_config_to_state(
    state: &mut AppState,
    config: &ScriptConfig,
    apply_startup_page: bool,
) {
    state.lang = state.profile.settings.language;
    state.theme_mode = state.profile.settings.theme_mode;
    state.follow_cover_accent = state.profile.settings.follow_cover_accent;
    if let Some(step) = config.seek_step {
        state.seek_step = step.clamp(1.0, 120.0);
    }
    if let Some(step) = config.volume_step {
        state.volume_step = step.clamp(0.01, 0.5);
    }
    state.playback.volume = state.profile.settings.volume;
    state.ui_scale = state.profile.settings.ui_scale.clamp(0.8, 2.0);
    state.apply_theme_overrides(config.accent, config.highlight, config.macaron);
    if apply_startup_page {
        if let Some(page) = config.startup_page {
            state.page = page;
        }
    }
    state.theme = state.theme_target;
}

impl eframe::App for EchoPortApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.update_frame(ctx);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        let color = self.state.theme.bg;
        [
            f32::from(color.r()) / 255.0,
            f32::from(color.g()) / 255.0,
            f32::from(color.b()) / 255.0,
            1.0,
        ]
    }
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

enum TextOrKey {
    Key(Key),
    Text(String),
}

trait IntoToast {
    fn into_text(self, state: &AppState) -> String;
}

impl IntoToast for Key {
    fn into_text(self, state: &AppState) -> String {
        state.tr(self).to_owned()
    }
}

impl IntoToast for TextOrKey {
    fn into_text(self, state: &AppState) -> String {
        match self {
            TextOrKey::Key(key) => state.tr(key).to_owned(),
            TextOrKey::Text(text) => text,
        }
    }
}

pub(crate) fn save_color_image(
    image: &egui::ColorImage,
    path: &std::path::Path,
) -> Result<(), String> {
    let width = image.width() as u32;
    let height = image.height() as u32;
    let mut buffer = Vec::with_capacity((width * height * 4) as usize);
    for pixel in &image.pixels {
        buffer.extend_from_slice(&[pixel.r(), pixel.g(), pixel.b(), pixel.a()]);
    }
    let rgba = image::RgbaImage::from_raw(width, height, buffer)
        .ok_or_else(|| "invalid screenshot buffer".to_owned())?;
    rgba.save(path).map_err(|err| err.to_string())
}

fn page_from_key(key: &str) -> Option<Page> {
    Some(match key {
        "home" => Page::Home,
        "search" => Page::Search,
        "library" => Page::Library,
        "playlists" => Page::Playlists,
        "downloads" => Page::Downloads,
        "now_playing" | "nowplaying" => Page::NowPlaying,
        "profile" => Page::Profile,
        "settings" => Page::Settings,
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// Offline demo data (screenshots / UI development)
// ---------------------------------------------------------------------------

fn seed_demo(state: &mut AppState, paths: &AppPaths) {
    let covers = demo_covers(paths);
    let tracks = demo_tracks();
    let mut profile = state.profile.clone();
    profile.playlists.clear();
    profile.favorites.clear();
    profile.history.clear();

    for track in &tracks {
        profile.remember(track);
    }
    let blue_hour = profile.create_playlist("蓝调时刻 / Blue Hour");
    let night = profile.create_playlist("深夜写作 / Night Writing");
    let macaron = profile.create_playlist("Macaron Mix");
    let blue_tracks: Vec<Track> = tracks.iter().take(4).cloned().collect();
    let night_tracks: Vec<Track> = tracks.iter().skip(2).take(4).cloned().collect();
    let macaron_tracks: Vec<Track> = tracks.iter().skip(1).step_by(2).cloned().collect();
    profile.add_tracks_to_playlist(&blue_hour, &blue_tracks);
    profile.add_tracks_to_playlist(&night, &night_tracks);
    profile.add_tracks_to_playlist(&macaron, &macaron_tracks);
    for track in tracks.iter().take(3) {
        profile.toggle_favorite(track);
    }
    for track in tracks.iter().take(5).rev() {
        profile.add_history(&track.id);
    }
    profile.playlists[0].description = "清晨与海风 / morning breeze".to_owned();
    profile.playlists[1].description = "低照度耳机时间 / low light, headphones".to_owned();
    profile.playlists[2].description = "马卡龙配色歌单 / pastel study mix".to_owned();
    state.profile = profile;

    state.library = LibraryView {
        tracks: tracks
            .iter()
            .filter(|t| t.platform == Platform::Local)
            .cloned()
            .collect(),
        scanning: false,
        scanned_once: true,
        folder_input: String::new(),
    };

    state.now_playing = tracks.first().cloned();
    state.queue = tracks.clone();
    state.queue_pos = 0;
    let palette = theme::extract_palette_file(&covers[0]);
    state.cover = CoverView {
        track_id: tracks.first().map(|t| t.id.clone()),
        path: covers.first().cloned(),
        palette,
        loading: false,
    };
    state.playback = PlaybackStatus {
        state: PlaybackState::Playing,
        position_secs: 96.0,
        duration_secs: tracks.first().and_then(|t| t.duration_secs),
        volume: 0.72,
        error: None,
        ended: false,
    };
    state.set_theme_mode(ThemeMode::Pulse);
    state.follow_cover_accent = false;
    state.set_follow_cover_accent(state.follow_cover_accent);
    state.theme = state.theme_target;
    state.now_playing_tab = NowPlayingTab::Track;

    // A believable search page.
    let mut results = Vec::new();
    for (index, platform) in Platform::REMOTE.into_iter().enumerate() {
        let picks: Vec<Track> = tracks.iter().skip(index).take(3).cloned().collect();
        results.push(crate::model::PlatformResults {
            platform,
            tracks: picks,
            error: None,
        });
    }
    state.search = SearchView {
        input: "Aurora Lune".to_owned(),
        query: "Aurora Lune".to_owned(),
        platforms: Platform::REMOTE.into_iter().collect(),
        loading: false,
        loading_more: None,
        tab: crate::state::SearchTab::All,
        results: Some(SearchResults {
            query: "Aurora Lune".to_owned(),
            results,
            artists: vec![crate::model::PlatformArtistResults {
                platform: Platform::YouTube,
                artists: vec![crate::model::Artist::new(
                    Platform::YouTube,
                    "aurora-lune",
                    "Aurora Lune",
                )
                .with_page("https://www.youtube.com/results?search_query=Aurora+Lune")],
                error: None,
            }],
            albums: Vec::new(),
        }),
        error: None,
    };

    // Downloads page with a mix of states.
    use crate::model::DownloadProgress;
    let _ = DownloadProgress::default();
    let mut jobs = Vec::new();
    let mut running = DownloadJob::queued(tracks[1].clone());
    running.state = JobState::Running;
    running.progress = 0.62;
    running.stage = "downloading audio".to_owned();
    jobs.push(running);
    let mut queued = DownloadJob::queued(tracks[2].clone());
    queued.state = JobState::Queued;
    queued.stage = "waiting".to_owned();
    jobs.push(queued);
    let mut done = DownloadJob::queued(tracks[3].clone());
    done.state = JobState::Done;
    done.progress = 1.0;
    done.path = Some(paths.data_dir.join("Music/EchoPort/demo.mp3"));
    jobs.push(done);
    state.downloads.jobs = jobs;

    state.background = BackgroundView {
        track_id: tracks.first().map(|t| t.id.clone()),
        loading: false,
        data: Some(SongBackground {
            bio: Some(crate::model::ArtistBio {
                title: "Aurora Lune".to_owned(),
                extract: "Aurora Lune is a fictional dream-pop project used to demonstrate the \
                          song background panel. It writes about shorelines, glass and the colour \
                          of the last hour of daylight."
                    .to_owned(),
                url: "https://en.wikipedia.org/wiki/Dream_pop".to_owned(),
                thumbnail: None,
            }),
            album: Some(crate::model::ArtistBio {
                title: "Tidal Glass (album)".to_owned(),
                extract: "Tidal Glass is the debut album: a slow, tide-paced record about \
                          coastlines and the objects people leave behind."
                    .to_owned(),
                url: "https://en.wikipedia.org/wiki/Dream_pop".to_owned(),
                thumbnail: None,
            }),
            song: Some(crate::model::ArtistBio {
                title: "Saltwater Room (single)".to_owned(),
                extract: "The lead single from Tidal Glass, written after a night walk along the \
                          harbour and recorded with a single microphone and a tape delay."
                    .to_owned(),
                url: "https://en.wikipedia.org/wiki/Dream_pop".to_owned(),
                thumbnail: None,
            }),
            links: vec![
                crate::model::BackgroundLink {
                    kind: crate::model::LinkKind::Wikipedia,
                    title: "Dream pop — Wikipedia".to_owned(),
                    url: "https://en.wikipedia.org/wiki/Dream_pop".to_owned(),
                    subtitle: Some("genre context".to_owned()),
                },
                crate::model::BackgroundLink {
                    kind: crate::model::LinkKind::Genius,
                    title: "Saltwater Room — lyrics & annotations".to_owned(),
                    url: "https://genius.com/".to_owned(),
                    subtitle: Some("Genius".to_owned()),
                },
            ],
            videos: vec![
                crate::model::BackgroundLink {
                    kind: crate::model::LinkKind::YouTube,
                    title: "Aurora Lune on writing Saltwater Room · interview".to_owned(),
                    url: "https://www.youtube.com/results?search_query=aurora+lune+interview"
                        .to_owned(),
                    subtitle: Some("YouTube · 12:04".to_owned()),
                },
                crate::model::BackgroundLink {
                    kind: crate::model::LinkKind::YouTube,
                    title: "The making of Tidal Glass · studio breakdown".to_owned(),
                    url: "https://www.youtube.com/results?search_query=tidal+glass+breakdown"
                        .to_owned(),
                    subtitle: Some("YouTube · 8:41".to_owned()),
                },
            ],
        }),
        error: None,
    };

    state.tooling = ToolingStatus {
        ytdlp_path: Some(paths.data_dir.join("yt-dlp")),
        ytdlp_version: Some("2026.10.21".to_owned()),
        ffmpeg_path: Some(paths.data_dir.join("ffmpeg")),
        checking: false,
    };
    state.profiles = vec!["Me".to_owned(), "Studio".to_owned()];
    state.selected_playlist = Some(blue_hour);
    state.script = ScriptStatus {
        loaded: true,
        config_file: paths.config_file.clone(),
        plugins_dir: paths.plugins_dir.clone(),
        plugins: vec!["daily-mix.lua".to_owned()],
        actions: vec![
            ScriptActionInfo {
                id: "focus_mode".to_owned(),
                title: "Focus mode / 专注模式".to_owned(),
                description: Some("Trim the queue to ambient tracks".to_owned()),
            },
            ScriptActionInfo {
                id: "lua_stats".to_owned(),
                title: "Print listening stats / 输出收听统计".to_owned(),
                description: Some("Writes a summary through echoport.notify".to_owned()),
            },
        ],
        warnings: Vec::new(),
        bindings: Vec::new(),
    };
}

fn demo_tracks() -> Vec<Track> {
    vec![
        Track::new(Platform::YouTube, "demo-1", "Saltwater Room", "Aurora Lune")
            .with_album("Tidal Glass")
            .with_duration(252.0)
            .with_page("https://www.youtube.com/results?search_query=saltwater+room"),
        Track::new(Platform::Netease, "demo-2", "玻璃海", "陈一")
            .with_album("夜航")
            .with_duration(228.0)
            .with_page("https://music.163.com/"),
        Track::new(
            Platform::Spotify,
            "demo-3",
            "Midnight Postcard",
            "The Quiet Hours",
        )
        .with_album("Letters Never Sent")
        .with_duration(201.0)
        .with_page("https://open.spotify.com/"),
        Track::new(Platform::Bilibili, "demo-4", "雨停之后", "林间")
            .with_album("城市气候")
            .with_duration(243.0)
            .with_page("https://www.bilibili.com/"),
        Track::new(Platform::SoundCloud, "demo-5", "Velvet Static", "Mira Vale")
            .with_duration(178.0)
            .with_page("https://soundcloud.com/"),
        Track::new(Platform::AppleMusic, "demo-6", "Paper Moon", "Aurora Lune")
            .with_album("Tidal Glass")
            .with_duration(224.0)
            .with_page("https://music.apple.com/"),
        Track {
            id: format!(
                "local:{}",
                crate::model::stable_hash("/demo/Cafe de Flore.mp3")
            ),
            platform: Platform::Local,
            title: "Café de Flore".to_owned(),
            artist: "Local Ensemble".to_owned(),
            album: Some("Sunday Recordings".to_owned()),
            duration_secs: Some(196.0),
            art_url: None,
            page_url: None,
            stream_url: None,
            local_path: Some(PathBuf::from("/demo/Cafe de Flore.mp3")),
            year: None,
            release_date: None,
        },
        Track::new(Platform::YouTube, "demo-8", "Slow Transit", "Kite & Harbor")
            .with_album("Lines")
            .with_duration(267.0)
            .with_page("https://www.youtube.com/results?search_query=slow+transit"),
    ]
}

/// Generate a handful of abstract macaron gradient covers for demo mode.
fn demo_covers(paths: &AppPaths) -> Vec<PathBuf> {
    let _ = std::fs::create_dir_all(&paths.covers_dir);
    // Cover art matching the three themes: Pulse (blue → lime), Tide
    // (teal → aqua) and Ink (slate → periwinkle).
    let specs: [([u8; 3], [u8; 3], [u8; 3]); 3] = [
        ([0x04, 0x4B, 0xB5], [0xD9, 0xFF, 0x9C], [0x0C, 0x1F, 0x3D]),
        ([0x2D, 0xAC, 0xB5], [0xC3, 0xE9, 0xEA], [0x1E, 0x2D, 0x58]),
        ([0x34, 0x3F, 0x51], [0xAF, 0xC7, 0xF2], [0xE5, 0xEE, 0xFF]),
    ];
    let mut out = Vec::new();
    for (index, (top, bottom, accent)) in specs.iter().enumerate() {
        let mut image = image::RgbImage::new(512, 512);
        for y in 0..512u32 {
            let t = y as f32 / 511.0;
            for x in 0..512u32 {
                let base = mix_u8(*top, *bottom, t);
                // Soft off-centre blobs give the generated art a bit of depth.
                let dx = x as f32 - 340.0;
                let dy = y as f32 - 150.0;
                let dist = (dx * dx + dy * dy).sqrt();
                let glow = (1.0 - (dist / 280.0).clamp(0.0, 1.0)).powf(2.0) * 0.55;
                let color = mix_u8(base, *accent, glow * 0.35);
                image.put_pixel(x, y, image::Rgb(color));
            }
        }
        let path = paths.covers_dir.join(format!("demo-cover-{index}.png"));
        if image.save(&path).is_ok() {
            out.push(path);
        } else {
            out.push(path);
        }
    }
    out
}

fn mix_u8(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    let channel = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t) as u8;
    [
        channel(a[0], b[0]),
        channel(a[1], b[1]),
        channel(a[2], b[2]),
    ]
}

fn encode_avatar_data_url(path: &std::path::Path) -> Result<String, String> {
    let image = image::open(path).map_err(|err| err.to_string())?;
    // Keep the profile JSON small (and portable): 256 px, PNG.
    let image = image.resize(256, 256, image::imageops::FilterType::Lanczos3);
    let mut bytes = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut bytes, image::ImageFormat::Png)
        .map_err(|err| err.to_string())?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes.into_inner())
    ))
}

fn is_safe_web_url(url: &str) -> bool {
    (url.starts_with("https://") || url.starts_with("http://"))
        && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}

fn is_audio_path(path: &std::path::Path) -> bool {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(
        extension.as_str(),
        "mp3"
            | "m4a"
            | "aac"
            | "flac"
            | "wav"
            | "ogg"
            | "opus"
            | "wma"
            | "alac"
            | "aiff"
            | "aif"
            | "ape"
    )
}
