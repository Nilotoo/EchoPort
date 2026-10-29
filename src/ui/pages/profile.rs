//! Profile: local profile switcher, profile editor (picture / bio / sharing),
//! statistics and listening history.

use egui::{
    Align, CornerRadius, Layout, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui, Vec2,
};

use crate::app::EchoPortApp;
use crate::i18n::Key;
use crate::state::{Command, Page};
use crate::theme::{self, Theme};
use crate::ui::style::{self, Icon, Variant};
use crate::ui::widgets::{self, TrackRow};
use crate::ui::Dialog;

pub fn draw(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme, pending: &mut Vec<Command>) {
    widgets::page_scroll(ui, |ui| {
        // Switching to another local profile closes a stale editor.
        if app.ui.profile_editing
            && app.ui.profile_bio_for.as_deref() != Some(app.state.profile.name.as_str())
        {
            app.ui.profile_editing = false;
        }
        header(ui, app, theme);
        ui.add_space(16.0);
        profiles_section(ui, app, theme, pending);
        ui.add_space(16.0);
        if app.ui.profile_editing {
            editor_card(ui, app, theme, pending);
            ui.add_space(16.0);
        }
        stats_section(ui, app, theme);
        ui.add_space(16.0);
        playlists_section(ui, app, theme);
        ui.add_space(16.0);
        favorite_albums_section(ui, app, theme, pending);
        ui.add_space(16.0);
        history_card(ui, app, theme, pending);
        ui.add_space(16.0);
        privacy_card(ui, app, theme, pending);
        ui.add_space(20.0);
    });
}

fn header(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(
                RichText::new(app.state.tr(Key::Profile))
                    .size(26.0)
                    .color(theme.text),
            );
            ui.label(
                RichText::new(app.state.tr(Key::PrivacyNote))
                    .size(11.5)
                    .color(theme.text_weak),
            );
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if style::button_icon(
                ui,
                theme,
                Icon::Plus,
                app.state.tr(Key::NewProfile),
                Variant::Secondary,
            )
            .clicked()
            {
                app.ui.dialog = Some(Dialog::NewProfile {
                    name: String::new(),
                });
            }
            if !app.ui.profile_editing
                && style::button(
                    ui,
                    theme,
                    app.state.tr(Key::EditProfile),
                    Variant::Secondary,
                )
                .clicked()
            {
                app.ui.profile_editing = true;
                app.ui.profile_bio = app.state.profile.bio.clone();
                app.ui.profile_bio_for = Some(app.state.profile.name.clone());
            }
            let _ = Page::Profile;
        });
    });
}

// ---------------------------------------------------------------------------
// Profiles overview
// ---------------------------------------------------------------------------

fn profiles_section(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme, pending: &mut Vec<Command>) {
    ui.label(
        RichText::new(app.state.tr(Key::Profiles))
            .size(14.0)
            .strong()
            .color(theme.text),
    );
    ui.add_space(8.0);
    let profiles = app.state.profiles.clone();
    let current = app.state.profile.name.clone();
    let available = ui.available_width();
    let columns = if available >= 980.0 { 3 } else { 2 };
    let gap = 12.0;
    let card_w = ((available - gap * (columns - 1) as f32) / columns as f32).clamp(240.0, 420.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        ui.spacing_mut().item_spacing.y = gap;
        for name in &profiles {
            profile_card(ui, app, theme, pending, name, &current, card_w);
        }
        // Inline ghost card for a new profile.
        let (rect, response) = ui.allocate_exact_size(Vec2::new(card_w, 94.0), Sense::click());
        let hover =
            ui.ctx()
                .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.12);
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(style::R_MD),
            theme::with_alpha(theme.surface_hover, (180.0 * hover) as u8),
        );
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(style::R_MD),
            Stroke::new(
                1.0_f32,
                theme::mix(theme.border, theme.accent, 0.30 * hover),
            ),
            StrokeKind::Inside,
        );
        style::paint_icon(
            ui.painter(),
            Icon::Plus,
            Rect::from_center_size(
                Pos2::new(rect.center().x, rect.center().y - 8.0),
                Vec2::splat(20.0),
            ),
            style::weak_text(theme),
        );
        style::paint_elided(
            ui.painter(),
            Pos2::new(rect.center().x - 40.0, rect.center().y + 17.0),
            app.state.tr(Key::NewProfile),
            style::font(11.5),
            style::weak_text(theme),
            90.0,
        );
        if response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            app.ui.dialog = Some(Dialog::NewProfile {
                name: String::new(),
            });
        }
    });
}

