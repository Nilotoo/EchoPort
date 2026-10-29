//! Integration tests: palette extraction and theme contrast (accessibility).
//!
//! EchoPort ships three hand-picked themes — **Pulse** (default), **Tide** and
//! **Ink** (dark) — and an optional "follow cover accent" mode that only tints
//! accent colours while playing. The contract requires body text contrast ≥ 4.0
//! in every theme, readable secondary text and deterministic palette extraction.

use std::io::Cursor;

use echoport::model::Platform;
use echoport::theme::{
    best_on, contrast, cover_glow, extract_palette_bytes, luminance, mix, platform_color,
    readable_on, Palette, Theme, ThemeMode,
};

fn assert_readable(
    label: &str,
    foreground: egui::Color32,
    background: egui::Color32,
    minimum: f32,
) {
    let ratio = contrast(foreground, background);
    assert!(
        ratio >= minimum,
        "{label}: contrast {ratio:.2} is below the required {minimum:.2} \
         (fg={foreground:?}, bg={background:?})"
    );
}

#[test]
fn contrast_maths_matches_wcag_reference_values() {
    let black = egui::Color32::BLACK;
    let white = egui::Color32::WHITE;
    assert!((contrast(black, white) - 21.0).abs() < 0.05);
    assert!(
        (contrast(white, black) - 21.0).abs() < 0.05,
        "contrast is symmetric"
    );
    assert!((contrast(black, black) - 1.0).abs() < 0.001);
    assert!((contrast(white, white) - 1.0).abs() < 0.001);
    assert!(contrast(black, white) > contrast(mix(white, black, 0.5), white));
    assert!((luminance(white) - 1.0).abs() < 0.001);
    assert!(luminance(black) < 0.001);
}

#[test]
fn readable_on_and_best_on_guarantee_contrast() {
    let weak = egui::Color32::from_rgb(0x3B, 0x82, 0xC6);
    let background = egui::Color32::from_rgb(0x3B, 0x82, 0xC6);
    assert!(
        contrast(weak, background) < 2.0,
        "test needs a low contrast pair"
    );
    let fixed = readable_on(weak, background, 4.0);
    assert_readable("readable_on", fixed, background, 4.0);
    // Already-readable colours are returned unchanged.
    let readable = readable_on(egui::Color32::BLACK, egui::Color32::WHITE, 4.0);
    assert_eq!(readable, egui::Color32::BLACK);

    // best_on picks whichever of black/white is more readable on a fill.
    for fill in [
        egui::Color32::from_rgb(0x25, 0x63, 0xEB), // vivid blue
        egui::Color32::from_rgb(0xC8, 0xFF, 0x3D), // fluorescent lime
        egui::Color32::from_rgb(0x1D, 0x2A, 0x38), // dark slate
    ] {
        let picked = best_on(fill);
        assert!(picked == egui::Color32::BLACK || picked == egui::Color32::WHITE);
        assert_readable("best_on", picked, fill, 3.0);
    }
}

#[test]
fn body_text_contrast_is_at_least_4_0_in_every_theme() {
    let fallback = Palette::fallback();
    let variants: [(&str, Option<&Palette>); 2] = [
        ("no cover palette", None),
        ("cover palette", Some(&fallback)),
    ];
    for mode in ThemeMode::ALL {
        for (label, palette) in variants {
            let theme = Theme::for_mode(mode, palette);
            assert_eq!(
                theme.dark,
                mode.is_dark(),
                "{mode:?} dark flag should be {}",
                mode.is_dark()
            );
            assert_readable(
                &format!("{mode:?}/{label} body text/background"),
                theme.text,
                theme.bg,
                4.0,
            );
            assert_readable(
                &format!("{mode:?}/{label} body text/panel"),
                theme.text,
                theme.panel,
                4.0,
            );
            assert_readable(
                &format!("{mode:?}/{label} body text/surface"),
                theme.text,
                theme.surface,
                4.0,
            );
            assert_readable(
                &format!("{mode:?}/{label} secondary text/background"),
                theme.text_weak,
                theme.bg,
                2.4,
            );
            // Button / chip labels must stay legible on their fills.
            assert_readable(
                &format!("{mode:?}/{label} on_accent/accent"),
                theme.on_accent,
                theme.accent,
                3.0,
            );
            assert_readable(
                &format!("{mode:?}/{label} on_highlight/highlight"),
                theme.on_highlight,
                theme.highlight,
                3.0,
            );
            // Theme surfaces must be distinguishable from the page background.
            assert_ne!(
                theme.panel, theme.bg,
                "{mode:?} panel and background are identical"
            );
            // The macaron palette always has five decoration colours.
            assert_eq!(theme.macaron.len(), 5);
        }
    }
}

#[test]
fn base_constructors_and_for_mode_agree() {
    assert_eq!(Theme::base(ThemeMode::Pulse), Theme::pulse());
    assert_eq!(Theme::base(ThemeMode::Tide), Theme::tide());
    assert_eq!(Theme::base(ThemeMode::Ink), Theme::ink());
    for mode in ThemeMode::ALL {
        let plain = Theme::for_mode(mode, None);
        assert_eq!(
            plain,
            Theme::base(mode),
            "{mode:?} without a palette is the base theme"
        );
        // A cover palette may tint accents but never flips the theme's darkness.
        let tinted = Theme::for_mode(mode, Some(&Palette::fallback()));
        assert_eq!(tinted.dark, plain.dark);
        assert_eq!(
            tinted.bg, plain.bg,
            "cover palette must not repaint the surface"
        );
        assert_eq!(
            tinted.text, plain.text,
            "cover palette must not change body text"
        );
    }
    // Pulse is the documented default.
    assert_eq!(ThemeMode::default(), ThemeMode::Pulse);
}

