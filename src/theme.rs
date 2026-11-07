//! Visual identity.
//!
//! Three hand-picked themes based on the user's colour references:
//!
//! * **Pulse** — porcelain white canvas, vivid blue primary, fluorescent lime
//!   highlight (default).
//! * **Tide**  — light aqua canvas, teal primary, deep navy text.
//! * **Ink**   — low saturation dark slate monochrome.
//!
//! Album artwork can additionally tint the accent / highlight / macaron colours
//! while playing (`Theme::apply_cover_palette`), but it never repaints the whole
//! surface — that keeps the three base themes crisp and readable.

use std::path::Path;

use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Visuals};
use serde::{Deserialize, Serialize};

use crate::model::Platform;

// ---------------------------------------------------------------------------
// Theme modes
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ThemeMode {
    /// Porcelain white + vivid blue + fluorescent lime.
    #[serde(alias = "blue_white", alias = "album", alias = "porcelain")]
    #[default]
    Pulse,
    /// Light aqua + teal + navy.
    Tide,
    /// Low saturation dark slate.
    #[serde(alias = "midnight")]
    Ink,
}

impl ThemeMode {
    /// Display order in the settings page: default first, then light, then dark.
    pub const ALL: [Self; 3] = [Self::Pulse, Self::Tide, Self::Ink];

    pub fn key(self) -> &'static str {
        match self {
            Self::Pulse => "pulse",
            Self::Tide => "tide",
            Self::Ink => "ink",
        }
    }

    /// Accepts the current keys and the pre-redesign aliases so old profiles
    /// and older demo scripts keep working.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "pulse" | "blue_white" | "album" | "porcelain" => Some(Self::Pulse),
            "tide" => Some(Self::Tide),
            "ink" | "midnight" => Some(Self::Ink),
            _ => None,
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Pulse => Self::Tide,
            Self::Tide => Self::Ink,
            Self::Ink => Self::Pulse,
        }
    }

    pub fn is_dark(self) -> bool {
        matches!(self, Self::Ink)
    }
}

// ---------------------------------------------------------------------------
// Palette extraction
// ---------------------------------------------------------------------------

/// A palette extracted from cover art, already ordered by visual importance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    /// Most vivid representative colour.
    pub vivid: Color32,
    /// A second, distinct hue for gradients / rings.
    pub secondary: Color32,
    /// Pastel version of `vivid`, used for fills.
    pub soft: Color32,
    /// Darkest representative colour (good for text on light backgrounds).
    pub deep: Color32,
    /// Five pastel accents derived from the artwork.
    pub macarons: [Color32; 5],
    /// Mean HSL lightness of the sampled image. Used to choose an automatically
    /// light or dark full theme when a custom image is active.
    pub mean_luminance: f32,
}

impl Palette {
    pub fn fallback() -> Self {
        Self {
            vivid: Color32::from_rgb(0x2D, 0xAC, 0xB5),
            secondary: Color32::from_rgb(0x37, 0x4D, 0x73),
            soft: Color32::from_rgb(0xDA, 0xF2, 0xF2),
            deep: Color32::from_rgb(0x1E, 0x2D, 0x58),
            macarons: MACARONS,
            mean_luminance: 0.5,
        }
    }
}

/// The signature macaron pastels used for generated covers / decorations.
pub const MACARONS: [Color32; 5] = [
    Color32::from_rgb(0xD9, 0xFF, 0x9C), // lime
    Color32::from_rgb(0xC3, 0xE9, 0xEA), // aqua
    Color32::from_rgb(0xA9, 0xD4, 0xFF), // sky
    Color32::from_rgb(0xCD, 0xBD, 0xFF), // lavender
    Color32::from_rgb(0xFF, 0xD9, 0xB0), // peach
];

/// Sidebar wallpaper crop aspect (width / height). Close to the real sidebar
/// proportions; the final sidebar still cover-crops but this keeps the crop
/// preview honest for the user.
pub const THEME_IMAGE_CROP_ASPECT: f32 = 0.30;

/// Map a zoomed/panned sidebar crop to source pixel coordinates.
/// `center_x` / `center_y` are normalized 0..1.
pub fn theme_image_crop_rect(
    image_width: u32,
    image_height: u32,
    zoom: f32,
    center_x: f32,
    center_y: f32,
) -> (u32, u32, u32, u32) {
    let iw = image_width.max(1) as f32;
    let ih = image_height.max(1) as f32;
    let zoom = zoom.clamp(1.0, 3.0);
    let (base_w, base_h) = if iw / ih > THEME_IMAGE_CROP_ASPECT {
        (ih * THEME_IMAGE_CROP_ASPECT, ih)
    } else {
        (iw, iw / THEME_IMAGE_CROP_ASPECT)
    };
    let crop_w = (base_w / zoom).clamp(1.0, iw);
    let crop_h = (base_h / zoom).clamp(1.0, ih);
    let cx = (center_x.clamp(0.0, 1.0) * iw).clamp(crop_w * 0.5, iw - crop_w * 0.5);
    let cy = (center_y.clamp(0.0, 1.0) * ih).clamp(crop_h * 0.5, ih - crop_h * 0.5);
    let x = (cx - crop_w * 0.5).round().clamp(0.0, iw - crop_w) as u32;
    let y = (cy - crop_h * 0.5).round().clamp(0.0, ih - crop_h) as u32;
    (
        x,
        y,
        crop_w.round().max(1.0) as u32,
        crop_h.round().max(1.0) as u32,
    )
}

