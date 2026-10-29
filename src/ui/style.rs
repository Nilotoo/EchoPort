//! Drawing primitives shared by every page.
//!
//! Everything here paints with the *current* `Theme`; the theme itself is
//! already colour-lerped by the app shell, so widgets never freeze a palette.
//! Icons are vector-drawn with painter primitives instead of emoji so they
//! stay crisp and identical on every platform.

use egui::{
    Align, Align2, Color32, CornerRadius, CursorIcon, FontId, Galley, Id, Layout, Margin, Painter,
    Pos2, Rect, Response, RichText, Sense, Shape, Stroke, StrokeKind, Ui, Vec2,
};
use std::sync::Arc;

use crate::i18n::Key;
use crate::state::AppState;
use crate::theme::{self, mix, readable_on, with_alpha, Theme};

// ---------------------------------------------------------------------------
// Radii and spacing constants (kept in one place so pages stay consistent)
// ---------------------------------------------------------------------------

pub const R_SM: u8 = 8;
pub const R_MD: u8 = 12;
pub const R_LG: u8 = 16;
pub const R_XL: u8 = 22;

#[allow(dead_code)]
pub const CARD_PAD: i8 = 16;

// ---------------------------------------------------------------------------
// Fonts
// ---------------------------------------------------------------------------

#[inline]
pub fn font(size: f32) -> FontId {
    FontId::proportional(size)
}

// ---------------------------------------------------------------------------
// Colours derived from the theme
// ---------------------------------------------------------------------------

/// Very soft tint used behind section headings / inset areas.
pub fn wash(theme: &Theme) -> Color32 {
    mix(
        theme.surface,
        theme.accent_soft,
        if theme.dark { 0.16 } else { 0.42 },
    )
}

/// Slightly darker wash used for the hovered version of inset areas.
#[allow(dead_code)]
pub fn wash_hover(theme: &Theme) -> Color32 {
    mix(
        theme.surface,
        theme.accent_soft,
        if theme.dark { 0.26 } else { 0.62 },
    )
}

pub fn shadow(theme: &Theme) -> egui::epaint::Shadow {
    egui::epaint::Shadow {
        offset: [0, if theme.dark { 8 } else { 5 }],
        blur: if theme.dark { 26 } else { 20 },
        spread: 0,
        color: Color32::from_black_alpha(if theme.dark { 90 } else { 18 }),
    }
}

/// Colour for body text on top of `fill` (used for accent chips/badges).
#[allow(dead_code)]
pub fn on(fill: Color32, theme: &Theme) -> Color32 {
    if theme::luminance(fill) > 0.55 {
        readable_on(theme.text, fill, 4.5)
    } else {
        readable_on(Color32::WHITE, fill, 3.0)
    }
}

pub fn success_color(theme: &Theme) -> Color32 {
    if theme.dark {
        Color32::from_rgb(0x8F, 0xD6, 0xA8)
    } else {
        Color32::from_rgb(0x2F, 0x7D, 0x50)
    }
}

pub fn danger_color(theme: &Theme) -> Color32 {
    if theme.dark {
        Color32::from_rgb(0xF2, 0x9B, 0x93)
    } else {
        Color32::from_rgb(0xA8, 0x3B, 0x33)
    }
}

// ---------------------------------------------------------------------------
// Text helpers
// ---------------------------------------------------------------------------

/// Lay out a single line of text, eliding with `…` when it does not fit.
pub fn elided(
    painter: &Painter,
    text: &str,
    font_id: FontId,
    color: Color32,
    max_width: f32,
) -> Arc<Galley> {
    use egui::text::{LayoutJob, TextFormat, TextWrapping};
    let mut job = LayoutJob::single_section(text.to_owned(), TextFormat::simple(font_id, color));
    job.wrap = TextWrapping {
        max_width: max_width.max(8.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    painter.layout_job(job)
}

/// Paint one elided line, left aligned at `left_pos` (vertically centered).
pub fn paint_elided(
    painter: &Painter,
    left_center: Pos2,
    text: &str,
    font_id: FontId,
    color: Color32,
    max_width: f32,
) {
    let galley = elided(painter, text, font_id, color, max_width);
    let pos = Pos2::new(left_center.x, left_center.y - galley.size().y * 0.5);
    painter.galley(pos, galley, color);
}

/// Paint one elided line, right aligned so that it ends at `right_center.x`.
pub fn paint_elided_right(
    painter: &Painter,
    right_center: Pos2,
    text: &str,
    font_id: FontId,
    color: Color32,
    max_width: f32,
) {
    let galley = elided(painter, text, font_id, color, max_width);
    let pos = Pos2::new(
        right_center.x - galley.size().x,
        right_center.y - galley.size().y * 0.5,
    );
    painter.galley(pos, galley, color);
}

/// A wrapped paragraph label (uses egui's own layout so wrapping/height are
/// handled correctly).
#[allow(dead_code)]
pub fn paragraph(ui: &mut Ui, text: &str, size: f32, color: Color32) -> Response {
    ui.label(RichText::new(text).size(size).color(color))
}

// ---------------------------------------------------------------------------
// Frames
// ---------------------------------------------------------------------------

/// Standard raised card: surface fill, hairline border, soft shadow.
#[allow(dead_code)]
pub fn card(theme: &Theme) -> egui::Frame {
    egui::Frame::new()
        .fill(theme.surface)
        .stroke(Stroke::new(1.0_f32, theme.border))
        .corner_radius(CornerRadius::same(R_LG))
        .inner_margin(Margin::same(CARD_PAD))
        .shadow(shadow(theme))
}

/// Flatter card without shadow (used inside scrollable pages to avoid clutter).
#[allow(dead_code)]
pub fn flat_card(theme: &Theme) -> egui::Frame {
    egui::Frame::new()
        .fill(theme.surface)
        .stroke(Stroke::new(1.0_f32, theme.border))
        .corner_radius(CornerRadius::same(R_LG))
        .inner_margin(Margin::same(CARD_PAD))
}

/// Inset / washed block without a border.
#[allow(dead_code)]
pub fn inset(theme: &Theme) -> egui::Frame {
    egui::Frame::new()
        .fill(wash(theme))
        .corner_radius(CornerRadius::same(R_MD))
        .inner_margin(Margin::symmetric(12, 9))
}

#[allow(dead_code)]
pub fn divider(ui: &mut Ui, theme: &Theme) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 1.0), Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        Stroke::new(1.0_f32, theme.border),
    );
}

/// Paint a soft translucent circle (decoration only, no interaction).
pub fn blob(painter: &Painter, center: Pos2, radius: f32, color: Color32) {
    painter.circle_filled(center, radius, color);
}

/// Cached tiny gradient texture (vertical or diagonal). Painting the gradient
/// as a *texture on a rounded rect* gives generated covers exactly the same
/// geometry + anti-aliased corners as real artwork, which a triangle-fan mesh
/// cannot do cleanly.
fn gradient_texture(
    ctx: &egui::Context,
    top: Color32,
    bottom: Color32,
    diagonal: bool,
) -> egui::TextureHandle {
    let key = egui::Id::new((
        "echoport-gradient",
        top.to_array(),
        bottom.to_array(),
        diagonal,
    ));
    if let Some(texture) = ctx.data(|data| data.get_temp::<egui::TextureHandle>(key)) {
        return texture;
    }
    let size = if diagonal { 32usize } else { 2usize };
    let mut pixels = Vec::with_capacity(size * size);
    for y in 0..size {
        for x in 0..size {
            let t = if diagonal {
                ((x + y) as f32 / (2.0 * (size - 1) as f32)).clamp(0.0, 1.0)
            } else {
                y as f32 / (size - 1) as f32
            };
            pixels.push(mix(top, bottom, t));
        }
    }
    let image = egui::ColorImage {
        size: [size, size],
        source_size: Vec2::new(size as f32, size as f32),
        pixels,
    };
    let texture = ctx.load_texture(
        format!("echoport-gradient-{}-{diagonal}", top.to_array()[0]),
        image,
        egui::TextureOptions::LINEAR,
    );
    ctx.data_mut(|data| data.insert_temp(key, texture.clone()));
    texture
}

