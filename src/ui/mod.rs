//! egui interface shell. The GUI workstream owns this module and can freely
//! restructure pages; `layout()` is the only entry point the app calls.
//!
//! Layout, top to bottom:
//! * player bar (bottom panel, full width, always visible)
//! * navigation sidebar (left)
//! * page content (central panel)
//! * overlays: queue panel, toasts, dialogs
//!
//! All transient widget state lives in [`UiState`]; the app owns everything
//! durable.

mod covers;
mod icons;
mod pages;
pub mod splash;
mod style;
mod widgets;
pub(crate) use widgets::looks_like_url;

use std::collections::HashSet;
use std::time::Duration;

use eframe::CreationContext;
use egui::{
    Align, Align2, Color32, CornerRadius, CursorIcon, Frame, Id, Layout, Margin, Order, Pos2, Rect,
    RichText, Sense, Stroke, StrokeKind, Ui, Vec2,
};

use crate::app::EchoPortApp;
use crate::i18n::{Key, Language};
use crate::model::{format_duration, DownloadFormat, Platform, PlaybackState, Track};
use crate::state::{Command, Page, ToastKind};
use crate::theme::{self, Theme, ThemeMode};

use covers::CoverCache;
use style::Icon;

// ---------------------------------------------------------------------------
// Ephemeral UI state
// ---------------------------------------------------------------------------

/// A modal dialog currently being edited. Owned by the UI only.
#[derive(Clone, Debug)]
pub enum Dialog {
    NewPlaylist {
        seed: Vec<Track>,
        name: String,
        description: String,
    },
    RenamePlaylist {
        id: String,
        name: String,
        description: String,
        existing_cover: Option<std::path::PathBuf>,
        new_cover: Option<std::path::PathBuf>,
        clear_cover: bool,
    },
    DeletePlaylist {
        id: String,
        name: String,
    },
    NewProfile {
        name: String,
        bio: String,
    },
    DeleteProfile {
        name: String,
    },
    Download {
        tracks: Vec<Track>,
        format: DownloadFormat,
    },
    ExportProfile,
}

#[derive(Clone, Debug)]
pub struct ThemeImageCrop {
    pub source: std::path::PathBuf,
    pub zoom: f32,
    pub center_x: f32,
    pub center_y: f32,
}

pub struct UiState {
    pub covers: CoverCache,
    /// Lazily uploaded textures for the embedded icon set.
    pub icon_assets: icons::SharedIconAssets,
    /// Page currently rendered (compared with `AppState::page` for fades).
    page: Page,
    page_fade: f32,
    pub dialog: Option<Dialog>,
    /// Cache-size readout, refreshed when the settings page is entered.
    cache_size: Option<u64>,
    cache_page: Option<Page>,
    volume_before_mute: f32,
    /// Live scrub position while the user drags the timeline.
    scrub_preview: Option<f64>,
    /// Current track id, used to reset stale scrub/drag state on track change.
    current_track: Option<String>,
    /// Single-click selection (highlight) for track rows / cards.
    pub selected_track: Option<String>,
    /// Single-click selection (highlight) for playlist rows.
    pub selected_playlist: Option<String>,
    /// Platforms whose already-fetched search results are fully expanded.
    pub search_expanded: HashSet<Platform>,
    /// Query the collapse state belongs to (reset when a new search lands).
    pub search_seen_query: Option<String>,
    /// Bio draft for the profile editor (committed via Save).
    pub profile_bio: String,
    /// Which profile the draft belongs to (reset when switching profiles).
    pub profile_bio_for: Option<String>,
    /// The profile editor card is only shown after the user asks to edit.
    pub profile_editing: bool,
    /// Track currently shown in the lyrics panel and its animated scroll
    /// position (in line units).
    pub lyrics_track: Option<String>,
    pub lyrics_scroll_pos: f32,
    /// Decoded optional theme image shown behind the sidebar.
    theme_image: Option<(String, egui::TextureHandle)>,
    /// Full-window background texture, decoded off the UI thread.
    full_theme_image: Option<(String, egui::TextureHandle)>,
    /// In-flight full-window decode. The UI never blocks on a large photo.
    full_theme_loading: Option<(
        String,
        std::sync::mpsc::Receiver<Result<egui::ColorImage, String>>,
    )>,
    /// Pending crop selection for a newly chosen theme image.
    pub crop_theme_image: Option<ThemeImageCrop>,
    /// Whether release notes older than the latest three are expanded.
    pub changelog_expanded: bool,
}

impl UiState {
    /// Lazily decode and upload the custom theme image. The cache is keyed by
    /// canonical path so replacing the image immediately refreshes the sidebar.
    /// Drop a decoded theme image after it was replaced or removed.
    pub fn invalidate_theme_image(&mut self) {
        self.theme_image = None;
        self.full_theme_image = None;
        self.full_theme_loading = None;
    }