/// Crop a sidebar strip from encoded image bytes and return PNG bytes. Used by
/// the settings crop dialog; keeping the math next to the rect helper makes it
/// independently testable.
pub fn crop_theme_image_png(
    bytes: &[u8],
    zoom: f32,
    center_x: f32,
    center_y: f32,
) -> Result<Vec<u8>, String> {
    let image = image::load_from_memory(bytes).map_err(|err| err.to_string())?;
    let (x, y, w, h) =
        theme_image_crop_rect(image.width(), image.height(), zoom, center_x, center_y);
    let cropped = image.crop_imm(x, y, w, h);
    let mut output = std::io::Cursor::new(Vec::new());
    cropped
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(|err| err.to_string())?;
    Ok(output.into_inner())
}

/// Decode an arbitrary image, bound it to `max_edge` and re-encode it as PNG.
/// Large camera photos are downscaled before the crop UI decodes them again.
pub fn prepare_theme_image_png(bytes: &[u8], max_edge: u32) -> Result<Vec<u8>, String> {
    let mut image = image::load_from_memory(bytes).map_err(|err| err.to_string())?;
    let longest = image.width().max(image.height());
    if longest > max_edge.max(1) {
        let scale = max_edge as f32 / longest as f32;
        image = image.resize(
            (image.width() as f32 * scale).round().max(1.0) as u32,
            (image.height() as f32 * scale).round().max(1.0) as u32,
            image::imageops::FilterType::Lanczos3,
        );
    }
    let mut output = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(|err| err.to_string())?;
    Ok(output.into_inner())
}

/// Full-window backgrounds need enough pixels to cover a desktop without
/// turning into a blurry smear. Both dimensions must be at least 480 px and the
/// long edge at least 960 px; tiny placeholder images are rejected.
pub const FULL_THEME_MIN_SHORT_EDGE: u32 = 480;
pub const FULL_THEME_MIN_LONG_EDGE: u32 = 960;

pub fn theme_image_meets_full_min(path: &std::path::Path) -> bool {
    // Theme originals are stored as extension-less `.img` files, so use the
    // bytes' magic rather than the path extension to pick a decoder.
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    let Ok(image) = image::load_from_memory(&bytes) else {
        return false;
    };
    image.width().min(image.height()) >= FULL_THEME_MIN_SHORT_EDGE
        && image.width().max(image.height()) >= FULL_THEME_MIN_LONG_EDGE
}

/// Extract a palette from a local image file.
pub fn extract_palette_file(path: &Path) -> Option<Palette> {
    let bytes = std::fs::read(path).ok()?;
    extract_palette_bytes(&bytes)
}

pub fn extract_palette_bytes(bytes: &[u8]) -> Option<Palette> {
    let image = image::load_from_memory(bytes).ok()?;
    extract_palette_image(&image)
}

pub fn extract_palette_image(image: &image::DynamicImage) -> Option<Palette> {
    // Downscale aggressively: we only need the character of the image.
    let small = image
        .resize_exact(96, 96, image::imageops::FilterType::Triangle)
        .to_rgb8();
    let mut buckets: std::collections::HashMap<u16, (u32, u64, u64, u64)> =
        std::collections::HashMap::with_capacity(1024);
    let mut lightness_sum = 0.0f64;
    let mut lightness_count = 0u64;
    for pixel in small.pixels() {
        let [r, g, b] = pixel.0;
        lightness_sum += f64::from(to_hsl(Color32::from_rgb(r, g, b)).2);
        lightness_count += 1;
        let key = (u16::from(r >> 4) << 8) | (u16::from(g >> 4) << 4) | u16::from(b >> 4);
        let entry = buckets.entry(key).or_insert((0, 0, 0, 0));
        entry.0 += 1;
        entry.1 += u64::from(r);
        entry.2 += u64::from(g);
        entry.3 += u64::from(b);
    }
    let mut candidates: Vec<(f32, Color32)> = buckets
        .values()
        .filter_map(|(count, r, g, b)| {
            if *count < 24 {
                return None;
            }
            let color = Color32::from_rgb(
                (r / u64::from(*count)) as u8,
                (g / u64::from(*count)) as u8,
                (b / u64::from(*count)) as u8,
            );
            let (_, chroma, lightness) = to_hsl(color);
            if lightness < 0.10 || lightness > 0.93 {
                return None;
            }
            let score =
                (*count as f32).sqrt() * (0.25 + chroma * 1.35) * (1.0 - (lightness - 0.5).abs());
            (chroma > 0.06 || *count > 160).then_some((score, color))
        })
        .collect();
    if candidates.is_empty() {
        return None;
    }
    candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut picked: Vec<(Color32, f32)> = Vec::new();
    for (_, color) in &candidates {
        let (hue, _, _) = to_hsl(*color);
        let too_close = picked
            .iter()
            .any(|(other, _)| hue_distance(hue, to_hsl(*other).0) < 28.0);
        if !too_close {
            picked.push((*color, hue));
        }
        if picked.len() >= 4 {
            break;
        }
    }
    if picked.is_empty() {
        picked.push((candidates[0].1, to_hsl(candidates[0].1).0));
    }
    let vivid = vivid_version(picked[0].0);
    let secondary = picked
        .get(1)
        .map(|(c, _)| vivid_version(*c))
        .unwrap_or_else(|| shade(vivid, 0.18));
    let deep = picked
        .iter()
        .map(|(c, _)| *c)
        .min_by_key(|c| {
            let (_, _, l) = to_hsl(*c);
            (l * 1000.0) as u32
        })
        .map(|c| shade(c, -0.35))
        .unwrap_or_else(|| shade(vivid, -0.55));
    let mut macarons = MACARONS;
    for (index, macaron) in macarons.iter_mut().enumerate() {
        let source = picked
            .get(index % picked.len())
            .map(|(c, _)| *c)
            .unwrap_or(vivid);
        let (h, s, _l) = to_hsl(source);
        *macaron = from_hsl(h, (s * 0.55 + 0.18).min(0.62), 0.82);
    }
    let soft = from_hsl(to_hsl(vivid).0, 0.55, 0.90);
    Some(Palette {
        vivid,
        secondary,
        soft,
        deep,
        macarons,
        mean_luminance: (lightness_sum / lightness_count.max(1) as f64) as f32,
    })
}

