//! Album page: cover, metadata, ordered tracks, favourite toggle and the
//! artist's other albums (album series / discography).

use egui::{Align, Layout, RichText, Sense, Ui, Vec2};

use crate::app::EchoPortApp;
use crate::i18n::Key;
use crate::state::Command;
use crate::theme::{self, Theme};
use crate::ui::style::{self, Icon, Variant};
use crate::ui::widgets::{self, TrackRow};

pub fn draw(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme, pending: &mut Vec<Command>) {
    widgets::page_scroll(ui, |ui| {
        if style::button_icon(
            ui,
            theme,
            Icon::ChevronLeft,
            app.state.tr(Key::BackToSearch),
            Variant::Ghost,
        )
        .clicked()
        {
            pending.push(Command::BackToSearch);
        }
        ui.add_space(6.0);

        let view = app.state.album_page.clone();
        let Some(album) = view.album.clone() else {
            style::empty_state(
                ui,
                theme,
                Icon::Music,
                app.state.tr(Key::NoResultsHint),
                app.state.tr(Key::SearchTryHint),
                Some(app.state.tr(Key::BackToSearch)),
            );
            return;
        };

        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                // Keep the artwork vertically aligned with the title block
                // instead of floating above it.
                ui.add_space(32.0);
                let (cover_rect, _) = ui.allocate_exact_size(Vec2::splat(124.0), Sense::hover());
                if let Some(path) = widgets::album_cover_path(app, &album) {
                    app.ui.covers.paint(
                        ui,
                        theme,
                        Some(&path),
                        cover_rect,
                        style::R_MD,
                        &album.title,
                        &album.id,
                    );
                } else {
                    let (a, b) = style::generated_cover_colors(theme, &album.id);
                    style::gradient_cover(
                        ui.painter(),
                        cover_rect,
                        style::R_MD,
                        a,
                        b,
                        Some(&album.title.chars().next().unwrap_or('♫').to_string()),
                    );
                }
            });
            ui.add_space(14.0);
            ui.vertical(|ui| {
                ui.add(
                    egui::Label::new(RichText::new(&album.title).size(25.0).color(theme.text))
                        .truncate(),
                );
                ui.add_space(3.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(&album.artist)
                            .size(13.0)
                            .color(theme.text_weak),
                    )
                    .truncate(),
                );
                ui.add_space(5.0);
                let count_label = app.state.tr(Key::AlbumTrackCount);
                let meta = match (album.year, album.track_count) {
                    (Some(year), Some(count)) => {
                        format!("{year} · {count} {count_label}")
                    }
                    (Some(year), None) => format!("{year}"),
                    (None, Some(count)) => format!("{count} {count_label}"),
                    _ => album.platform.label().to_owned(),
                };
                ui.horizontal(|ui| {
                    let (dot, _) = ui.allocate_exact_size(Vec2::splat(9.0), Sense::hover());
                    ui.painter().circle_filled(
                        dot.center(),
                        3.5,
                        theme::platform_color(album.platform),
                    );
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(format!("{} · {meta}", album.platform.label()))
                            .size(10.5)
                            .color(theme.text_weak),
                    );
                });
                ui.add_space(8.0);
                let favorite = app.state.profile.is_favorite_album(&album.id);
                if style::button_icon(
                    ui,
                    theme,
                    if favorite {
                        Icon::HeartFill
                    } else {
                        Icon::Heart
                    },
                    app.state.tr(if favorite {
                        Key::FavoriteAlbums
                    } else {
                        Key::FavoriteAlbum
                    }),
                    if favorite {
                        Variant::Tonal
                    } else {
                        Variant::Primary
                    },
                )
                .clicked()
                {
                    pending.push(Command::ToggleFavoriteAlbum(Box::new(album.clone())));
                }
                ui.add_space(3.0);
                if let Some(page) = album.page_url.as_ref() {
                    if style::button(ui, theme, app.state.tr(Key::SourcePage), Variant::Ghost)
                        .clicked()
                    {
                        pending.push(Command::OpenUrl(page.clone()));
                    }
                }
            });
        });
        ui.add_space(14.0);

        if view.loading {
            let time = ui.ctx().input(|input| input.time);
            style::loading_pulse(ui, theme, app.state.tr(Key::Searching), time);
            ui.add_space(8.0);
        }
        if let Some(error) = view.error.as_ref() {
            widgets::callout(ui, theme, Icon::Info, error, false);
            ui.add_space(8.0);
        }

        // 1) The songs of this album come right after the album header.
        if !view.tracks.is_empty() {
            ui.horizontal(|ui| {
                style::section_header(ui, theme, app.state.tr(Key::Songs), None);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if style::button_icon(
                        ui,
                        theme,
                        Icon::Play,
                        app.state.tr(Key::PlayAll),
                        Variant::Primary,
                    )
                    .clicked()
                    {
                        pending.push(Command::PlayTracks(view.tracks.clone()));
                    }
                });
            });
            ui.add_space(4.0);
            for (index, track) in view.tracks.iter().enumerate() {
                let mut row = TrackRow::new(&view.tracks, index).salt("album-detail");
                row.height = 50.0;
                widgets::track_row(ui, app, theme, pending, track, row);
            }
            ui.add_space(12.0);
        } else if !view.loading {
            style::hint_line(ui, theme, app.state.tr(Key::NoResultsHint));
            ui.add_space(8.0);
        }

        // 2) Album background / Wikipedia details sit between the songs and
        //    the artist's other albums.
        if let Some(bio) = view.bio.as_ref() {
            style::card(theme).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                style::section_header(ui, theme, app.state.tr(Key::AlbumInfo), None);
                ui.add_space(4.0);
                ui.label(RichText::new(&bio.extract).size(12.0).color(theme.text));
                if !bio.url.is_empty() {
                    ui.add_space(8.0);
                    if widgets::inline_link(ui, theme, app.state.tr(Key::OpenWikipedia)).clicked() {
                        pending.push(Command::OpenUrl(bio.url.clone()));
                    }
                }
            });
            ui.add_space(14.0);
        }

        // 3) The artist's other albums / album series come last.
        if !view.series.is_empty() {
            style::section_header(ui, theme, app.state.tr(Key::AlbumSeries), None);
            ui.add_space(6.0);
            let width = ((ui.available_width() - 20.0) / 3.0).clamp(190.0, 310.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                for other in &view.series {
                    widgets::album_card(ui, app, theme, pending, other, width);
                }
            });
            ui.add_space(14.0);
        }

        ui.add_space(20.0);
    });
}