fn profile_card(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
    name: &str,
    current: &str,
    width: f32,
) {
    let selected = name == current;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 94.0), Sense::click());
    let hover =
        ui.ctx()
            .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.10);
    let fill = if selected {
        theme::with_alpha(theme.accent_soft, 200)
    } else if hover > 0.01 {
        theme::with_alpha(theme.surface_hover, (235.0 * hover) as u8)
    } else {
        theme.surface
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(style::R_MD), fill);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(style::R_MD),
        Stroke::new(1.0_f32, if selected { theme.accent } else { theme.border }),
        StrokeKind::Inside,
    );
    // The current profile carries the real picture/bio; other profiles only
    // have their name here, so they use the deterministic colour fallback.
    let avatar_rect = Rect::from_min_size(
        Pos2::new(rect.left() + 14.0, rect.top() + 14.0),
        Vec2::splat(46.0),
    );
    if selected {
        widgets::paint_profile_avatar(ui, theme, &app.state.profile, avatar_rect);
    } else {
        style::avatar(ui.painter(), avatar_rect, name, theme);
    }
    let text_left = avatar_rect.right() + 12.0;
    let text_right = rect.right() - 14.0;
    style::paint_elided(
        ui.painter(),
        Pos2::new(text_left, rect.top() + 20.0),
        name,
        style::font(13.5),
        theme.text,
        text_right - text_left,
    );
    let bio = if selected {
        let bio = app.state.profile.bio.trim();
        if bio.is_empty() {
            app.state.tr(Key::LocalProfile).to_owned()
        } else {
            bio.to_owned()
        }
    } else {
        app.state.tr(Key::SwitchProfile).to_owned()
    };
    style::paint_elided(
        ui.painter(),
        Pos2::new(text_left, rect.top() + 39.0),
        &bio,
        style::font(10.5),
        if selected {
            style::weak_text(theme)
        } else {
            theme.text_weak
        },
        text_right - text_left,
    );
    if selected {
        let badge_rect = Rect::from_min_size(
            Pos2::new(rect.left() + 14.0, rect.bottom() - 27.0),
            Vec2::new(58.0, 18.0),
        );
        ui.painter().rect_filled(
            badge_rect,
            CornerRadius::same(9),
            theme::with_alpha(theme.accent, 32),
        );
        style::paint_elided(
            ui.painter(),
            Pos2::new(badge_rect.left() + 8.0, badge_rect.center().y),
            app.state.tr(Key::CurrentProfile),
            style::font(9.0),
            theme.accent,
            badge_rect.width() - 10.0,
        );
    } else {
        let delete_rect = Rect::from_center_size(
            Pos2::new(rect.right() - 20.0, rect.bottom() - 20.0),
            Vec2::splat(24.0),
        );
        if style::icon_button_at(
            ui,
            delete_rect,
            ("profile-delete", name.to_owned()),
            theme,
            Icon::Trash,
            false,
            app.state.tr(Key::DeleteProfile),
        )
        .clicked()
        {
            app.ui.dialog = Some(Dialog::DeleteProfile {
                name: name.to_owned(),
            });
        }
    }
    if response.clicked() && !selected {
        pending.push(Command::SwitchProfile(name.to_owned()));
    }
}

// ---------------------------------------------------------------------------
// Current profile editor
// ---------------------------------------------------------------------------