    /// Kick off background decoding for the full-window photo. Called during
    /// startup so the first switch to Full-window does not freeze the UI while
    /// a multi-megabyte JPEG/PNG is decoded and uploaded.
    pub fn request_full_theme_image(&mut self, path: &std::path::Path) {
        let key = path.to_string_lossy().into_owned();
        if self.full_theme_image.as_ref().map(|(key, _)| key) == Some(&key) {
            return;
        }
        if self
            .full_theme_loading
            .as_ref()
            .is_some_and(|(loading, _)| loading == &key)
        {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let path = path.to_path_buf();
        std::thread::spawn(move || {
            let result = (|| -> Result<egui::ColorImage, String> {
                let bytes = std::fs::read(&path).map_err(|err| err.to_string())?;
                let mut image = image::load_from_memory(&bytes).map_err(|err| err.to_string())?;
                if image.width() > 8_000
                    || image.height() > 8_000
                    || u64::from(image.width()) * u64::from(image.height()) > 24_000_000
                {
                    return Err("theme image is too large".to_owned());
                }
                if image.width() > 1600 || image.height() > 1600 {
                    image = image.thumbnail(1600, 1600);
                }
                let rgba = image.to_rgba8();
                Ok(egui::ColorImage::from_rgba_unmultiplied(
                    [rgba.width() as usize, rgba.height() as usize],
                    rgba.as_raw(),
                ))
            })();
            let _ = tx.send(result);
        });
        self.full_theme_loading = Some((key, rx));
    }

    /// Poll the background decode and upload the texture when it is ready.
    pub fn full_theme_image_texture(
        &mut self,
        ctx: &egui::Context,
        path: &std::path::Path,
    ) -> Option<egui::TextureHandle> {
        let key = path.to_string_lossy().into_owned();
        if let Some((cached_key, texture)) = self.full_theme_image.as_ref() {
            if cached_key == &key {
                return Some(texture.clone());
            }
        }
        if let Some((loading_key, rx)) = self.full_theme_loading.as_ref() {
            if loading_key != &key {
                self.full_theme_loading = None;
            } else {
                match rx.try_recv() {
                    Ok(Ok(image)) => {
                        let texture = ctx.load_texture(
                            format!("theme-full:{key}"),
                            image,
                            egui::TextureOptions::LINEAR,
                        );
                        self.full_theme_image = Some((key.clone(), texture.clone()));
                        self.full_theme_loading = None;
                        return Some(texture);
                    }
                    Ok(Err(err)) => {
                        log::warn!("could not decode full theme image: {err}");
                        self.full_theme_loading = None;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        self.full_theme_loading = None;
                    }
                }
            }
        }
        if self.full_theme_image.is_none() && self.full_theme_loading.is_none() {
            self.request_full_theme_image(path);
        }
        if self.full_theme_loading.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(30));
        }
        None
    }

    pub fn theme_image_texture(
        &mut self,
        ctx: &egui::Context,
        path: &std::path::Path,
    ) -> Option<egui::TextureHandle> {
        let key = path.to_string_lossy().into_owned();
        if self.theme_image.as_ref().map(|(key, _)| key) != Some(&key) {
            let bytes = std::fs::read(path).ok()?;
            let image = image::load_from_memory(&bytes).ok()?;
            if image.width() > 8_000
                || image.height() > 8_000
                || u64::from(image.width()) * u64::from(image.height()) > 24_000_000
            {
                return None;
            }
            // The sidebar is only a few hundred px wide; uploading a full
            // 24 MP camera frame wastes GPU memory and slows resize redraws.
            let image = if image.width() > 1600 || image.height() > 1600 {
                image.thumbnail(1600, 1600)
            } else {
                image
            };
            let rgba = image.to_rgba8();
            let color_image = egui::ColorImage::from_rgba_unmultiplied(
                [rgba.width() as usize, rgba.height() as usize],
                rgba.as_raw(),
            );
            let texture = ctx.load_texture(&key, color_image, egui::TextureOptions::LINEAR);
            self.theme_image = Some((key.clone(), texture));
        }
        self.theme_image
            .as_ref()
            .map(|(_, texture)| texture.clone())
    }

    pub fn new(_cc: &CreationContext<'_>, _state: &crate::state::AppState) -> Self {
        let icon_assets = icons::install(&_cc.egui_ctx);
        Self {
            covers: CoverCache::new(),
            icon_assets,
            page: Page::Home,
            page_fade: 1.0,
            dialog: None,
            cache_size: None,
            cache_page: None,
            volume_before_mute: 0.7,
            scrub_preview: None,
            current_track: None,
            selected_track: None,
            selected_playlist: None,
            search_expanded: HashSet::new(),
            search_seen_query: None,
            profile_bio: String::new(),
            profile_bio_for: None,
            profile_editing: false,
            lyrics_track: None,
            lyrics_scroll_pos: 0.0,
            theme_image: None,
            full_theme_image: None,
            full_theme_loading: None,
            crop_theme_image: None,
            changelog_expanded: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Shell
// ---------------------------------------------------------------------------

pub fn layout(ctx: &egui::Context, app: &mut EchoPortApp) {
    // Full-window mode must be translucent on the same frame the setting is
    // toggled, not only after a restart. Re-assert the alpha locally in case a
    // command was dispatched after the previous layout pass.
    let mut theme = app.state.theme;
    if app.state.full_theme_image_active() {
        theme = theme.translucent();
        app.state.theme = theme;
    }
    let mut pending: Vec<Command> = Vec::new();

    // Phone/tablet shell: no permanent sidebar; bottom navigation + compact
    // two-row player. `ECHOPORT_MOBILE=1` gives a desktop preview for QA.
    let mobile = cfg!(target_os = "android")
        || std::env::var("ECHOPORT_MOBILE").is_ok()
        || ctx.screen_rect().width() < 700.0;
    if mobile {
        mobile_layout(ctx, app, &theme, &mut pending);
        for command in pending {
            app.dispatch(command);
        }
        return;
    }

    // Page cross-fade (very short, so navigation feels instant).
    let dt = ctx.input(|input| input.stable_dt).min(0.05);
    if app.ui.page != app.state.page {
        app.ui.page = app.state.page;
        app.ui.page_fade = 0.0;
        app.ui.scrub_preview = None;
    }
    if app.ui.page_fade < 1.0 {
        app.ui.page_fade = (app.ui.page_fade + dt / 0.16).min(1.0);
        ctx.request_repaint_after(Duration::from_millis(16));
    }
    let page_fade = app.ui.page_fade;

    // Track changes must clear any stale scrub preview / drag state so the
    // timeline immediately reflects the new track's position.
    let track_key = app.state.now_playing.as_ref().map(|track| track.id.clone());
    if track_key != app.ui.current_track {
        app.ui.current_track = track_key;
        app.ui.scrub_preview = None;
    }
    // Keep the timeline moving while audio is active, even if the player
    // callback is a little late with its status update.
    if (app.state.playback.state == PlaybackState::Playing
        || app.state.playback.state == PlaybackState::Loading)
        && !app.state.ui_backgrounded
    {
        ctx.request_repaint_after(Duration::from_millis(50));
    }

    let screen = ctx.screen_rect();
    let compact = screen.width() < 1060.0;
    let height_scale = (screen.height() / 800.0).clamp(0.85, 1.6);
    let min_player = if screen.height() < 620.0 { 70.0 } else { 84.0 };
    let player_height = (96.0 * height_scale).clamp(min_player, 132.0);
    let sidebar_width = if compact {
        202.0
    } else {
        (226.0 + (screen.width() - 1200.0) * 0.028).clamp(226.0, 276.0)
    };
    let margin_x = if compact {
        18
    } else {
        (26.0 + (screen.width() - 1200.0) * 0.022).clamp(26.0, 56.0) as i8
    };
    let margin_top = (14.0 * height_scale).clamp(12.0, 26.0) as i8;
    let margin_bottom = (8.0 * height_scale).clamp(6.0, 16.0) as i8;

    // Cache size readout is IO, so we only read it once per visit.
    if app.state.page == Page::Settings && app.ui.cache_page != Some(Page::Settings) {
        app.ui.cache_size = Some(app.cache_size());
        app.ui.cache_page = Some(Page::Settings);
    } else if app.state.page != Page::Settings {
        app.ui.cache_page = None;
    }

    let full_theme_image = app.state.full_theme_image_active();
    if full_theme_image {
        paint_full_theme_image(ctx, app, &theme);
    }
    egui::TopBottomPanel::bottom("echoport-player")
        .exact_height(player_height)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().fill(theme.panel))
        .show(ctx, |ui| {
            player_bar(ui, app, &theme, player_height, sidebar_width, &mut pending);
        });

    let sidebar_has_image = !full_theme_image && app.state.theme_image_path().is_some();
    egui::SidePanel::left("echoport-nav")
        .exact_width(sidebar_width)
        .resizable(false)
        .show_separator_line(false)
        .frame(
            Frame::new()
                .fill(theme.panel)
                .inner_margin(Margin::symmetric(if sidebar_has_image { 0 } else { 14 }, 0)),
        )
        .show(ctx, |ui| {
            if sidebar_has_image {
                // Paint on the full panel before insetting the navigation content.
                paint_sidebar_theme_image(ui, app, &theme);
                let content_rect = ui.max_rect().shrink2(Vec2::new(14.0, 0.0));
                let mut content = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(content_rect)
                        .layout(Layout::top_down(Align::Min)),
                );
                sidebar(&mut content, app, &theme, &mut pending);
            } else {
                sidebar(ui, app, &theme, &mut pending);
            }
        });

    egui::CentralPanel::default()
        .frame(Frame::new().fill(theme.bg).inner_margin(Margin {
            left: margin_x,
            right: margin_x,
            top: margin_top,
            bottom: margin_bottom,
        }))
        .show(ctx, |ui| {
            ui.multiply_opacity(page_fade);
            pages::draw(ui, app, &theme, &mut pending);
        });

    if app.state.queue_panel_open {
        queue_panel(ctx, app, &theme, player_height, &mut pending);
    }

    crash_prompt(ctx, app, &theme, player_height);
    let toast_lift = if app.crash_notice.is_some() {
        170.0
    } else {
        0.0
    };
    toasts(ctx, app, &theme, player_height, toast_lift);
    drop_prompt(ctx, app, &theme, player_height, &mut pending);
    dialogs(ctx, app, &theme, &mut pending);
    theme_image_crop_dialog(ctx, app, &theme, &mut pending);
    handle_escape(ctx, app);
    splash::draw(ctx, app, &mut pending);

    for command in pending {
        app.dispatch(command);
    }
}

// ---------------------------------------------------------------------------
// Mobile shell
// ---------------------------------------------------------------------------

fn mobile_layout(
    ctx: &egui::Context,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
) {
    let dt = ctx.input(|input| input.stable_dt).min(0.05);
    if app.ui.page != app.state.page {
        app.ui.page = app.state.page;
        app.ui.page_fade = 0.0;
        app.ui.scrub_preview = None;
    }
    if app.ui.page_fade < 1.0 {
        app.ui.page_fade = (app.ui.page_fade + dt / 0.16).min(1.0);
        ctx.request_repaint_after(Duration::from_millis(16));
    }
    let track_key = app.state.now_playing.as_ref().map(|track| track.id.clone());
    if track_key != app.ui.current_track {
        app.ui.current_track = track_key;
        app.ui.scrub_preview = None;
    }
    if (app.state.playback.state == PlaybackState::Playing
        || app.state.playback.state == PlaybackState::Loading)
        && !app.state.ui_backgrounded
    {
        ctx.request_repaint_after(Duration::from_millis(50));
    }

    let page_fade = app.ui.page_fade;
    // Taller than the original 118: the scrubber now sits directly under the
    // title row (as in the annotated portrait reference) instead of hugging
    // the system navigation area. The extra height also extends the player's
    // top boundary away from the bottom navigation.
    let player_height = 136.0;
    let nav_height = 72.0;
    #[cfg(target_os = "android")]
    let (status_inset, android_bottom_inset) = {
        let (top, bottom) = crate::android::current_system_insets().unwrap_or((0.0, 0.0));
        let top = std::env::var("ECHOPORT_ANDROID_TOP_INSET")
            .ok()
            .and_then(|value| value.parse::<f32>().ok())
            .map(|value| value.clamp(0.0, 72.0))
            .unwrap_or(top);
        let bottom = std::env::var("ECHOPORT_ANDROID_BOTTOM_INSET")
            .ok()
            .and_then(|value| value.parse::<f32>().ok())
            .map(|value| value.clamp(0.0, 160.0))
            .unwrap_or(bottom);
        (top, bottom)
    };
    #[cfg(not(target_os = "android"))]
    let (status_inset, android_bottom_inset) = (14.0_f32, 0.0_f32);
    // NativeActivity is edge-to-edge on Android: the navigation bar is painted
    // over the bottom of the window. Reserve the current system inset (queried
    // through JNI) so the player is never under the system buttons.
    let bottom_system_inset = android_bottom_inset;
    if bottom_system_inset > 0.0 {
        egui::TopBottomPanel::bottom("echoport-mobile-system-inset")
            .exact_height(bottom_system_inset)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(theme.bg))
            .show(ctx, |_ui| {});
    }

    // Player sits directly above the bottom navigation.
    egui::TopBottomPanel::bottom("echoport-mobile-player")
        .exact_height(player_height)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().fill(theme.panel))
        .show(ctx, |ui| {
            mobile_player(ui, app, theme, player_height, pending);
        });
    egui::TopBottomPanel::bottom("echoport-mobile-nav")
        .exact_height(nav_height)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().fill(theme.panel))
        .show(ctx, |ui| {
            mobile_nav(ui, app, theme);
        });

    egui::CentralPanel::default()
        .frame(Frame::new().fill(theme.bg).inner_margin(Margin {
            left: 12,
            right: 12,
            top: status_inset as i8,
            bottom: 6,
        }))
        .show(ctx, |ui| {
            ui.multiply_opacity(page_fade);
            pages::draw(ui, app, theme, pending);
        });

    let overlay_lift = player_height + nav_height + bottom_system_inset;
    if app.state.queue_panel_open {
        queue_panel(ctx, app, theme, overlay_lift, pending);
    }
    toasts(ctx, app, theme, overlay_lift, 0.0);
    drop_prompt(ctx, app, theme, overlay_lift, pending);
    dialogs(ctx, app, theme, pending);
    handle_escape(ctx, app);
    splash::draw(ctx, app, pending);
}

