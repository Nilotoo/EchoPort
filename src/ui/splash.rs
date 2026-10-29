//! Very short startup animation with an optional update prompt.
//!
//! The splash lasts <1 s. If the asynchronous GitHub update check finds a new
//! release while it is visible, the splash stays with two choices: keep the
//! current version or open the release page.

use egui::{
    Align, Color32, CornerRadius, Layout, Mesh, Pos2, Rect, RichText, Sense, Stroke, StrokeKind,
    Vec2,
};

use crate::app::EchoPortApp;
use crate::i18n::Key;
use crate::state::Command;
use crate::theme::{self, Theme};
use crate::ui::style::{self, Variant};

pub fn draw(ctx: &egui::Context, app: &mut EchoPortApp, pending: &mut Vec<Command>) {
    if app.splash_done {
        return;
    }
    let elapsed = app.startup_started.elapsed().as_secs_f32();
    let theme = app.state.theme;
    let update = app
        .state
        .updates
        .available
        .as_ref()
        .filter(|info| crate::update::is_newer(&info.latest, &info.current))
        .filter(|_| !app.state.updates.dismissed)
        .cloned();
    let prompt_ready = update.is_some() && elapsed >= 0.30;
    // Give a slow update check a moment to land so an available update can be
    // shown right on the loading screen (but never hold startup hostage).
    // If the (usually fast) GitHub check is still in flight, keep the splash
    // alive a little longer so an update prompt can actually be shown.
    let waiting_for_check = app.state.updates.checking && elapsed < 2.50;
    if !prompt_ready && !waiting_for_check && elapsed >= 0.90 {
        app.splash_done = true;
        return;
    }

    let screen = ctx.screen_rect();
    let alpha = (elapsed / 0.14).clamp(0.0, 1.0);
    let bg = Color32::from_rgba_unmultiplied(
        theme.bg.r(),
        theme.bg.g(),
        theme.bg.b(),
        (255.0 * alpha) as u8,
    );
    egui::Area::new(egui::Id::new("echoport-splash"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let painter = ui.painter().clone();
            painter.rect_filled(screen, CornerRadius::ZERO, bg);
            // Consume clicks on the splash background so the main UI cannot be
            // interacted with while the overlay is visible.
            let _ = ui.interact(screen, egui::Id::new("splash-block"), Sense::click());

            let center_x = screen.center().x;
            let logo = Rect::from_center_size(
                Pos2::new(center_x, screen.center().y - 92.0),
                Vec2::splat(92.0),
            );
            paint_logo(&painter, logo, &theme, alpha);
            let wordmark =
                ui.painter()
                    .layout_no_wrap("EchoPort".to_owned(), style::font(25.0), theme.text);
            let word_pos = Pos2::new(center_x - wordmark.size().x * 0.5, logo.bottom() + 18.0);
            let text = Color32::from_rgba_unmultiplied(
                theme.text.r(),
                theme.text.g(),
                theme.text.b(),
                (255.0 * alpha) as u8,
            );
            painter.galley(word_pos, wordmark.clone(), text);
            let dot = Color32::from_rgba_unmultiplied(
                theme.accent.r(),
                theme.accent.g(),
                theme.accent.b(),
                (255.0 * alpha) as u8,
            );
            painter.circle_filled(
                Pos2::new(
                    word_pos.x + wordmark.size().x + 5.0,
                    word_pos.y + wordmark.size().y * 0.72,
                ),
                3.8,
                dot,
            );

            // Thin progress line (finishes when the splash would close).
            let progress = (elapsed / 0.90).clamp(0.0, 1.0);
            let bar = Rect::from_center_size(
                Pos2::new(center_x, word_pos.y + wordmark.size().y + 22.0),
                Vec2::new(126.0, 2.0),
            );
            painter.rect_filled(
                bar,
                CornerRadius::same(1),
                theme::with_alpha(theme.border, (255.0 * alpha) as u8),
            );
            let filled =
                Rect::from_min_size(bar.min, Vec2::new(bar.width() * progress, bar.height()));
            painter.rect_filled(
                filled,
                CornerRadius::same(1),
                Color32::from_rgba_unmultiplied(
                    theme.accent.r(),
                    theme.accent.g(),
                    theme.accent.b(),
                    (255.0 * alpha) as u8,
                ),
            );

            if prompt_ready {
                if let Some(info) = update.as_ref() {
                    let card_rect = Rect::from_center_size(
                        Pos2::new(center_x, screen.center().y + 100.0),
                        Vec2::new(376.0, 144.0),
                    );
                    painter.rect_filled(card_rect, CornerRadius::same(style::R_LG), theme.surface);
                    painter.rect_stroke(
                        card_rect,
                        CornerRadius::same(style::R_LG),
                        Stroke::new(1.0_f32, theme.border),
                        StrokeKind::Inside,
                    );
                    let mut child = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(card_rect.shrink(16.0))
                            .layout(Layout::top_down(Align::Center)),
                    );
                    child.label(
                        RichText::new(app.state.tr(Key::UpdateAvailable))
                            .size(15.0)
                            .strong()
                            .color(theme.text),
                    );
                    child.label(
                        RichText::new(format!("v{} · {}", info.latest, info.title))
                            .size(11.5)
                            .color(theme.text_weak),
                    );
                    child.add_space(10.0);
                    let stay_label = app.state.tr(Key::StayCurrent);
                    let update_label = app.state.tr(Key::UpdateNow);
                    let font = style::font(13.0);
                    let width_of = |label: &str| {
                        child
                            .painter()
                            .layout_no_wrap(label.to_owned(), font.clone(), theme.text)
                            .size()
                            .x
                            + 30.0
                    };
                    let total = width_of(stay_label) + width_of(update_label) + 10.0;
                    child.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        ui.add_space(((ui.available_width() - total) * 0.5).max(0.0));
                        if style::button(ui, &theme, stay_label, Variant::Secondary).clicked() {
                            pending.push(Command::DismissUpdate);
                        }
                        if style::button(ui, &theme, update_label, Variant::Primary).clicked() {
                            pending.push(Command::OpenUpdatePage);
                        }
                    });
                }
            }
        });
    ctx.request_repaint_after(std::time::Duration::from_millis(16));
}