fn editor_card(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme, pending: &mut Vec<Command>) {
    let key = app.state.profile.name.clone();
    if app.ui.profile_bio_for.as_deref() != Some(key.as_str()) {
        app.ui.profile_bio_for = Some(key.clone());
        app.ui.profile_bio = app.state.profile.bio.clone();
    }
    let mut close_editor = false;
    style::card(theme).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(app.state.tr(Key::EditProfile))
                    .size(15.0)
                    .strong()
                    .color(theme.text),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if style::icon_button(
                    ui,
                    theme,
                    Icon::Close,
                    22.0,
                    false,
                    app.state.tr(Key::Close),
                )
                .clicked()
                {
                    close_editor = true;
                }
                ui.add_space(4.0);
                ui.label(
                    RichText::new(&key)
                        .size(10.5)
                        .color(style::weak_text(theme)),
                );
            });
        });
        ui.add_space(10.0);
        ui.horizontal_top(|ui| {
            let avatar_rect = Rect::from_min_size(ui.cursor().min, Vec2::splat(72.0));
            ui.allocate_rect(avatar_rect, Sense::hover());
            widgets::paint_profile_avatar(ui, theme, &app.state.profile, avatar_rect);
            ui.add_space(14.0);
            ui.vertical(|ui| {
                if style::button(
                    ui,
                    theme,
                    app.state.tr(Key::ChooseAvatar),
                    Variant::Secondary,
                )
                .clicked()
                {
                    pending.push(Command::SetProfileAvatar);
                }
                ui.add_space(4.0);
                if app.state.profile.avatar.is_some()
                    && style::button(ui, theme, app.state.tr(Key::RemoveAvatar), Variant::Ghost)
                        .clicked()
                {
                    pending.push(Command::ClearProfileAvatar);
                }
            });
            ui.add_space(18.0);
            let remaining = ui.available_width().max(220.0);
            ui.vertical(|ui| {
                ui.set_width(remaining.min(520.0));
                ui.label(
                    RichText::new(app.state.tr(Key::BioLabel))
                        .size(11.0)
                        .color(style::weak_text(theme)),
                );
                ui.add_space(4.0);
                let edit_height = 74.0;
                let (rect, _) = ui.allocate_exact_size(
                    Vec2::new(ui.available_width(), edit_height),
                    Sense::hover(),
                );
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(style::R_SM), theme.surface);
                ui.painter().rect_stroke(
                    rect,
                    CornerRadius::same(style::R_SM),
                    Stroke::new(1.0_f32, theme.border),
                    StrokeKind::Inside,
                );
                ui.put(
                    rect.shrink(8.0),
                    egui::TextEdit::multiline(&mut app.ui.profile_bio)
                        .frame(false)
                        .font(style::font(12.0))
                        .text_color(theme.text)
                        .desired_width(rect.width() - 16.0)
                        .desired_rows(2),
                );
                ui.add_space(4.0);
                style::hint_line(ui, theme, app.state.tr(Key::ProfileBioHint));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if style::button(ui, theme, app.state.tr(Key::Save), Variant::Primary).clicked()
                    {
                        pending.push(Command::SetProfileBio(app.ui.profile_bio.trim().to_owned()));
                        close_editor = true;
                    }
                    if style::button(ui, theme, app.state.tr(Key::Cancel), Variant::Ghost).clicked()
                    {
                        close_editor = true;
                    }
                });
            });
        });
        ui.add_space(10.0);
        style::divider(ui, theme);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if style::button_icon(
                ui,
                theme,
                Icon::Download,
                app.state.tr(Key::ExportProfile),
                Variant::Secondary,
            )
            .clicked()
            {
                pending.push(Command::ExportProfile);
            }
            ui.add_space(4.0);
            if style::button_icon(
                ui,
                theme,
                Icon::Folder,
                app.state.tr(Key::ImportProfile),
                Variant::Ghost,
            )
            .clicked()
            {
                pending.push(Command::ImportProfile);
            }
        });
    });
    if close_editor {
        app.ui.profile_editing = false;
        app.ui.profile_bio = app.state.profile.bio.clone();
    }
}

// ---------------------------------------------------------------------------
// Statistics
// ---------------------------------------------------------------------------

fn stats_section(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(app.state.tr(Key::Statistics))
                .size(14.0)
                .strong()
                .color(theme.text),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(format!(
                    "{} {}",
                    app.state.tr(Key::Joined),
                    style::format_date(app.state.profile.created_at)
                ))
                .size(10.5)
                .color(theme.text_weak),
            );
        });
    });
    ui.add_space(8.0);
    let stats = [
        (
            Icon::Music,
            theme.macaron[0],
            format!("{}", app.state.profile.history.len()),
            app.state.tr(Key::SongsPlayed),
        ),
        (
            Icon::HeartFill,
            theme.macaron[1],
            format!("{}", app.state.profile.favorites.len()),
            app.state.tr(Key::FavoritesCount),
        ),
        (
            Icon::Playlist,
            theme.macaron[3],
            format!("{}", app.state.profile.playlists.len()),
            app.state.tr(Key::Playlists),
        ),
        (
            Icon::Library,
            theme.macaron[2],
            format!("{}", app.state.profile.tracks.len()),
            app.state.tr(Key::StoredTracks),
        ),
    ];
    let available = ui.available_width();
    let columns = if available >= 760.0 { 4 } else { 2 };
    let gap = 10.0;
    let card_w = (available - gap * (columns - 1) as f32) / columns as f32;
    for chunk in stats.chunks(columns) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (icon, color, value, label) in chunk {
                ui.allocate_ui_with_layout(
                    Vec2::new(card_w, 66.0),
                    Layout::top_down(Align::Min),
                    |ui| style::stat_card(ui, theme, *icon, *color, value, label),
                );
                ui.add_space(gap);
            }
        });
        ui.add_space(gap);
    }
}

// ---------------------------------------------------------------------------
// Playlists (with cover images)
// ---------------------------------------------------------------------------

