//! Playlists: list + detail view, create / rename / delete, mixed-platform
//! tracks, play all and shuffle.

use egui::{Align, Layout, RichText, Ui, Vec2};

use crate::app::EchoPortApp;
use crate::i18n::Key;
use crate::state::{Command, Page};
use crate::theme::{self, Theme};
use crate::ui::style::{self, Icon, Variant};
use crate::ui::widgets::{self, TrackRow};
use crate::ui::Dialog;

pub fn draw(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme, pending: &mut Vec<Command>) {
    widgets::page_scroll(ui, |ui| {
        header(ui, app, theme, pending);
        ui.add_space(14.0);
        let available = ui.available_width();
        let list_width = if available < 860.0 { 260.0 } else { 316.0 };
        let detail_width = (available - list_width - 16.0).max(260.0);
        let playlists = app.state.profile.playlists.clone();
        let list_height = (58.0 + playlists.len() as f32 * 64.0).max(260.0);
        let selected = app.state.selected_playlist().cloned();
        let tracks = selected
            .as_ref()
            .map(|playlist| app.state.profile.playlist_tracks(&playlist.id))
            .unwrap_or_default();
        let detail_height = if selected.is_some() {
            (238.0 + tracks.len() as f32 * 50.0).max(320.0)
        } else {
            320.0
        };
        let height = list_height
            .max(detail_height)
            .max(ui.available_height() - 12.0);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.allocate_ui_with_layout(
                Vec2::new(list_width, height),
                Layout::top_down(Align::Min),
                |ui| {
                    ui.set_min_size(Vec2::new(list_width, height));
                    list_panel(ui, app, theme, &playlists, list_width, height);
                },
            );
            ui.add_space(16.0);
            ui.allocate_ui_with_layout(
                Vec2::new(detail_width, height),
                Layout::top_down(Align::Min),
                |ui| {
                    ui.set_min_size(Vec2::new(detail_width, height));
                    detail_panel(
                        ui,
                        app,
                        theme,
                        pending,
                        selected.as_ref(),
                        &tracks,
                        detail_width,
                        height,
                    );
                },
            );
        });
        ui.add_space(12.0);
    });
}

fn header(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme, pending: &mut Vec<Command>) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(
                RichText::new(app.state.tr(Key::Playlists))
                    .size(26.0)
                    .color(theme.text),
            );
            ui.label(
                RichText::new(app.state.tr(Key::NoPlaylistsHint))
                    .size(11.5)
                    .color(theme.text_weak),
            );
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if style::button_icon(
                ui,
                theme,
                Icon::Plus,
                app.state.tr(Key::NewPlaylist),
                Variant::Primary,
            )
            .clicked()
            {
                app.ui.dialog = Some(Dialog::NewPlaylist {
                    seed: Vec::new(),
                    name: String::new(),
                });
            }
            ui.add_space(6.0);
            if style::button_icon(
                ui,
                theme,
                Icon::Download,
                app.state.tr(Key::ImportPlaylist),
                Variant::Secondary,
            )
            .clicked()
            {
                pending.push(Command::ImportPlaylist);
            }
            let _ = Page::Playlists;
        });
    });
}

fn list_panel(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    playlists: &[crate::model::Playlist],
    width: f32,
    height: f32,
) {
    style::card(theme).show(ui, |ui| {
        ui.set_min_size(Vec2::new(width - 32.0, height - 32.0));
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
        ui.add_space(4.0);
        style::divider(ui, theme);
        ui.add_space(4.0);
        if playlists.is_empty() {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| {
                style::empty_state(
                    ui,
                    theme,
                    Icon::Playlist,
                    app.state.tr(Key::EmptyPlaylists),
                    app.state.tr(Key::NoPlaylistsHint),
                    Some(app.state.tr(Key::CreatePlaylist)),
                )
                .map(|response| {
                    if response.clicked() {
                        app.ui.dialog = Some(Dialog::NewPlaylist {
                            seed: Vec::new(),
                            name: String::new(),
                        });
                    }
                });
            });
            return;
        }
        let selected = app.state.selected_playlist.clone();
        for playlist in playlists {
            let count = playlist.len();
            let subtitle = if playlist.description.is_empty() {
                format!("{} {}", count, app.state.tr(Key::TracksCount))
            } else {
                playlist.description.clone()
            };
            let is_selected = selected.as_deref() == Some(playlist.id.as_str())
                || app.ui.selected_playlist.as_deref() == Some(playlist.id.as_str());
            let response = widgets::playlist_list_row(ui, theme, playlist, &subtitle, is_selected)
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(app.state.tr(Key::DoubleClickOpen));
            if response.clicked() {
                app.ui.selected_playlist = Some(playlist.id.clone());
                app.ui.selected_track = None;
            }
            if response.double_clicked() {
                app.state.selected_playlist = Some(playlist.id.clone());
                app.ui.selected_playlist = Some(playlist.id.clone());
            }
            ui.add_space(2.0);
        }
    });
}

