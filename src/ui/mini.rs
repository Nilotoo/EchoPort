//! Always-on-top mini player window ("floating card").
//!
//! Opened by clicking the EchoPort brand in the sidebar. It is rendered as an
//! immediate egui viewport so it can read the same app state; the window stays
//! on top of other apps and can restore the main window.

use egui::{Align, CornerRadius, Layout, Rect, RichText, Sense, Vec2};

use crate::app::EchoPortApp;
use crate::i18n::Key;
use crate::state::Command;
use crate::theme::Theme;
use crate::ui::style::{self, Icon};
use crate::ui::widgets;

const MINI_ID: &str = "echoport-mini-player";

pub fn show(ctx: &egui::Context, app: &mut EchoPortApp) {
    if !app.mini_player {
        return;
    }
    let theme = app.state.theme;
    let viewport_id = egui::ViewportId::from_hash_of(MINI_ID);
    let builder = egui::ViewportBuilder::default()
        .with_title("EchoPort")
        .with_inner_size([420.0, 142.0])
        .with_min_inner_size([360.0, 124.0])
        .with_resizable(true)
        .with_always_on_top();
    let commands = ctx.show_viewport_immediate(viewport_id, builder, |mini_ctx, _class| {
        let mut pending: Vec<Command> = Vec::new();
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme.panel)
                    .inner_margin(egui::Margin::same(12)),
            )
            .show(mini_ctx, |ui| {
                draw(ui, app, &theme, &mut pending);
            });
        if mini_ctx.input(|input| input.viewport().close_requested()) {
            app.mini_player = false;
        }
        if app.state.playback.state.is_playing() {
            mini_ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
        // QA hook: capture the mini window itself (egui viewport screenshot).
        if let Ok(path) = std::env::var("ECHOPORT_MINI_SHOT") {
            let requested_id = egui::Id::new("echoport-mini-shot-requested");
            let requested = mini_ctx
                .data(|data| data.get_temp::<bool>(requested_id))
                .unwrap_or(false);
            if !requested && mini_ctx.input(|input| input.time) > 2.0 {
                mini_ctx.data_mut(|data| data.insert_temp(requested_id, true));
                mini_ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                    egui::UserData::default(),
                ));
                mini_ctx.request_repaint();
            }
            mini_ctx.request_repaint();
            let mut captured = None;
            mini_ctx.input(|input| {
                for event in &input.events {
                    if let egui::Event::Screenshot { image, .. } = event {
                        captured = Some(image.clone());
                    }
                }
            });
            if let Some(image) = captured {
                if let Some(parent) = std::path::Path::new(&path).parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                match crate::app::save_color_image(&image, std::path::Path::new(&path)) {
                    Ok(()) => log::info!("mini player screenshot written to {path}"),
                    Err(err) => log::error!("failed to write mini player screenshot: {err}"),
                }
                pending.push(Command::ToggleMiniPlayer);
            }
        }
        pending
    });
    for command in commands {
        app.dispatch(command);
    }
}

fn draw(ui: &mut egui::Ui, app: &mut EchoPortApp, theme: &Theme, pending: &mut Vec<Command>) {
    let track = app.state.now_playing.clone();
    let playing = app.state.playback.state.is_playing();
    let position = app.state.playback.position_secs;
    let duration = app.state.playback.duration_secs.unwrap_or(0.0);
    let cover_side = 82.0;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);
        let (cover_rect, cover_response) =
            ui.allocate_exact_size(Vec2::splat(cover_side), Sense::click());
        match track.as_ref() {
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
                    cover_rect.shrink(20.0),
                    theme.text_weak,
                );
            }
        }
        if cover_response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(app.state.tr(Key::OpenMainWindow))
            .clicked()
        {
            pending.push(Command::RestoreMainWindow);
        }
        ui.add_space(12.0);
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.add_space(2.0);
                match track.as_ref() {
                    Some(track) => {
                        ui.label(
                            RichText::new(&track.title)
                                .size(16.0)
                                .strong()
                                .color(theme.text),
                        );
                    }
                    None => {
                        ui.label(
                            RichText::new(app.state.tr(Key::NothingPlaying))
                                .size(14.0)
                                .color(theme.text_weak),
                        );
                    }
                }
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
                        pending.push(Command::ToggleMiniPlayer);
                    }
                    if style::icon_button(
                        ui,
                        theme,
                        Icon::External,
                        22.0,
                        false,
                        app.state.tr(Key::OpenMainWindow),
                    )
                    .clicked()
                    {
                        pending.push(Command::RestoreMainWindow);
                    }
                });
            });
            if let Some(track) = track.as_ref() {
                ui.add_space(1.0);
                ui.label(
                    RichText::new(format!("{} · {}", track.artist, track.platform.label()))
                        .size(11.5)
                        .color(theme.text_weak),
                );
            }
            ui.add_space(8.0);
            let bar_width = (ui.available_width() - 8.0).max(90.0);
            let (bar_rect, _) = ui.allocate_exact_size(Vec2::new(bar_width, 10.0), Sense::hover());
            let bar = Rect::from_center_size(bar_rect.center(), Vec2::new(bar_rect.width(), 4.0));
            ui.painter()
                .rect_filled(bar, CornerRadius::same(2), style::wash(theme));
            let fraction = if duration > 0.0 {
                (position / duration).clamp(0.0, 1.0) as f32
            } else {
                0.0
            };
            let filled =
                Rect::from_min_size(bar.min, Vec2::new(bar.width() * fraction, bar.height()));
            ui.painter()
                .rect_filled(filled, CornerRadius::same(2), theme.accent);
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(crate::model::format_duration(position))
                        .size(10.0)
                        .color(theme.text_weak),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(crate::model::format_duration(duration))
                            .size(10.0)
                            .color(theme.text_weak),
                    );
                });
            });
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let (play_rect, play_response) =
                ui.allocate_exact_size(Vec2::splat(40.0), Sense::click());
            if style::play_button_at(ui, play_rect, theme, playing)
                .union(play_response)
                .clicked()
            {
                pending.push(Command::TogglePlayPause);
            }
            ui.add_space(2.0);
            if style::icon_button(ui, theme, Icon::Next, 26.0, false, app.state.tr(Key::Next))
                .clicked()
            {
                pending.push(Command::Next);
            }
            if style::icon_button(
                ui,
                theme,
                Icon::Previous,
                26.0,
                false,
                app.state.tr(Key::Previous),
            )
            .clicked()
            {
                pending.push(Command::Previous);
            }
        });
    });
}