/// The current app mark without the old outer ring: a soft blue→pink sphere on
/// a rounded light tile.
fn paint_logo(painter: &egui::Painter, rect: Rect, theme: &Theme, alpha: f32) {
    let a = |color: Color32| {
        Color32::from_rgba_unmultiplied(
            color.r(),
            color.g(),
            color.b(),
            (f32::from(color.a()) * alpha) as u8,
        )
    };
    painter.rect_filled(
        rect,
        CornerRadius::same(20),
        a(Color32::from_rgb(246, 249, 254)),
    );
    painter.rect_stroke(
        rect,
        CornerRadius::same(20),
        Stroke::new(1.0_f32, a(theme::with_alpha(theme.border, 200))),
        StrokeKind::Inside,
    );
    let center = rect.center();
    let radius = rect.width() * 0.30;
    let accent = Color32::from_rgb(0x3B, 0x82, 0xC6);
    let rim = Color32::from_rgb(0xF7, 0xC8, 0xD8);
    let mut mesh = Mesh::default();
    mesh.vertices.push(egui::epaint::Vertex {
        pos: center,
        uv: Pos2::ZERO,
        color: a(accent),
    });
    let segments = 48;
    for index in 0..=segments {
        let angle = index as f32 / segments as f32 * std::f32::consts::TAU;
        mesh.vertices.push(egui::epaint::Vertex {
            pos: center + Vec2::angled(angle) * radius,
            uv: Pos2::ZERO,
            color: a(theme::mix(accent, rim, 0.9)),
        });
    }
    for index in 0..segments {
        mesh.add_triangle(0, index as u32 + 1, index as u32 + 2);
    }
    painter.add(egui::Shape::mesh(mesh));
    // Small highlight so the sphere keeps the old icon's soft look.
    painter.circle_filled(
        center + Vec2::new(-radius * 0.28, -radius * 0.30),
        radius * 0.22,
        a(Color32::from_rgba_unmultiplied(255, 255, 255, 70)),
    );
}
