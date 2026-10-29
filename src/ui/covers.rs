//! Cover-art texture cache.
//!
//! Decoding happens at most once per path; the resulting texture is cached in
//! `UiState` and downscaled to a display-friendly size (we never want a
//! 2000x2000 JPEG uploaded to the GPU for a 44px thumbnail).
//!
//! When a track has no cover we paint a deterministic macaron gradient derived
//! from the track title, so every row still looks intentional and the same
//! track always gets the same colours.

use std::collections::HashMap;
use std::path::Path;

use egui::epaint::RectShape;
use egui::{Color32, Context, CornerRadius, Rect, TextureHandle, TextureOptions, Ui};

use crate::theme::{self, Theme};
use crate::ui::style;

const MAX_TEXTURE_SIDE: u32 = 256;
const MAX_TEXTURES: usize = 64;
const BLUR_SIDE: u32 = 64;

pub struct CoverCache {
    textures: HashMap<String, TextureHandle>,
    /// Paths that failed to decode, with the time of the attempt; retried a
    /// couple of seconds later in case the file is still being written.
    misses: HashMap<String, f64>,
    clock: u64,
    last_used: HashMap<String, u64>,
    /// Cross-fade state for the now-playing cover.
    fade_key: Option<String>,
    fade_at: f64,
}

impl Default for CoverCache {
    fn default() -> Self {
        Self::new()
    }
}

impl CoverCache {
    pub fn new() -> Self {
        Self {
            textures: HashMap::new(),
            misses: HashMap::new(),
            clock: 0,
            last_used: HashMap::new(),
            fade_key: None,
            fade_at: f64::NEG_INFINITY,
        }
    }

    fn texture(&mut self, ctx: &Context, path: &Path, now: f64) -> Option<TextureHandle> {
        self.clock = self.clock.wrapping_add(1);
        let base_key = path.to_string_lossy().into_owned();
        if let Some(texture) = self.textures.get(&base_key) {
            self.last_used.insert(base_key, self.clock);
            return Some(texture.clone());
        }
        if let Some(attempt) = self.misses.get(&base_key) {
            if now - attempt < 2.0 {
                return None;
            }
        }
        let Some((texture, blurred)) = load_textures(ctx, path) else {
            self.misses.insert(base_key, now);
            return None;
        };
        for (key, handle) in [(base_key.clone(), texture), (blur_key(path), blurred)] {
            if self.textures.len() >= MAX_TEXTURES {
                self.evict_oldest();
            }
            self.textures.insert(key.clone(), handle);
            self.last_used.insert(key, self.clock);
        }
        self.textures.get(&base_key).cloned()
    }

    /// A tiny pre-blurred version of the cover, prepared once per path and
    /// reused as the immersive Now Playing backdrop. No image work per frame.
    pub fn blurred(&mut self, ctx: &Context, path: &Path, now: f64) -> Option<TextureHandle> {
        self.clock = self.clock.wrapping_add(1);
        let key = blur_key(path);
        if let Some(texture) = self.textures.get(&key) {
            self.last_used.insert(key, self.clock);
            return Some(texture.clone());
        }
        let _ = self.texture(ctx, path, now)?;
        let texture = self.textures.get(&key).cloned();
        if texture.is_some() {
            self.last_used.insert(key, self.clock);
        }
        texture
    }

    fn evict_oldest(&mut self) {
        let Some((oldest, _)) = self
            .last_used
            .iter()
            .min_by_key(|(_, tick)| **tick)
            .map(|(key, tick)| (key.clone(), *tick))
        else {
            return;
        };
        self.last_used.remove(&oldest);
        self.textures.remove(&oldest);
    }

    /// Paint a cover, falling back to the generated placeholder.
    pub fn paint(
        &mut self,
        ui: &Ui,
        theme: &Theme,
        path: Option<&Path>,
        rect: Rect,
        radius: u8,
        title: &str,
        seed: &str,
    ) {
        let now = ui.ctx().input(|input| input.time);
        let texture = path.and_then(|path| self.texture(ui.ctx(), path, now));
        match texture {
            Some(texture) => paint_texture(ui, &texture, rect, radius, 255),
            None => paint_placeholder(ui, theme, rect, radius, title, seed),
        }
    }

