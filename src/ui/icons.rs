//! Embedded black/white icon set (white PNG variants, tinted at paint time).
//!
//! Assets live in `assets/icons/` (MIT-0, © Singular Programmer). The white
//! glyphs are embedded in the binary and uploaded lazily as egui textures;
//! the cache lives in `UiState::icon_assets` and is shared with the painting
//! helpers through the egui context, so textures are uploaded at most once per
//! icon and never touched per frame.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use egui::{Color32, ColorImage, Context, Id, Painter, Rect, TextureHandle, TextureOptions};

/// Key under which the shared cache is stored in the egui context memory.
const CACHE_KEY: &str = "echoport.icon-assets";

/// Texture cache for the embedded icon set.
#[derive(Default)]
pub struct IconAssets {
    textures: HashMap<u8, TextureHandle>,
}

impl IconAssets {
    fn texture(&mut self, ctx: &Context, id: u8) -> Option<&TextureHandle> {
        if !self.textures.contains_key(&id) {
            let bytes = asset_bytes(id)?;
            let image = decode_png(bytes)?;
            let texture =
                ctx.load_texture(format!("echoport-icon-{id}"), image, TextureOptions::LINEAR);
            self.textures.insert(id, texture);
        }
        self.textures.get(&id)
    }
}

/// Shared handle stored in both `UiState` and the egui context.
pub type SharedIconAssets = Arc<Mutex<IconAssets>>;

/// Register a fresh cache in the egui context; returns the shared handle that
/// `UiState` keeps for the lifetime of the app.
pub fn install(ctx: &Context) -> SharedIconAssets {
    let assets = SharedIconAssets::default();
    ctx.data_mut(|data| data.insert_temp(Id::new(CACHE_KEY), assets.clone()));
    assets
}

fn texture_for(ctx: &Context, id: u8) -> Option<TextureHandle> {
    let assets = ctx.data(|data| data.get_temp::<SharedIconAssets>(Id::new(CACHE_KEY)))?;
    let mut cache = assets.lock().ok()?;
    cache.texture(ctx, id).cloned()
}

/// Paint an asset icon tinted with `tint` into `rect`. Returns `false` if the
/// asset is missing/undecodable (callers can fall back to a drawn glyph).
pub fn paint(painter: &Painter, id: u8, rect: Rect, tint: Color32) -> bool {
    let Some(texture) = texture_for(painter.ctx(), id) else {
        return false;
    };
    painter.image(
        texture.id(),
        rect,
        Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0)),
        tint,
    );
    true
}

/// Paint an asset icon rotated by `quarter_turns` × 90° clockwise. Used for the
/// play triangle, which points up in the pack but must point right in the UI.
pub fn paint_rotated(
    painter: &Painter,
    id: u8,
    rect: Rect,
    tint: Color32,
    quarter_turns: u8,
) -> bool {
    let Some(texture) = texture_for(painter.ctx(), id) else {
        return false;
    };
    // UVs at the four rect corners after rotating the source image clockwise.
    let corners = match quarter_turns % 4 {
        0 => [
            egui::pos2(0.0, 0.0),
            egui::pos2(1.0, 0.0),
            egui::pos2(1.0, 1.0),
            egui::pos2(0.0, 1.0),
        ],
        1 => [
            egui::pos2(0.0, 1.0),
            egui::pos2(0.0, 0.0),
            egui::pos2(1.0, 0.0),
            egui::pos2(1.0, 1.0),
        ],
        2 => [
            egui::pos2(1.0, 1.0),
            egui::pos2(0.0, 1.0),
            egui::pos2(0.0, 0.0),
            egui::pos2(1.0, 0.0),
        ],
        _ => [
            egui::pos2(1.0, 0.0),
            egui::pos2(1.0, 1.0),
            egui::pos2(0.0, 1.0),
            egui::pos2(0.0, 0.0),
        ],
    };
    let positions = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
    ];
    let mut mesh = egui::Mesh::with_texture(texture.id());
    for (pos, uv) in positions.into_iter().zip(corners) {
        mesh.vertices.push(egui::epaint::Vertex {
            pos,
            uv,
            color: tint,
        });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
    true
}

fn decode_png(bytes: &[u8]) -> Option<ColorImage> {
    let image = image::load_from_memory(bytes).ok()?.into_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    Some(ColorImage::from_rgba_unmultiplied(size, image.as_raw()))
}

/// White 48 px PNG variants (kept at 48 px so 16–22 px UI sizes stay crisp at
/// 125 % / 150 % UI scale).
fn asset_bytes(id: u8) -> Option<&'static [u8]> {
    Some(match id {
        0 => include_bytes!("../../assets/icons/white/png/icon-0-48px.png"),
        1 => include_bytes!("../../assets/icons/white/png/icon-1-48px.png"),
        2 => include_bytes!("../../assets/icons/white/png/icon-2-48px.png"),
        4 => include_bytes!("../../assets/icons/white/png/icon-4-48px.png"),
        7 => include_bytes!("../../assets/icons/white/png/icon-7-48px.png"),
        10 => include_bytes!("../../assets/icons/white/png/icon-10-48px.png"),
        11 => include_bytes!("../../assets/icons/white/png/icon-11-48px.png"),
        14 => include_bytes!("../../assets/icons/white/png/icon-14-48px.png"),
        15 => include_bytes!("../../assets/icons/white/png/icon-15-48px.png"),
        16 => include_bytes!("../../assets/icons/white/png/icon-16-48px.png"),
        17 => include_bytes!("../../assets/icons/white/png/icon-17-48px.png"),
        19 => include_bytes!("../../assets/icons/white/png/icon-19-48px.png"),
        20 => include_bytes!("../../assets/icons/white/png/icon-20-48px.png"),
        24 => include_bytes!("../../assets/icons/white/png/icon-24-48px.png"),
        25 => include_bytes!("../../assets/icons/white/png/icon-25-48px.png"),
        34 => include_bytes!("../../assets/icons/white/png/icon-34-48px.png"),
        35 => include_bytes!("../../assets/icons/white/png/icon-35-48px.png"),
        37 => include_bytes!("../../assets/icons/white/png/icon-37-48px.png"),
        39 => include_bytes!("../../assets/icons/white/png/icon-39-48px.png"),
        // User supplied transparent note artwork (500×500); tinted at paint time.
        200 => include_bytes!("../../assets/icons/note_white.png"),
        _ => return None,
    })
}
