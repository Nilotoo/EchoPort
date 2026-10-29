//! EchoPort desktop entry point.

// Keep the GUI clean on Windows: no console window is allocated for the app.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::sync::Arc;

use echoport::{app::EchoPortApp, theme::Theme};

fn main() -> eframe::Result {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .parse_default_env()
        .init();

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1360.0, 860.0])
        .with_min_inner_size([1040.0, 660.0])
        .with_title("EchoPort")
        .with_app_id("echoport");
    if let Some(icon) = build_icon() {
        viewport = viewport.with_icon(Arc::new(icon));
    }

    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "EchoPort",
        options,
        Box::new(|cc| {
            Theme::apply_text_styles(&cc.egui_ctx);
            Ok(Box::new(EchoPortApp::new(cc)))
        }),
    )
}

/// App icon: the same artwork as `assets/icon.png` (soft blue→pink sphere on a
/// rounded porcelain tile) — no outer ring.
fn build_icon() -> Option<egui::IconData> {
    let image = image::load_from_memory(include_bytes!("../assets/icon.png"))
        .ok()?
        .into_rgba8();
    let (width, height) = image.dimensions();
    Some(egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    })
}
