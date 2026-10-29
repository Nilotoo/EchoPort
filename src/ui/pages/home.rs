//! Home: greeting, continue-listening hero, recently played, favourites,
//! playlists and the "one library, many sources" shortcuts.

use egui::{
    Align, CornerRadius, Layout, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui, Vec2,
};

use crate::app::EchoPortApp;
use crate::i18n::Key;
use crate::model::{Platform, PlaybackState, Track};
use crate::state::{Command, Page};
use crate::theme::{self, Theme};
use crate::ui::style::{self, Icon, Variant};
use crate::ui::widgets;

pub fn draw(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme, pending: &mut Vec<Command>) {
    widgets::page_scroll(ui, |ui| {
        let screen_h = ui.ctx().screen_rect().height();
        ui.add_space(((screen_h - 900.0) * 0.22).clamp(0.0, 120.0));
        header(ui, app, theme);
        ui.add_space(12.0);
        hero(ui, app, theme, pending);
        ui.add_space(14.0);
        recently_played(ui, app, theme, pending);
        ui.add_space(12.0);
        bottom_row(ui, app, theme, pending);
        ui.add_space(8.0);
    });
}

fn header(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme) {
    ui.horizontal(|ui| {
        let name = app.state.profile.name.clone();
        let base = app.state.tr(Key::WelcomeBack);
        ui.vertical(|ui| {
            ui.label(
                RichText::new(style::greeting(app.state.lang, base, &name))
                    .size(26.0)
                    .color(theme.text),
            );
            ui.label(
                RichText::new(app.state.tr(Key::SourcesHint))
                    .size(11.5)
                    .color(theme.text_weak),
            );
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if style::button_icon(
                ui,
                theme,
                Icon::Search,
                app.state.tr(Key::SearchNav),
                Variant::Secondary,
            )
            .clicked()
            {
                app.set_page(Page::Search);
            }
            ui.add_space(2.0);
            if style::button_icon(
                ui,
                theme,
                Icon::Music,
                app.state.tr(Key::NowPlaying),
                Variant::Ghost,
            )
            .clicked()
            {
                app.set_page(Page::NowPlaying);
            }
        });
    });
}

// ---------------------------------------------------------------------------
// Continue listening
// ---------------------------------------------------------------------------

