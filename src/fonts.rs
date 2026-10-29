//! System font discovery.
//!
//! egui ships Latin fonts only. CJK text (song titles, the Chinese UI) needs a
//! fallback face, so we look for the usual system fonts and register them. No
//! giant font is bundled with the binary.

use std::path::PathBuf;
use std::sync::Arc;

/// Candidate (path, face index) pairs per platform, first existing wins.
fn cjk_candidates() -> Vec<(PathBuf, u32)> {
    #[cfg(target_os = "linux")]
    {
        vec![
            (
                PathBuf::from("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc"),
                2,
            ),
            (
                PathBuf::from("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"),
                2,
            ),
            (
                PathBuf::from("/usr/share/fonts/noto/NotoSansCJK-Regular.ttc"),
                2,
            ),
            (
                PathBuf::from("/usr/share/fonts/truetype/wqy/wqy-microhei.ttc"),
                0,
            ),
            (
                PathBuf::from("/usr/share/fonts/wenquanyi/wqy-microhei/wqy-microhei.ttc"),
                0,
            ),
            (
                PathBuf::from("/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc"),
                2,
            ),
            (
                PathBuf::from("/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc"),
                2,
            ),
            (PathBuf::from("/System/Library/Fonts/PingFang.ttc"), 0),
        ]
    }
    #[cfg(target_os = "windows")]
    {
        vec![
            (PathBuf::from(r"C:\Windows\Fonts\msyh.ttc"), 0),
            (PathBuf::from(r"C:\Windows\Fonts\msyh.ttf"), 0),
            (PathBuf::from(r"C:\Windows\Fonts\simhei.ttf"), 0),
            (PathBuf::from(r"C:\Windows\Fonts\simsun.ttc"), 0),
        ]
    }
    #[cfg(target_os = "macos")]
    {
        vec![
            (PathBuf::from("/System/Library/Fonts/PingFang.ttc"), 0),
            (PathBuf::from("/System/Library/Fonts/STHeiti Medium.ttc"), 0),
            (PathBuf::from("/Library/Fonts/Arial Unicode.ttf"), 0),
        ]
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        Vec::new()
    }
}

fn find_font() -> Option<(Vec<u8>, u32)> {
    if let Ok(explicit) = std::env::var("ECHOPORT_CJK_FONT") {
        let path = PathBuf::from(explicit);
        if let Ok(bytes) = std::fs::read(&path) {
            return Some((bytes, 0));
        }
    }
    for (path, index) in cjk_candidates() {
        if let Ok(bytes) = std::fs::read(&path) {
            return Some((bytes, index));
        }
    }
    // Last resort: ask fontconfig on Linux.
    #[cfg(target_os = "linux")]
    {
        if let Ok(output) = std::process::Command::new("fc-match")
            .args(["-f", "%{file}", ":lang=zh-cn"])
            .output()
        {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if !path.is_empty() {
                if let Ok(bytes) = std::fs::read(&path) {
                    return Some((bytes, 0));
                }
            }
        }
    }
    None
}

/// Install the CJK fallback (idempotent, safe to call once at startup).
pub fn install(ctx: &egui::Context) {
    let Some((bytes, index)) = find_font() else {
        log::warn!("no CJK font found; Chinese text may render as tofu");
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "cjk-fallback".to_owned(),
        Arc::new(egui::FontData {
            font: bytes.into(),
            index,
            tweak: Default::default(),
        }),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push("cjk-fallback".to_owned());
    }
    ctx.set_fonts(fonts);
}