fn mobile_nav(ui: &mut egui::Ui, app: &mut EchoPortApp, theme: &Theme) {
    let full = ui.max_rect();
    ui.painter().line_segment(
        [full.left_top(), full.right_top()],
        Stroke::new(1.0_f32, theme.border),
    );
    let nav: [(Page, Key, Icon); 5] = [
        (Page::Home, Key::Home, Icon::Home),
        (Page::Search, Key::SearchNav, Icon::Search),
        (Page::Library, Key::Library, Icon::Library),
        (Page::Playlists, Key::Playlists, Icon::Playlist),
        (Page::Settings, Key::Settings, Icon::Settings),
    ];
    let width = ui.available_width() / nav.len() as f32;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (page, key, icon) in nav {
            let selected = app.state.page == page
                || (matches!(page, Page::Home) && app.state.page == Page::NowPlaying);
            let (rect, response) =
                ui.allocate_exact_size(Vec2::new(width, ui.available_height()), Sense::click());
            if selected {
                ui.painter().rect_filled(
                    Rect::from_center_size(
                        Pos2::new(rect.center().x, rect.top() + 3.0),
                        Vec2::new(24.0, 3.0),
                    ),
                    CornerRadius::same(2),
                    theme.accent,
                );
            }
            let color = if selected {
                theme.accent
            } else {
                theme.text_weak
            };
            style::paint_icon(
                ui.painter(),
                icon,
                Rect::from_center_size(
                    Pos2::new(rect.center().x, rect.center().y - 6.0),
                    Vec2::splat(19.0),
                ),
                color,
            );
            ui.painter().text(
                Pos2::new(rect.center().x, rect.center().y + 15.0),
                Align2::CENTER_CENTER,
                app.state.tr(key),
                style::font(9.5),
                color,
            );
            if response.clicked() {
                app.set_page(page);
            }
        }
    });
}

fn mobile_player(
    ui: &mut egui::Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    height: f32,
    pending: &mut Vec<Command>,
) {
    let full = ui.max_rect();
    ui.painter().line_segment(
        [full.left_top(), full.right_top()],
        Stroke::new(1.0_f32, theme.border),
    );
    let cover = 44.0;
    let cover_rect = Rect::from_min_size(
        Pos2::new(full.left() + 12.0, full.top() + 10.0),
        Vec2::splat(cover),
    );
    let track = app.state.now_playing.clone();
    match track.as_ref() {
        Some(track) => widgets::paint_track_cover(ui, app, theme, track, cover_rect, style::R_MD),
        None => {
            ui.painter().rect_filled(
                cover_rect,
                CornerRadius::same(style::R_MD),
                style::wash(theme),
            );
            let note = cover_rect.shrink(12.0);
            if !style::paint_note(ui.painter(), note, theme.text_weak) {
                style::paint_icon(ui.painter(), Icon::Music, note, theme.text_weak);
            }
        }
    }
    let cover_response = ui.interact(
        cover_rect,
        ui.id().with("mobile-player-cover"),
        Sense::click(),
    );
    if cover_response.clicked() && track.is_some() {
        app.set_page(Page::NowPlaying);
    }
    let text_left = cover_rect.right() + 10.0;
    let controls_w = 92.0;
    let text_right = (full.right() - controls_w - 10.0).max(text_left + 40.0);
    let text_rect = Rect::from_min_max(
        Pos2::new(text_left, full.top() + 8.0),
        Pos2::new(text_right, full.top() + 58.0),
    );
    style::absolute(ui, text_rect, |ui| {
        if let Some(track) = &track {
            if ui
                .add(
                    egui::Label::new(
                        RichText::new(&track.title)
                            .size(14.0)
                            .strong()
                            .color(theme.text),
                    )
                    .truncate(),
                )
                .clicked()
            {
                app.set_page(Page::NowPlaying);
            }
            ui.add(
                egui::Label::new(
                    RichText::new(track.display_artist())
                        .size(11.0)
                        .color(theme.text_weak),
                )
                .truncate(),
            );
        } else {
            ui.label(
                RichText::new(app.state.tr(Key::EmptyStateHint))
                    .size(12.0)
                    .color(theme.text_weak),
            );
        }
    });
    let play_center = Pos2::new(full.right() - 66.0, full.top() + 34.0);
    let play_rect = Rect::from_center_size(play_center, Vec2::splat(42.0));
    if ui
        .interact(play_rect, ui.id().with("mobile-play"), Sense::click())
        .clicked()
    {
        pending.push(Command::TogglePlayPause);
    }
    ui.painter().circle_filled(
        play_center,
        21.0,
        if app.state.playback.state.is_playing() {
            theme.accent
        } else {
            theme.accent_soft
        },
    );
    style::paint_icon(
        ui.painter(),
        if app.state.playback.state.is_playing() {
            Icon::Pause
        } else {
            Icon::Play
        },
        play_rect.shrink(12.0),
        if app.state.playback.state.is_playing() {
            theme.on_accent
        } else {
            theme.accent
        },
    );
    let next_center = Pos2::new(full.right() - 22.0, full.top() + 34.0);
    let next_rect = Rect::from_center_size(next_center, Vec2::splat(30.0));
    if ui
        .interact(next_rect, ui.id().with("mobile-next"), Sense::click())
        .clicked()
    {
        pending.push(Command::Next);
    }
    style::paint_icon(ui.painter(), Icon::Next, next_rect.shrink(8.0), theme.text);

    let duration = app.state.playback.duration_secs.unwrap_or(0.0);
    let position = app
        .ui
        .scrub_preview
        .unwrap_or(app.state.playback.position_secs);
    let scrub_rect = Rect::from_min_max(
        Pos2::new(full.left() + 12.0, full.top() + 70.0),
        Pos2::new(full.right() - 12.0, full.top() + 86.0),
    );
    let mut scrub_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(scrub_rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    let (response, new_position) =
        style::scrubber(&mut scrub_ui, theme, position, duration, "mobile-scrub");
    if response.dragged() {
        app.ui.scrub_preview = new_position;
    } else if app.ui.scrub_preview.is_some() && !ui.input(|input| input.pointer.primary_down()) {
        if let Some(seconds) = new_position.or(app.ui.scrub_preview) {
            pending.push(Command::Seek(seconds));
        }
        app.ui.scrub_preview = None;
    } else if response.clicked() {
        if let Some(seconds) = new_position {
            pending.push(Command::Seek(seconds));
        }
    }
    let time = if duration > 0.0 {
        format!(
            "{} / {}",
            format_duration(position),
            format_duration(duration)
        )
    } else {
        String::new()
    };
    ui.painter().text(
        Pos2::new(full.left() + 12.0, full.top() + 98.0),
        Align2::LEFT_CENTER,
        time,
        style::font(9.5),
        theme.text_weak,
    );
    let _ = height;
}

/// Paint the user's theme image behind the sidebar. The central panel is left
/// untouched; the image is softly veiled so navigation keeps its contrast.
fn paint_full_theme_image(ctx: &egui::Context, app: &mut EchoPortApp, theme: &Theme) {
    let Some(path) = app.state.full_theme_image_path() else {
        return;
    };
    let Some(texture) = app.ui.full_theme_image_texture(ctx, &path) else {
        return;
    };
    let screen = ctx.screen_rect();
    let image_size = texture.size_vec2();
    if screen.width() <= 1.0 || screen.height() <= 1.0 || image_size.x <= 0.0 || image_size.y <= 0.0
    {
        return;
    }
    let mut uv = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0));
    let image_aspect = image_size.x / image_size.y;
    let screen_aspect = screen.width() / screen.height();
    if image_aspect > screen_aspect {
        let visible = screen_aspect / image_aspect;
        uv.min.x = (1.0 - visible) * 0.5;
        uv.max.x = 1.0 - uv.min.x;
    } else if image_aspect < screen_aspect {
        let visible = image_aspect / screen_aspect;
        uv.min.y = (1.0 - visible) * 0.5;
        uv.max.y = 1.0 - uv.min.y;
    }
    let painter = ctx.layer_painter(egui::LayerId::background());
    painter.image(texture.id(), screen, uv, egui::Color32::WHITE);
    // A real scrim, not just a tint: text must stay readable over arbitrary
    // bright/dark areas of a photo in full-window mode.
    let veil = theme::with_alpha(theme.panel, if theme.dark { 164 } else { 146 });
    painter.rect_filled(screen, 0.0, veil);
}

fn paint_sidebar_theme_image(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme) {
    let Some(path) = app.state.theme_image_path() else {
        return;
    };
    let Some(texture) = app.ui.theme_image_texture(ui.ctx(), &path) else {
        return;
    };
    let rect = ui.max_rect();
    if rect.width() <= 1.0 || rect.height() <= 1.0 {
        return;
    }
    let image_size = texture.size_vec2();
    if image_size.x <= 0.0 || image_size.y <= 0.0 {
        return;
    }
    // Cover-crop the image instead of stretching it.
    let image_aspect = image_size.x / image_size.y;
    let rect_aspect = rect.width() / rect.height();
    let mut uv = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0));
    if image_aspect > rect_aspect {
        let visible = rect_aspect / image_aspect;
        uv.min.x = (1.0 - visible) * 0.5;
        uv.max.x = 1.0 - uv.min.x;
    } else if image_aspect < rect_aspect {
        let visible = image_aspect / rect_aspect;
        uv.min.y = (1.0 - visible) * 0.5;
        uv.max.y = 1.0 - uv.min.y;
    }
    ui.painter()
        .image(texture.id(), rect, uv, egui::Color32::WHITE);
    // The full image theme may be light or dark independently of the old mode;
    // veil the wallpaper with the derived panel colour so icons/text retain
    // their computed contrast.
    let scrim = theme::with_alpha(theme.panel, if theme.dark { 145 } else { 205 });
    ui.painter().rect_filled(rect, 0.0, scrim);
}

// ---------------------------------------------------------------------------
// Theme image crop
// ---------------------------------------------------------------------------