fn playlists_section(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme) {
    let playlists = app.state.profile.playlists.clone();
    if playlists.is_empty() {
        return;
    }
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(app.state.tr(Key::YourPlaylists))
                .size(14.0)
                .strong()
                .color(theme.text),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(format!("{}", playlists.len()))
                    .size(10.5)
                    .color(theme.text_weak),
            );
        });
    });
    ui.add_space(8.0);
    style::card(theme).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        for playlist in playlists.iter().take(4) {
            let subtitle = if playlist.description.trim().is_empty() {
                format!("{} {}", playlist.len(), app.state.tr(Key::TracksCount))
            } else {
                playlist.description.clone()
            };
            let selected = app.state.selected_playlist.as_deref() == Some(&playlist.id);
            let response =
                widgets::playlist_home_row(ui, app, theme, playlist, &subtitle, selected)
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
            if response.clicked() {
                app.state.selected_playlist = Some(playlist.id.clone());
                app.ui.selected_playlist = Some(playlist.id.clone());
                app.state.page = Page::Playlists;
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Favorite albums
// ---------------------------------------------------------------------------

fn favorite_albums_section(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
) {
    let albums = app.state.profile.favorite_albums.clone();
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(app.state.tr(Key::FavoriteAlbums))
                .size(14.0)
                .strong()
                .color(theme.text),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(format!("{}", albums.len()))
                    .size(10.5)
                    .color(theme.text_weak),
            );
        });
    });
    ui.add_space(8.0);
    if albums.is_empty() {
        style::hint_line(ui, theme, app.state.tr(Key::NoFavoriteAlbums));
        return;
    }
    let width = ((ui.available_width() - 20.0) / 2.0).clamp(220.0, 360.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        for album in &albums {
            widgets::album_card(ui, app, theme, pending, album, width);
        }
    });
}

// ---------------------------------------------------------------------------
// Listening history
// ---------------------------------------------------------------------------

fn history_card(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme, pending: &mut Vec<Command>) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(app.state.tr(Key::ListeningHistory))
                .size(14.0)
                .strong()
                .color(theme.text),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(style::tracks_count(
                    &app.state,
                    app.state.profile.history.len(),
                ))
                .size(10.5)
                .color(theme.text_weak),
            );
        });
    });
    ui.add_space(8.0);
    let recent = app.state.profile.recent_tracks(12);
    if recent.is_empty() {
        widgets::callout(
            ui,
            theme,
            Icon::Clock,
            app.state.tr(Key::EmptyHistory),
            false,
        );
        return;
    }
    let width = ui.available_width();
    let row_h = 46.0;
    // The page itself scrolls, so give the listening history room to breathe
    // instead of clipping it to the remaining viewport height.
    let height = 46.0 + recent.len() as f32 * row_h;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(style::R_LG), theme.surface);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(style::R_LG),
        Stroke::new(1.0_f32, theme.border),
        StrokeKind::Inside,
    );
    let inner = rect.shrink(8.0);
    style::absolute(ui, inner, |ui| {
        ui.add_space(8.0);
        egui::ScrollArea::vertical()
            .id_salt("profile-history")
            .max_height((inner.height() - 12.0).max(row_h))
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 0.0;
                for (index, track) in recent.iter().enumerate() {
                    let mut row = TrackRow::compact(&recent, index);
                    row.height = row_h;
                    widgets::track_row(ui, app, theme, pending, track, row);
                }
            });
    });
}

// ---------------------------------------------------------------------------
// Privacy / data
// ---------------------------------------------------------------------------

fn privacy_card(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme, pending: &mut Vec<Command>) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 112.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(style::R_LG), style::wash(theme));
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(style::R_LG),
        Stroke::new(1.0_f32, theme.border),
        StrokeKind::Inside,
    );
    let icon_center = Pos2::new(rect.left() + 34.0, rect.top() + 34.0);
    ui.painter()
        .circle_filled(icon_center, 18.0, theme::with_alpha(theme.accent, 32));
    style::paint_icon(
        ui.painter(),
        Icon::Check,
        Rect::from_center_size(icon_center, Vec2::splat(18.0)),
        theme.accent,
    );
    style::paint_elided(
        ui.painter(),
        Pos2::new(rect.left() + 60.0, rect.top() + 28.0),
        app.state.tr(Key::DataStoredLocally),
        style::font(13.0),
        theme.text,
        rect.width() - 220.0,
    );
    style::paint_elided(
        ui.painter(),
        Pos2::new(rect.left() + 60.0, rect.top() + 47.0),
        app.state.tr(Key::Privacy),
        style::font(10.0),
        theme.text_weak,
        rect.width() - 220.0,
    );
    let body = ui.painter().layout(
        app.state.tr(Key::PrivacyNote).to_owned(),
        style::font(11.0),
        theme.text_weak,
        rect.width() - 240.0,
    );
    ui.painter().galley(
        Pos2::new(rect.left() + 18.0, rect.top() + 66.0),
        body,
        theme.text_weak,
    );
    let button_rect = Rect::from_center_size(
        Pos2::new(rect.right() - 110.0, rect.center().y),
        Vec2::new(190.0, 30.0),
    );
    if style::button_at(
        ui,
        button_rect,
        "profile-open-data",
        theme,
        app.state.tr(Key::OpenDataFolder),
        Variant::Secondary,
    ) {
        pending.push(Command::OpenDataFolder);
    }
}