fn hero(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme, pending: &mut Vec<Command>) {
    let width = ui.available_width();
    let height = (width * 0.115).clamp(96.0, 152.0);
    let s = height / 106.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(style::R_LG), theme.surface);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(style::R_LG),
        Stroke::new(1.0_f32, theme.border),
        StrokeKind::Inside,
    );
    // Crisp geometric deco on the right: rings in the palette's strong accents.
    let mut clip = ui.painter().clone();
    clip.set_clip_rect(rect);
    style::blob(
        &clip,
        Pos2::new(rect.right() - 82.0 * s, rect.center().y + 12.0 * s),
        44.0 * s,
        theme::with_alpha(theme.highlight, 70),
    );
    style::blob(
        &clip,
        Pos2::new(rect.right() - 142.0 * s, rect.center().y - 16.0 * s),
        21.0 * s,
        theme::with_alpha(theme.accent, 28),
    );
    clip.circle_stroke(
        Pos2::new(rect.right() - 118.0 * s, rect.center().y + 6.0 * s),
        13.0 * s,
        Stroke::new(1.5_f32, theme::with_alpha(theme.accent, 60)),
    );

    let mut pending_open = false;
    match app.state.now_playing.clone() {
        Some(track) => {
            let cover_rect = Rect::from_center_size(
                Pos2::new(rect.left() + 20.0 * s + 44.0 * s, rect.center().y),
                Vec2::splat(88.0 * s),
            );
            let cover_response =
                ui.interact(cover_rect, ui.id().with("home-hero-cover"), Sense::click());
            widgets::paint_track_cover(ui, app, theme, &track, cover_rect, style::R_MD);
            let cover_response = cover_response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(app.state.tr(Key::DoubleClickOpen));
            if cover_response.clicked() {
                app.ui.selected_track = Some(track.id.clone());
            }
            if cover_response.double_clicked() {
                pending_open = true;
            }
            let text_left = cover_rect.right() + 18.0;
            let text_right = rect.right() - 140.0 * s;
            let playing = app.state.playback.state.is_playing();
            let title_click = ui
                .interact(
                    Rect::from_min_max(
                        Pos2::new(text_left, rect.top() + 20.0 * s),
                        Pos2::new(text_right, rect.top() + 58.0 * s),
                    ),
                    ui.id().with("home-hero-title"),
                    Sense::click(),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            style::paint_elided(
                ui.painter(),
                Pos2::new(text_left, rect.top() + 30.0 * s),
                app.state.tr(Key::ContinueListening),
                style::font(10.0 * s),
                theme.text_weak,
                text_right - text_left,
            );
            style::paint_elided(
                ui.painter(),
                Pos2::new(text_left, rect.top() + 50.0 * s),
                &track.title,
                style::font(19.0 * s),
                theme.text,
                text_right - text_left,
            );
            let subtitle = if app.state.playback.state == PlaybackState::Loading {
                app.state.tr(Key::LoadingAudio).to_owned()
            } else {
                let album = track
                    .album
                    .clone()
                    .unwrap_or_else(|| track.platform.label().to_owned());
                format!("{} · {album}", track.display_artist())
            };
            style::paint_elided(
                ui.painter(),
                Pos2::new(text_left, rect.top() + 71.0 * s),
                &subtitle,
                style::font(11.5 * s),
                theme.text_weak,
                text_right - text_left,
            );
            let title_click = title_click.on_hover_text(app.state.tr(Key::DoubleClickOpen));
            if title_click.clicked() {
                app.ui.selected_track = Some(track.id.clone());
            }
            if title_click.double_clicked() {
                pending_open = true;
            }
            let duration = app.state.playback.duration_secs.unwrap_or(0.0);
            let position = app.state.playback.position_secs;
            let bar_width = (text_right - text_left - 118.0).max(120.0);
            let bar_rect = Rect::from_min_size(
                Pos2::new(text_left, rect.bottom() - 25.0 * s),
                Vec2::new(bar_width, 5.0 * s),
            );
            let fraction = if duration > 0.0 {
                (position / duration).clamp(0.0, 1.0) as f32
            } else {
                0.0
            };
            ui.painter().rect_filled(
                bar_rect,
                CornerRadius::same(3),
                theme::with_alpha(theme.border, 190),
            );
            ui.painter().rect_filled(
                Rect::from_min_size(
                    bar_rect.min,
                    Vec2::new(bar_rect.width() * fraction, bar_rect.height()),
                ),
                CornerRadius::same(3),
                theme.accent,
            );
            style::paint_elided(
                ui.painter(),
                Pos2::new(bar_rect.right() + 50.0, bar_rect.center().y),
                &format!(
                    "{} / {}",
                    crate::model::format_duration(position),
                    crate::model::format_duration(duration)
                ),
                style::font(10.0 * s),
                theme.text_weak,
                90.0,
            );
            let play_center = Pos2::new(rect.right() - 48.0 * s, rect.center().y);
            let play_rect = Rect::from_center_size(play_center, Vec2::splat(48.0 * s));
            let play_response =
                ui.interact(play_rect, ui.id().with("home-hero-play"), Sense::click());
            let hover = ui.ctx().animate_bool_with_time(
                play_response.id.with("hover"),
                play_response.hovered(),
                0.12,
            );
            ui.painter().circle_filled(
                play_center,
                24.0 * s,
                if playing {
                    theme.accent
                } else {
                    crate::theme::mix(theme.accent, theme.on_accent, 0.06)
                },
            );
            if hover > 0.01 {
                ui.painter().circle_stroke(
                    play_center,
                    24.0 * s + 3.0 * hover,
                    Stroke::new(
                        1.0_f32,
                        theme::with_alpha(theme.accent, (110.0 * hover) as u8),
                    ),
                );
            }
            style::paint_icon(
                ui.painter(),
                if playing { Icon::Pause } else { Icon::Play },
                play_rect.shrink(15.0 * s),
                theme.on_accent,
            );
            if play_response.clicked() {
                pending.push(Command::TogglePlayPause);
            }
        }
        None => {
            style::paint_icon(
                ui.painter(),
                Icon::Music,
                Rect::from_center_size(
                    Pos2::new(rect.left() + 54.0, rect.center().y),
                    Vec2::splat(46.0),
                ),
                theme.text_weak,
            );
            style::paint_elided(
                ui.painter(),
                Pos2::new(rect.left() + 96.0, rect.center().y - 10.0),
                app.state.tr(Key::ContinueListening),
                style::font(16.0),
                theme.text,
                rect.width() - 220.0,
            );
            style::paint_elided(
                ui.painter(),
                Pos2::new(rect.left() + 96.0, rect.center().y + 12.0),
                app.state.tr(Key::EmptyStateHint),
                style::font(11.5),
                theme.text_weak,
                rect.width() - 220.0,
            );
            if style::button(ui, theme, app.state.tr(Key::SearchNav), Variant::Primary).clicked() {
                app.set_page(Page::Search);
            }
        }
    }
    if pending_open {
        app.set_page(Page::NowPlaying);
    }
}

