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
pub enum ThemeMode {
    /// Porcelain white + vivid blue + fluorescent lime.
    #[serde(alias = "blue_white", alias = "album", alias = "porcelain")]
    Pulse,
    /// Light aqua + teal + navy.
    Tide,
    /// Low saturation dark slate.
    #[serde(alias = "midnight")]
    Ink,
}

impl Default for ThemeMode {
    fn default() -> Self {
        Self::Pulse
    }
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
}

impl Palette {
    pub fn fallback() -> Self {
        Self {
            vivid: Color32::from_rgb(0x2D, 0xAC, 0xB5),
            secondary: Color32::from_rgb(0x37, 0x4D, 0x73),
            soft: Color32::from_rgb(0xDA, 0xF2, 0xF2),
            deep: Color32::from_rgb(0x1E, 0x2D, 0x58),
            macarons: MACARONS,
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
    for pixel in small.pixels() {
        let [r, g, b] = pixel.0;
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
    from_hsl(
        h,
        (s * 0.85 + 0.05).clamp(0.25, 0.75),
        l.max(0.62).min(0.78),
    )
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