fn theme_image_crop_dialog(
    ctx: &egui::Context,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
) {
    let Some(mut crop) = app.ui.crop_theme_image.take() else {
        return;
    };
    let mut keep = true;
    let mut apply = false;
    let frame = Frame::new()
        .fill(theme.surface)
        .stroke(Stroke::new(1.0_f32, theme.border))
        .corner_radius(CornerRadius::same(style::R_LG + 2))
        .inner_margin(Margin::same(20))
        .shadow(style::shadow(theme));
    egui::Modal::new(Id::new("echoport-theme-image-crop"))
        .frame(frame)
        .backdrop_color(Color32::from_black_alpha(if theme.dark { 140 } else { 80 }))
        .show(ctx, |ui| {
            ui.set_width(760.0);
            ui.label(
                RichText::new(app.state.tr(Key::CropThemeImage))
                    .size(18.0)
                    .strong()
                    .color(theme.text),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(app.state.tr(Key::CropThemeImageHint))
                    .size(11.0)
                    .color(theme.text_weak),
            );
            ui.add_space(12.0);

            if let Some(texture) = app.ui.theme_image_texture(ui.ctx(), &crop.source) {
                let preview_size = Vec2::new(ui.available_width(), 390.0);
                let (preview, response) =
                    ui.allocate_exact_size(preview_size, Sense::click_and_drag());
                ui.painter()
                    .rect_filled(preview, CornerRadius::same(style::R_MD), theme.bg);
                let image_size = texture.size_vec2();
                let scale = (preview.width() / image_size.x)
                    .min(preview.height() / image_size.y)
                    .max(0.0001);
                let image_rect = Rect::from_center_size(
                    preview.center(),
                    Vec2::new(image_size.x * scale, image_size.y * scale),
                );
                ui.painter().image(
                    texture.id(),
                    image_rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );

                let image_w = image_size.x.max(1.0) as u32;
                let image_h = image_size.y.max(1.0) as u32;
                let (cx, cy, cw, ch) = crate::theme::theme_image_crop_rect(
                    image_w,
                    image_h,
                    crop.zoom,
                    crop.center_x,
                    crop.center_y,
                );
                let crop_rect = Rect::from_min_size(
                    Pos2::new(
                        image_rect.left() + cx as f32 * scale,
                        image_rect.top() + cy as f32 * scale,
                    ),
                    Vec2::new(cw as f32 * scale, ch as f32 * scale),
                );
                // Dim everything outside the selected strip.
                let shade = Color32::from_black_alpha(150);
                if crop_rect.top() > preview.top() {
                    ui.painter().rect_filled(
                        Rect::from_min_max(
                            preview.min,
                            Pos2::new(preview.right(), crop_rect.top()),
                        ),
                        0.0,
                        shade,
                    );
                }
                if crop_rect.bottom() < preview.bottom() {
                    ui.painter().rect_filled(
                        Rect::from_min_max(
                            Pos2::new(preview.left(), crop_rect.bottom()),
                            preview.max,
                        ),
                        0.0,
                        shade,
                    );
                }
                if crop_rect.left() > preview.left() {
                    ui.painter().rect_filled(
                        Rect::from_min_max(
                            Pos2::new(preview.left(), crop_rect.top()),
                            Pos2::new(crop_rect.left(), crop_rect.bottom()),
                        ),
                        0.0,
                        shade,
                    );
                }
                if crop_rect.right() < preview.right() {
                    ui.painter().rect_filled(
                        Rect::from_min_max(
                            Pos2::new(crop_rect.right(), crop_rect.top()),
                            Pos2::new(preview.right(), crop_rect.bottom()),
                        ),
                        0.0,
                        shade,
                    );
                }
                ui.painter().rect_stroke(
                    crop_rect,
                    CornerRadius::same(4),
                    Stroke::new(2.0_f32, theme.accent),
                    egui::StrokeKind::Inside,
                );
                // Corner brackets.
                let bracket = Stroke::new(3.0_f32, Color32::WHITE);
                let arm = 14.0;
                for (corner, dx, dy) in [
                    (crop_rect.left_top(), 1.0, 1.0),
                    (crop_rect.right_top(), -1.0, 1.0),
                    (crop_rect.left_bottom(), 1.0, -1.0),
                    (crop_rect.right_bottom(), -1.0, -1.0),
                ] {
                    ui.painter()
                        .line_segment([corner, corner + Vec2::new(arm * dx, 0.0)], bracket);
                    ui.painter()
                        .line_segment([corner, corner + Vec2::new(0.0, arm * dy)], bracket);
                }
                if response.dragged() {
                    // `drag_delta()` is cumulative since press; using it every frame
                    // made the crop fly from one edge to the other. Use the
                    // per-frame pointer delta instead.
                    let delta = ui.input(|input| input.pointer.delta());
                    crop.center_x = (crop.center_x + delta.x / image_rect.width()).clamp(0.0, 1.0);
                    crop.center_y = (crop.center_y + delta.y / image_rect.height()).clamp(0.0, 1.0);
                }
                // Arrow keys nudge the crop by one source pixel for fine control.
                let (left, right, up, down) = ctx.input(|input| {
                    (
                        input.key_pressed(egui::Key::ArrowLeft),
                        input.key_pressed(egui::Key::ArrowRight),
                        input.key_pressed(egui::Key::ArrowUp),
                        input.key_pressed(egui::Key::ArrowDown),
                    )
                });
                let dx = 1.0 / image_size.x.max(1.0);
                let dy = 1.0 / image_size.y.max(1.0);
                if left {
                    crop.center_x = (crop.center_x - dx).clamp(0.0, 1.0);
                }
                if right {
                    crop.center_x = (crop.center_x + dx).clamp(0.0, 1.0);
                }
                if up {
                    crop.center_y = (crop.center_y - dy).clamp(0.0, 1.0);
                }
                if down {
                    crop.center_y = (crop.center_y + dy).clamp(0.0, 1.0);
                }
                if response.hovered() {
                    ctx.set_cursor_icon(egui::CursorIcon::Grab);
                }
            } else {
                ui.colored_label(theme.text_weak, app.state.tr(Key::ThemeImageFailed));
            }

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(app.state.tr(Key::Zoom))
                        .size(11.0)
                        .color(theme.text_weak),
                );
                ui.add(
                    egui::Slider::new(&mut crop.zoom, 1.0..=3.0)
                        .show_value(false)
                        .trailing_fill(true),
                );
                ui.label(
                    RichText::new(format!("{:.1}×", crop.zoom))
                        .size(10.0)
                        .color(theme.text_weak),
                );
            });
            ui.add_space(12.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if style::button(ui, theme, app.state.tr(Key::Save), style::Variant::Primary)
                    .clicked()
                {
                    apply = true;
                }
                ui.add_space(6.0);
                if style::button(ui, theme, app.state.tr(Key::Cancel), style::Variant::Ghost)
                    .clicked()
                {
                    keep = false;
                }
            });
        });
    if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
        keep = false;
    }
    if apply {
        pending.push(Command::SetThemeImageCropped {
            source: crop.source,
            zoom: crop.zoom,
            center_x: crop.center_x,
            center_y: crop.center_y,
        });
    } else if keep {
        app.ui.crop_theme_image = Some(crop);
    }
}

// ---------------------------------------------------------------------------
// Sidebar
// ---------------------------------------------------------------------------

fn sidebar(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme, pending: &mut Vec<Command>) {
    let screen_h = ui.ctx().screen_rect().height();
    let short = screen_h < 620.0;
    ui.spacing_mut().item_spacing = Vec2::new(4.0, if short { 1.0 } else { 3.0 });
    ui.add_space(if short { 6.0 } else { 16.0 });
    let _ = brand(ui, theme, (screen_h / 800.0).clamp(0.95, 1.35));
    ui.add_space(if short { 8.0 } else { 24.0 });

    let active_downloads = app.state.downloads.active().count();
    let nav: [(Page, Key, Icon); 5] = [
        (Page::Home, Key::Home, Icon::Home),
        (Page::Search, Key::SearchNav, Icon::Search),
        (Page::Library, Key::Library, Icon::Library),
        (Page::Playlists, Key::Playlists, Icon::Playlist),
        (Page::Downloads, Key::Downloads, Icon::Download),
    ];
    for (page, key, icon) in nav {
        let badge = if page == Page::Downloads && active_downloads > 0 {
            Some(active_downloads.to_string())
        } else {
            None
        };
        if nav_row(
            ui,
            app.state.lang,
            theme,
            icon,
            app.state.tr(key),
            app.state.page == page,
            badge.as_deref(),
        ) {
            app.set_page(page);
        }
    }

    ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
        ui.add_space(12.0);
        ui.label(
            RichText::new(format!("v{}", app.version()))
                .size(10.5)
                .color(theme.text_weak),
        );
        if app.state.demo {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(app.state.tr(Key::DemoMode))
                        .size(9.5)
                        .color(theme.text_weak),
                );
            });
        }
        ui.add_space(10.0);
        // Language quick toggle.
        let languages = [Language::En, Language::Zh];
        let labels = ["EN", "中文"];
        let selected = if app.state.lang == Language::En { 0 } else { 1 };
        if let Some(index) = style::segmented(
            ui,
            theme,
            &labels,
            selected,
            (26.0 * (ui.ctx().screen_rect().height() / 800.0)).clamp(26.0, 32.0),
        ) {
            if let Some(lang) = languages.get(index) {
                if *lang != app.state.lang {
                    pending.push(Command::SetLanguage(*lang));
                }
            }
        }
        ui.add_space(8.0);
        if nav_row(
            ui,
            app.state.lang,
            theme,
            Icon::Settings,
            app.state.tr(Key::Settings),
            app.state.page == Page::Settings,
            None,
        ) {
            app.set_page(Page::Settings);
        }
        if nav_row(
            ui,
            app.state.lang,
            theme,
            Icon::Profile,
            app.state.tr(Key::Profile),
            app.state.page == Page::Profile,
            None,
        ) {
            app.set_page(Page::Profile);
        }
        ui.add_space(4.0);
        if app.state.now_playing.is_some() && !short {
            widgets::sidebar_now_playing(ui, app, theme, pending);
            ui.add_space(8.0);
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
            ui.painter().hline(
                rect.x_range(),
                rect.center().y,
                Stroke::new(1.0_f32, theme.border),
            );
            ui.add_space(6.0);
        }
    });
}

