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
use crate::model::{format_duration, Platform, PlaybackState, Track};
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
    },
    DeleteProfile {
        name: String,
    },
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
}

impl UiState {
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
        }
    }
}

// ---------------------------------------------------------------------------
// Shell
// ---------------------------------------------------------------------------

pub fn layout(ctx: &egui::Context, app: &mut EchoPortApp) {
    let theme = app.state.theme;
    let mut pending: Vec<Command> = Vec::new();

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

    egui::TopBottomPanel::bottom("echoport-player")
        .exact_height(player_height)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().fill(theme.panel))
        .show(ctx, |ui| {
            player_bar(ui, app, &theme, player_height, sidebar_width, &mut pending);
        });

    egui::SidePanel::left("echoport-nav")
        .exact_width(sidebar_width)
        .resizable(false)
        .show_separator_line(false)
        .frame(
            Frame::new()
                .fill(theme.panel)
                .inner_margin(Margin::symmetric(14, 0)),
        )
        .show(ctx, |ui| {
            sidebar(ui, app, &theme, &mut pending);
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

    toasts(ctx, app, &theme, player_height);
    drop_prompt(ctx, app, &theme, player_height, &mut pending);
    dialogs(ctx, app, &theme, &mut pending);
    handle_escape(ctx, app);
    splash::draw(ctx, app, &mut pending);

    for command in pending {
        app.dispatch(command);
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
    let content_center_x = inner.left() + (inner.width() + sidebar_width) * 0.5;
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

    // Subtle divider between the navigation and the transport column.
    let line_top = panel.top() + (height * 0.18).clamp(10.0, 22.0);
    let line_bottom = panel.bottom() - (height * 0.18).clamp(10.0, 22.0);
    ui.painter().vline(
        inner.left() + left_w,
        line_top..=line_bottom,
        Stroke::new(
            1.0_f32,
            theme::with_alpha(theme.border, if theme.dark { 110 } else { 150 }),
        ),
    );
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
            style::paint_icon(
                ui.painter(),
                Icon::Music,
                cover_rect.shrink(16.0),
                theme.text_weak,
            );
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
                            if !app.state.queue.is_empty() {
                                if style::icon_button(
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

fn toasts(ctx: &egui::Context, app: &mut EchoPortApp, theme: &Theme, player_height: f32) {
    if app.state.toasts.is_empty() {
        return;
    }
    let now = ctx.input(|input| input.time);
    let mut dismiss = None;
    egui::Area::new(Id::new("toasts"))
        .order(Order::Foreground)
        .anchor(
            Align2::RIGHT_BOTTOM,
            Vec2::new(-18.0, -(player_height + 14.0)),
        )
        // Toasts are informational only: never intercept clicks on the app
        // underneath (auto-fade handles dismissal).
        .interactable(false)
        .show(ctx, |ui| {
            ui.set_width(310.0);
            ui.with_layout(Layout::bottom_up(Align::Max), |ui| {
                for (index, toast) in app.state.toasts.iter().enumerate().rev() {
                    let age = (now - toast.created_at).max(0.0);
                    let alpha = if age > 3.4 {
                        (1.0 - ((age - 3.4) / 0.8) as f32).clamp(0.0, 1.0)
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
                        } else if toast.kind == ToastKind::Error {
                            Icon::Info
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
                            });
                    });
                    ui.add_space(6.0);
                }
            });
        });
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
            };
            ui.label(RichText::new(title).size(18.0).strong().color(theme.text));
            ui.add_space(6.0);
            if !body.is_empty() {
                ui.label(RichText::new(body).size(12.0).color(theme.text_weak));
                ui.add_space(10.0);
            }
            let mut submitted = false;
            match &mut dialog {
                Dialog::NewPlaylist { name, .. } | Dialog::NewProfile { name } => {
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
                    ui.add(
                        egui::TextEdit::multiline(description)
                            .font(style::font(12.0))
                            .text_color(theme.text)
                            .desired_rows(2)
                            .desired_width(ui.available_width()),
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
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("Images", &["png", "jpg", "jpeg", "webp", "gif", "bmp"])
                                .pick_file()
                            {
                                *new_cover = Some(path);
                                *clear_cover = false;
                            }
                        }
                        if existing_cover.is_some() || new_cover.is_some() {
                            if style::button(
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
                Dialog::DeletePlaylist { .. } | Dialog::DeleteProfile { .. } => {}
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
                        Dialog::NewPlaylist { seed, name } => {
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
                        Dialog::NewProfile { name } => {
                            if !name.trim().is_empty() {
                                pending.push(Command::NewProfile(name.trim().to_owned()));
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