/// Vertical (or diagonal) gradient clipped to a rounded rectangle.
pub fn rounded_gradient(
    painter: &Painter,
    rect: Rect,
    radius: u8,
    top: Color32,
    bottom: Color32,
    diagonal: bool,
) {
    let texture = gradient_texture(painter.ctx(), top, bottom, diagonal);
    let uv = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0));
    painter.add(
        egui::epaint::RectShape::filled(rect, CornerRadius::same(radius), Color32::WHITE)
            .with_texture(texture.id(), uv),
    );
}

/// A rounded rectangle with a diagonal-ish two-colour tint; used for
/// generated playlist covers.
pub fn gradient_cover(
    painter: &Painter,
    rect: Rect,
    radius: u8,
    a: Color32,
    b: Color32,
    glyph: Option<&str>,
) {
    let top = mix(a, Color32::WHITE, 0.18);
    let bottom = mix(b, Color32::WHITE, 0.06);
    let mid = mix(top, bottom, 0.5);
    let ink = if crate::theme::luminance(mid) > 0.55 {
        crate::theme::shade(a, -0.62)
    } else {
        Color32::WHITE
    };
    let variant = crate::model::stable_hash(glyph.unwrap_or("cover"))
        .as_bytes()
        .first()
        .copied()
        .unwrap_or(0) as usize
        % 3;
    paint_generated_cover(
        painter,
        rect,
        radius,
        top,
        bottom,
        ink,
        mix(b, Color32::WHITE, 0.2),
        glyph.unwrap_or(""),
        variant,
    );
}

/// A deterministic "designed sleeve" for generated covers (missing artwork,
/// playlists, empty states). Composition instead of decoration: gradient,
/// sleeve band, accent rule and a big monogram — no random rings.
pub fn paint_generated_cover(
    painter: &Painter,
    rect: Rect,
    radius: u8,
    top: Color32,
    bottom: Color32,
    ink: Color32,
    accent: Color32,
    glyph: &str,
    variant: usize,
) {
    rounded_gradient(painter, rect, radius, top, bottom, variant % 2 == 1);
    // Clip all decorative shapes to the rounded shape so nothing can poke
    // outside the corners (the old mesh-based gradient left square tips on
    // small covers, which read as "square cover inside a rounded card").
    let safe = rect.shrink(radius as f32 * 0.5).intersect(rect);
    let painter = &painter.with_clip_rect(safe);
    let w = rect.width();
    let h = rect.height();
    let pad = (w * 0.09).max(3.0);
    let strip_h = (h * 0.17).max(7.0);
    let strip = Rect::from_min_max(
        Pos2::new(rect.left() + pad, rect.bottom() - pad - strip_h),
        Pos2::new(rect.right() - pad, rect.bottom() - pad),
    );
    painter.rect_filled(strip, CornerRadius::same(3), with_alpha(ink, 30));
    painter.rect_filled(
        Rect::from_min_size(
            Pos2::new(
                strip.left() + pad * 0.6,
                strip.center().y - (h * 0.012).max(1.0),
            ),
            Vec2::new(strip.width() * 0.34, (h * 0.024).max(1.5)),
        ),
        CornerRadius::same(1),
        with_alpha(accent, 230),
    );
    for i in 0..2 {
        painter.rect_filled(
            Rect::from_min_size(
                Pos2::new(
                    strip.right() - pad * 0.6 - strip.width() * (0.22 - i as f32 * 0.06),
                    strip.center().y - (h * 0.022) + i as f32 * (h * 0.032),
                ),
                Vec2::new(
                    strip.width() * (0.22 - i as f32 * 0.06),
                    (h * 0.013).max(1.0),
                ),
            ),
            CornerRadius::same(1),
            with_alpha(ink, 70),
        );
    }
    let mark = (w * 0.055).max(2.0);
    for i in 0..2 {
        let m = Rect::from_min_size(
            Pos2::new(
                rect.right() - pad - mark - i as f32 * (mark * 1.8),
                rect.top() + pad,
            ),
            Vec2::new(mark, mark),
        );
        painter.rect_filled(m, CornerRadius::same(1), with_alpha(ink, 34));
    }
    if !glyph.is_empty() {
        let glyph_size = (w * 0.40).clamp(10.0, 220.0);
        let (pos, align) = match variant % 3 {
            0 => (
                Pos2::new(rect.center().x, rect.center().y - strip_h * 0.30),
                Align2::CENTER_CENTER,
            ),
            1 => (
                Pos2::new(rect.left() + pad * 1.4, strip.top() - pad * 0.8),
                Align2::LEFT_BOTTOM,
            ),
            _ => (
                Pos2::new(rect.right() - pad * 1.4, rect.top() + pad * 1.6),
                Align2::RIGHT_TOP,
            ),
        };
        painter.text(pos, align, glyph, font(glyph_size), with_alpha(ink, 225));
    }
}

// ---------------------------------------------------------------------------
// Icons
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Home,
    Search,
    Library,
    Playlist,
    Download,
    Profile,
    Settings,
    Play,
    Pause,
    Previous,
    Next,
    Shuffle,
    RepeatAll,
    RepeatOne,
    Heart,
    HeartFill,
    Volume,
    VolumeMute,
    Queue,
    Plus,
    Close,
    Check,
    More,
    Trash,
    External,
    /// Share / link icon from the pack (#34) — used for the source-page shortcut.
    LinkShare,
    /// Globe icon from the pack (#25) — used on the search page illustration.
    Globe,
    Folder,
    Music,
    Clock,
    Info,
    ChevronLeft,
    ChevronRight,
    Copy,
    Link,
    Refresh,
    #[allow(dead_code)]
    History,
}

fn arc(center: Pos2, radius: f32, from: f32, to: f32, steps: usize) -> Vec<Pos2> {
    let mut points = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let angle = from + (to - from) * t;
        points.push(center + Vec2::angled(angle) * radius);
    }
    points
}

fn filled_triangle(painter: &Painter, points: [Pos2; 3], color: Color32) {
    painter.add(Shape::convex_polygon(points.to_vec(), color, Stroke::NONE));
}

/// Map an icon to the embedded asset set (white PNG, tinted at paint time).
/// `None` means the pack has no matching glyph and we keep the hand-drawn one.
fn asset_icon_id(icon: Icon) -> Option<u8> {
    Some(match icon {
        Icon::Search => 35,
        Icon::Library => 2,
        Icon::Playlist => 1,
        Icon::Download => 7,
        Icon::Settings => 39,
        Icon::Play => 20,
        Icon::Pause => 19,
        Icon::Previous => 16,
        Icon::Next => 17,
        Icon::Heart => 24,
        Icon::Queue => 0,
        Icon::More => 4,
        Icon::Trash => 37,
        Icon::External => 10,
        Icon::LinkShare => 34,
        Icon::Globe => 25,
        Icon::ChevronLeft => 14,
        Icon::ChevronRight => 15,
        _ => return None,
    })
}

