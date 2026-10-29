//! Artist / producer page: avatar header, Wikipedia bio, popular tracks fetched
//! from the provider and the user's own listening history for that artist.

use egui::{Align, Layout, Rect, RichText, Sense, Ui, Vec2};

use crate::app::EchoPortApp;
use crate::i18n::Key;
use crate::state::Command;
use crate::theme::{self, Theme};
use crate::ui::style::{self, Icon, Variant};
use crate::ui::widgets::{self, TrackRow};

pub fn draw(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme, pending: &mut Vec<Command>) {
    widgets::page_scroll(ui, |ui| {
        ui.horizontal(|ui| {
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
        });
        ui.add_space(6.0);

        let view = app.state.artist_page.clone();
        let Some(artist) = view.artist.clone() else {
            let response = style::empty_state(
                ui,
                theme,
                Icon::Profile,
                app.state.tr(Key::ArtistMissing),
                app.state.tr(Key::SearchTryHint),
                Some(app.state.tr(Key::BackToSearch)),
            );
            if response.map(|response| response.clicked()).unwrap_or(false) {
                pending.push(Command::BackToSearch);
            }
            return;
        };

        // Header: avatar + name + platform badge + source page shortcut.
        ui.horizontal_top(|ui| {
            let avatar = Rect::from_min_size(ui.cursor().min, Vec2::splat(76.0));
            ui.allocate_rect(avatar, Sense::hover());
            widgets::paint_artist_avatar(ui, app, theme, &artist, avatar);
            ui.add_space(14.0);
            ui.vertical(|ui| {
                ui.add_space(6.0);
                ui.add(
                    egui::Label::new(RichText::new(&artist.name).size(26.0).color(theme.text))
                        .truncate(),
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let (dot, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
                    ui.painter().circle_filled(
                        dot.center(),
                        4.0,
                        theme::platform_color(artist.platform),
                    );
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(artist.platform.label())
                            .size(11.5)
                            .color(style::weak_text(theme)),
                    );
                });
                if view.loading {
                    ui.add_space(6.0);
                    let time = ui.ctx().input(|input| input.time);
                    style::loading_pulse(ui, theme, app.state.tr(Key::Searching), time);
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_millis(90));
                }
            });
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                ui.add_space(8.0);
                if artist.page_url.is_some()
                    && style::button(ui, theme, app.state.tr(Key::SourcePage), Variant::Secondary)
                        .clicked()
                {
                    if let Some(url) = &artist.page_url {
                        pending.push(Command::OpenUrl(url.clone()));
                    }
                }
            });
        });
        ui.add_space(16.0);

        if let Some(error) = &view.error {
            widgets::callout(ui, theme, Icon::Info, error, false);
            ui.add_space(8.0);
        }

        // Wikipedia biography.
        style::card(theme).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            style::section_header(ui, theme, app.state.tr(Key::ArtistBio), None);
            match &view.bio {
                Some(bio) => {
                    ui.label(RichText::new(&bio.extract).size(12.5).color(theme.text));
                    if !bio.url.is_empty() {
                        ui.add_space(8.0);
                        if widgets::inline_link(ui, theme, app.state.tr(Key::OpenWikipedia))
                            .clicked()
                        {
                            pending.push(Command::OpenUrl(bio.url.clone()));
                        }
                    }
                }
                None => {
                    ui.label(
                        RichText::new(app.state.tr(Key::NoArtistBio))
                            .size(11.5)
                            .color(theme.text_weak),
                    );
                }
            }
        });
        ui.add_space(12.0);

        // Popular tracks from the platform.
        style::card(theme).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            style::section_header(
                ui,
                theme,
                app.state.tr(Key::PopularTracks),
                Some(&format!("{}", view.hot_tracks.len())),
            );
            if view.hot_tracks.is_empty() {
                if !view.loading {
                    ui.label(
                        RichText::new(app.state.tr(Key::SearchNoResults))
                            .size(11.5)
                            .color(theme.text_weak),
                    );
                }
            } else {
                for (index, track) in view.hot_tracks.iter().enumerate() {
                    let mut row = TrackRow::new(&view.hot_tracks, index).salt("artist-hot");
                    row.height = 50.0;
                    widgets::track_row(ui, app, theme, pending, track, row);
                }
            }
        });
        ui.add_space(12.0);

        // Local listening history for this artist.
        if !view.local_tracks.is_empty() {
            style::card(theme).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                style::section_header(
                    ui,
                    theme,
                    app.state.tr(Key::YourListeningHistory),
                    Some(&format!("{}", view.local_tracks.len())),
                );
                for (index, track) in view.local_tracks.iter().enumerate() {
                    let mut row = TrackRow::new(&view.local_tracks, index).salt("artist-history");
                    row.height = 50.0;
                    widgets::track_row(ui, app, theme, pending, track, row);
                }
            });
        }
        ui.add_space(24.0);
    });
}
