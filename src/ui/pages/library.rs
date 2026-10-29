//! Library: local music scan, folder management and local tracks.

use egui::{Align, Layout, RichText, Ui, Vec2};

use crate::app::EchoPortApp;
use crate::i18n::Key;
use crate::state::{Command, Page};
use crate::theme::Theme;
use crate::ui::style::{self, Icon, Variant};
use crate::ui::widgets::{self, TrackRow};

pub fn draw(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme, pending: &mut Vec<Command>) {
    widgets::page_scroll(ui, |ui| {
        header(ui, app, theme, pending);
        ui.add_space(12.0);
        stats(ui, app, theme);
        ui.add_space(14.0);
        let available = ui.available_width();
        let left_w = (available * 0.61).max(360.0);
        let right_w = (available - left_w - 16.0).max(220.0);
        let tracks = app.state.library.tracks.clone();
        let left_h = if tracks.is_empty() {
            272.0
        } else {
            58.0 + tracks.len() as f32 * 50.0
        };
        let height = left_h.max(300.0).max(ui.available_height() - 12.0);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.allocate_ui_with_layout(
                Vec2::new(left_w, height),
                Layout::top_down(Align::Min),
                |ui| {
                    ui.set_min_size(Vec2::new(left_w, height));
                    tracks_panel(ui, app, theme, pending, &tracks, left_w, height);
                },
            );
            ui.add_space(16.0);
            ui.allocate_ui_with_layout(
                Vec2::new(right_w, height),
                Layout::top_down(Align::Min),
                |ui| {
                    ui.set_min_size(Vec2::new(right_w, height));
                    folders_panel(ui, app, theme, pending, right_w, height);
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
                RichText::new(app.state.tr(Key::LocalMusic))
                    .size(26.0)
                    .color(theme.text),
            );
            ui.label(
                RichText::new(app.state.tr(Key::LocalOnlyNote))
                    .size(11.5)
                    .color(theme.text_weak),
            );
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let scanning = app.state.library.scanning;
            let label = if scanning {
                app.state.tr(Key::Scanning)
            } else {
                app.state.tr(Key::ScanLibrary)
            };
            let scan = if scanning {
                style::button_icon(ui, theme, Icon::Clock, label, Variant::Tonal)
            } else {
                style::button_icon(ui, theme, Icon::Refresh, label, Variant::Primary)
            };
            if scan.clicked() && !scanning {
                pending.push(Command::ScanLibrary);
            }
            ui.add_space(4.0);
            if style::button_icon(
                ui,
                theme,
                Icon::Folder,
                app.state.tr(Key::AddMusicFolder),
                Variant::Secondary,
            )
            .clicked()
            {
                pending.push(Command::PickMusicFolder);
            }
        });
    });
}

fn stats(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme) {
    let tracks = app.state.library.tracks.clone();
    let total_secs: f64 = tracks.iter().filter_map(|track| track.duration_secs).sum();
    let folders = app.state.profile.settings.music_dirs.len();
    let gap = 12.0;
    let width = ((ui.available_width() - gap * 2.0) / 3.0).max(150.0);
    let cards = [
        (
            Icon::Music,
            theme.macaron[0],
            format!("{}", tracks.len()),
            app.state.tr(Key::TracksLabel),
        ),
        (
            Icon::Clock,
            theme.macaron[1],
            style::coarse_duration(&app.state, total_secs),
            app.state.tr(Key::TotalDuration),
        ),
        (
            Icon::Folder,
            theme.macaron[3],
            format!("{folders}"),
            app.state.tr(Key::FoldersLabel),
        ),
    ];
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (index, (icon, color, value, label)) in cards.iter().enumerate() {
            if index > 0 {
                ui.add_space(gap);
            }
            ui.allocate_ui_with_layout(
                Vec2::new(width, 64.0),
                Layout::top_down(Align::Min),
                |ui| {
                    ui.set_min_size(Vec2::new(width, 64.0));
                    style::stat_card(ui, theme, *icon, *color, value, label);
                },
            );
        }
    });
}

fn tracks_panel(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
    tracks: &[crate::model::Track],
    width: f32,
    height: f32,
) {
    style::card(theme).show(ui, |ui| {
        ui.set_min_size(Vec2::new(width - 32.0, height - 32.0));
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(app.state.tr(Key::TracksLabel))
                    .size(14.0)
                    .strong()
                    .color(theme.text),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(style::tracks_count(&app.state, tracks.len()))
                        .size(10.5)
                        .color(theme.text_weak),
                );
            });
        });
        ui.add_space(4.0);
        style::divider(ui, theme);
        ui.add_space(4.0);
        if tracks.is_empty() {
            ui.add_space(18.0);
            let mut action = None;
            ui.vertical_centered(|ui| {
                style::empty_state(
                    ui,
                    theme,
                    Icon::Music,
                    app.state.tr(Key::EmptyLibrary),
                    app.state.tr(Key::LibraryEmptyHint),
                    None,
                );
                if style::button_icon(
                    ui,
                    theme,
                    Icon::Folder,
                    app.state.tr(Key::AddMusicFolder),
                    Variant::Primary,
                )
                .clicked()
                {
                    action = Some(Command::PickMusicFolder);
                }
            });
            if let Some(command) = action {
                pending.push(command);
            }
            return;
        }
        for (index, track) in tracks.iter().enumerate() {
            let mut row = TrackRow::new(tracks, index);
            row.height = 50.0;
            row.salt = "library";
            widgets::track_row(ui, app, theme, pending, track, row);
        }
    });
}

fn folders_panel(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
    width: f32,
    height: f32,
) {
    style::card(theme).show(ui, |ui| {
        ui.set_min_size(Vec2::new(width - 32.0, height - 32.0));
        ui.label(
            RichText::new(app.state.tr(Key::MusicFolders))
                .size(14.0)
                .strong()
                .color(theme.text),
        );
        ui.add_space(4.0);
        style::divider(ui, theme);
        ui.add_space(6.0);
        let dirs = app.state.profile.settings.music_dirs.clone();
        if dirs.is_empty() {
            ui.label(
                RichText::new(app.state.tr(Key::NoFoldersYet))
                    .size(11.5)
                    .color(theme.text_weak),
            );
            ui.add_space(6.0);
        }
        let mut remove = None;
        for dir in &dirs {
            ui.horizontal(|ui| {
                let (icon_rect, _) =
                    ui.allocate_exact_size(Vec2::new(16.0, 30.0), egui::Sense::hover());
                style::paint_icon(ui.painter(), Icon::Folder, icon_rect, theme.accent);
                ui.add_space(2.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(dir.to_string_lossy().into_owned())
                            .size(11.0)
                            .color(theme.text),
                    )
                    .truncate(),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if style::icon_button(
                        ui,
                        theme,
                        Icon::Close,
                        24.0,
                        false,
                        app.state.tr(Key::RemoveFolder),
                    )
                    .clicked()
                    {
                        remove = Some(dir.clone());
                    }
                });
            });
            ui.add_space(2.0);
        }
        if let Some(dir) = remove {
            pending.push(Command::RemoveMusicDir(dir));
        }
        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            if style::button_icon(
                ui,
                theme,
                Icon::Plus,
                app.state.tr(Key::AddMusicFolder),
                Variant::Secondary,
            )
            .clicked()
            {
                pending.push(Command::PickMusicFolder);
            }
            ui.add_space(8.0);
            style::hint_line(ui, theme, app.state.tr(Key::ScanHint));
        });
    });
}

/// Shared page guard so `Page` stays referenced on this module.
#[allow(dead_code)]
fn _page(_: Page) {}