    /// Paint the now-playing cover with a short cross-fade whenever the track
    /// key changes. The placeholder is painted underneath so there is never a
    /// flash of empty space.
    pub fn paint_fading(
        &mut self,
        ui: &Ui,
        theme: &Theme,
        path: Option<&Path>,
        rect: Rect,
        radius: u8,
        title: &str,
        seed: &str,
        track_key: &str,
    ) {
        let now = ui.ctx().input(|input| input.time);
        if self.fade_key.as_deref() != Some(track_key) {
            self.fade_key = Some(track_key.to_owned());
            self.fade_at = now;
        }
        let fade = (((now - self.fade_at) / 0.20) as f32).clamp(0.0, 1.0);
        if fade < 1.0 {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(16));
        }
        let texture = path.and_then(|path| self.texture(ui.ctx(), path, now));
        if texture.is_none() || fade < 0.45 {
            paint_placeholder(ui, theme, rect, radius, title, seed);
        }
        if let Some(texture) = texture {
            if fade >= 0.999 {
                paint_texture(ui, &texture, rect, radius, 255);
            } else {
                // Ease out for a soft landing.
                let eased = 1.0 - (1.0 - fade) * (1.0 - fade);
                paint_texture(ui, &texture, rect, radius, (eased * 255.0) as u8);
            }
        }
    }

    pub fn len(&self) -> usize {
        self.textures.len()
    }
}

fn paint_texture(ui: &Ui, texture: &TextureHandle, rect: Rect, radius: u8, alpha: u8) {
    let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    let tint = Color32::from_white_alpha(alpha);
    ui.painter().add(
        RectShape::filled(rect, CornerRadius::same(radius), tint).with_texture(texture.id(), uv),
    );
}

/// Deterministic generated cover used whenever no artwork is available.
/// Gradients stay inside the palette's accent family so a wall of placeholder
/// art still looks like one designed set.
pub fn paint_placeholder(ui: &Ui, theme: &Theme, rect: Rect, radius: u8, title: &str, seed: &str) {
    let painter = ui.painter();
    let (top, bottom) = style::generated_cover_colors(theme, seed);
    let mid = theme::mix(top, bottom, 0.5);
    let ink = if theme::luminance(mid) > 0.55 {
        theme::shade(theme.accent, -0.58)
    } else {
        Color32::WHITE
    };
    let glyph = title
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();
    let variant = crate::model::stable_hash(seed)
        .as_bytes()
        .first()
        .copied()
        .unwrap_or(0) as usize
        % 3;
    style::paint_generated_cover(
        painter,
        rect,
        radius,
        top,
        bottom,
        ink,
        theme.highlight,
        &glyph,
        variant,
    );
}

fn blur_key(path: &Path) -> String {
    format!("{}#blur", path.to_string_lossy())
}

fn load_textures(ctx: &Context, path: &Path) -> Option<(TextureHandle, TextureHandle)> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() < 32 {
        return None;
    }
    let image = image::load_from_memory(&bytes).ok()?;
    let image = if image.width() > MAX_TEXTURE_SIDE || image.height() > MAX_TEXTURE_SIDE {
        image.thumbnail(MAX_TEXTURE_SIDE, MAX_TEXTURE_SIDE)
    } else {
        image
    };
    // The immersive backdrop: a 64px blurred copy, generated exactly once.
    let blurred = image
        .resize_exact(BLUR_SIDE, BLUR_SIDE, image::imageops::FilterType::Triangle)
        .blur(10.0)
        .to_rgba8();
    let blurred_image = egui::ColorImage::from_rgba_unmultiplied(
        [BLUR_SIDE as usize, BLUR_SIDE as usize],
        blurred.as_raw(),
    );
    let blur_name = format!("cover-blur:{}", path.to_string_lossy());
    let blurred = ctx.load_texture(blur_name, blurred_image, TextureOptions::LINEAR);

    let rgba = image.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
    let name = format!("cover:{}", path.to_string_lossy());
    let texture = ctx.load_texture(name, color_image, TextureOptions::LINEAR);
    Some((texture, blurred))
}