// ---------------------------------------------------------------------------
// Theme
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub dark: bool,
    pub bg: Color32,
    pub panel: Color32,
    pub surface: Color32,
    pub surface_hover: Color32,
    pub border: Color32,
    pub text: Color32,
    pub text_weak: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub on_accent: Color32,
    /// Strong secondary accent (lime for Pulse, teal for Tide).
    pub highlight: Color32,
    pub on_highlight: Color32,
    pub macaron: [Color32; 5],
    /// Two colours used for the soft glow behind the now playing cover.
    pub glow: [Color32; 2],
}

impl Default for Theme {
    fn default() -> Self {
        Self::pulse()
    }
}

impl Theme {
    // -- The three base themes ------------------------------------------------

    /// Porcelain white + vivid blue + fluorescent lime.
    pub fn pulse() -> Self {
        Self {
            dark: false,
            bg: Color32::from_rgb(0xED, 0xF0, 0xF5),
            panel: Color32::WHITE,
            surface: Color32::WHITE,
            surface_hover: Color32::from_rgb(0xE5, 0xEC, 0xF6),
            border: Color32::from_rgb(0xD3, 0xDC, 0xE9),
            text: Color32::from_rgb(0x0C, 0x1F, 0x3D),
            text_weak: Color32::from_rgb(0x5A, 0x6A, 0x84),
            accent: Color32::from_rgb(0x04, 0x4B, 0xB5),
            accent_soft: Color32::from_rgb(0xDC, 0xE7, 0xFB),
            on_accent: Color32::WHITE,
            highlight: Color32::from_rgb(0xBB, 0xFF, 0x5C),
            on_highlight: Color32::from_rgb(0x0C, 0x1F, 0x3D),
            macaron: [
                Color32::from_rgb(0xD9, 0xFF, 0x9C),
                Color32::from_rgb(0xA9, 0xD4, 0xFF),
                Color32::from_rgb(0xC3, 0xE9, 0xEA),
                Color32::from_rgb(0xCD, 0xBD, 0xFF),
                Color32::from_rgb(0xFF, 0xD9, 0xB0),
            ],
            glow: [
                Color32::from_rgb(0xDC, 0xE7, 0xFB),
                Color32::from_rgb(0xEA, 0xF6, 0xCF),
            ],
        }
    }

    /// Light aqua + teal + deep navy.
    pub fn tide() -> Self {
        Self {
            dark: false,
            bg: Color32::from_rgb(0xC3, 0xE9, 0xEA),
            panel: Color32::from_rgb(0xF3, 0xFB, 0xFB),
            surface: Color32::WHITE,
            surface_hover: Color32::from_rgb(0xE0, 0xF4, 0xF4),
            border: Color32::from_rgb(0xA8, 0xD8, 0xDB),
            text: Color32::from_rgb(0x1E, 0x2D, 0x58),
            text_weak: Color32::from_rgb(0x3F, 0x58, 0x77),
            accent: Color32::from_rgb(0x17, 0x74, 0x7D),
            accent_soft: Color32::from_rgb(0xDA, 0xF2, 0xF2),
            on_accent: Color32::WHITE,
            highlight: Color32::from_rgb(0x2D, 0xAC, 0xB5),
            on_highlight: Color32::from_rgb(0x08, 0x2C, 0x31),
            macaron: [
                Color32::from_rgb(0xA8, 0xE4, 0xE6),
                Color32::from_rgb(0x9B, 0xDD, 0xE0),
                Color32::from_rgb(0xC3, 0xE9, 0xEA),
                Color32::from_rgb(0x9F, 0xB4, 0xD2),
                Color32::from_rgb(0xF3, 0xE8, 0xC9),
            ],
            glow: [
                Color32::from_rgb(0xD7, 0xF2, 0xF2),
                Color32::from_rgb(0xF5, 0xEB, 0xD5),
            ],
        }
    }