/// Paint an icon centred inside `rect` using `color`.
/// Paint the user-supplied transparent note artwork (tinted to the theme).
pub fn paint_note(painter: &Painter, rect: Rect, color: Color32) -> bool {
    super::icons::paint(painter, 200, rect, color)
}

pub fn paint_icon(painter: &Painter, icon: Icon, rect: Rect, color: Color32) {
    // Shuffle: crossing arrows derived from the icon pack's SVG path
    // `icon-10.svg` (`M7 17L17 7` shaft + arrow head). The pack head is very
    // wide; when two copies cross, shorten just the head barbs so the result
    // stays a readable X at 16–20 px (the shaft/stroke/caps are the pack's).
    if icon == Icon::Shuffle {
        let side = rect.width().min(rect.height());
        let c = rect.center();
        let unit = side / 19.0;
        let stroke = Stroke::new((1.8 * unit).max(1.3), color);
        let svg = |x: f32, y: f32| c + Vec2::new((x - 12.0) * unit, (y - 12.0) * unit);
        let draw = |rotate: bool| {
            let tp = |x: f32, y: f32| {
                if rotate {
                    svg(24.0 - y, x)
                } else {
                    svg(x, y)
                }
            };
            painter.line_segment([tp(7.0, 17.0), tp(17.0, 7.0)], stroke);
            // Short L-bracket head at the tip, same 90° style as the SVG.
            painter.line_segment([tp(17.0, 7.0), tp(14.4, 7.0)], stroke);
            painter.line_segment([tp(17.0, 7.0), tp(17.0, 9.6)], stroke);
            for (x, y) in [(7.0, 17.0), (14.4, 7.0), (17.0, 9.6)] {
                painter.circle_filled(tp(x, y), stroke.width * 0.5, color);
            }
        };
        draw(false);
        draw(true);
        return;
    }
    if let Some(id) = asset_icon_id(icon) {
        // Asset glyphs are square: always fit them into a centred square so
        // narrow cells (e.g. a row's 30 px play column) never stretch them.
        let side = rect.width().min(rect.height());
        let square = Rect::from_center_size(rect.center(), Vec2::splat(side));
        let target = square.shrink(side * 0.06);
        // The pack's triangle points up; rotate it +90° so play points right.
        let painted = if icon == Icon::Play {
            crate::ui::icons::paint_rotated(painter, id, target, color, 1)
        } else {
            crate::ui::icons::paint(painter, id, target, color)
        };
        if painted {
            return;
        }
    }
    let c = rect.center();
    let s = rect.width().min(rect.height());
    let sw = (s * 0.085).max(1.25);
    let stroke = Stroke::new(sw, color);
    let r = |v: f32| v * s;
    let p = |x: f32, y: f32| c + Vec2::new(r(x), r(y));
    match icon {
        Icon::Home => {
            painter.add(Shape::line(
                vec![p(-0.40, -0.02), p(0.0, -0.38), p(0.40, -0.02)],
                stroke,
            ));
            painter.add(Shape::line(
                vec![
                    p(-0.30, -0.06),
                    p(-0.30, 0.36),
                    p(0.30, 0.36),
                    p(0.30, -0.06),
                ],
                stroke,
            ));
        }
        Icon::Profile => {
            painter.circle_stroke(p(0.0, -0.16), r(0.20), stroke);
            painter.add(Shape::line(
                arc(
                    p(0.0, 0.42),
                    r(0.34),
                    std::f32::consts::PI * 1.12,
                    std::f32::consts::PI * 1.88,
                    14,
                ),
                stroke,
            ));
        }
        Icon::RepeatAll | Icon::RepeatOne => {
            painter.rect_stroke(
                Rect::from_center_size(c, Vec2::splat(r(0.64))),
                CornerRadius::same(4),
                stroke,
                StrokeKind::Middle,
            );
            filled_triangle(
                painter,
                [p(0.10, -0.44), p(0.32, -0.32), p(0.10, -0.20)],
                color,
            );
            filled_triangle(
                painter,
                [p(-0.10, 0.44), p(-0.32, 0.32), p(-0.10, 0.20)],
                color,
            );
            if icon == Icon::RepeatOne {
                painter.text(c, Align2::CENTER_CENTER, "1", font(r(0.42)), color);
            }
        }
        Icon::Heart | Icon::HeartFill => {
            let (left, right) = (p(-0.16, -0.10), p(0.16, -0.10));
            if icon == Icon::HeartFill {
                painter.circle_filled(left, r(0.20), color);
                painter.circle_filled(right, r(0.20), color);
                filled_triangle(
                    painter,
                    [p(-0.35, -0.04), p(0.35, -0.04), p(0.0, 0.40)],
                    color,
                );
            } else {
                painter.circle_stroke(left, r(0.18), stroke);
                painter.circle_stroke(right, r(0.18), stroke);
                painter.add(Shape::line(
                    vec![p(-0.32, -0.02), p(0.0, 0.38), p(0.32, -0.02)],
                    stroke,
                ));
            }
        }
        Icon::Volume | Icon::VolumeMute => {
            painter.rect_filled(
                Rect::from_min_max(p(-0.36, -0.15), p(-0.14, 0.15)),
                CornerRadius::same(2),
                color,
            );
            filled_triangle(
                painter,
                [p(-0.14, -0.15), p(0.08, -0.34), p(0.08, 0.34)],
                color,
            );
            if icon == Icon::Volume {
                painter.add(Shape::line(
                    arc(p(0.05, 0.0), r(0.16), -0.7, 0.7, 8),
                    stroke,
                ));
                painter.add(Shape::line(
                    arc(p(0.05, 0.0), r(0.30), -0.7, 0.7, 10),
                    stroke,
                ));
            } else {
                painter.line_segment([p(0.14, -0.34), p(0.40, 0.34)], stroke);
                painter.line_segment([p(0.40, -0.34), p(0.14, 0.34)], stroke);
            }
        }
        Icon::Plus => {
            painter.line_segment([p(-0.30, 0.0), p(0.30, 0.0)], stroke);
            painter.line_segment([p(0.0, -0.30), p(0.0, 0.30)], stroke);
        }
        Icon::Close => {
            painter.line_segment([p(-0.26, -0.26), p(0.26, 0.26)], stroke);
            painter.line_segment([p(0.26, -0.26), p(-0.26, 0.26)], stroke);
        }
        Icon::Check => {
            painter.add(Shape::line(
                vec![p(-0.30, 0.02), p(-0.06, 0.26), p(0.32, -0.26)],
                stroke,
            ));
        }
        Icon::Folder => {
            painter.add(Shape::line(
                vec![
                    p(-0.38, -0.24),
                    p(-0.12, -0.24),
                    p(-0.02, -0.12),
                    p(0.38, -0.12),
                    p(0.38, 0.32),
                    p(-0.38, 0.32),
                    p(-0.38, -0.24),
                ],
                stroke,
            ));
        }
        Icon::Music => {
            painter.circle_filled(p(-0.16, 0.24), r(0.14).max(2.0), color);
            painter.line_segment([p(0.0, 0.24), p(0.0, -0.32)], stroke);
            painter.add(Shape::line(
                vec![p(0.0, -0.32), p(0.30, -0.24), p(0.30, -0.06)],
                stroke,
            ));
        }
        Icon::Clock => {
            painter.circle_stroke(c, r(0.34), stroke);
            painter.line_segment([c, p(0.0, -0.20)], stroke);
            painter.line_segment([c, p(0.16, 0.10)], stroke);
        }
        Icon::Info => {
            painter.circle_stroke(c, r(0.34), stroke);
            painter.circle_filled(p(0.0, -0.17), r(0.055).max(1.2), color);
            painter.line_segment([p(0.0, -0.04), p(0.0, 0.20)], stroke);
        }
        Icon::Copy => {
            painter.rect_stroke(
                Rect::from_min_max(p(-0.34, -0.34), p(0.14, 0.14)),
                CornerRadius::same(3),
                stroke,
                StrokeKind::Middle,
            );
            painter.rect_stroke(
                Rect::from_min_max(p(-0.14, -0.14), p(0.34, 0.34)),
                CornerRadius::same(3),
                stroke,
                StrokeKind::Middle,
            );
        }
        Icon::Refresh => {
            // Reload: ~280° arc with a gap at the upper right; the arrow head
            // sits tangentially on the arc end (no floating/disjoint triangle).
            let radius = r(0.30);
            let start = 0.50_f32;
            let end = 5.35_f32;
            painter.add(Shape::line(arc(c, radius, start, end, 32), stroke));
            let tangent = Vec2::angled(end + std::f32::consts::FRAC_PI_2);
            let end_point = c + Vec2::angled(end) * radius;
            let tip = end_point + tangent * (sw * 2.8);
            let perp = Vec2::new(-tangent.y, tangent.x) * (sw * 1.7);
            filled_triangle(painter, [tip, end_point + perp, end_point - perp], color);
        }
        #[allow(unused)]
        Icon::History => {
            painter.circle_stroke(c, r(0.32), stroke);
            painter.line_segment([c, p(0.0, -0.20)], stroke);
            painter.line_segment([c, p(0.16, 0.08)], stroke);
            painter.line_segment([p(-0.32, -0.22), p(-0.32, 0.02)], stroke);
            filled_triangle(
                painter,
                [p(-0.44, -0.32), p(-0.20, -0.32), p(-0.32, -0.10)],
                color,
            );
        }
        Icon::Link => {
            painter.add(Shape::line(
                arc(p(-0.10, 0.0), r(0.22), 2.2, 4.1, 10),
                stroke,
            ));
            painter.add(Shape::line(
                arc(p(0.10, 0.0), r(0.22), -0.94, 1.0, 10),
                stroke,
            ));
            painter.line_segment([p(-0.20, 0.0), p(0.20, 0.0)], stroke);
        }
        // Replaced icons normally return early after painting the asset;
        // this keeps the match exhaustive if an asset ever fails to load.
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Buttons
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    Primary,
    Tonal,
    Secondary,
    Ghost,
    Danger,
}

fn variant_colors(theme: &Theme, variant: Variant, t: f32) -> (Color32, Color32, Color32) {
    match variant {
        Variant::Primary => {
            let base = theme.accent;
            let bg = mix(base, theme.on_accent, 0.10 * t);
            (bg, theme.on_accent, Color32::TRANSPARENT)
        }
        Variant::Tonal => {
            let bg = mix(theme.accent_soft, theme.accent, 0.10 + 0.10 * t);
            (bg, readable_on(theme.accent, bg, 3.2), Color32::TRANSPARENT)
        }
        Variant::Secondary => {
            let bg = mix(theme.surface, theme.accent_soft, 0.16 + 0.22 * t);
            let border = mix(theme.border, theme.accent, 0.20 + 0.35 * t);
            (bg, theme.text, border)
        }
        Variant::Ghost => {
            let bg = with_alpha(theme.surface_hover, (150.0 * t) as u8);
            (
                bg,
                mix(theme.text_weak, theme.text, t),
                Color32::TRANSPARENT,
            )
        }
        Variant::Danger => {
            let red = danger_color(theme);
            let bg = mix(theme.surface, red, 0.06 + 0.10 * t);
            (bg, red, mix(theme.border, red, 0.25))
        }
    }
}

fn paint_button(
    ui: &mut Ui,
    theme: &Theme,
    icon: Option<Icon>,
    label: Option<&str>,
    variant: Variant,
    height: f32,
    min_width: Option<f32>,
    text_size: f32,
) -> Response {
    let text_font = font(text_size);
    let text_width = label
        .map(|text| {
            ui.painter()
                .layout_no_wrap(text.to_owned(), text_font.clone(), theme.text)
                .size()
                .x
        })
        .unwrap_or(0.0);
    let icon_w = if icon.is_some() {
        text_size * 1.05
    } else {
        0.0
    };
    let gap = if icon.is_some() && label.is_some() {
        7.0
    } else {
        0.0
    };
    let h_pad = 15.0;
    let width = (text_width + icon_w + gap + h_pad * 2.0).max(min_width.unwrap_or(0.0));
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    let t = ui
        .ctx()
        .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.12);
    let (bg, fg, border) = variant_colors(theme, variant, t);
    if bg.a() > 0 {
        ui.painter().rect_filled(rect, CornerRadius::same(R_MD), bg);
    }
    if border.a() > 0 {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(R_MD),
            Stroke::new(1.0_f32, border),
            StrokeKind::Inside,
        );
    }
    let icon_rect = Rect::from_center_size(
        Pos2::new(rect.left() + h_pad + icon_w * 0.5, rect.center().y),
        Vec2::splat(icon_w),
    );
    if let Some(icon) = icon {
        paint_icon(ui.painter(), icon, icon_rect, fg);
    }
    if let Some(label) = label {
        let text_left = rect.left() + h_pad + icon_w + gap;
        let galley = elided(
            ui.painter(),
            label,
            text_font,
            fg,
            rect.right() - h_pad - text_left,
        );
        ui.painter().galley(
            Pos2::new(text_left, rect.center().y - galley.size().y * 0.5),
            galley,
            fg,
        );
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

pub fn button(ui: &mut Ui, theme: &Theme, label: &str, variant: Variant) -> Response {
    paint_button(ui, theme, None, Some(label), variant, 34.0, None, 13.0)
}

pub fn button_sized(
    ui: &mut Ui,
    theme: &Theme,
    label: &str,
    variant: Variant,
    height: f32,
) -> Response {
    paint_button(ui, theme, None, Some(label), variant, height, None, 13.0)
}

pub fn button_icon(
    ui: &mut Ui,
    theme: &Theme,
    icon: Icon,
    label: &str,
    variant: Variant,
) -> Response {
    paint_button(
        ui,
        theme,
        Some(icon),
        Some(label),
        variant,
        34.0,
        None,
        13.0,
    )
}

pub fn icon_only(
    ui: &mut Ui,
    theme: &Theme,
    icon: Icon,
    size: f32,
    variant: Variant,
    tooltip: &str,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let t = ui
        .ctx()
        .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.12);
    let (bg, fg, border) = variant_colors(theme, variant, t);
    if bg.a() > 0 {
        ui.painter().rect_filled(rect, CornerRadius::same(R_SM), bg);
    }
    if border.a() > 0 {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(R_SM),
            Stroke::new(1.0_f32, border),
            StrokeKind::Inside,
        );
    }
    paint_icon(ui.painter(), icon, rect.shrink(size * 0.28), fg);
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    if tooltip.is_empty() {
        response
    } else {
        response.on_hover_text(tooltip)
    }
}

