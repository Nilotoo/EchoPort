slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    if let Ok(page) = std::env::var("SLINT_SHOT_PAGE") {
        if let Ok(page) = page.parse::<i32>() {
            ui.set_page(page);
        }
    }
    if let Ok(path) = std::env::var("SLINT_SHOT") {
        let frames = std::env::var("SLINT_SHOT_FRAMES")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(1);
        let weak = ui.as_weak();
        let timer = slint::Timer::default();
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        if frames > 1 {
            let counter = counter.clone();
            let path = path.clone();
            timer.start(
                slint::TimerMode::Repeated,
                std::time::Duration::from_millis(120),
                move || {
                    let index = counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if index >= frames {
                        return;
                    }
                    if let Some(ui) = weak.upgrade() {
                        if let Ok(buffer) = ui.window().take_snapshot() {
                            if let Some(image) = image::RgbaImage::from_raw(
                                buffer.width(),
                                buffer.height(),
                                buffer.as_bytes().to_vec(),
                            ) {
                                let frame_path = path.replace(".png", &format!("-{index:02}.png"));
                                if let Err(err) = image.save(&frame_path) {
                                    eprintln!("screenshot frame save failed: {err}");
                                }
                            }
                        }
                    }
                },
            );
        } else {
            timer.start(
                slint::TimerMode::SingleShot,
                std::time::Duration::from_millis(2200),
                move || {
                    if let Some(ui) = weak.upgrade() {
                        if let Ok(buffer) = ui.window().take_snapshot() {
                            if let Some(image) = image::RgbaImage::from_raw(
                                buffer.width(),
                                buffer.height(),
                                buffer.as_bytes().to_vec(),
                            ) {
                                if let Err(err) = image.save(&path) {
                                    eprintln!("screenshot save failed: {err}");
                                } else {
                                    println!("screenshot: {path}");
                                }
                            }
                        }
                    }
                },
            );
        }
        std::mem::forget(timer);
    }
    ui.run()
}