// ---------------------------------------------------------------------------
// Recently played
// ---------------------------------------------------------------------------

fn recently_played(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme, pending: &mut Vec<Command>) {
    style::section_header(ui, theme, app.state.tr(Key::RecentlyPlayed), None);
    let tracks = app.state.recent_tracks(6);
    if tracks.is_empty() {
        widgets::callout(
            ui,
            theme,
            Icon::Clock,
            app.state.tr(Key::EmptyHistory),
            false,
        );
        return;
    }
    let available = ui.available_width();
    let gap = 14.0;
    let count = tracks.len() as f32;
    let card_width = ((available - gap * (count - 1.0)) / count).max(96.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (index, track) in tracks.iter().enumerate() {
            widgets::media_card(ui, app, theme, pending, track, &tracks, index, card_width);
            if index + 1 < tracks.len() {
                ui.add_space(gap);
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Bottom row: favourites, playlists, sources
// ---------------------------------------------------------------------------

fn bottom_row(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme, pending: &mut Vec<Command>) {
    ui.columns(3, |cols| {
        card_shell(&mut cols[0], theme, |ui| {
            favourites(ui, app, theme, pending)
        });
        card_shell(&mut cols[1], theme, |ui| playlists(ui, app, theme));
        card_shell(&mut cols[2], theme, |ui| sources(ui, app, theme));
    });
}

fn card_shell<R>(ui: &mut Ui, theme: &Theme, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    style::card(theme)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let tall = (ui.ctx().screen_rect().height() - 800.0) * 0.17;
            ui.set_min_height((166.0 + tall).clamp(160.0, 290.0));
            add_contents(ui)
        })
        .inner
}

fn favourites(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme, pending: &mut Vec<Command>) {
    ui.horizontal(|ui| {
        ui.add_space(2.0);
        ui.label(
            RichText::new(app.state.tr(Key::Favorites))
                .size(15.0)
                .strong()
                .color(theme.text),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(format!("{}", app.state.favorite_tracks().len()))
                    .size(11.0)
                    .color(theme.text_weak),
            );
        });
    });
    ui.add_space(4.0);
    let tracks = app.state.favorite_tracks();
    if tracks.is_empty() {
        mini_hint(ui, theme, app.state.tr(Key::EmptyFavorites));
        return;
    }
    let row_width = ui.available_width();
    for (index, track) in tracks.iter().take(3).enumerate() {
        mini_track_row(ui, app, theme, pending, track, &tracks, index, row_width);
        if index + 1 < tracks.len().min(3) {
            ui.add_space(2.0);
        }
    }
}

fn playlists(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(app.state.tr(Key::YourPlaylists))
                .size(15.0)
                .strong()
                .color(theme.text),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(format!("{}", app.state.profile.playlists.len()))
                    .size(11.0)
                    .color(theme.text_weak),
            );
        });
    });
    ui.add_space(4.0);
    let playlists = app.state.profile.playlists.clone();
    if playlists.is_empty() {
        mini_hint(ui, theme, app.state.tr(Key::NoPlaylistsHint));
        return;
    }
    for playlist in playlists.iter().take(3) {
        let count = playlist.len();
        let subtitle = format!("{} {}", count, app.state.tr(Key::TracksCount));
        let selected = app.ui.selected_playlist.as_deref() == Some(playlist.id.as_str());
        let response = widgets::playlist_home_row(ui, app, theme, playlist, &subtitle, selected)
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(app.state.tr(Key::DoubleClickOpen));
        if response.clicked() {
            app.ui.selected_playlist = Some(playlist.id.clone());
            app.ui.selected_track = None;
        }
        if response.double_clicked() {
            app.state.selected_playlist = Some(playlist.id.clone());
            app.ui.selected_playlist = Some(playlist.id.clone());
            app.set_page(Page::Playlists);
        }
        ui.add_space(2.0);
    }
}

fn sources(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme) {
    ui.label(
        RichText::new(app.state.tr(Key::SourcesTitle))
            .size(15.0)
            .strong()
            .color(theme.text),
    );
    ui.add_space(8.0);
    let sources = [
        (Platform::YouTube, Key::SourceSearchable),
        (Platform::Netease, Key::SourceSearchable),
        (Platform::SoundCloud, Key::SourceSearchable),
        (Platform::Bilibili, Key::SourceSearchable),
        (Platform::Spotify, Key::SourceLinkImport),
        (Platform::AppleMusic, Key::SourceSearchable),
    ];
    let row_width = ui.available_width();
    let chip_width = ((row_width - 8.0) / 2.0).max(90.0);
    for chunk in sources.chunks(2) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (index, (platform, status)) in chunk.iter().enumerate() {
                if index > 0 {
                    ui.add_space(8.0);
                }
                let response =
                    source_chip_fixed(ui, theme, *platform, app.state.tr(*status), chip_width);
                if response.clicked() {
                    app.state.search.platforms = [*platform].into_iter().collect();
                    app.set_page(Page::Search);
                }
            }
        });
        ui.add_space(6.0);
    }
}