fn brand(ui: &mut Ui, theme: &Theme, scale: f32) -> egui::Response {
    let font = style::font((24.0 * scale).clamp(24.0, 31.0));
    let galley = ui
        .painter()
        .layout_no_wrap("EchoPort".to_owned(), font, theme.text);
    let width = galley.size().x + 16.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 32.0), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    ui.painter().galley(
        Pos2::new(rect.left(), rect.center().y - galley.size().y * 0.5),
        galley.clone(),
        theme.text,
    );
    // The one and only decorative dot in the interface.
    ui.painter().circle_filled(
        Pos2::new(rect.left() + galley.size().x + 5.5, rect.center().y + 7.0),
        4.4,
        theme.accent,
    );
    let (tagline_rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 16.0), Sense::hover());
    style::paint_elided(
        ui.painter(),
        Pos2::new(tagline_rect.left(), tagline_rect.center().y),
        ui_tagline(theme),
        style::font(11.0),
        theme.text_weak,
        tagline_rect.width(),
    );
    response
}

fn ui_tagline(_theme: &Theme) -> &'static str {
    // The tagline is a static decorative line; translated via i18n in nav_row
    // callers would add noise here, so use the English tagline only in the
    // brand lockup where the app name already implies locale-neutral branding.
    "a quiet port for music"
}

fn nav_row(
    ui: &mut Ui,
    _lang: Language,
    theme: &Theme,
    icon: Icon,
    label: &str,
    selected: bool,
    badge: Option<&str>,
) -> bool {
    let scale = (ui.ctx().screen_rect().height() / 800.0).clamp(0.95, 1.35);
    let width = ui.available_width();
    let row_h = 38.0 * scale;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, row_h), Sense::click());
    let hover_t =
        ui.ctx()
            .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.12);
    let fill = if selected {
        theme::with_alpha(theme.accent_soft, 200)
    } else if hover_t > 0.01 {
        theme::with_alpha(theme.surface_hover, (225.0 * hover_t) as u8)
    } else {
        Color32::TRANSPARENT
    };
    if fill.a() > 0 {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(style::R_MD), fill);
    }
    if selected {
        ui.painter().rect_filled(
            Rect::from_min_size(
                Pos2::new(rect.left(), rect.center().y - 10.0),
                Vec2::new(3.0, 20.0),
            ),
            CornerRadius::same(2),
            theme.accent,
        );
    }
    let icon_rect = Rect::from_center_size(
        Pos2::new(rect.left() + 22.0 * scale, rect.center().y),
        Vec2::splat(17.0 * scale),
    );
    style::paint_icon(
        ui.painter(),
        icon,
        icon_rect,
        if selected {
            theme.accent
        } else {
            theme::mix(theme.text_weak, theme.text, hover_t.max(0.55))
        },
    );
    let text_color = if selected {
        theme.accent
    } else {
        theme::mix(theme.text_weak, theme.text, hover_t.max(0.55))
    };
    let mut right = rect.right() - 12.0;
    if let Some(badge) = badge {
        let font = style::font(10.0);
        let galley = ui
            .painter()
            .layout_no_wrap(badge.to_owned(), font, theme.on_accent);
        let badge_rect = Rect::from_center_size(
            Pos2::new(right - galley.size().x * 0.5 - 7.0, rect.center().y),
            Vec2::new(galley.size().x + 14.0, 17.0),
        );
        ui.painter()
            .rect_filled(badge_rect, CornerRadius::same(9), theme.highlight);
        ui.painter().galley(
            Pos2::new(
                badge_rect.center().x - galley.size().x * 0.5,
                badge_rect.center().y - galley.size().y * 0.5,
            ),
            galley,
            theme.on_highlight,
        );
        right = badge_rect.left() - 6.0;
    }
    style::paint_elided(
        ui.painter(),
        Pos2::new(rect.left() + 41.0 * scale, rect.center().y),
        label,
        style::font(13.0 * scale),
        text_color,
        right - rect.left() - 41.0 * scale,
    );
    response.on_hover_cursor(CursorIcon::PointingHand).clicked()
}

// ---------------------------------------------------------------------------
// Player bar
// ---------------------------------------------------------------------------

fn player_bar(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    height: f32,
    sidebar_width: f32,
    pending: &mut Vec<Command>,
) {
    let rect = ui.max_rect();
    let panel = Rect::from_min_size(rect.min, Vec2::new(rect.width(), height));
    ui.painter().hline(
        panel.x_range(),
        panel.top(),
        Stroke::new(1.0_f32, theme.border),
    );
    let inner = Rect::from_min_max(
        Pos2::new(panel.left(), panel.top() + 1.0),
        Pos2::new(panel.right(), panel.top() + height),
    );
    let compact = panel.width() < 1000.0;
    let scale_h = (height / 96.0).clamp(0.88, 1.35);
    let cluster_w =
        (30.0 + 34.0 + 46.0 + 34.0 + 30.0 + 30.0 + 4.0 + 6.0 + 6.0 + 6.0 + 4.0) * scale_h;
    let mut left_w = if compact {
        (panel.width() * 0.245).clamp(170.0, 260.0)
    } else {
        (318.0 + (panel.width() - 1200.0) * 0.055).clamp(318.0, 430.0)
    };
    let mut right_w = if compact {
        (panel.width() * 0.255).clamp(180.0, 280.0)
    } else {
        (280.0 + (panel.width() - 1200.0) * 0.040).clamp(280.0, 370.0)
    };
    // Keep enough room for the transport cluster on very narrow/zoomed windows.
    let middle = panel.width() - left_w - right_w;
    if middle < cluster_w + 16.0 {
        let deficit = cluster_w + 16.0 - middle;
        let left_cut = (deficit * 0.5).min((left_w - 150.0).max(0.0));
        let right_cut = (deficit - left_cut).min((right_w - 160.0).max(0.0));
        left_w -= left_cut;
        right_w -= right_cut;
    }
    let left_rect = Rect::from_min_size(
        Pos2::new(inner.left(), inner.top()),
        Vec2::new(left_w, inner.height()),
    );
    let right_rect = Rect::from_min_size(
        Pos2::new(inner.right() - right_w, inner.top()),
        Vec2::new(right_w, inner.height()),
    );
    // The page content starts after the sidebar, so centre the transport on
    // the *content column* centre, not the full window centre.
    // Keep the original centred player layout, but nudge the whole transport
    // cluster (transport + scrubber) left as a group.
    let content_center_x = inner.left() + (inner.width() + sidebar_width) * 0.5 - 64.0;
    let half = (right_rect.left() - content_center_x)
        .min(content_center_x - left_rect.right())
        .max(120.0);
    let center_rect = Rect::from_center_size(
        Pos2::new(content_center_x, inner.center().y),
        Vec2::new(half * 2.0, inner.height()),
    );
    let track = app.state.now_playing.clone();
    let playing = app.state.playback.state.is_playing();
    let loading = app.state.playback.state == PlaybackState::Loading;

    style::absolute(ui, left_rect, |ui| {
        player_left(ui, app, theme, track.as_ref(), loading, inner.height());
    });
    if center_rect.width() > 200.0 {
        style::absolute(ui, center_rect, |ui| {
            player_center(ui, app, theme, playing, inner.height(), pending);
        });
    }
    style::absolute(ui, right_rect, |ui| {
        player_right(ui, app, theme, track.as_ref(), compact, pending);
    });
    ui.advance_cursor_after_rect(panel);
}

fn player_left(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    track: Option<&Track>,
    loading: bool,
    height: f32,
) {
    let _ = loading;
    let full = ui.max_rect();
    let cover = (height * 0.56).clamp(44.0, 74.0);
    let cover_rect = Rect::from_center_size(
        Pos2::new(full.left() + 16.0 + cover * 0.5, full.center().y),
        Vec2::splat(cover),
    );
    let cover_response = ui.interact(cover_rect, ui.id().with("player-cover"), Sense::click());
    match track {
        Some(track) => {
            widgets::paint_track_cover(ui, app, theme, track, cover_rect, style::R_MD);
        }
        None => {
            ui.painter().rect_filled(
                cover_rect,
                CornerRadius::same(style::R_MD),
                style::wash(theme),
            );
            let note = cover_rect.shrink(16.0);
            if !style::paint_note(ui.painter(), note, theme.text_weak) {
                style::paint_icon(ui.painter(), Icon::Music, note, theme.text_weak);
            }
        }
    }
    if loading {
        ui.painter().rect_filled(
            cover_rect,
            CornerRadius::same(style::R_MD),
            Color32::from_black_alpha(70),
        );
        let time = ui.ctx().input(|input| input.time);
        style::equalizer(
            ui.painter(),
            Rect::from_center_size(cover_rect.center(), Vec2::new(20.0, 20.0)),
            theme,
            true,
            time,
        );
    }
    if cover_response.clicked() && track.is_some() {
        app.set_page(Page::NowPlaying);
    }
    let text_left = cover_rect.right() + 12.0;
    let text_right = (full.right() - 12.0).max(text_left + 40.0);
    let text_rect = Rect::from_min_max(
        Pos2::new(text_left, full.top()),
        Pos2::new(text_right, full.bottom()),
    );
    style::absolute(ui, text_rect, |ui| {
        ui.add_space((height - 40.0).max(4.0) * 0.5);
        match track {
            Some(track) => {
                let play_state = app.state.playback.state;
                let mut open_now = false;
                ui.horizontal(|ui| {
                    let title = ui.add(
                        egui::Label::new(
                            RichText::new(&track.title)
                                .size((13.5 * (height / 96.0)).clamp(13.5, 18.0))
                                .strong()
                                .color(theme.text),
                        )
                        .truncate(),
                    );
                    if title.clicked() {
                        open_now = true;
                    }
                    ui.add_space(3.0);
                    let (badge_rect, _) =
                        ui.allocate_exact_size(Vec2::new(14.0, 13.0), Sense::hover());
                    let time = ui.ctx().input(|input| input.time);
                    match play_state {
                        PlaybackState::Playing => {
                            style::equalizer(ui.painter(), badge_rect, theme, true, time);
                        }
                        PlaybackState::Loading => {
                            style::loading_dots(ui.painter(), badge_rect, theme, time);
                            ui.ctx().request_repaint_after(Duration::from_millis(90));
                        }
                        PlaybackState::Error => {
                            style::paint_icon(
                                ui.painter(),
                                Icon::Info,
                                badge_rect,
                                style::danger_color(theme),
                            );
                        }
                        _ => {}
                    }
                });
                if open_now {
                    app.set_page(Page::NowPlaying);
                }
                let subtitle = format!("{} · {}", track.display_artist(), track.platform.label());
                ui.add(
                    egui::Label::new(
                        RichText::new(subtitle)
                            .size((10.5 * (height / 96.0)).clamp(10.5, 14.0))
                            .color(theme.text_weak),
                    )
                    .truncate(),
                );
            }
            None => {
                ui.label(
                    RichText::new(app.state.tr(Key::EmptyStateHint))
                        .size(11.5)
                        .color(theme.text_weak),
                );
            }
        }
    });
}