#[test]
fn theme_modes_roundtrip_cycle_and_accept_legacy_keys() {
    assert_eq!(ThemeMode::ALL.len(), 3);
    let mut mode = ThemeMode::Pulse;
    let mut seen = Vec::new();
    for _ in 0..3 {
        seen.push(mode);
        assert_eq!(ThemeMode::from_key(mode.key()), Some(mode));
        mode = mode.next();
    }
    assert_eq!(mode, ThemeMode::Pulse, "cycle returns to the start");
    assert_eq!(
        seen,
        vec![ThemeMode::Pulse, ThemeMode::Tide, ThemeMode::Ink]
    );
    assert_eq!(ThemeMode::Pulse.key(), "pulse");
    assert_eq!(ThemeMode::Tide.key(), "tide");
    assert_eq!(ThemeMode::Ink.key(), "ink");

    // Pre-redesign profiles / scripts used these names; they must keep loading.
    assert_eq!(ThemeMode::from_key("blue_white"), Some(ThemeMode::Pulse));
    assert_eq!(ThemeMode::from_key("album"), Some(ThemeMode::Pulse));
    assert_eq!(ThemeMode::from_key("midnight"), Some(ThemeMode::Ink));
    assert_eq!(ThemeMode::from_key("porcelain"), Some(ThemeMode::Pulse));
    assert_eq!(ThemeMode::from_key("nope"), None);

    assert!(!ThemeMode::Pulse.is_dark());
    assert!(!ThemeMode::Tide.is_dark());
    assert!(ThemeMode::Ink.is_dark());
}

#[test]
fn legacy_theme_mode_json_still_deserializes() {
    // Old profiles stored "blue_white" / "midnight" / "album" in settings.
    for (json, expected) in [
        (r#""blue_white""#, ThemeMode::Pulse),
        (r#""midnight""#, ThemeMode::Ink),
        (r#""album""#, ThemeMode::Pulse),
        (r#""pulse""#, ThemeMode::Pulse),
        (r#""tide""#, ThemeMode::Tide),
        (r#""ink""#, ThemeMode::Ink),
    ] {
        let mode: ThemeMode = serde_json::from_str(json).expect("theme mode must deserialize");
        assert_eq!(mode, expected, "legacy theme key {json}");
    }
    // Round-trips through the current key names.
    for mode in ThemeMode::ALL {
        let json = serde_json::to_string(&mode).expect("serialize");
        let back: ThemeMode = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, mode);
    }
}

#[test]
fn themes_without_a_cover_palette_are_the_base_theme() {
    assert_eq!(Theme::for_mode(ThemeMode::Pulse, None), Theme::pulse());
    let theme = Theme::for_mode(ThemeMode::Tide, Some(&Palette::fallback()));
    assert!(!theme.dark);
    assert_eq!(theme.macaron.len(), 5);
    let glow = cover_glow(&theme);
    assert_eq!(glow.len(), 2);
    assert_eq!(glow[0].a(), 200);
}

#[test]
fn platform_colours_are_distinct_for_every_source() {
    let mut seen: Vec<egui::Color32> = Vec::new();
    for platform in Platform::ALL {
        let colour = platform_color(platform);
        assert!(
            !seen.contains(&colour),
            "duplicate platform colour for {platform:?}"
        );
        seen.push(colour);
        assert!(
            contrast(colour, egui::Color32::WHITE) > 2.0
                && contrast(colour, egui::Color32::BLACK) > 2.0,
            "platform dot {platform:?} is hard to see on both light and dark rows: {colour:?}"
        );
    }
    assert_eq!(seen.len(), 8);
}

#[test]
fn palette_extraction_is_deterministic_and_needs_some_colour() {
    let mut image = image::RgbaImage::new(64, 64);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        *pixel = match (x < 32, y < 32) {
            (true, true) => image::Rgba([0xC0, 0x39, 0x2B, 0xFF]), // brick red
            (false, true) => image::Rgba([0x2D, 0x6A, 0x9F, 0xFF]), // deep blue
            (true, false) => image::Rgba([0xF0, 0xC4, 0x5C, 0xFF]), // mustard
            (false, false) => image::Rgba([0x5B, 0x2A, 0x86, 0xFF]), // purple
        };
    }
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
        .expect("encode test png");

    let first = extract_palette_bytes(&png).expect("colourful image yields a palette");
    let second = extract_palette_bytes(&png).expect("palette extraction is deterministic");
    assert_eq!(first, second);
    assert_eq!(first.macarons.len(), 5);
    assert_ne!(first.vivid, first.deep);
    assert!(luminance(first.deep) < luminance(first.vivid) + 0.0001);

    // Unreadable / colourless inputs are rejected instead of producing garbage.
    assert_eq!(extract_palette_bytes(b"not an image"), None);
    assert_eq!(extract_palette_bytes(&[]), None);
    let white = image::RgbaImage::from_pixel(32, 32, image::Rgba([0xFF, 0xFF, 0xFF, 0xFF]));
    let mut white_png = Vec::new();
    image::DynamicImage::ImageRgba8(white)
        .write_to(&mut Cursor::new(&mut white_png), image::ImageFormat::Png)
        .expect("encode white png");
    assert_eq!(
        extract_palette_bytes(&white_png),
        None,
        "white only art has no palette"
    );
}

#[test]
fn theme_lerp_moves_between_modes() {
    let light = Theme::pulse();
    let dark = Theme::ink();
    assert_eq!(light.lerp(&dark, 0.0), light);
    assert_eq!(light.lerp(&dark, 1.0), dark);
    let middle = light.lerp(&dark, 0.5);
    assert!(luminance(middle.bg) < luminance(light.bg));
    assert!(luminance(middle.bg) > luminance(dark.bg));
    assert_eq!(middle.macaron.len(), 5);
}