fn source_chip_fixed(
    ui: &mut Ui,
    theme: &Theme,
    platform: Platform,
    status: &str,
    width: f32,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 36.0), Sense::click());
    let hover_t =
        ui.ctx()
            .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.12);
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(style::R_MD),
        crate::theme::mix(theme.surface, theme.surface_hover, hover_t),
    );
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(style::R_MD),
        Stroke::new(
            1.0_f32,
            crate::theme::mix(theme.border, theme.accent, 0.25 * hover_t),
        ),
        StrokeKind::Inside,
    );
    let dot = Pos2::new(rect.left() + 12.0, rect.center().y);
    ui.painter()
        .circle_filled(dot, 4.0, crate::theme::platform_color(platform));
    style::paint_elided(
        ui.painter(),
        Pos2::new(dot.x + 9.0, rect.center().y - 6.0),
        platform.label(),
        style::font(11.5),
        theme.text,
        rect.width() - 30.0,
    );
    style::paint_elided(
        ui.painter(),
        Pos2::new(dot.x + 9.0, rect.center().y + 7.0),
        status,
        style::font(9.5),
        theme.text_weak,
        rect.width() - 30.0,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

// ---------------------------------------------------------------------------
// Small shared pieces for the home cards
// ---------------------------------------------------------------------------

fn mini_hint(ui: &mut Ui, theme: &Theme, text: &str) {
    ui.label(RichText::new(text).size(11.0).color(theme.text_weak));
}

fn mini_track_row(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
    track: &Track,
    list: &[Track],
    index: usize,
    width: f32,
) {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 42.0), Sense::click());
    let hover_t =
        ui.ctx()
            .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.10);
    if hover_t > 0.01 {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(style::R_SM),
            theme::with_alpha(theme.surface_hover, (230.0 * hover_t) as u8),
        );
    }
    let cover_rect = Rect::from_center_size(
        Pos2::new(rect.left() + 18.0, rect.center().y),
        Vec2::splat(32.0),
    );
    widgets::paint_track_cover(ui, app, theme, track, cover_rect, 7);
    let text_left = cover_rect.right() + 9.0;
    let is_current = app
        .state
        .now_playing
        .as_ref()
        .map(|now| now.id == track.id)
        .unwrap_or(false);
    let selected = app.ui.selected_track.as_deref() == Some(track.id.as_str());
    if selected && !is_current {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(style::R_SM),
            theme::with_alpha(theme.accent_soft, 96),
        );
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(style::R_SM),
            Stroke::new(1.0_f32, theme::with_alpha(theme.accent, 120)),
            StrokeKind::Inside,
        );
    }
    style::paint_elided(
        ui.painter(),
        Pos2::new(text_left, rect.center().y - 6.5),
        &track.title,
        style::font(11.5),
        if is_current { theme.accent } else { theme.text },
        rect.width() - (text_left - rect.left()) - 30.0,
    );
    style::paint_elided(
        ui.painter(),
        Pos2::new(text_left, rect.center().y + 8.0),
        track.display_artist(),
        style::font(10.0),
        theme.text_weak,
        rect.width() - (text_left - rect.left()) - 30.0,
    );
    // Heart toggle on the right.
    let heart_rect = Rect::from_center_size(
        Pos2::new(rect.right() - 14.0, rect.center().y),
        Vec2::splat(24.0),
    );
    let favorite = app.state.is_favorite(&track.id);
    let heart = style::icon_button_at(
        ui,
        heart_rect,
        ("home-fav", track.id.as_str()),
        theme,
        if favorite {
            Icon::HeartFill
        } else {
            Icon::Heart
        },
        favorite,
        "",
    );
    if heart.clicked() {
        pending.push(Command::ToggleFavorite(Box::new(track.clone())));
    } else if response.clicked() {
        app.ui.selected_track = Some(track.id.clone());
        app.ui.selected_playlist = None;
    }
    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(app.state.tr(Key::DoubleClickPlay));
    if response.double_clicked() {
        if is_current {
            pending.push(Command::TogglePlayPause);
        } else {
            pending.push(widgets::play_from(list, index));
        }
    }
}