fn player_center(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    playing: bool,
    height: f32,
    pending: &mut Vec<Command>,
) {
    // Shared transport cluster: source · prev · play · next · repeat · shuffle.
    // Anchor the *play* button on the content-column centre even though the
    // cluster is asymmetric (source left, repeat+shuffle right).
    let s = (height / 96.0).clamp(0.88, 1.35);
    ui.add_space((height - 74.0 * s).max(4.0) * 0.42);
    let sizes = [30.0 * s, 34.0 * s, 46.0 * s, 34.0 * s, 30.0 * s, 30.0 * s];
    let gaps = [4.0, 6.0, 6.0, 6.0, 4.0];
    let play_offset = sizes[0] + gaps[0] + sizes[1] + gaps[1] + sizes[2] * 0.5;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(((ui.available_width() * 0.5) - play_offset).max(0.0));
        widgets::transport_row(
            ui, app, theme, pending, playing, "bar", sizes[2], &sizes, &gaps,
        );
    });
    ui.add_space(3.0);

    let duration = app.state.playback.duration_secs.unwrap_or(0.0);
    let position = app
        .ui
        .scrub_preview
        .unwrap_or(app.state.playback.position_secs);
    // Symmetric time labels + scrubber: the row's centre is the content-column
    // centre, so the scrubber centre, the play button and the page transport
    // all share the same x.
    let row_w = ui.available_width();
    let (row, _) = ui.allocate_exact_size(Vec2::new(row_w, 18.0), Sense::hover());
    let label_w = 42.0;
    let font = style::font(10.0);
    ui.painter().text(
        Pos2::new(row.left() + label_w, row.center().y),
        Align2::RIGHT_CENTER,
        format_duration(position),
        font.clone(),
        theme.text_weak,
    );
    ui.painter().text(
        Pos2::new(row.right() - label_w, row.center().y),
        Align2::LEFT_CENTER,
        format_duration(duration),
        font,
        theme.text_weak,
    );
    let scrub_rect = Rect::from_min_max(
        Pos2::new(row.left() + label_w + 12.0, row.top()),
        Pos2::new(row.right() - label_w - 12.0, row.bottom()),
    );
    let mut scrub_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(scrub_rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    let (response, new_position) =
        style::scrubber(&mut scrub_ui, theme, position, duration, "player-scrub");
    let pointer_down = ui.input(|input| input.pointer.primary_down());
    if response.dragged() {
        app.ui.scrub_preview = new_position;
    } else if app.ui.scrub_preview.is_some() && !pointer_down {
        // Release can happen outside the widget; always commit the preview.
        if let Some(seconds) = new_position.or(app.ui.scrub_preview) {
            pending.push(Command::Seek(seconds));
        }
        app.ui.scrub_preview = None;
    } else if response.clicked() {
        if let Some(seconds) = new_position {
            pending.push(Command::Seek(seconds));
        }
    }
}

fn player_right(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    track: Option<&Track>,
    compact: bool,
    pending: &mut Vec<Command>,
) {
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        let tools_width = ui.max_rect().width();
        let narrow = tools_width < 220.0;
        // Move the whole tool cluster a bit further from the window edge.
        ui.add_space(12.0 + if narrow { 24.0 } else { 32.0 });
        let is_background = app.state.now_playing_tab == crate::state::NowPlayingTab::Background
            && app.state.page == Page::NowPlaying;
        if style::icon_button(
            ui,
            theme,
            Icon::Info,
            26.0,
            is_background,
            app.state.tr(Key::SongBackground),
        )
        .clicked()
        {
            if let Some(track) = track {
                pending.push(Command::LoadBackground(Box::new(track.clone())));
            }
        }
        let volume = app.state.playback.volume;
        let slider_width = if compact {
            if narrow {
                40.0
            } else {
                52.0
            }
        } else {
            76.0
        };
        let mut new_volume = None;
        ui.allocate_ui_with_layout(
            Vec2::new(slider_width, 16.0),
            Layout::left_to_right(Align::Center),
            |ui| {
                if let Some(value) = style::volume_slider(ui, theme, volume, slider_width) {
                    new_volume = Some(value);
                }
            },
        );
        if let Some(value) = new_volume {
            pending.push(Command::SetVolume(value));
        }
        if style::icon_button(
            ui,
            theme,
            if volume <= 0.001 {
                Icon::VolumeMute
            } else {
                Icon::Volume
            },
            24.0,
            false,
            app.state.tr(Key::Volume),
        )
        .clicked()
        {
            if volume > 0.001 {
                app.ui.volume_before_mute = volume;
                pending.push(Command::SetVolume(0.0));
            } else {
                pending.push(Command::SetVolume(app.ui.volume_before_mute.max(0.2)));
            }
        }
        if style::icon_button(
            ui,
            theme,
            Icon::Queue,
            26.0,
            app.state.queue_panel_open,
            app.state.tr(if app.state.queue_panel_open {
                Key::HideQueue
            } else {
                Key::ShowQueue
            }),
        )
        .clicked()
        {
            pending.push(Command::ToggleQueuePanel);
        }
        let is_favorite = track
            .map(|track| app.state.is_favorite(&track.id))
            .unwrap_or(false);
        if style::icon_button(
            ui,
            theme,
            if is_favorite {
                Icon::HeartFill
            } else {
                Icon::Heart
            },
            26.0,
            is_favorite,
            app.state.tr(if is_favorite {
                Key::RemoveFromFavorites
            } else {
                Key::AddToFavorites
            }),
        )
        .clicked()
        {
            if let Some(track) = track {
                pending.push(Command::ToggleFavorite(Box::new(track.clone())));
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Queue overlay
// ---------------------------------------------------------------------------

fn queue_panel(
    ctx: &egui::Context,
    app: &mut EchoPortApp,
    theme: &Theme,
    player_height: f32,
    pending: &mut Vec<Command>,
) {
    let max_list = (ctx.screen_rect().height() - player_height - 120.0).clamp(150.0, 440.0);
    let area = egui::Area::new(Id::new("queue-overlay"))
        .order(Order::Foreground)
        .anchor(
            Align2::RIGHT_BOTTOM,
            Vec2::new(-16.0, -(player_height + 10.0)),
        )
        .interactable(true)
        .show(ctx, |ui| {
            Frame::new()
                .fill(theme.surface)
                .stroke(Stroke::new(1.0_f32, theme.border))
                .corner_radius(CornerRadius::same(style::R_LG + 2))
                .inner_margin(Margin::same(14))
                .shadow(style::shadow(theme))
                .show(ui, |ui| {
                    ui.set_width(332.0);
                    ui.set_min_height(max_list + 56.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(app.state.tr(Key::Queue))
                                .size(15.0)
                                .strong()
                                .color(theme.text),
                        );
                        ui.label(
                            RichText::new(style::tracks_count(&app.state, app.state.queue.len()))
                                .size(11.0)
                                .color(theme.text_weak),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if style::icon_button(
                                ui,
                                theme,
                                Icon::Close,
                                24.0,
                                false,
                                app.state.tr(Key::Close),
                            )
                            .clicked()
                            {
                                pending.push(Command::ToggleQueuePanel);
                            }
                            if !app.state.queue.is_empty()
                                && style::icon_button(
                                    ui,
                                    theme,
                                    Icon::Trash,
                                    24.0,
                                    false,
                                    app.state.tr(Key::ClearQueue),
                                )
                                .clicked()
                            {
                                pending.push(Command::ClearQueue);
                            }
                        });
                    });
                    ui.add_space(6.0);
                    if app.state.queue.is_empty() {
                        ui.add_space(24.0);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                RichText::new(app.state.tr(Key::QueueEmpty))
                                    .size(12.0)
                                    .color(theme.text_weak),
                            );
                        });
                        ui.add_space(24.0);
                    } else {
                        let queue = app.state.queue.clone();
                        egui::ScrollArea::vertical()
                            .id_salt("queue-scroll")
                            .max_height(max_list)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.spacing_mut().item_spacing.y = 4.0;
                                for index in 0..queue.len() {
                                    widgets::queue_row(ui, app, theme, pending, &queue, index);
                                }
                            });
                    }
                });
        });
    let _ = area;
}

// ---------------------------------------------------------------------------
// Toasts
// ---------------------------------------------------------------------------