    /// Low saturation dark slate monochrome.
    pub fn ink() -> Self {
        Self {
            dark: true,
            bg: Color32::from_rgb(0x1D, 0x24, 0x2E),
            panel: Color32::from_rgb(0x23, 0x2C, 0x39),
            surface: Color32::from_rgb(0x34, 0x3F, 0x51),
            surface_hover: Color32::from_rgb(0x3E, 0x4A, 0x5F),
            border: Color32::from_rgb(0x46, 0x53, 0x6A),
            text: Color32::from_rgb(0xE5, 0xEE, 0xFF),
            text_weak: Color32::from_rgb(0xA3, 0xB2, 0xC8),
            accent: Color32::from_rgb(0xAF, 0xC7, 0xF2),
            accent_soft: Color32::from_rgb(0x2E, 0x3B, 0x50),
            on_accent: Color32::from_rgb(0x10, 0x16, 0x1F),
            highlight: Color32::from_rgb(0xD7, 0xE4, 0xFF),
            on_highlight: Color32::from_rgb(0x10, 0x16, 0x1F),
            macaron: [
                Color32::from_rgb(0x8F, 0xA9, 0xC6),
                Color32::from_rgb(0xAF, 0xC7, 0xF2),
                Color32::from_rgb(0xC9, 0xD6, 0xE8),
                Color32::from_rgb(0xE5, 0xEE, 0xFF),
                Color32::from_rgb(0x5C, 0x71, 0x8C),
            ],
            glow: [
                Color32::from_rgb(0x2B, 0x36, 0x48),
                Color32::from_rgb(0x1F, 0x2A, 0x38),
            ],
        }
    }

