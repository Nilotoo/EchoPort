//! Android entry point (NativeActivity + eframe/glow).

use crate::app::EchoPortApp;
use crate::theme::Theme;

#[no_mangle]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info),
    );
    if let Some(data) = app.internal_data_path() {
        let cache = data.join("cache");
        let _ = std::fs::create_dir_all(&cache);
        std::env::set_var("ECHOPORT_ANDROID_DATA", &data);
        std::env::set_var("ECHOPORT_ANDROID_CACHE", &cache);
        std::env::set_var("ECHOPORT_ANDROID_MUSIC", data.join("Music"));
    }
    std::env::set_var("ECHOPORT_MOBILE", "1");
    // The desktop update installer must never run on Android; APK updates are
    // delivered by the store / sideloaded package.
    std::env::set_var("ECHOPORT_NO_UPDATE_CHECK", "1");

    let options = eframe::NativeOptions {
        android_app: Some(app),
        ..Default::default()
    };
    if let Err(err) = eframe::run_native(
        "EchoPort",
        options,
        Box::new(|cc| {
            Theme::apply_text_styles(&cc.egui_ctx);
            Ok(Box::new(EchoPortApp::new(cc)))
        }),
    ) {
        log::error!("EchoPort Android failed to start: {err}");
    }
}