/// Non-modal prompt shown after audio files are dropped: playback has already
/// started, so the user can decide whether to keep the files in the Library.
fn drop_prompt(
    ctx: &egui::Context,
    app: &mut EchoPortApp,
    theme: &Theme,
    player_height: f32,
    pending: &mut Vec<Command>,
) {
    let Some(tracks) = app.state.drop_prompt.clone() else {
        return;
    };
    let mut save = false;
    let mut dismiss = false;
    egui::Area::new(Id::new("drop-prompt"))
        .order(Order::Foreground)
        .anchor(
            Align2::RIGHT_BOTTOM,
            Vec2::new(-18.0, -(player_height + 74.0)),
        )
        .interactable(true)
        .show(ctx, |ui| {
            Frame::new()
                .fill(theme.surface)
                .stroke(Stroke::new(1.0_f32, theme.border))
                .corner_radius(CornerRadius::same(style::R_MD))
                .inner_margin(Margin::symmetric(14, 12))
                .shadow(style::shadow(theme))
                .show(ui, |ui| {
                    ui.set_width(330.0);
                    ui.label(
                        RichText::new(app.state.tr(Key::DroppedAudioPrompt))
                            .size(11.5)
                            .color(theme.text),
                    );
                    ui.add_space(3.0);
                    let name = tracks
                        .first()
                        .map(|track| {
                            track
                                .local_path
                                .as_ref()
                                .and_then(|path| path.file_name())
                                .map(|name| name.to_string_lossy().into_owned())
                                .unwrap_or_else(|| track.title.clone())
                        })
                        .unwrap_or_default();
                    let extra = tracks.len().saturating_sub(1);
                    let detail = if extra == 0 {
                        name
                    } else {
                        format!("{name} +{extra}")
                    };
                    ui.label(RichText::new(detail).size(10.0).color(theme.text_weak));
                    ui.add_space(9.0);
                    ui.horizontal(|ui| {
                        if style::button(
                            ui,
                            theme,
                            app.state.tr(Key::SaveToLibrary),
                            style::Variant::Primary,
                        )
                        .clicked()
                        {
                            save = true;
                        }
                        if style::button(
                            ui,
                            theme,
                            app.state.tr(Key::DontSave),
                            style::Variant::Ghost,
                        )
                        .clicked()
                        {
                            dismiss = true;
                        }
                    });
                });
        });
    if save {
        pending.push(Command::SaveDroppedToLibrary(tracks));
    } else if dismiss {
        app.state.drop_prompt = None;
    }
}

fn crash_prompt(ctx: &egui::Context, app: &mut EchoPortApp, theme: &Theme, player_height: f32) {
    if app.crash_notice.is_none() {
        return;
    }
    let mut open_logs = false;
    let mut dismiss = false;
    egui::Area::new(Id::new("crash-prompt"))
        .order(Order::Foreground)
        .anchor(
            Align2::RIGHT_BOTTOM,
            Vec2::new(-18.0, -(player_height + 16.0)),
        )
        .interactable(true)
        .show(ctx, |ui| {
            Frame::new()
                .fill(theme.surface)
                .stroke(Stroke::new(1.0_f32, style::danger_color(theme)))
                .corner_radius(CornerRadius::same(style::R_MD))
                .inner_margin(Margin::symmetric(14, 12))
                .shadow(style::shadow(theme))
                .show(ui, |ui| {
                    ui.set_width(360.0);
                    ui.label(
                        RichText::new(app.state.tr(Key::CrashDetected))
                            .size(12.5)
                            .strong()
                            .color(theme.text),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(app.state.tr(Key::CrashDetectedBody))
                            .size(11.0)
                            .color(theme.text_weak),
                    );
                    ui.add_space(9.0);
                    ui.horizontal(|ui| {
                        if style::button(
                            ui,
                            theme,
                            app.state.tr(Key::OpenLogsAndCopy),
                            style::Variant::Primary,
                        )
                        .clicked()
                        {
                            open_logs = true;
                        }
                        if style::button(ui, theme, app.state.tr(Key::Close), style::Variant::Ghost)
                            .clicked()
                        {
                            dismiss = true;
                        }
                    });
                });
        });
    if open_logs {
        app.open_logs();
    }
    if open_logs || dismiss {
        app.crash_notice = None;
    }
}

fn toasts(
    ctx: &egui::Context,
    app: &mut EchoPortApp,
    theme: &Theme,
    player_height: f32,
    lift: f32,
) {
    if app.state.toasts.is_empty() {
        return;
    }
    let has_error = app
        .state
        .toasts
        .iter()
        .any(|toast| toast.kind == ToastKind::Error);
    let now = ctx.input(|input| input.time);
    let mut dismiss = None;
    let mut open_logs = false;
    egui::Area::new(Id::new("toasts"))
        .order(Order::Foreground)
        .anchor(
            Align2::RIGHT_BOTTOM,
            Vec2::new(-18.0, -(player_height + 14.0 + lift)),
        )
        // Info/success toasts let clicks pass through; error toasts expose a
        // "view logs" action and therefore must be interactive.
        .interactable(has_error)
        .show(ctx, |ui| {
            ui.set_width(310.0);
            ui.with_layout(Layout::bottom_up(Align::Max), |ui| {
                for (index, toast) in app.state.toasts.iter().enumerate().rev() {
                    let age = (now - toast.created_at).max(0.0);
                    let ttl = if toast.kind == ToastKind::Error {
                        8.0
                    } else {
                        3.4
                    };
                    let alpha = if age > ttl {
                        (1.0 - ((age - ttl) / 0.8) as f32).clamp(0.0, 1.0)
                    } else {
                        1.0
                    };
                    if alpha <= 0.0 {
                        continue;
                    }
                    ui.scope(|ui| {
                        ui.multiply_opacity(alpha);
                        let accent = match toast.kind {
                            ToastKind::Info => theme.accent,
                            ToastKind::Success => style::success_color(theme),
                            ToastKind::Error => style::danger_color(theme),
                        };
                        let icon = if toast.kind == ToastKind::Success {
                            Icon::Check
                        } else {
                            Icon::Info
                        };
                        Frame::new()
                            .fill(theme.surface)
                            .stroke(Stroke::new(1.0_f32, theme::mix(theme.border, accent, 0.35)))
                            .corner_radius(CornerRadius::same(style::R_MD))
                            .inner_margin(Margin::symmetric(12, 9))
                            .shadow(style::shadow(theme))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let (rect, _) =
                                        ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
                                    ui.painter().rect_filled(
                                        rect,
                                        CornerRadius::same(5),
                                        theme::with_alpha(accent, 40),
                                    );
                                    style::paint_icon(ui.painter(), icon, rect.shrink(4.0), accent);
                                    let response = ui.add(
                                        egui::Label::new(
                                            RichText::new(&toast.text).size(11.5).color(theme.text),
                                        )
                                        .wrap(),
                                    );
                                    if response.clicked() {
                                        dismiss = Some(index);
                                    }
                                });
                                if toast.kind == ToastKind::Error {
                                    ui.add_space(5.0);
                                    ui.horizontal(|ui| {
                                        if style::button(
                                            ui,
                                            theme,
                                            app.state.tr(Key::ViewLogs),
                                            style::Variant::Ghost,
                                        )
                                        .clicked()
                                        {
                                            open_logs = true;
                                            dismiss = Some(index);
                                        }
                                    });
                                }
                            });
                    });
                    ui.add_space(6.0);
                }
            });
        });
    if open_logs {
        app.open_logs();
    }
    if let Some(index) = dismiss {
        if index < app.state.toasts.len() {
            app.state.toasts.remove(index);
        }
    }
}

// ---------------------------------------------------------------------------
// Dialogs
// ---------------------------------------------------------------------------