    pub fn base(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::Pulse => Self::pulse(),
            ThemeMode::Tide => Self::tide(),
            ThemeMode::Ink => Self::ink(),
        }
    }

    pub fn for_mode(mode: ThemeMode, palette: Option<&Palette>) -> Self {
        let theme = Self::base(mode);
        match palette {
            Some(palette) => theme.apply_cover_palette(palette),
            None => theme,
        }
    }

    /// Tint the *accents* (never the background/text) with artwork colours.
    pub fn apply_cover_palette(mut self, palette: &Palette) -> Self {
        if self.dark {
            self.accent = brighten_for_dark(palette.vivid);
            self.accent_soft = mix(self.accent_soft, palette.vivid, 0.34);
            self.highlight = brighten_for_dark(palette.secondary);
            let mut macarons = self.macaron;
            for (index, macaron) in macarons.iter_mut().enumerate() {
                let (h, s, l) = to_hsl(palette.macarons[index]);
                *macaron = from_hsl(h, (s * 0.75).min(0.55), (l * 0.55).max(0.32));
            }
            self.macaron = macarons;
            self.glow = [
                mix(palette.vivid, self.bg, 0.62),
                mix(palette.secondary, self.panel, 0.55),
            ];
        } else {
            self.accent = readable_on(palette.vivid, self.bg, 4.0);
            self.accent_soft = mix(self.accent_soft, palette.soft, 0.6);
            self.highlight = palette.secondary;
            self.macaron = palette.macarons;
            self.glow = [
                mix(palette.vivid, Color32::WHITE, 0.72),
                mix(palette.secondary, Color32::WHITE, 0.80),
            ];
        }
        self.on_accent = best_on(self.accent);
        self.on_highlight = best_on(self.highlight);
        self
    }

    /// Make the large surfaces translucent so a full-window photo can show
    /// through while cards and panels remain readable over the image veil.
    pub fn translucent(self) -> Self {
        Self {
            bg: with_alpha(self.bg, 88),
            panel: with_alpha(self.panel, 148),
            surface: with_alpha(self.surface, 176),
            surface_hover: with_alpha(self.surface_hover, 200),
            border: with_alpha(self.border, 200),
            ..self
        }
    }

    /// Build a complete theme from a user image palette. The image decides the
    /// light/dark base; every foreground/background pair is contrast-checked so
    /// text and icons stay readable on the generated surfaces.
    pub fn from_image_palette(palette: &Palette) -> Self {
        let dark = palette.mean_luminance < 0.47;
        if dark {
            let bg = mix(Color32::from_rgb(0x0A, 0x0F, 0x18), palette.deep, 0.40);
            let panel = mix(bg, Color32::WHITE, 0.055);
            let surface = mix(panel, palette.vivid, 0.17);
            let surface_hover = mix(surface, Color32::WHITE, 0.10);
            let border = mix(surface, Color32::WHITE, 0.22);
            let text = readable_on(Color32::from_rgb(0xF4, 0xF7, 0xFF), bg, 7.0);
            let text_weak = readable_on(mix(text, bg, 0.42), bg, 4.5);
            let accent = readable_on(brighten_for_dark(palette.vivid), bg, 3.2);
            let highlight = brighten_for_dark(palette.secondary);
            let mut macarons = MACARONS;
            for (index, macaron) in macarons.iter_mut().enumerate() {
                let (h, s, l) = to_hsl(palette.macarons[index]);
                *macaron = from_hsl(h, (s * 0.72).min(0.52), (l * 0.52).max(0.30));
            }
            Self {
                dark: true,
                bg,
                panel,
                surface,
                surface_hover,
                border,
                text,
                text_weak,
                accent,
                accent_soft: mix(panel, accent, 0.22),
                on_accent: best_on(accent),
                highlight,
                on_highlight: best_on(highlight),
                macaron: macarons,
                glow: [
                    mix(palette.vivid, bg, 0.58),
                    mix(palette.secondary, panel, 0.52),
                ],
            }
        } else {
            let paper = Color32::from_rgb(0xFF, 0xFF, 0xFF);
            let bg = mix(paper, palette.soft, 0.62);
            let panel = mix(paper, palette.soft, 0.44);
            let surface = mix(panel, palette.vivid, 0.09);
            let surface_hover = mix(surface, palette.vivid, 0.12);
            let border = mix(surface, palette.vivid, 0.30);
            let text = readable_on(palette.deep, bg, 7.0);
            let text_weak = readable_on(mix(text, bg, 0.34), bg, 4.5);
            let accent = readable_on(palette.vivid, bg, 4.5);
            let highlight = readable_on(palette.secondary, bg, 3.2);
            Self {
                dark: false,
                bg,
                panel,
                surface,
                surface_hover,
                border,
                text,
                text_weak,
                accent,
                accent_soft: mix(surface, palette.soft, 0.72),
                on_accent: best_on(accent),
                highlight,
                on_highlight: best_on(highlight),
                macaron: palette.macarons,
                glow: [
                    mix(palette.vivid, Color32::WHITE, 0.74),
                    mix(palette.secondary, Color32::WHITE, 0.82),
                ],
            }
        }
    }

    // -- Script / user overrides ---------------------------------------------

    pub fn with_accent(mut self, accent: Color32) -> Self {
        self.accent = accent;
        self.accent_soft = mix(self.surface, accent, if self.dark { 0.22 } else { 0.14 });
        self.on_accent = best_on(accent);
        self
    }

    pub fn with_highlight(mut self, highlight: Color32) -> Self {
        self.highlight = highlight;
        self.on_highlight = best_on(highlight);
        self
    }

    pub fn with_macaron(mut self, macaron: [Color32; 5]) -> Self {
        self.macaron = macaron;
        self
    }

    pub fn accent_card(&self) -> Color32 {
        mix(
            self.surface,
            self.accent,
            if self.dark { 0.22 } else { 0.10 },
        )
    }

    pub fn lerp(&self, other: &Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        if t >= 0.999 {
            return *other;
        }
        if t <= 0.001 {
            return *self;
        }
        Self {
            dark: if t > 0.5 { other.dark } else { self.dark },
            bg: mix(self.bg, other.bg, t),
            panel: mix(self.panel, other.panel, t),
            surface: mix(self.surface, other.surface, t),
            surface_hover: mix(self.surface_hover, other.surface_hover, t),
            border: mix(self.border, other.border, t),
            text: mix(self.text, other.text, t),
            text_weak: mix(self.text_weak, other.text_weak, t),
            accent: mix(self.accent, other.accent, t),
            accent_soft: mix(self.accent_soft, other.accent_soft, t),
            on_accent: mix(self.on_accent, other.on_accent, t),
            highlight: mix(self.highlight, other.highlight, t),
            on_highlight: mix(self.on_highlight, other.on_highlight, t),
            macaron: [
                mix(self.macaron[0], other.macaron[0], t),
                mix(self.macaron[1], other.macaron[1], t),
                mix(self.macaron[2], other.macaron[2], t),
                mix(self.macaron[3], other.macaron[3], t),
                mix(self.macaron[4], other.macaron[4], t),
            ],
            glow: [
                mix(self.glow[0], other.glow[0], t),
                mix(self.glow[1], other.glow[1], t),
            ],
        }
    }

    // -----------------------------------------------------------------------
    // egui integration
    // -----------------------------------------------------------------------

    pub fn to_visuals(&self) -> Visuals {
        let mut visuals = if self.dark {
            Visuals::dark()
        } else {
            Visuals::light()
        };
        visuals.dark_mode = self.dark;
        visuals.override_text_color = Some(self.text);
        visuals.weak_text_color = Some(self.text_weak);
        visuals.panel_fill = self.panel;
        visuals.window_fill = self.surface;
        visuals.extreme_bg_color = if self.dark {
            mix(self.bg, Color32::BLACK, 0.35)
        } else {
            mix(self.bg, Color32::WHITE, 0.45)
        };
        visuals.text_edit_bg_color = Some(if self.dark {
            mix(self.surface, Color32::BLACK, 0.18)
        } else {
            mix(self.surface, self.bg, 0.55)
        });
        visuals.faint_bg_color = if self.dark {
            mix(self.bg, self.surface, 0.35)
        } else {
            mix(self.bg, Color32::WHITE, 0.55)
        };
        visuals.selection = egui::style::Selection {
            bg_fill: with_alpha(self.accent, 70),
            stroke: Stroke::new(1.0_f32, self.accent),
        };
        visuals.hyperlink_color = self.accent;
        visuals.window_corner_radius = CornerRadius::same(14);
        visuals.menu_corner_radius = CornerRadius::same(12);
        visuals.window_stroke = Stroke::new(1.0_f32, self.border);
        visuals.window_shadow = egui::epaint::Shadow {
            offset: [0, 8],
            blur: 28,
            spread: 0,
            color: Color32::from_black_alpha(if self.dark { 90 } else { 22 }),
        };
        visuals.popup_shadow = visuals.window_shadow;

        let radius = CornerRadius::same(10);
        visuals.widgets.noninteractive = egui::style::WidgetVisuals {
            bg_fill: self.surface,
            weak_bg_fill: self.surface,
            bg_stroke: Stroke::new(1.0_f32, self.border),
            corner_radius: radius,
            fg_stroke: Stroke::new(1.0_f32, self.text),
            expansion: 0.0,
        };
        visuals.widgets.inactive = egui::style::WidgetVisuals {
            bg_fill: if self.dark {
                mix(self.surface, Color32::WHITE, 0.04)
            } else {
                mix(self.bg, Color32::WHITE, 0.35)
            },
            weak_bg_fill: self.surface,
            bg_stroke: Stroke::new(1.0_f32, self.border),
            corner_radius: radius,
            fg_stroke: Stroke::new(1.0_f32, self.text),
            expansion: 0.0,
        };
        visuals.widgets.hovered = egui::style::WidgetVisuals {
            bg_fill: mix(
                self.surface,
                self.accent,
                if self.dark { 0.16 } else { 0.09 },
            ),
            weak_bg_fill: mix(self.surface, self.accent, 0.06),
            bg_stroke: Stroke::new(1.0_f32, with_alpha(self.accent, 150)),
            corner_radius: radius,
            fg_stroke: Stroke::new(1.2_f32, self.text),
            expansion: 0.5,
        };
        visuals.widgets.active = egui::style::WidgetVisuals {
            bg_fill: mix(
                self.surface,
                self.accent,
                if self.dark { 0.24 } else { 0.16 },
            ),
            weak_bg_fill: self.accent_soft,
            bg_stroke: Stroke::new(1.0_f32, self.accent),
            corner_radius: radius,
            fg_stroke: Stroke::new(1.4_f32, self.text),
            expansion: 0.0,
        };
        visuals.widgets.open = visuals.widgets.hovered;
        visuals
    }

    /// Text styles / spacing that stay constant across themes.
    pub fn apply_text_styles(ctx: &egui::Context) {
        ctx.style_mut(|style| {
            use FontFamily::Proportional;
            style.text_styles = [
                (TextStyle::Small, FontId::new(11.5, Proportional)),
                (TextStyle::Body, FontId::new(14.0, Proportional)),
                (TextStyle::Button, FontId::new(14.0, Proportional)),
                (TextStyle::Heading, FontId::new(21.0, Proportional)),
                (
                    TextStyle::Monospace,
                    FontId::new(13.0, FontFamily::Monospace),
                ),
            ]
            .into();
            style.spacing.item_spacing = egui::vec2(9.0, 8.0);
            style.spacing.button_padding = egui::vec2(11.0, 5.0);
            style.spacing.interact_size.y = 26.0;
            style.spacing.slider_width = 130.0;
            style.visuals.button_frame = false;
            style.visuals.striped = false;
            style.animation_time = 0.18;
        });
    }
}