pub fn icon_button(
    ui: &mut Ui,
    theme: &Theme,
    icon: Icon,
    size: f32,
    active: bool,
    tooltip: &str,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let t = ui
        .ctx()
        .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.12);
    let bg = if active {
        mix(theme.accent_soft, theme.accent, 0.10 * t)
    } else {
        with_alpha(theme.surface_hover, (210.0 * t) as u8)
    };
    if bg.a() > 0 {
        ui.painter().rect_filled(rect, CornerRadius::same(R_SM), bg);
    }
    let color = if active {
        theme.accent
    } else {
        mix(theme.text_weak, theme.text, t)
    };
    let inner = if icon == Icon::Shuffle {
        rect.shrink(size * 0.13)
    } else {
        rect.shrink(size * 0.24)
    };
    paint_icon(ui.painter(), icon, inner, color);
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    if tooltip.is_empty() {
        response
    } else {
        response.on_hover_text(tooltip)
    }
}

/// Big round transport play button.
/// Paint the round play/pause button into an explicit rect (used by the
/// transport row so every button can share one horizontal centre line).
pub fn play_button_at(ui: &mut Ui, rect: Rect, theme: &Theme, playing: bool) -> Response {
    let size = rect.width().min(rect.height());
    let response = ui.interact(
        rect,
        ui.id()
            .with(("play-button", rect.min.x.to_bits(), rect.min.y.to_bits())),
        Sense::click(),
    );
    let t = ui
        .ctx()
        .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.12);
    let fill = mix(theme.accent, theme.on_accent, 0.10 * t);
    ui.painter()
        .rect_filled(rect, CornerRadius::same((size / 2.0) as u8), fill);
    // Soft halo on hover.
    if t > 0.01 {
        ui.painter().circle_stroke(
            rect.center(),
            size * (0.52 + 0.05 * t),
            Stroke::new(1.0_f32, with_alpha(theme.accent, (90.0 * t) as u8)),
        );
    }
    let icon = if playing { Icon::Pause } else { Icon::Play };
    paint_icon(
        ui.painter(),
        icon,
        rect.shrink(size * 0.28),
        theme.on_accent,
    );
    response
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(if playing { "" } else { "" })
}