fn dialogs(ctx: &egui::Context, app: &mut EchoPortApp, theme: &Theme, pending: &mut Vec<Command>) {
    let Some(mut dialog) = app.ui.dialog.take() else {
        return;
    };
    let mut keep = true;
    let frame = Frame::new()
        .fill(theme.surface)
        .stroke(Stroke::new(1.0_f32, theme.border))
        .corner_radius(CornerRadius::same(style::R_LG + 2))
        .inner_margin(Margin::same(22))
        .shadow(style::shadow(theme));
    let response = egui::Modal::new(Id::new("echoport-dialog"))
        .frame(frame)
        .backdrop_color(Color32::from_black_alpha(if theme.dark { 120 } else { 70 }))
        .show(ctx, |ui| {
            ui.set_width(360.0);
            let (title, body, confirm_label, danger) = match &dialog {
                Dialog::NewPlaylist { .. } => (
                    app.state.tr(Key::NewPlaylist),
                    app.state.tr(Key::NoPlaylistsHint),
                    app.state.tr(Key::CreatePlaylist),
                    false,
                ),
                Dialog::RenamePlaylist { .. } => (
                    app.state.tr(Key::EditPlaylist),
                    "",
                    app.state.tr(Key::Save),
                    false,
                ),
                Dialog::DeletePlaylist { .. } => (
                    app.state.tr(Key::DeletePlaylist),
                    app.state.tr(Key::ConfirmDeletePlaylist),
                    app.state.tr(Key::Delete),
                    true,
                ),
                Dialog::NewProfile { .. } => (
                    app.state.tr(Key::NewProfile),
                    app.state.tr(Key::PrivacyNote),
                    app.state.tr(Key::Create),
                    false,
                ),
                Dialog::DeleteProfile { .. } => (
                    app.state.tr(Key::DeleteProfile),
                    app.state.tr(Key::DeleteProfileConfirm),
                    app.state.tr(Key::Delete),
                    true,
                ),
                Dialog::Download { .. } => (
                    app.state.tr(Key::DownloadOptions),
                    "",
                    app.state.tr(Key::Download),
                    false,
                ),
                Dialog::ExportProfile => (
                    app.state.tr(Key::ExportProfile),
                    app.state.tr(Key::ExportProfileWarning),
                    app.state.tr(Key::ExportProfile),
                    false,
                ),
            };
            ui.label(RichText::new(title).size(18.0).strong().color(theme.text));
            ui.add_space(6.0);
            if !body.is_empty() {
                ui.label(RichText::new(body).size(12.0).color(theme.text_weak));
                ui.add_space(10.0);
            }
            let mut submitted = false;
            match &mut dialog {
                Dialog::NewPlaylist {
                    name, description, ..
                } => {
                    let response = dialog_text_input(
                        ui,
                        theme,
                        "dialog-name",
                        name,
                        app.state.tr(Key::PlaylistName),
                    );
                    if response.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter))
                    {
                        submitted = true;
                    }
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(app.state.tr(Key::PlaylistDescription))
                            .size(11.0)
                            .color(theme.text_weak),
                    );
                    ui.add_space(4.0);
                    dialog_multiline_input(
                        ui,
                        theme,
                        "dialog-playlist-description",
                        description,
                        app.state.tr(Key::PlaylistDescription),
                        2,
                    );
                }
                Dialog::NewProfile { name, bio } => {
                    let response = dialog_text_input(
                        ui,
                        theme,
                        "dialog-name",
                        name,
                        app.state.tr(Key::PlaylistName),
                    );
                    if response.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter))
                    {
                        submitted = true;
                    }
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(app.state.tr(Key::BioLabel))
                            .size(11.0)
                            .color(theme.text_weak),
                    );
                    ui.add_space(4.0);
                    dialog_multiline_input(
                        ui,
                        theme,
                        "dialog-profile-bio",
                        bio,
                        app.state.tr(Key::BioLabel),
                        3,
                    );
                }
                Dialog::RenamePlaylist {
                    name,
                    description,
                    existing_cover,
                    new_cover,
                    clear_cover,
                    ..
                } => {
                    let response = dialog_text_input(
                        ui,
                        theme,
                        "dialog-name",
                        name,
                        app.state.tr(Key::PlaylistName),
                    );
                    if response.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter))
                    {
                        submitted = true;
                    }
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(app.state.tr(Key::PlaylistDescription))
                            .size(11.0)
                            .color(theme.text_weak),
                    );
                    ui.add_space(4.0);
                    dialog_multiline_input(
                        ui,
                        theme,
                        "dialog-playlist-description",
                        description,
                        app.state.tr(Key::PlaylistDescription),
                        2,
                    );
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if style::button(
                            ui,
                            theme,
                            app.state.tr(Key::ChooseCover),
                            style::Variant::Secondary,
                        )
                        .clicked()
                        {
                            if let Some(path) = crate::platform::pick_file(
                                "Cover",
                                &[("Images", &["png", "jpg", "jpeg", "webp", "gif", "bmp"])],
                            ) {
                                *new_cover = Some(path);
                                *clear_cover = false;
                            }
                        }
                        if (existing_cover.is_some() || new_cover.is_some())
                            && style::button(
                                ui,
                                theme,
                                app.state.tr(Key::RemoveCover),
                                style::Variant::Ghost,
                            )
                            .clicked()
                        {
                            *new_cover = None;
                            *clear_cover = true;
                        }
                    });
                    if let Some(path) = new_cover.as_ref() {
                        ui.add_space(2.0);
                        style::hint_line(
                            ui,
                            theme,
                            &path
                                .file_name()
                                .map(|name| name.to_string_lossy().into_owned())
                                .unwrap_or_default(),
                        );
                    }
                }
                Dialog::Download { format, .. } => {
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(app.state.tr(Key::DownloadFormat))
                            .size(11.0)
                            .color(theme.text_weak),
                    );
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        for candidate in DownloadFormat::ALL {
                            if style::chip(ui, theme, candidate.label(), None, *format == candidate)
                                .clicked()
                            {
                                *format = candidate;
                            }
                        }
                    });
                    ui.add_space(8.0);
                    let folder = app
                        .state
                        .profile
                        .settings
                        .download_dir
                        .to_string_lossy()
                        .into_owned();
                    style::hint_line(ui, theme, &folder);
                }
                Dialog::ExportProfile
                | Dialog::DeletePlaylist { .. }
                | Dialog::DeleteProfile { .. } => {}
            }
            ui.add_space(16.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let confirm = style::button(
                    ui,
                    theme,
                    confirm_label,
                    if danger {
                        style::Variant::Danger
                    } else {
                        style::Variant::Primary
                    },
                );
                if confirm.clicked() {
                    submitted = true;
                }
                ui.add_space(6.0);
                let cancel =
                    style::button(ui, theme, app.state.tr(Key::Cancel), style::Variant::Ghost);
                if cancel.clicked() {
                    keep = false;
                }
                if submitted {
                    match &dialog {
                        Dialog::NewPlaylist {
                            seed,
                            name,
                            description,
                        } => {
                            let name = if name.trim().is_empty() {
                                format!(
                                    "{} {}",
                                    app.state.tr(Key::NewPlaylist),
                                    app.state.profile.playlists.len() + 1
                                )
                            } else {
                                name.trim().to_owned()
                            };
                            pending.push(Command::CreatePlaylist {
                                name,
                                description: description.trim().to_owned(),
                                tracks: seed.clone(),
                            });
                            keep = false;
                        }
                        Dialog::RenamePlaylist {
                            id,
                            name,
                            description,
                            new_cover,
                            clear_cover,
                            ..
                        } => {
                            if !name.trim().is_empty() {
                                pending.push(Command::RenamePlaylist {
                                    id: id.clone(),
                                    name: name.trim().to_owned(),
                                });
                                pending.push(Command::SetPlaylistDescription {
                                    id: id.clone(),
                                    description: description.clone(),
                                });
                                if *clear_cover {
                                    pending.push(Command::SetPlaylistCover {
                                        id: id.clone(),
                                        source: None,
                                    });
                                } else if let Some(path) = new_cover.clone() {
                                    pending.push(Command::SetPlaylistCover {
                                        id: id.clone(),
                                        source: Some(path),
                                    });
                                }
                            }
                            keep = false;
                        }
                        Dialog::DeletePlaylist { id, .. } => {
                            pending.push(Command::DeletePlaylist(id.clone()));
                            keep = false;
                        }
                        Dialog::ExportProfile => {
                            pending.push(Command::ExportProfile);
                            keep = false;
                        }
                        Dialog::Download { tracks, format } => {
                            pending.push(Command::StartDownload {
                                tracks: tracks.clone(),
                                format: *format,
                            });
                            keep = false;
                        }
                        Dialog::NewProfile { name, bio } => {
                            if !name.trim().is_empty() {
                                pending.push(Command::NewProfile {
                                    name: name.trim().to_owned(),
                                    bio: bio.trim().to_owned(),
                                });
                            }
                            keep = false;
                        }
                        Dialog::DeleteProfile { name } => {
                            pending.push(Command::DeleteProfile(name.clone()));
                            keep = false;
                        }
                    }
                }
            });
        });
    if response.backdrop_response.clicked() {
        keep = false;
    }
    if keep {
        app.ui.dialog = Some(dialog);
    }
}

/// Multiline variant of the dialog input. The frame and inner padding are drawn
/// explicitly so the cursor starts inside the box (the raw multiline text edit
/// previously showed a grey native background with the caret on its edge).
fn dialog_multiline_input(
    ui: &mut Ui,
    theme: &Theme,
    id_salt: &str,
    value: &mut String,
    hint: &str,
    rows: usize,
) -> egui::Response {
    let id = Id::new(id_salt);
    let height = 10.0 + rows as f32 * 19.0;
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let focused = ui.ctx().memory(|memory| memory.has_focus(id));
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(style::R_MD),
        if focused {
            theme.surface
        } else {
            style::wash(theme)
        },
    );
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(style::R_MD),
        Stroke::new(
            if focused { 1.5_f32 } else { 1.0_f32 },
            if focused { theme.accent } else { theme.border },
        ),
        StrokeKind::Inside,
    );
    // Deliberately inset the actual editor: this gives the caret and first line
    // the same left/top breathing room as the single-line name field.
    let inner = rect.shrink2(Vec2::new(12.0, 9.0));
    ui.put(
        inner,
        egui::TextEdit::multiline(value)
            .id(id)
            .frame(false)
            .margin(Margin::ZERO)
            .hint_text(hint)
            .font(style::font(12.5))
            .text_color(theme.text)
            .desired_width(inner.width()),
    )
}

fn dialog_text_input(
    ui: &mut Ui,
    theme: &Theme,
    id_salt: &str,
    value: &mut String,
    hint: &str,
) -> egui::Response {
    let id = Id::new(id_salt);
    let height = 38.0;
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let focused = ui.ctx().memory(|memory| memory.has_focus(id));
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(style::R_MD),
        if focused {
            theme.surface
        } else {
            style::wash(theme)
        },
    );
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(style::R_MD),
        Stroke::new(
            if focused { 1.5_f32 } else { 1.0_f32 },
            if focused { theme.accent } else { theme.border },
        ),
        StrokeKind::Inside,
    );
    let inner = rect.shrink2(Vec2::new(12.0, 8.0));
    ui.put(
        inner,
        egui::TextEdit::singleline(value)
            .id(id)
            .frame(false)
            .hint_text(hint)
            .font(style::font(13.5))
            .text_color(theme.text)
            .desired_width(inner.width()),
    )
}

// ---------------------------------------------------------------------------
// Keyboard (Esc only; Space and arrows are handled by the app shell so they
// respect text-field focus)
// ---------------------------------------------------------------------------

fn handle_escape(ctx: &egui::Context, app: &mut EchoPortApp) {
    if ctx.memory(|memory| memory.focused().is_some()) {
        return;
    }
    if !ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
        return;
    }
    if app.ui.dialog.is_some() {
        app.ui.dialog = None;
    } else if egui::Popup::is_any_open(ctx) {
        egui::Popup::close_all(ctx);
    } else if app.state.queue_panel_open {
        app.state.queue_panel_open = false;
    } else if app.state.page == Page::NowPlaying {
        app.set_page(Page::Home);
    } else if app.state.selected_playlist.is_some() {
        app.state.selected_playlist = None;
    }
}

// ---------------------------------------------------------------------------
// Theme mode helpers used by the settings page
// ---------------------------------------------------------------------------

pub fn theme_mode_label(state: &crate::state::AppState, mode: ThemeMode) -> &'static str {
    match mode {
        ThemeMode::Pulse => state.tr(Key::ThemePulse),
        ThemeMode::Tide => state.tr(Key::ThemeTide),
        ThemeMode::Ink => state.tr(Key::ThemeInk),
    }
}

pub fn theme_mode_hint(state: &crate::state::AppState, mode: ThemeMode) -> &'static str {
    match mode {
        ThemeMode::Pulse => state.tr(Key::ThemePulseHint),
        ThemeMode::Tide => state.tr(Key::ThemeTideHint),
        ThemeMode::Ink => state.tr(Key::ThemeInkHint),
    }
}
