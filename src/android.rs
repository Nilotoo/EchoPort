//! Android entry point (NativeActivity + eframe/glow).

use std::sync::atomic::{AtomicPtr, AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::app::EchoPortApp;
use crate::theme::Theme;

static VM_PTR: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());
static ACTIVITY_PTR: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());
static DENSITY: AtomicU32 = AtomicU32::new(420);
static INSET_CACHE: Mutex<Option<(Instant, (f32, f32))>> = Mutex::new(None);

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
    capture_system_handles(&app);
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

fn capture_system_handles(app: &winit::platform::android::activity::AndroidApp) {
    VM_PTR.store(app.vm_as_ptr(), Ordering::Release);
    ACTIVITY_PTR.store(app.activity_as_ptr(), Ordering::Release);
    if let Some(density) = app.config().density() {
        DENSITY.store(density, Ordering::Release);
    }
}

/// Current Android system-bar insets in egui logical points. Queried lazily
/// after the window exists, because `getRootWindowInsets()` is allowed to
/// return null during `android_main` before the first layout.
pub fn current_system_insets() -> Option<(f32, f32)> {
    if let Ok(cache) = INSET_CACHE.lock() {
        if let Some((at, value)) = *cache {
            if at.elapsed() < Duration::from_millis(500) {
                return Some(value);
            }
        }
    }
    let value = query_system_insets()?;
    if let Ok(mut cache) = INSET_CACHE.lock() {
        *cache = Some((Instant::now(), value));
    }
    Some(value)
}

fn query_system_insets() -> Option<(f32, f32)> {
    use jni::objects::{JObject, JValue};
    use jni::{jni_sig, jni_str, JavaVM};

    let vm_ptr = VM_PTR.load(Ordering::Acquire);
    let activity_ptr = ACTIVITY_PTR.load(Ordering::Acquire);
    if vm_ptr.is_null() || activity_ptr.is_null() {
        return None;
    }
    let vm = unsafe { JavaVM::from_raw(vm_ptr.cast()) };
    let density = DENSITY.load(Ordering::Acquire) as f32;
    let scale = (density / 160.0).max(1.0);
    let result = vm.attach_current_thread(|env| -> jni::errors::Result<(f32, f32)> {
        let activity = unsafe { JObject::from_raw(env, activity_ptr as jni::sys::jobject) };
        let window = env
            .call_method(
                &activity,
                jni_str!("getWindow"),
                jni_sig!("()Landroid/view/Window;"),
                &[],
            )
            .and_then(|value| value.l())?;
        let decor = env
            .call_method(
                &window,
                jni_str!("getDecorView"),
                jni_sig!("()Landroid/view/View;"),
                &[],
            )
            .and_then(|value| value.l())?;
        let insets = env
            .call_method(
                &decor,
                jni_str!("getRootWindowInsets"),
                jni_sig!("()Landroid/view/WindowInsets;"),
                &[],
            )
            .and_then(|value| value.l())?;
        if insets.is_null() {
            log::warn!("Android getRootWindowInsets returned null");
            return Ok((0.0, 0.0));
        }
        // API 30+: WindowInsets.getInsets(int), type mask 7 =
        // statusBars | navigationBars | captionBar.
        let (top, bottom) = match env
            .call_method(
                &insets,
                jni_str!("getInsets"),
                jni_sig!("(I)Landroid/graphics/Insets;"),
                &[JValue::Int(7)],
            )
            .and_then(|value| value.l())
        {
            Ok(insets_obj) => (
                env.get_field(&insets_obj, jni_str!("top"), jni_sig!("I"))?
                    .i()?,
                env.get_field(&insets_obj, jni_str!("bottom"), jni_sig!("I"))?
                    .i()?,
            ),
            // API 26..29 fallback.
            Err(_) => {
                let top = env
                    .call_method(&insets, jni_str!("getStableInsetTop"), jni_sig!("()I"), &[])?
                    .i()?;
                let bottom = env
                    .call_method(
                        &insets,
                        jni_str!("getStableInsetBottom"),
                        jni_sig!("()I"),
                        &[],
                    )?
                    .i()?;
                (top, bottom)
            }
        };
        log::debug!("Android window insets: bottom={bottom}px top={top}px density={density}");
        Ok((top.max(0) as f32 / scale, bottom.max(0) as f32 / scale))
    });
    match result {
        Ok(value) => Some(value),
        Err(err) => {
            log::warn!("could not read Android window insets: {err}");
            None
        }
    }
}