// ---------------------------------------------------------------------------
// Chips, tabs and segmented controls
// ---------------------------------------------------------------------------

pub fn chip(
    ui: &mut Ui,
    theme: &Theme,
    label: &str,
    dot: Option<Color32>,
    selected: bool,
) -> Response {
    let text_font = font(12.5);
    let text_width = ui
        .painter()
        .layout_no_wrap(label.to_owned(), text_font.clone(), theme.text)
        .size()
        .x;
    let dot_space = if dot.is_some() { 13.0 } else { 0.0 };
    let width = text_width + dot_space + 24.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 28.0), Sense::click());
    let t = ui
        .ctx()
        .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.12);
    let (bg, border, fg) = if selected {
        (
            mix(theme.accent_soft, theme.accent, 0.12),
            mix(theme.accent, theme.accent_soft, 0.35),
            readable_on(
                theme.accent,
                mix(theme.accent_soft, theme.accent, 0.12),
                3.4,
            ),
        )
    } else {
        (
            mix(theme.surface, theme.surface_hover, t),
            mix(theme.border, theme.accent, 0.30 * t),
            mix(theme.text_weak, theme.text, t),
        )
    };
    ui.painter().rect_filled(rect, CornerRadius::same(14), bg);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(14),
        Stroke::new(1.0_f32, border),
        StrokeKind::Inside,
    );
    let mut x = rect.left() + 12.0;
    if let Some(color) = dot {
        ui.painter()
            .circle_filled(Pos2::new(x + 3.0, rect.center().y), 3.0, color);
        x += 13.0;
    }
    let galley = elided(ui.painter(), label, text_font, fg, rect.right() - 10.0 - x);
    ui.painter().galley(
        Pos2::new(x, rect.center().y - galley.size().y * 0.5),
        galley,
        fg,
    );
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// A static, non-interactive small badge.
#[allow(dead_code)]
pub fn badge(ui: &mut Ui, _theme: &Theme, label: &str, color: Color32) {
    let text_font = font(10.5);
    let text_width = ui
        .painter()
        .layout_no_wrap(label.to_owned(), text_font.clone(), color)
        .size()
        .x;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(text_width + 14.0, 19.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(9), with_alpha(color, 26));
    let galley = elided(ui.painter(), label, text_font, color, rect.width());
    ui.painter().galley(
        Pos2::new(
            rect.center().x - galley.size().x * 0.5,
            rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        color,
    );
}

/// Pill segmented control. Returns the index the user clicked, if any.
pub fn segmented(
    ui: &mut Ui,
    theme: &Theme,
    labels: &[&str],
    selected: usize,
    height: f32,
) -> Option<usize> {
    let text_font = font(12.5);
    let mut widths = Vec::with_capacity(labels.len());
    for label in labels {
        let w = ui
            .painter()
            .layout_no_wrap((*label).to_owned(), text_font.clone(), theme.text)
            .size()
            .x;
        widths.push(w + 30.0);
    }
    let total: f32 = widths.iter().sum::<f32>() + 6.0 * (labels.len() as f32 - 1.0) + 8.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(total, height), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same((height / 2.0) as u8), wash(theme));
    let mut x = rect.left() + 4.0;
    let mut clicked = None;
    for (index, label) in labels.iter().enumerate() {
        let seg = Rect::from_min_size(
            Pos2::new(x, rect.top() + 4.0),
            Vec2::new(widths[index], height - 8.0),
        );
        let id = response.id.with(index);
        let seg_response = ui.interact(seg, id, Sense::click());
        let t = ui
            .ctx()
            .animate_bool_with_time(id.with("hover"), seg_response.hovered(), 0.12);
        if index == selected {
            let fill = if theme.dark {
                with_alpha(theme.accent, 44)
            } else {
                theme.surface
            };
            ui.painter()
                .rect_filled(seg, CornerRadius::same(((height - 8.0) / 2.0) as u8), fill);
            ui.painter().rect_stroke(
                seg,
                CornerRadius::same(((height - 8.0) / 2.0) as u8),
                Stroke::new(1.0_f32, mix(theme.border, theme.accent, 0.20)),
                StrokeKind::Inside,
            );
        } else if t > 0.01 {
            ui.painter().rect_filled(
                seg,
                CornerRadius::same(((height - 8.0) / 2.0) as u8),
                with_alpha(theme.surface_hover, (120.0 * t) as u8),
            );
        }
        let color = if index == selected {
            theme.accent
        } else {
            mix(theme.text_weak, theme.text, t)
        };
        let galley = elided(ui.painter(), label, text_font.clone(), color, seg.width());
        ui.painter().galley(
            Pos2::new(
                seg.center().x - galley.size().x * 0.5,
                seg.center().y - galley.size().y * 0.5,
            ),
            galley,
            color,
        );
        if seg_response.clicked() {
            clicked = Some(index);
        }
        x += widths[index] + 6.0;
    }
    if labels.is_empty() {
        None
    } else {
        clicked
    }
}

/// Small dot used for platform colours.
#[allow(dead_code)]
pub fn platform_dot(
    painter: &Painter,
    center: Pos2,
    radius: f32,
    platform: crate::model::Platform,
) {
    painter.circle_filled(center, radius, theme::platform_color(platform));
}

