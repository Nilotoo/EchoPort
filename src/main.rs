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
    install_crash_log();

    let mut size = [1360.0, 860.0];
    let mut min_size = [1040.0, 660.0];
    if let Ok(value) = std::env::var("ECHOPORT_WINDOW") {
        let parts: Vec<f32> = value
            .split('x')
            .filter_map(|part| part.trim().parse::<f32>().ok())
            .collect();
        if parts.len() == 2 && parts[0] >= 320.0 && parts[1] >= 480.0 {
            size = [parts[0], parts[1]];
            min_size = [parts[0].min(980.0), parts[1].min(620.0)];
        }
    }
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size(size)
        .with_min_inner_size(min_size)
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

/// Write a local crash log before the default panic hook runs. This keeps a
/// `panic=unwind` failure inspectable even when the GUI disappears instantly.
fn install_crash_log() {
    std::panic::set_hook(Box::new(|info| {
        use std::io::Write;
        let paths = echoport::store::AppPaths::resolve();
        let _ = std::fs::create_dir_all(&paths.data_dir);
        let log_path = paths.data_dir.join("crash.log");
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_secs())
                .unwrap_or(0);
            let payload = info
                .payload()
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
                .unwrap_or("unknown panic");
            let _ = writeln!(
                file,
                "time={now} location={:?} message={payload}\n{}",
                info.location(),
                std::backtrace::Backtrace::force_capture()
            );
        }
        eprintln!("EchoPort panicked: {info}");
    }));
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