/// Muted accent used for a platform badge.
pub fn platform_color(platform: Platform) -> Color32 {
    match platform {
        Platform::Local => Color32::from_rgb(0x6E, 0x8D, 0xB0),
        Platform::Spotify => Color32::from_rgb(0x1D, 0xB9, 0x54),
        Platform::AppleMusic => Color32::from_rgb(0xE8, 0x4A, 0x5F),
        Platform::Netease => Color32::from_rgb(0xC2, 0x2A, 0x2A),
        Platform::SoundCloud => Color32::from_rgb(0xF2, 0x6B, 0x2E),
        Platform::Bilibili => Color32::from_rgb(0xEE, 0x6D, 0x9C),
        Platform::YouTube => Color32::from_rgb(0xE0, 0x3B, 0x3B),
        Platform::InternetArchive => Color32::from_rgb(0x8B, 0x6B, 0x4A),
    }
}

// ---------------------------------------------------------------------------
// Colour maths (small, dependency-free)
// ---------------------------------------------------------------------------

/// Hue in degrees, saturation and lightness in `0..=1`.
pub fn to_hsl(color: Color32) -> (f32, f32, f32) {
    let r = f32::from(color.r()) / 255.0;
    let g = f32::from(color.g()) / 255.0;
    let b = f32::from(color.b()) / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let lightness = (max + min) * 0.5;
    if (max - min).abs() < f32::EPSILON {
        return (0.0, 0.0, lightness);
    }
    let delta = max - min;
    let saturation = if lightness > 0.5 {
        delta / (2.0 - max - min)
    } else {
        delta / (max + min)
    };
    let hue = if max == r {
        ((g - b) / delta + if g < b { 6.0 } else { 0.0 }) * 60.0
    } else if max == g {
        ((b - r) / delta + 2.0) * 60.0
    } else {
        ((r - g) / delta + 4.0) * 60.0
    };
    (hue, saturation.clamp(0.0, 1.0), lightness.clamp(0.0, 1.0))
}