// ---------------------------------------------------------------------------
// Progress / sliders
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub fn progress_bar(ui: &mut Ui, theme: &Theme, fraction: f32, width: Option<f32>, fill: Color32) {
    let width = width.unwrap_or(ui.available_width());
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 5.0), Sense::hover());
    let fraction = fraction.clamp(0.0, 1.0);
    ui.painter()
        .rect_filled(rect, CornerRadius::same(3), with_alpha(theme.border, 190));
    if fraction > 0.0 {
        let filled =
            Rect::from_min_size(rect.min, Vec2::new(rect.width() * fraction, rect.height()));
        ui.painter()
            .rect_filled(filled, CornerRadius::same(3), fill);
    }
}

/// A thin interactive timeline. Returns the response and, when the user
/// scrubbed, the new position in seconds.
pub fn scrubber(
    ui: &mut Ui,
    theme: &Theme,
    position: f64,
    duration: f64,
    id_salt: impl std::hash::Hash,
) -> (Response, Option<f64>) {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 18.0), Sense::click_and_drag());
    let center_y = rect.center().y;
    let track = Rect::from_min_max(
        Pos2::new(rect.left(), center_y - 2.0),
        Pos2::new(rect.right(), center_y + 2.0),
    );
    let fraction = if duration > 0.0 {
        (position / duration).clamp(0.0, 1.0) as f32
    } else {
        0.0
    };
    ui.painter()
        .rect_filled(track, CornerRadius::same(2), with_alpha(theme.border, 200));
    let filled = Rect::from_min_max(
        track.min,
        Pos2::new(track.left() + track.width() * fraction, track.max.y),
    );
    ui.painter()
        .rect_filled(filled, CornerRadius::same(2), theme.accent);
    let t = ui.ctx().animate_bool_with_time(
        Id::new(("scrub-hover", id_salt)),
        response.hovered() || response.dragged(),
        0.12,
    );
    let thumb_x = track.left() + track.width() * fraction;
    let thumb_rim = mix(theme.accent, theme.text, 0.15);
    if response.hovered() || response.dragged() {
        ui.painter()
            .circle_filled(Pos2::new(thumb_x, center_y), 5.0 + 2.0 * t, theme.highlight);
        ui.painter().circle_stroke(
            Pos2::new(thumb_x, center_y),
            5.0 + 2.0 * t,
            Stroke::new(1.5_f32, thumb_rim),
        );
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    } else if fraction > 0.0 {
        ui.painter()
            .circle_filled(Pos2::new(thumb_x, center_y), 3.6, theme.highlight);
        ui.painter().circle_stroke(
            Pos2::new(thumb_x, center_y),
            3.6,
            Stroke::new(1.0_f32, thumb_rim),
        );
    }
    let mut new_position = None;
    if response.dragged() || response.clicked() {
        if let Some(pointer) = response.interact_pointer_pos() {
            if duration > 0.0 {
                let f = ((pointer.x - track.left()) / track.width().max(1.0)).clamp(0.0, 1.0);
                new_position = Some(f as f64 * duration);
            }
        }
    }
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    (response, new_position)
}

/// Compact volume slider. Returns `Some(new_volume)` when dragged.
pub fn volume_slider(ui: &mut Ui, theme: &Theme, volume: f32, width: f32) -> Option<f32> {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 18.0), Sense::click_and_drag());
    let center_y = rect.center().y;
    let track = Rect::from_min_max(
        Pos2::new(rect.left(), center_y - 1.8),
        Pos2::new(rect.right(), center_y + 1.8),
    );
    let fraction = volume.clamp(0.0, 1.0);
    ui.painter()
        .rect_filled(track, CornerRadius::same(2), with_alpha(theme.border, 200));
    let filled = Rect::from_min_max(
        track.min,
        Pos2::new(track.left() + track.width() * fraction, track.max.y),
    );
    ui.painter()
        .rect_filled(filled, CornerRadius::same(2), theme.accent);
    let thumb_x = track.left() + track.width() * fraction;
    ui.painter().circle_filled(
        Pos2::new(thumb_x, center_y),
        if response.hovered() { 5.0 } else { 3.6 },
        theme.accent,
    );
    if response.dragged() || response.clicked() {
        if let Some(pointer) = response.interact_pointer_pos() {
            let f = ((pointer.x - track.left()) / track.width().max(1.0)).clamp(0.0, 1.0);
            return Some(f);
        }
    }
    None
}

/// Three softly pulsing dots used for buffering / loading states.
pub fn loading_dots(painter: &Painter, rect: Rect, theme: &Theme, time: f64) {
    let color = theme.accent;
    for i in 0..3 {
        let phase = time as f32 * 3.4 - i as f32 * 0.55;
        let alpha = (0.25 + 0.75 * (0.5 + 0.5 * phase.sin())) * 255.0;
        painter.circle_filled(
            Pos2::new(
                rect.left() + rect.width() * (0.18 + i as f32 * 0.32),
                rect.center().y,
            ),
            (rect.height() * 0.16).max(1.2),
            with_alpha(color, alpha as u8),
        );
    }
}

/// Animated equalizer bars driven by the theme's strong accents (highlight +
/// accent), so the "playing" state reads as intentionally branded in every
/// palette. Idle / paused draws a calm low state.
pub fn equalizer(painter: &Painter, rect: Rect, theme: &Theme, playing: bool, time: f64) {
    let colors = [theme.highlight, theme.accent, theme.highlight, theme.accent];
    let bars = colors.len().clamp(1, 5);
    let bar_w = (rect.width() / (bars as f32 * 1.75)).max(1.5);
    let gap = (rect.width() - bar_w * bars as f32) / (bars as f32).max(1.0);
    for i in 0..bars {
        let phase = time as f32 * 6.5 + i as f32 * 1.7;
        let level = if playing {
            0.35 + 0.65 * (0.5 + 0.5 * (phase.sin() * 0.7 + (phase * 1.6).sin() * 0.3))
        } else {
            0.24 + 0.06 * i as f32
        };
        let h = (rect.height() * level).max(2.0);
        let x = rect.left() + i as f32 * (bar_w + gap);
        let bar = Rect::from_min_size(Pos2::new(x, rect.bottom() - h), Vec2::new(bar_w, h));
        let color = if playing {
            colors[i % colors.len()]
        } else {
            with_alpha(colors[i % colors.len()], 120)
        };
        painter.rect_filled(bar, CornerRadius::same((bar_w * 0.5) as u8), color);
    }
}

// ---------------------------------------------------------------------------
// Small composed pieces
// ---------------------------------------------------------------------------

pub fn avatar(painter: &Painter, rect: Rect, name: &str, theme: &Theme) {
    let hue = (crate::model::stable_hash(name).as_bytes()[0] as f32 / 255.0) * 360.0;
    let fill = theme::from_hsl(hue, 0.42, if theme.dark { 0.40 } else { 0.86 });
    painter.rect_filled(rect, CornerRadius::same((rect.width() * 0.32) as u8), fill);
    let letter = name
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        letter,
        font(rect.width() * 0.44),
        theme::readable_on(fill, theme::shade(fill, -0.55), 3.0),
    );
}

/// Section header with an optional trailing widget. Returns the space that is
/// left for the caller's own right-side widgets.
pub fn section_header(
    ui: &mut Ui,
    theme: &Theme,
    title: &str,
    trailing: Option<&str>,
) -> Option<Response> {
    let mut trailing_response = None;
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).size(17.0).strong().color(theme.text));
        if let Some(label) = trailing {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                trailing_response =
                    Some(ui.link(RichText::new(label).size(12.0).color(theme.accent)));
            });
        }
    });
    ui.add_space(2.0);
    trailing_response
}