fn detail_panel(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
    selected: Option<&crate::model::Playlist>,
    tracks: &[crate::model::Track],
    width: f32,
    height: f32,
) {
    style::card(theme).show(ui, |ui| {
        ui.set_min_size(Vec2::new(width - 32.0, height - 32.0));
        let Some(playlist) = selected else {
            ui.add_space((height * 0.24).min(150.0));
            let mut create = false;
            ui.vertical_centered(|ui| {
                create = style::empty_state(
                    ui,
                    theme,
                    Icon::Playlist,
                    app.state.tr(Key::EmptyPlaylists),
                    app.state.tr(Key::NoPlaylistsHint),
                    Some(app.state.tr(Key::CreatePlaylist)),
                )
                .map(|response| response.clicked())
                .unwrap_or(false);
            });
            if create {
                app.ui.dialog = Some(Dialog::NewPlaylist {
                    seed: Vec::new(),
                    name: String::new(),
                });
            }
            return;
        };
        // Header: generated cover + title / description / meta.
        ui.horizontal_top(|ui| {
            let (cover_rect, _) = ui.allocate_exact_size(Vec2::splat(92.0), egui::Sense::hover());
            let (color_a, color_b) = style::generated_cover_colors(theme, &playlist.id);
            let glyph = playlist
                .name
                .chars()
                .find(|c| c.is_alphanumeric())
                .map(|c| c.to_uppercase().to_string())
                .unwrap_or_else(|| "♫".to_owned());
            style::gradient_cover(
                ui.painter(),
                cover_rect,
                style::R_MD,
                color_a,
                color_b,
                Some(&glyph),
            );
            ui.add_space(14.0);
            ui.vertical(|ui| {
                ui.add(
                    egui::Label::new(RichText::new(&playlist.name).size(22.0).color(theme.text))
                        .truncate(),
                );
                ui.add_space(2.0);
                let description = if playlist.description.is_empty() {
                    app.state.tr(Key::EmptyStateHint).to_owned()
                } else {
                    playlist.description.clone()
                };
                ui.add(
                    egui::Label::new(RichText::new(description).size(11.5).color(theme.text_weak))
                        .truncate(),
                );
                ui.add_space(3.0);
                let total_secs: f64 = tracks.iter().filter_map(|track| track.duration_secs).sum();
                ui.label(
                    RichText::new(format!(
                        "{} · {} · {}",
                        style::tracks_count(&app.state, tracks.len()),
                        style::coarse_duration(&app.state, total_secs),
                        widgets::platforms_label(tracks)
                    ))
                    .size(10.5)
                    .color(theme.text_weak),
                );
            });
        });
        ui.add_space(12.0);
        // Actions.
        ui.horizontal(|ui| {
            let play_all = style::button_icon(
                ui,
                theme,
                Icon::Play,
                app.state.tr(Key::PlayAll),
                Variant::Primary,
            );
            if play_all.clicked() && !tracks.is_empty() {
                pending.push(Command::PlayPlaylist(playlist.id.clone()));
            }
            ui.add_space(2.0);
            if style::button_icon(
                ui,
                theme,
                Icon::Shuffle,
                app.state.tr(Key::ShuffleAll),
                Variant::Tonal,
            )
            .clicked()
                && !tracks.is_empty()
            {
                let shuffled = shuffle_tracks(tracks, ui.ctx().input(|input| input.time));
                if !app.state.shuffle {
                    pending.push(Command::ToggleShuffle);
                }
                pending.push(Command::PlayTracks(shuffled));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if style::button(ui, theme, app.state.tr(Key::Delete), Variant::Ghost).clicked() {
                    app.ui.dialog = Some(Dialog::DeletePlaylist {
                        id: playlist.id.clone(),
                        name: playlist.name.clone(),
                    });
                }
                ui.add_space(2.0);
                if style::button(ui, theme, app.state.tr(Key::Rename), Variant::Ghost).clicked() {
                    app.ui.dialog = Some(Dialog::RenamePlaylist {
                        id: playlist.id.clone(),
                        name: playlist.name.clone(),
                    });
                }
                ui.add_space(2.0);
                if style::button(ui, theme, app.state.tr(Key::ExportPlaylist), Variant::Ghost)
                    .clicked()
                {
                    pending.push(Command::ExportPlaylist(playlist.id.clone()));
                }
            });
        });
        ui.add_space(4.0);
        style::hint_line(ui, theme, app.state.tr(Key::PlaylistShareHint));
        ui.add_space(6.0);
        style::divider(ui, theme);
        ui.add_space(4.0);
        if tracks.is_empty() {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(app.state.tr(Key::EmptyPlaylist))
                        .size(13.0)
                        .color(theme.text_weak),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new(app.state.tr(Key::NoPlaylistsHint))
                        .size(11.0)
                        .color(theme.text_weak),
                );
            });
            return;
        }
        for (index, track) in tracks.iter().enumerate() {
            let mut row = TrackRow::new(tracks, index)
                .playlist(&playlist.id)
                .salt("playlist-detail");
            row.height = 50.0;
            widgets::track_row(ui, app, theme, pending, track, row);
        }
    });
}

fn shuffle_tracks(tracks: &[crate::model::Track], salt: f64) -> Vec<crate::model::Track> {
    let mut out = tracks.to_vec();
    let salt = (salt * 1_000.0) as u64;
    out.sort_by_key(|track| crate::model::stable_hash(&format!("{}:{salt}", track.id)));
    out
}

/// Keep the theme helper referenced for future inline editing.
#[allow(dead_code)]
fn _theme_anchor(theme: &Theme) -> theme::Theme {
    *theme
}