pub fn from_hsl(hue: f32, saturation: f32, lightness: f32) -> Color32 {
    let h = ((hue % 360.0) + 360.0) % 360.0 / 360.0;
    let s = saturation.clamp(0.0, 1.0);
    let l = lightness.clamp(0.0, 1.0);
    if s <= f32::EPSILON {
        let v = (l * 255.0).round() as u8;
        return Color32::from_rgb(v, v, v);
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let channel = |mut t: f32| -> f32 {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    let r = (channel(h + 1.0 / 3.0) * 255.0).round().clamp(0.0, 255.0) as u8;
    let g = (channel(h) * 255.0).round().clamp(0.0, 255.0) as u8;
    let b = (channel(h - 1.0 / 3.0) * 255.0).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(r, g, b)
}

pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let lerp = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgb(lerp(a.r(), b.r()), lerp(a.g(), b.g()), lerp(a.b(), b.b()))
}

pub fn with_alpha(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

pub fn luminance(color: Color32) -> f32 {
    fn channel(v: u8) -> f32 {
        let v = f32::from(v) / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
}

pub fn contrast(a: Color32, b: Color32) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Mix with white/black until the colour is readable on `background`.
pub fn readable_on(color: Color32, background: Color32, min_contrast: f32) -> Color32 {
    if contrast(color, background) >= min_contrast {
        return color;
    }
    let toward = if luminance(background) > 0.4 {
        Color32::BLACK
    } else {
        Color32::WHITE
    };
    for step in 1..=10 {
        let candidate = mix(color, toward, step as f32 * 0.09);
        if contrast(candidate, background) >= min_contrast {
            return candidate;
        }
    }
    toward
}

/// Pick white or near-black text for a filled colour swatch.
pub fn best_on(color: Color32) -> Color32 {
    if contrast(Color32::WHITE, color) >= contrast(Color32::BLACK, color) {
        Color32::WHITE
    } else {
        Color32::BLACK
    }
}

pub fn shade(color: Color32, amount: f32) -> Color32 {
    let target = if amount >= 0.0 {
        Color32::WHITE
    } else {
        Color32::BLACK
    };
    mix(color, target, amount.abs().clamp(0.0, 1.0))
}

fn vivid_version(color: Color32) -> Color32 {
    let (h, s, l) = to_hsl(color);
    let s = (s * 1.25 + 0.18).clamp(0.28, 0.78);
    let l = l.clamp(0.32, 0.62);
    from_hsl(h, s, l)
}

fn brighten_for_dark(color: Color32) -> Color32 {
    let (h, s, l) = to_hsl(color);
    from_hsl(h, (s * 0.85 + 0.05).clamp(0.25, 0.75), l.clamp(0.62, 0.78))
}

fn hue_distance(a: f32, b: f32) -> f32 {
    let diff = (a - b).abs() % 360.0;
    diff.min(360.0 - diff)
}

/// Determine a cover glow gradient for the now playing page.
pub fn cover_glow(theme: &Theme) -> [Color32; 2] {
    [
        with_alpha(theme.glow[0], 200),
        with_alpha(theme.glow[1], 200),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsl_roundtrip() {
        for color in [
            Color32::from_rgb(0x04, 0x4B, 0xB5),
            Color32::from_rgb(0x2D, 0xAC, 0xB5),
            Color32::from_rgb(0x10, 0x20, 0x30),
        ] {
            let (h, s, l) = to_hsl(color);
            let back = from_hsl(h, s, l);
            assert!(
                (i32::from(back.r()) - i32::from(color.r())).abs() <= 2,
                "{color:?} -> {back:?}"
            );
            assert!((i32::from(back.g()) - i32::from(color.g())).abs() <= 2);
            assert!((i32::from(back.b()) - i32::from(color.b())).abs() <= 2);
        }
    }

    #[test]
    fn themes_are_readable() {
        let palette = Palette::fallback();
        for mode in ThemeMode::ALL {
            for palette in [None, Some(&palette)] {
                let theme = Theme::for_mode(mode, palette);
                assert!(
                    contrast(theme.text, theme.bg) >= 4.0,
                    "text contrast in {mode:?}"
                );
                assert!(
                    contrast(theme.text, theme.surface) >= 4.0,
                    "text on surface in {mode:?}"
                );
                assert!(
                    contrast(theme.text_weak, theme.bg) >= 2.4,
                    "weak contrast in {mode:?}"
                );
                assert!(
                    contrast(theme.accent, theme.bg) >= 3.0,
                    "accent contrast in {mode:?}"
                );
                assert!(
                    contrast(theme.on_accent, theme.accent) >= 3.0,
                    "on_accent in {mode:?}"
                );
            }
        }
    }

    #[test]
    fn image_theme_auto_contrast() {
        for mean_luminance in [0.15f32, 0.85] {
            let mut palette = Palette::fallback();
            palette.mean_luminance = mean_luminance;
            let theme = Theme::from_image_palette(&palette);
            assert_eq!(theme.dark, mean_luminance < 0.47);
            assert!(contrast(theme.text, theme.bg) >= 4.0);
            assert!(contrast(theme.text, theme.surface) >= 4.0);
            assert!(contrast(theme.accent, theme.bg) >= 3.0);
            assert!(contrast(theme.on_accent, theme.accent) >= 3.0);
        }
    }

    #[test]
    fn image_crop_stays_inside_source() {
        let (x, y, w, h) = theme_image_crop_rect(2000, 1000, 1.0, 0.5, 0.5);
        assert_eq!((w, h), (300, 1000));
        assert_eq!((x, y), (850, 0));
        let (x, y, w, h) = theme_image_crop_rect(2000, 1000, 2.0, 0.0, 0.0);
        assert!(x + w <= 2000 && y + h <= 1000);
        assert!(w > 0 && h > 0);
    }

    #[test]
    fn crop_theme_image_produces_png() {
        let mut source = image::RgbaImage::new(120, 80);
        for (x, y, pixel) in source.enumerate_pixels_mut() {
            *pixel = image::Rgba([(x % 255) as u8, (y % 255) as u8, 80, 255]);
        }
        let mut encoded = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(source)
            .write_to(&mut encoded, image::ImageFormat::Png)
            .expect("encode source");
        let cropped =
            crop_theme_image_png(encoded.get_ref(), 1.0, 0.5, 0.5).expect("crop should succeed");
        let decoded = image::load_from_memory(&cropped).expect("decode crop");
        assert!(decoded.width() > 0 && decoded.height() > 0);
        assert_eq!(
            decoded.width() as f32 / decoded.height() as f32,
            THEME_IMAGE_CROP_ASPECT
        );
    }

    #[test]
    fn full_theme_minimum_accepts_valid_photo() {
        let dir = std::env::temp_dir();
        let path = dir.join("echoport-full-min-test.img");
        let image = image::RgbImage::from_fn(960, 480, |x, y| {
            image::Rgb([(x % 255) as u8, (y % 255) as u8, 80])
        });
        image
            .write_to(
                &mut std::io::BufWriter::new(std::fs::File::create(&path).expect("file")),
                image::ImageFormat::Png,
            )
            .expect("save test image");
        assert!(theme_image_meets_full_min(&path));
        let small = image::RgbImage::new(300, 200);
        let small_path = dir.join("echoport-small-test.img");
        small
            .write_to(
                &mut std::io::BufWriter::new(std::fs::File::create(&small_path).expect("file")),
                image::ImageFormat::Png,
            )
            .expect("save small");
        assert!(!theme_image_meets_full_min(&small_path));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&small_path);
    }

    #[test]
    fn oversized_theme_image_is_downscaled() {
        let mut source = image::RgbImage::new(4200, 100);
        for (x, _, pixel) in source.enumerate_pixels_mut() {
            *pixel = image::Rgb([(x % 255) as u8, 40, 80]);
        }
        let mut encoded = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(source)
            .write_to(&mut encoded, image::ImageFormat::Png)
            .expect("encode source");
        let prepared =
            prepare_theme_image_png(encoded.get_ref(), 4096).expect("prepare should succeed");
        let decoded = image::load_from_memory(&prepared).expect("decode prepared");
        assert_eq!(decoded.width(), 4096);
        assert_eq!(decoded.height(), 98);
    }

    #[test]
    fn lerp_endpoints() {
        let a = Theme::pulse();
        let b = Theme::ink();
        assert_eq!(a.lerp(&b, 0.0), a);
        assert_eq!(a.lerp(&b, 1.0), b);
    }

    #[test]
    fn mode_aliases_roundtrip() {
        assert_eq!(ThemeMode::from_key("pulse"), Some(ThemeMode::Pulse));
        assert_eq!(ThemeMode::from_key("blue_white"), Some(ThemeMode::Pulse));
        assert_eq!(ThemeMode::from_key("midnight"), Some(ThemeMode::Ink));
        assert_eq!(ThemeMode::from_key("album"), Some(ThemeMode::Pulse));
        assert_eq!(ThemeMode::default(), ThemeMode::Pulse);
        assert_eq!(ThemeMode::ALL.len(), 3);
    }
}