pub fn empty_state(
    ui: &mut Ui,
    theme: &Theme,
    icon: Icon,
    title: &str,
    body: &str,
    action: Option<&str>,
) -> Option<Response> {
    let mut action_response = None;
    ui.vertical_centered(|ui| {
        ui.add_space(10.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(96.0, 78.0), Sense::hover());
        let painter = ui.painter();
        let card = Rect::from_center_size(rect.center(), Vec2::splat(58.0));
        painter.rect_filled(card, CornerRadius::same(18), wash(theme));
        for (index, color) in theme.macaron.iter().enumerate() {
            let angle = index as f32 * 1.7 + 0.4;
            painter.circle_filled(
                rect.center() + Vec2::new(angle.cos(), angle.sin()) * 34.0,
                3.0,
                with_alpha(*color, 220),
            );
        }
        paint_icon(painter, icon, card.shrink(16.0), theme.accent);
        ui.add_space(6.0);
        ui.label(RichText::new(title).size(15.5).strong().color(theme.text));
        ui.add_space(2.0);
        ui.label(RichText::new(body).size(12.5).color(theme.text_weak));
        if let Some(label) = action {
            ui.add_space(10.0);
            action_response = Some(button(ui, theme, label, Variant::Primary));
        }
        ui.add_space(6.0);
    });
    action_response
}

/// An icon button at an explicit rectangle (used by manually laid out rows).
pub fn icon_button_at(
    ui: &mut Ui,
    rect: Rect,
    id_salt: impl std::hash::Hash,
    theme: &Theme,
    icon: Icon,
    active: bool,
    tooltip: &str,
) -> Response {
    let response = ui.interact(rect, ui.id().with(id_salt), Sense::click());
    let t = ui
        .ctx()
        .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.12);
    let bg = if active {
        mix(theme.accent_soft, theme.accent, 0.10 * t)
    } else {
        with_alpha(theme.surface_hover, (210.0 * t) as u8)
    };
    if bg.a() > 0 {
        ui.painter().rect_filled(rect, CornerRadius::same(R_SM), bg);
    }
    let color = if active {
        theme.accent
    } else {
        mix(theme.text_weak, theme.text, t)
    };
    let inner = rect.shrink(rect.width().min(rect.height()) * 0.24);
    paint_icon(ui.painter(), icon, inner, color);
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    if tooltip.is_empty() {
        response
    } else {
        response.on_hover_text(tooltip)
    }
}

/// Frame used for dropdown menus.
pub fn menu_frame(theme: &Theme) -> egui::Frame {
    egui::Frame::new()
        .fill(theme.surface)
        .stroke(Stroke::new(1.0_f32, theme.border))
        .corner_radius(CornerRadius::same(R_MD))
        .inner_margin(Margin::same(6))
        .shadow(shadow(theme))
}

/// One row inside a dropdown menu.
pub fn menu_item(ui: &mut Ui, theme: &Theme, icon: Icon, label: &str, danger: bool) -> Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 30.0), Sense::click());
    let t = ui
        .ctx()
        .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.10);
    let fg = if danger {
        danger_color(theme)
    } else {
        mix(theme.text, theme.accent, t * 0.25)
    };
    if response.hovered() {
        let fill = if danger {
            with_alpha(danger_color(theme), 22)
        } else {
            with_alpha(theme.accent_soft, 150)
        };
        ui.painter().rect_filled(rect, CornerRadius::same(6), fill);
    }
    let icon_rect = Rect::from_center_size(
        Pos2::new(rect.left() + 16.0, rect.center().y),
        Vec2::splat(16.0),
    );
    paint_icon(ui.painter(), icon, icon_rect, fg);
    paint_elided(
        ui.painter(),
        Pos2::new(rect.left() + 32.0, rect.center().y),
        label,
        font(12.5),
        fg,
        rect.width() - 40.0,
    );
    response.on_hover_cursor(CursorIcon::PointingHand)
}

pub fn menu_separator(ui: &mut Ui, theme: &Theme) {
    ui.add_space(3.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        Stroke::new(1.0_f32, theme.border),
    );
    ui.add_space(3.0);
}

pub fn menu_heading(ui: &mut Ui, theme: &Theme, label: &str) {
    ui.add_space(4.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 18.0), Sense::hover());
    paint_elided(
        ui.painter(),
        Pos2::new(rect.left() + 12.0, rect.center().y),
        label,
        font(10.5),
        theme.text_weak,
        rect.width() - 12.0,
    );
}

/// Draw content inside an already-allocated rectangle without letting the
/// content's natural size advance the parent cursor again.
pub fn absolute<R>(ui: &mut Ui, rect: Rect, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    let result = ui
        .scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
            ui.set_min_size(rect.size());
            add_contents(ui)
        })
        .inner;
    ui.advance_cursor_after_rect(rect);
    result
}

/// A labelled numeric statistic card with a soft macaron icon tile.
pub fn stat_card(ui: &mut Ui, theme: &Theme, icon: Icon, color: Color32, value: &str, label: &str) {
    egui::Frame::new()
        .fill(theme.surface)
        .stroke(Stroke::new(1.0_f32, theme.border))
        .corner_radius(CornerRadius::same(R_MD))
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(32.0), Sense::hover());
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(R_SM), with_alpha(color, 36));
                paint_icon(ui.painter(), icon, rect.shrink(8.0), color);
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    paint_elided(
                        ui.painter(),
                        Pos2::new(ui.cursor().min.x, ui.cursor().min.y + 8.0),
                        value,
                        font(16.0),
                        theme.text,
                        ui.available_width() - 4.0,
                    );
                    ui.add_space(17.0);
                    paint_elided(
                        ui.painter(),
                        Pos2::new(ui.cursor().min.x, ui.cursor().min.y + 4.0),
                        label,
                        font(10.5),
                        theme.text_weak,
                        ui.available_width() - 4.0,
                    );
                    ui.add_space(14.0);
                });
            });
        });
}

/// An iOS-style toggle switch. Returns the click response.
pub fn switch(ui: &mut Ui, theme: &Theme, on: bool, id_salt: impl std::hash::Hash) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(42.0, 24.0), Sense::click());
    let t = ui
        .ctx()
        .animate_bool_with_time(Id::new(("switch-on", &id_salt)), on, 0.14);
    let hover =
        ui.ctx()
            .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.10);
    let track_fill = mix(theme.surface_hover, theme.accent, t);
    let stroke_color = mix(theme.border, theme.accent, t * 0.8 + hover * 0.2);
    ui.painter()
        .rect_filled(rect, CornerRadius::same(12), track_fill);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(12),
        Stroke::new(1.0_f32, stroke_color),
        StrokeKind::Inside,
    );
    let knob_x = rect.left() + 12.0 + t * 18.0;
    ui.painter().circle_filled(
        Pos2::new(knob_x + 1.0, rect.center().y + 1.0),
        8.0,
        Color32::from_black_alpha(if theme.dark { 60 } else { 16 }),
    );
    let knob = if t > 0.5 {
        theme.surface
    } else {
        theme.surface
    };
    ui.painter()
        .circle_filled(Pos2::new(knob_x, rect.center().y), 8.0, knob);
    if on {
        paint_icon(
            ui.painter(),
            Icon::Check,
            Rect::from_center_size(Pos2::new(knob_x, rect.center().y), Vec2::splat(9.0)),
            theme.accent,
        );
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// Curated generated-cover gradient for a seed (used for missing artwork and
/// playlist covers). Keeps every cover inside the palette's colour family and
/// avoids random pastel pairs that read muddy.
pub fn generated_cover_colors(theme: &Theme, seed: &str) -> (Color32, Color32) {
    let bytes = crate::model::stable_hash(seed).into_bytes();
    let variant = bytes.first().copied().unwrap_or(0) as usize % 5;
    if theme.dark {
        // Rich, low-key covers that sit naturally on the dark canvas.
        let deep = theme.bg;
        match variant {
            0 => (
                mix(theme.accent, theme.panel, 0.18),
                mix(theme.accent, deep, 0.58),
            ),
            1 => (
                mix(theme.highlight, theme.panel, 0.10),
                mix(theme.accent, deep, 0.74),
            ),
            2 => (
                mix(theme.macaron[1], theme.panel, 0.26),
                mix(theme.accent, deep, 0.78),
            ),
            3 => (
                mix(theme.macaron[3], theme.panel, 0.20),
                mix(theme.highlight, deep, 0.62),
            ),
            _ => (
                mix(theme.macaron[2], theme.panel, 0.24),
                mix(theme.accent, deep, 0.52),
            ),
        }
    } else {
        match variant {
            0 => (
                theme::shade(theme.accent, -0.05),
                mix(theme.accent, theme.macaron[2], 0.68),
            ),
            1 => (
                theme::shade(theme.accent, -0.26),
                mix(theme.accent, theme.macaron[1], 0.58),
            ),
            2 => (
                mix(theme.accent, theme.macaron[3], 0.42),
                mix(theme.macaron[3], Color32::WHITE, 0.10),
            ),
            3 => (
                theme::shade(theme.accent, -0.12),
                mix(theme.highlight, theme.macaron[2], 0.58),
            ),
            _ => (
                theme::shade(theme.accent, -0.34),
                mix(theme.accent_soft, theme.accent, 0.34),
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// Small shared string helpers
// ---------------------------------------------------------------------------

/// "Welcome back, {name}" with locale-aware punctuation.
pub fn greeting(lang: crate::i18n::Language, base: &str, name: &str) -> String {
    match lang {
        crate::i18n::Language::Zh => format!("{base}，{name}"),
        crate::i18n::Language::En => format!("{base}, {name}"),
    }
}

/// "3 tracks" / "1 track" / "3 首".
pub fn tracks_count(state: &AppState, count: usize) -> String {
    let unit = if count == 1 {
        state.tr(Key::TrackSingular)
    } else {
        state.tr(Key::TracksCount)
    };
    format!("{count} {unit}")
}

/// Slightly stronger secondary text for use on colourful / pastel themes.
/// Guarantees a minimum contrast against the page background so theme-derived
/// palettes never wash the small labels out.
pub fn weak_text(theme: &Theme) -> Color32 {
    theme::readable_on(mix(theme.text_weak, theme.text, 0.45), theme.bg, 4.0)
}

/// A simple key/value row used on the profile and settings pages.
#[allow(dead_code)]
pub fn kv_row(ui: &mut Ui, theme: &Theme, key: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(key).size(12.0).color(theme.text_weak));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(value).size(12.5).color(theme.text));
        });
    });
}

/// Tooltip-friendly hint line with a small info dot.
pub fn hint_line(ui: &mut Ui, theme: &Theme, text: &str) {
    ui.horizontal_wrapped(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(12.0, 14.0), Sense::hover());
        paint_icon(ui.painter(), Icon::Info, rect, theme.text_weak);
        ui.label(RichText::new(text).size(11.5).color(theme.text_weak));
    });
}

/// Convert a unix timestamp to `YYYY-MM-DD` without pulling in a date crate.
pub fn format_date(unix_secs: u64) -> String {
    let days = (unix_secs / 86_400) as i64;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// Human friendly duration like "3 h 12 min" / "3 小时 12 分钟".
pub fn coarse_duration(state: &AppState, secs: f64) -> String {
    let minutes = (secs / 60.0).round() as u64;
    let hour = state.tr(Key::DurationHour);
    let minute = state.tr(Key::DurationMinute);
    if minutes >= 60 {
        format!("{} {hour} {} {minute}", minutes / 60, minutes % 60)
    } else {
        format!("{minutes} {minute}")
    }
}

/// A tiny pulsing "loading" row used while remote work is in flight.
pub fn loading_pulse(ui: &mut Ui, theme: &Theme, label: &str, time: f64) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(34.0, 16.0), Sense::hover());
        for i in 0..3 {
            let phase = time as f32 * 3.2 - i as f32 * 0.5;
            let alpha = (0.35 + 0.65 * (0.5 + 0.5 * phase.sin())) * 255.0;
            ui.painter().circle_filled(
                Pos2::new(rect.left() + 6.0 + i as f32 * 11.0, rect.center().y),
                3.0,
                with_alpha(theme.accent, alpha as u8),
            );
        }
        ui.label(RichText::new(label).size(12.5).color(theme.text_weak));
    });
}

/// A button painted at an exact rectangle. Returns whether it was clicked.
pub fn button_at(
    ui: &mut Ui,
    rect: Rect,
    id_salt: impl std::hash::Hash,
    theme: &Theme,
    label: &str,
    variant: Variant,
) -> bool {
    let response = ui.interact(rect, ui.id().with(id_salt), Sense::click());
    let t = ui
        .ctx()
        .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.12);
    let (bg, fg, border) = variant_colors(theme, variant, t);
    if bg.a() > 0 {
        ui.painter().rect_filled(rect, CornerRadius::same(R_MD), bg);
    }
    if border.a() > 0 {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(R_MD),
            Stroke::new(1.0_f32, border),
            StrokeKind::Inside,
        );
    }
    let galley = elided(ui.painter(), label, font(12.5), fg, rect.width() - 12.0);
    ui.painter().galley(
        Pos2::new(
            rect.center().x - galley.size().x * 0.5,
            rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        fg,
    );
    if response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    response.clicked()
}

/// Like [`button_at`] but with an icon followed by a label, painted at an
/// exact rectangle. Returns whether it was clicked.
pub fn button_icon_at(
    ui: &mut Ui,
    rect: Rect,
    id_salt: impl std::hash::Hash,
    theme: &Theme,
    icon: Icon,
    label: &str,
    variant: Variant,
) -> bool {
    let response = ui.interact(rect, ui.id().with(id_salt), Sense::click());
    let t = ui
        .ctx()
        .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.12);
    let (bg, fg, border) = variant_colors(theme, variant, t);
    if bg.a() > 0 {
        ui.painter().rect_filled(rect, CornerRadius::same(R_MD), bg);
    }
    if border.a() > 0 {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(R_MD),
            Stroke::new(1.0_f32, border),
            StrokeKind::Inside,
        );
    }
    let font = font(13.0);
    let text_w = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font.clone(), fg)
        .size()
        .x;
    let icon_w = 16.0;
    let gap = 7.0;
    let total = icon_w + gap + text_w;
    let start = rect.center().x - total * 0.5;
    paint_icon(
        ui.painter(),
        icon,
        Rect::from_center_size(
            Pos2::new(start + icon_w * 0.5, rect.center().y),
            Vec2::splat(icon_w),
        ),
        fg,
    );
    let galley = elided(
        ui.painter(),
        label,
        font,
        fg,
        rect.right() - (start + icon_w + gap) - 4.0,
    );
    ui.painter().galley(
        Pos2::new(
            start + icon_w + gap,
            rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        fg,
    );
    if response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    response.clicked()
}
