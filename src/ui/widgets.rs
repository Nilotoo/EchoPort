//! Reusable interactive rows and cards shared across pages (track lists,
//! playlist rows, media cards and their action menus).

use std::path::PathBuf;

use egui::{
    Align2, Color32, CornerRadius, CursorIcon, Layout, Pos2, Rect, Response, RichText, Sense,
    Stroke, StrokeKind, Ui, Vec2,
};

use crate::app::EchoPortApp;
use crate::i18n::Key;
use crate::model::{format_duration, Artist, Platform, Playlist, Profile, Track};
use crate::state::{Command, NowPlayingTab, Page};
use crate::theme::{self, Theme};

use super::style::{self, Icon};
use super::{Dialog, UiState};

// ---------------------------------------------------------------------------
// Cover helpers
// ---------------------------------------------------------------------------

/// Resolve the cover path for a track without doing any network or disk IO
/// (the app pre-fetches covers on worker threads).
pub fn cover_for(app: &EchoPortApp, track: &Track) -> Option<PathBuf> {
    let is_current = app
        .state
        .now_playing
        .as_ref()
        .map(|t| t.id == track.id)
        .unwrap_or(false);
    if is_current && app.state.cover.track_id.as_deref() == Some(track.id.as_str()) {
        app.state.cover.path.clone()
    } else {
        // Lists/cards use the low-res rendition when it is cached; the
        // NowPlaying view keeps the full-resolution cover.
        app.art
            .cached_thumb(track)
            .or_else(|| app.art.cached(track))
    }
}

pub fn paint_track_cover(
    ui: &Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    track: &Track,
    rect: Rect,
    radius: u8,
) {
    let path = cover_for(app, track);
    app.ui.covers.paint(
        ui,
        theme,
        path.as_deref(),
        rect,
        radius,
        &track.title,
        &track.id,
    );
}

/// Best-effort human page for the current track: the stored original page,
/// otherwise a platform search URL for "artist title".
pub fn track_source_url(track: &Track) -> Option<String> {
    if let Some(url) = track.page_url.as_deref().map(str::trim) {
        if !url.is_empty() {
            return Some(url.to_owned());
        }
    }
    let query = format!("{} {}", track.display_artist(), track.title);
    let q = url_encode(&query);
    let url = match track.platform {
        crate::model::Platform::YouTube => {
            format!("https://www.youtube.com/results?search_query={q}")
        }
        crate::model::Platform::Netease => format!("https://music.163.com/#/search/m/?s={q}"),
        crate::model::Platform::SoundCloud => format!("https://soundcloud.com/search?q={q}"),
        crate::model::Platform::Bilibili => format!("https://search.bilibili.com/all?keyword={q}"),
        crate::model::Platform::Spotify => format!("https://open.spotify.com/search/{q}"),
        crate::model::Platform::AppleMusic => {
            format!("https://music.apple.com/us/search?term={q}")
        }
        crate::model::Platform::Local => return None,
    };
    Some(url)
}

fn url_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// A source-page shortcut button (replaces the old shuffle slot): opens a small
/// menu with "Copy link" / "Open original page" for the current track.
pub fn source_page_button_at(
    ui: &mut Ui,
    rect: Rect,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
    track: Option<&Track>,
) -> Response {
    let size = rect.width().min(rect.height());
    let url = track.and_then(track_source_url);
    let mut response = ui.interact(
        rect,
        ui.id()
            .with(("source-page", rect.min.x.to_bits(), rect.min.y.to_bits())),
        Sense::click(),
    );
    let enabled = url.is_some();
    let t = ui.ctx().animate_bool_with_time(
        response.id.with("hover"),
        response.hovered() && enabled,
        0.12,
    );
    if enabled && t > 0.01 {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(style::R_SM),
            theme::with_alpha(theme.surface_hover, (210.0 * t) as u8),
        );
    }
    let color = if enabled {
        theme::mix(theme.text_weak, theme.text, t)
    } else {
        theme::with_alpha(theme.text_weak, 110)
    };
    style::paint_icon(
        ui.painter(),
        Icon::LinkShare,
        rect.shrink(size * 0.24),
        color,
    );
    response = response.on_hover_cursor(if enabled {
        CursorIcon::PointingHand
    } else {
        CursorIcon::Default
    });
    if !enabled {
        return response.on_hover_text(app.state.tr(Key::ErrorNoSelection));
    }
    let Some(url) = url else {
        return response;
    };
    let current_track = track.cloned();
    let popup = egui::Popup::from_toggle_button_response(&response)
        .kind(egui::PopupKind::Menu)
        .layout(Layout::top_down_justified(egui::Align::Min))
        .style(egui::containers::menu::menu_style)
        .frame(style::menu_frame(theme))
        .width(206.0)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClick);

    popup.show(|ui| {
        ui.set_width(206.0);
        if style::menu_item(ui, theme, Icon::Copy, app.state.tr(Key::CopyLink), false).clicked() {
            pending.push(Command::CopyText(url.clone()));
        }
        if style::menu_item(
            ui,
            theme,
            Icon::External,
            app.state.tr(Key::OpenOriginalPage),
            false,
        )
        .clicked()
        {
            pending.push(Command::OpenUrl(url.clone()));
        }
        let _ = &current_track;
    });
    response
}

/// A horizontally-centred slot in a fixed-height transport row: guarantees all
/// buttons share the play button's horizontal centre line.
fn centered_slot(ui: &mut Ui, size: f32, row_h: f32) -> Rect {
    let (slot, _) = ui.allocate_exact_size(Vec2::new(size, row_h), Sense::hover());
    Rect::from_center_size(slot.center(), Vec2::splat(size))
}

/// Transport cluster shared by the bottom player bar and the NowPlaying page:
/// source · prev · play · next · repeat · shuffle, all vertically centred on one
/// row line (`row_h`) so different icon sizes never look top-aligned.
#[allow(clippy::too_many_arguments)]
pub fn transport_row(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
    playing: bool,
    salt: &'static str,
    row_h: f32,
    sizes: &[f32; 6],
    gaps: &[f32; 5],
) {
    ui.spacing_mut().item_spacing.x = 0.0;
    let row_h = row_h.max(sizes.iter().copied().fold(0.0_f32, f32::max));
    let track = app.state.now_playing.clone();
    let rect = centered_slot(ui, sizes[0], row_h);
    source_page_button_at(ui, rect, app, theme, pending, track.as_ref());
    ui.add_space(gaps[0]);
    let rect = centered_slot(ui, sizes[1], row_h);
    if style::icon_button_at(
        ui,
        rect,
        ("transport-prev", salt),
        theme,
        Icon::Previous,
        false,
        "",
    )
    .on_hover_text(app.state.tr(Key::Previous))
    .clicked()
    {
        pending.push(Command::Previous);
    }
    ui.add_space(gaps[1]);
    let rect = centered_slot(ui, sizes[2], row_h);
    if style::play_button_at(ui, rect, theme, playing).clicked() {
        pending.push(Command::TogglePlayPause);
    }
    ui.add_space(gaps[2]);
    let rect = centered_slot(ui, sizes[3], row_h);
    if style::icon_button_at(
        ui,
        rect,
        ("transport-next", salt),
        theme,
        Icon::Next,
        false,
        "",
    )
    .on_hover_text(app.state.tr(Key::Next))
    .clicked()
    {
        pending.push(Command::Next);
    }
    ui.add_space(gaps[3]);
    let repeat_icon = match app.state.repeat {
        crate::model::RepeatMode::One => Icon::RepeatOne,
        _ => Icon::RepeatAll,
    };
    let repeat_active = app.state.repeat != crate::model::RepeatMode::Off;
    let tooltip = match app.state.repeat {
        crate::model::RepeatMode::Off => Key::RepeatOff,
        crate::model::RepeatMode::All => Key::RepeatAll,
        crate::model::RepeatMode::One => Key::RepeatOne,
    };
    let rect = centered_slot(ui, sizes[4], row_h);
    if style::icon_button_at(
        ui,
        rect,
        ("transport-repeat", salt),
        theme,
        repeat_icon,
        repeat_active,
        "",
    )
    .on_hover_text(app.state.tr(tooltip))
    .clicked()
    {
        pending.push(Command::CycleRepeat);
    }
    ui.add_space(gaps[4]);
    let rect = centered_slot(ui, sizes[5], row_h);
    if style::icon_button_at(
        ui,
        rect,
        ("transport-shuffle", salt),
        theme,
        Icon::Shuffle,
        app.state.shuffle,
        "",
    )
    .on_hover_text(app.state.tr(Key::Shuffle))
    .clicked()
    {
        pending.push(Command::ToggleShuffle);
    }
}

/// Decoded avatar texture for a profile's `data:image/png;base64,…` data URL.
/// Decoding happens once and the texture is cached in egui memory.
fn profile_avatar_texture(ctx: &egui::Context, data_url: &str) -> Option<egui::TextureHandle> {
    use base64::Engine as _;
    let key = egui::Id::new((
        "echoport-profile-avatar",
        crate::model::stable_hash(data_url),
    ));
    if let Some(texture) = ctx.data(|data| data.get_temp::<egui::TextureHandle>(key)) {
        return Some(texture);
    }
    let payload = data_url
        .split_once(',')
        .map(|(_, rest)| rest)
        .unwrap_or(data_url);
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload.trim())
        .ok()?;
    let image = image::load_from_memory(&bytes).ok()?.into_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    let texture = ctx.load_texture("profile-avatar", color, egui::TextureOptions::LINEAR);
    ctx.data_mut(|data| data.insert_temp(key, texture.clone()));
    Some(texture)
}

/// Profile avatar: decoded picture when available, otherwise the deterministic
/// pastel colour + initial fallback.
pub fn paint_profile_avatar(ui: &Ui, theme: &Theme, profile: &Profile, rect: Rect) {
    if let Some(texture) = profile
        .avatar
        .as_deref()
        .and_then(|data_url| profile_avatar_texture(ui.ctx(), data_url))
    {
        let radius = CornerRadius::same((rect.width() * 0.32) as u8);
        let uv = Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0));
        ui.painter().add(
            egui::epaint::RectShape::filled(rect, radius, Color32::WHITE)
                .with_texture(texture.id(), uv),
        );
        ui.painter().rect_stroke(
            rect,
            radius,
            Stroke::new(1.0_f32, theme::with_alpha(theme.border, 140)),
            StrokeKind::Inside,
        );
        return;
    }
    style::avatar(ui.painter(), rect, &profile.name, theme);
}

/// Path of a cached artist avatar, if the provider already downloaded one.
/// Only a cheap local metadata check happens here (no network IO in layout).
pub fn artist_avatar_path(app: &EchoPortApp, artist: &Artist) -> Option<PathBuf> {
    let url = artist.image_url.as_deref()?.trim();
    if url.is_empty() {
        return None;
    }
    app.art.cached_url(url)
}

/// Circular artist avatar: cached image when available, otherwise a calm
/// accent-tinted circle with the artist's initial.
pub fn paint_artist_avatar(
    ui: &Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    artist: &Artist,
    rect: Rect,
) {
    let size = rect.width().min(rect.height());
    let radius = (size * 0.5).round().clamp(1.0, 255.0) as u8;
    if let Some(path) = artist_avatar_path(app, artist) {
        app.ui.covers.paint(
            ui,
            theme,
            Some(&path),
            rect,
            radius,
            &artist.name,
            &artist.id,
        );
        return;
    }
    ui.painter().circle_filled(
        rect.center(),
        size * 0.5,
        theme::with_alpha(theme.accent_soft, 210),
    );
    ui.painter().circle_stroke(
        rect.center(),
        size * 0.5,
        Stroke::new(1.0_f32, theme::with_alpha(theme.border, 160)),
    );
    let letter = artist
        .name
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();
    if !letter.is_empty() {
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            letter,
            style::font((size * 0.40).max(10.0)),
            theme::readable_on(theme.text, theme.accent_soft, 4.0),
        );
    }
}

/// Cached cover path for an album (no network IO).
pub fn album_cover_path(app: &EchoPortApp, album: &crate::model::Album) -> Option<PathBuf> {
    album
        .art_url
        .as_deref()
        .and_then(|url| app.art.cached_url(url))
}

/// Album card used in search results, the album page series and favorites.
pub fn album_card(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
    album: &crate::model::Album,
    width: f32,
) {
    let width = width.max(180.0);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 82.0), Sense::click());
    let hover =
        ui.ctx()
            .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.12);
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(style::R_MD),
        theme::mix(theme.surface, theme.surface_hover, hover),
    );
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(style::R_MD),
        Stroke::new(
            1.0_f32,
            theme::mix(theme.border, theme.accent, 0.22 * hover),
        ),
        StrokeKind::Inside,
    );
    let cover = Rect::from_min_size(
        Pos2::new(rect.left() + 10.0, rect.center().y - 27.0),
        Vec2::splat(54.0),
    );
    if let Some(path) = album_cover_path(app, album) {
        app.ui.covers.paint(
            ui,
            theme,
            Some(&path),
            cover,
            style::R_SM,
            &album.title,
            &album.id,
        );
    } else {
        let (a, b) = style::generated_cover_colors(theme, &album.id);
        style::gradient_cover(
            ui.painter(),
            cover,
            style::R_SM,
            a,
            b,
            Some(&album.title.chars().next().unwrap_or('♫').to_string()),
        );
    }
    let text_left = cover.right() + 10.0;
    let text_right = rect.right() - 42.0;
    style::paint_elided(
        ui.painter(),
        Pos2::new(text_left, rect.center().y - 17.0),
        &album.title,
        style::font(12.5),
        theme.text,
        (text_right - text_left).max(20.0),
    );
    style::paint_elided(
        ui.painter(),
        Pos2::new(text_left, rect.center().y - 1.0),
        &album.artist,
        style::font(10.5),
        theme.text_weak,
        (text_right - text_left).max(20.0),
    );
    let meta = match (album.year, album.track_count) {
        (Some(year), Some(count)) => format!("{year} · {count}"),
        (Some(year), None) => format!("{year}"),
        (None, Some(count)) => format!("{count}"),
        _ => album.platform.label().to_owned(),
    };
    style::paint_elided(
        ui.painter(),
        Pos2::new(text_left, rect.center().y + 15.0),
        &meta,
        style::font(9.5),
        style::weak_text(theme),
        (text_right - text_left).max(20.0),
    );
    let favorite = app.state.profile.is_favorite_album(&album.id);
    let heart = Rect::from_center_size(
        Pos2::new(rect.right() - 22.0, rect.center().y),
        Vec2::splat(28.0),
    );
    if style::icon_button_at(
        ui,
        heart,
        ("album-favorite", album.id.clone(), favorite),
        theme,
        if favorite {
            Icon::HeartFill
        } else {
            Icon::Heart
        },
        favorite,
        app.state.tr(if favorite {
            Key::FavoriteAlbums
        } else {
            Key::AlbumFavorited
        }),
    )
    .clicked()
    {
        pending.push(Command::ToggleFavoriteAlbum(Box::new(album.clone())));
    }
    if response
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(app.state.tr(Key::AlbumPage))
        .clicked()
    {
        pending.push(Command::OpenAlbum(Box::new(album.clone())));
    }
}

/// Open the now-playing page for a track.
#[allow(dead_code)]
pub fn open_now_playing(app: &mut EchoPortApp) {
    app.set_page(Page::NowPlaying);
}

/// Queue command for "play this item from this visible list".
pub fn play_from(list: &[Track], index: usize) -> Command {
    Command::PlayTracksFrom {
        tracks: list.to_vec(),
        index,
    }
}

// ---------------------------------------------------------------------------
// Track row
// ---------------------------------------------------------------------------

pub struct TrackRow<'a> {
    /// The visible list this row belongs to; used for "play from here".
    pub list: &'a [Track],
    pub index: usize,
    /// Set when the row lives inside a playlist (enables "remove").
    pub playlist_id: Option<&'a str>,
    /// Extra id salt so the same track can appear on several pages/sections
    /// without clashing egui widget ids.
    pub salt: &'a str,
    pub height: f32,
    pub show_cover: bool,
    pub show_album: bool,
}

impl<'a> TrackRow<'a> {
    pub fn new(list: &'a [Track], index: usize) -> Self {
        Self {
            list,
            index,
            playlist_id: None,
            salt: "list",
            height: 52.0,
            show_cover: true,
            show_album: true,
        }
    }

    pub fn compact(list: &'a [Track], index: usize) -> Self {
        Self {
            list,
            index,
            playlist_id: None,
            salt: "list",
            height: 44.0,
            show_cover: true,
            show_album: false,
        }
    }

    pub fn playlist(mut self, playlist_id: &'a str) -> Self {
        self.playlist_id = Some(playlist_id);
        self
    }

    pub fn salt(mut self, salt: &'a str) -> Self {
        self.salt = salt;
        self
    }
}

pub fn track_row(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
    track: &Track,
    row: TrackRow<'_>,
) {
    let width = ui.available_width();
    let height = row.height;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let hovered = ui.rect_contains_pointer(rect);
    let time = ui.ctx().input(|input| input.time);
    let hover_t = ui.ctx().animate_bool_with_time(
        ui.id()
            .with(("track-row-hover", track.id.as_str(), row.salt, row.index)),
        hovered,
        0.10,
    );
    let is_current = app
        .state
        .now_playing
        .as_ref()
        .map(|current| current.id == track.id)
        .unwrap_or(false);
    let playing = is_current && app.state.playback.state.is_playing();
    let selected = app.ui.selected_track.as_deref() == Some(track.id.as_str());
    let row_fill = if is_current {
        theme::with_alpha(theme.accent_soft, 170)
    } else if selected {
        theme::with_alpha(theme.accent_soft, 96)
    } else if hover_t > 0.01 {
        theme::with_alpha(theme.surface_hover, (235.0 * hover_t) as u8)
    } else {
        Color32::TRANSPARENT
    };
    if row_fill.a() > 0 {
        ui.painter().rect_filled(
            rect.shrink2(Vec2::new(2.0, 1.0)),
            CornerRadius::same(style::R_SM),
            row_fill,
        );
    }
    if selected && !is_current {
        ui.painter().rect_stroke(
            rect.shrink2(Vec2::new(2.0, 1.0)),
            CornerRadius::same(style::R_SM),
            Stroke::new(1.0_f32, theme::with_alpha(theme.accent, 120)),
            StrokeKind::Inside,
        );
    }
    // Whole-row single click = select/highlight, double click = play.
    let body = ui.interact(
        rect,
        ui.id()
            .with(("track-row-body", track.id.as_str(), row.salt, row.index)),
        Sense::click(),
    );

    let mut left = rect.left() + 8.0;
    // --- play / index cell -------------------------------------------------
    let cell = Rect::from_min_size(Pos2::new(left, rect.top()), Vec2::new(30.0, height));
    let play_response = ui.interact(
        cell,
        ui.id()
            .with(("track-row-play", track.id.as_str(), row.salt, row.index)),
        Sense::click(),
    );
    let cell_hover = play_response.hovered();
    if playing {
        style::equalizer(
            ui.painter(),
            Rect::from_center_size(cell.center(), Vec2::new(15.0, 15.0)),
            theme,
            true,
            time,
        );
    } else if is_current && app.state.playback.state == crate::model::PlaybackState::Loading {
        style::loading_dots(
            ui.painter(),
            Rect::from_center_size(cell.center(), Vec2::new(16.0, 13.0)),
            theme,
            time,
        );
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(90));
    } else if cell_hover || is_current {
        style::paint_icon(
            ui.painter(),
            if is_current && app.state.playback.state == crate::model::PlaybackState::Playing {
                Icon::Pause
            } else {
                Icon::Play
            },
            cell.shrink(8.0),
            theme.accent,
        );
    } else {
        ui.painter().text(
            cell.center(),
            Align2::CENTER_CENTER,
            format!("{}", row.index + 1),
            style::font(11.5),
            theme.text_weak,
        );
    }
    if play_response.clicked() {
        if is_current {
            pending.push(Command::TogglePlayPause);
        } else {
            pending.push(play_from(row.list, row.index));
        }
    }
    left += 30.0;

    // --- cover -------------------------------------------------------------
    if row.show_cover {
        let cover_rect =
            Rect::from_center_size(Pos2::new(left + 19.0, rect.center().y), Vec2::splat(38.0));
        let cover_response = ui.interact(
            cover_rect,
            ui.id()
                .with(("track-row-cover", track.id.as_str(), row.salt, row.index)),
            Sense::click(),
        );
        paint_track_cover(ui, app, theme, track, cover_rect, style::R_SM);
        if cover_response.clicked() {
            if is_current {
                pending.push(Command::TogglePlayPause);
            } else {
                pending.push(play_from(row.list, row.index));
            }
        }
        left += 38.0 + 10.0;
    }

    // --- right hand side widgets ------------------------------------------
    let more_rect = Rect::from_center_size(
        Pos2::new(rect.right() - 8.0 - 13.0, rect.center().y),
        Vec2::splat(26.0),
    );
    let more_response = style::icon_button_at(
        ui,
        more_rect,
        ("track-row-more", track.id.as_str(), row.salt, row.index),
        theme,
        Icon::More,
        false,
        "",
    );
    more_menu(ui, app, theme, pending, track, &row, &more_response);

    let heart_rect = Rect::from_center_size(
        Pos2::new(more_rect.left() - 4.0 - 13.0, rect.center().y),
        Vec2::splat(26.0),
    );
    let favorite = app.state.is_favorite(&track.id);
    let heart_response = ui.interact(
        heart_rect,
        ui.id()
            .with(("track-row-fav", track.id.as_str(), row.salt, row.index)),
        Sense::click(),
    );
    let heart_t = ui.ctx().animate_bool_with_time(
        heart_response.id.with("hover"),
        heart_response.hovered(),
        0.12,
    );
    if heart_response.hovered() {
        ui.painter().rect_filled(
            heart_rect,
            CornerRadius::same(style::R_SM),
            theme::with_alpha(theme.surface_hover, 210),
        );
    }
    let heart_color = if favorite {
        theme.accent
    } else {
        theme::mix(theme.text_weak, theme.text, heart_t)
    };
    style::paint_icon(
        ui.painter(),
        if favorite {
            Icon::HeartFill
        } else {
            Icon::Heart
        },
        heart_rect.shrink(7.0),
        heart_color,
    );
    let heart_response = heart_response
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(app.state.tr(if favorite {
            Key::RemoveFromFavorites
        } else {
            Key::AddToFavorites
        }));
    if heart_response.clicked() {
        pending.push(Command::ToggleFavorite(Box::new(track.clone())));
    }

    let duration_right = heart_rect.left() - 12.0;
    let duration_text = track
        .duration_secs
        .map(format_duration)
        .unwrap_or_else(|| "--:--".to_owned());
    style::paint_elided_right(
        ui.painter(),
        Pos2::new(duration_right, rect.center().y),
        &duration_text,
        style::font(11.5),
        theme.text_weak,
        52.0,
    );

    // --- title / artist (clickable) ---------------------------------------
    let text_left = left;
    let text_right = duration_right - 56.0;
    if text_right - text_left > 24.0 {
        let text_rect = Rect::from_min_max(
            Pos2::new(text_left, rect.top()),
            Pos2::new(text_right, rect.bottom()),
        );
        let text_response = ui.interact(
            text_rect,
            ui.id()
                .with(("track-row-title", track.id.as_str(), row.salt, row.index)),
            Sense::click(),
        );
        let title_center_y = if row.show_album {
            rect.center().y - 8.0
        } else {
            rect.center().y - 5.5
        };
        style::paint_elided(
            ui.painter(),
            Pos2::new(text_left, title_center_y),
            &track.title,
            style::font(13.5),
            if is_current { theme.accent } else { theme.text },
            text_right - text_left,
        );
        let artist = if track.artist.trim().is_empty() {
            app.state.tr(Key::UnknownArtist)
        } else {
            &track.artist
        };
        let mut subtitle = String::from(artist);
        if row.show_album {
            if let Some(album) = &track.album {
                subtitle.push_str("  ·  ");
                subtitle.push_str(album);
            }
            subtitle.push_str("  ·  ");
            subtitle.push_str(track.platform.label());
        } else {
            subtitle.push_str("  ·  ");
            subtitle.push_str(track.platform.label());
        }
        style::paint_elided(
            ui.painter(),
            Pos2::new(text_left, title_center_y + 17.0),
            &subtitle,
            style::font(11.0),
            theme.text_weak,
            text_right - text_left,
        );
        // Platform colour dot at the start of the subtitle.
        if text_response.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        if text_response.clicked() {
            app.ui.selected_track = Some(track.id.clone());
            app.ui.selected_playlist = None;
        }
        if text_response.double_clicked() {
            if is_current {
                pending.push(Command::TogglePlayPause);
            } else {
                pending.push(play_from(row.list, row.index));
            }
        }
    }
    // Row-level hit area (children above still win for their own clicks).
    let body = body
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(app.state.tr(Key::DoubleClickPlay));
    if body.clicked() {
        app.ui.selected_track = Some(track.id.clone());
        app.ui.selected_playlist = None;
    }
    if body.double_clicked() {
        if is_current {
            pending.push(Command::TogglePlayPause);
        } else {
            pending.push(play_from(row.list, row.index));
        }
    }
}

// ---------------------------------------------------------------------------
// Row action menu
// ---------------------------------------------------------------------------

fn more_menu(
    _ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
    track: &Track,
    row: &TrackRow<'_>,
    button_response: &Response,
) {
    let popup = egui::Popup::from_toggle_button_response(button_response)
        .kind(egui::PopupKind::Menu)
        .layout(Layout::top_down_justified(egui::Align::Min))
        .style(egui::containers::menu::menu_style)
        .frame(style::menu_frame(theme))
        .width(224.0)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClick);
    let favorite = app.state.is_favorite(&track.id);
    popup.show(|ui| {
        ui.set_width(224.0);
        if style::menu_item(ui, theme, Icon::Play, app.state.tr(Key::Play), false).clicked() {
            pending.push(play_from(row.list, row.index));
        }
        if style::menu_item(ui, theme, Icon::Next, app.state.tr(Key::PlayNext), false).clicked() {
            pending.push(Command::PlayNext(Box::new(track.clone())));
        }
        if style::menu_item(ui, theme, Icon::Queue, app.state.tr(Key::AddToQueue), false).clicked()
        {
            pending.push(Command::AddToQueue(vec![track.clone()]));
        }
        style::menu_separator(ui, theme);
        style::menu_heading(ui, theme, app.state.tr(Key::AddToPlaylist));
        let playlists = app.state.profile.playlists.clone();
        let mut shown = 0;
        for playlist in &playlists {
            if shown >= 5 {
                break;
            }
            if style::menu_item(ui, theme, Icon::Playlist, &playlist.name, false).clicked() {
                pending.push(Command::AddToPlaylist {
                    playlist_id: playlist.id.clone(),
                    tracks: vec![track.clone()],
                });
            }
            shown += 1;
        }
        if style::menu_item(ui, theme, Icon::Plus, app.state.tr(Key::NewPlaylist), false).clicked()
        {
            app.ui.dialog = Some(Dialog::NewPlaylist {
                seed: vec![track.clone()],
                name: String::new(),
            });
        }
        style::menu_separator(ui, theme);
        if style::menu_item(
            ui,
            theme,
            Icon::Download,
            app.state.tr(Key::Download),
            false,
        )
        .clicked()
        {
            pending.push(Command::Download(vec![track.clone()]));
        }
        if style::menu_item(
            ui,
            theme,
            if favorite {
                Icon::HeartFill
            } else {
                Icon::Heart
            },
            app.state.tr(if favorite {
                Key::RemoveFromFavorites
            } else {
                Key::AddToFavorites
            }),
            false,
        )
        .clicked()
        {
            pending.push(Command::ToggleFavorite(Box::new(track.clone())));
        }
        if let Some(url) = &track.page_url {
            if style::menu_item(
                ui,
                theme,
                Icon::External,
                app.state.tr(Key::OpenSourcePage),
                false,
            )
            .clicked()
            {
                pending.push(Command::OpenUrl(url.clone()));
            }
            if style::menu_item(ui, theme, Icon::Copy, app.state.tr(Key::CopyLink), false).clicked()
            {
                pending.push(Command::CopyText(url.clone()));
            }
        }
        if let Some(playlist_id) = row.playlist_id {
            style::menu_separator(ui, theme);
            if style::menu_item(
                ui,
                theme,
                Icon::Trash,
                app.state.tr(Key::RemoveFromPlaylist),
                true,
            )
            .clicked()
            {
                pending.push(Command::RemoveFromPlaylist {
                    playlist_id: playlist_id.to_owned(),
                    track_id: track.id.clone(),
                });
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Media card (recently played / collections)
// ---------------------------------------------------------------------------

pub fn media_card(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
    track: &Track,
    list: &[Track],
    index: usize,
    width: f32,
) {
    let cover_size = width;
    let height = cover_size + 40.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    let hover_t =
        ui.ctx()
            .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.15);
    let lift = 3.0 * hover_t;
    let card = rect.translate(Vec2::new(0.0, -lift));
    if hover_t > 0.01 {
        ui.painter().rect_filled(
            card.expand(2.0).translate(Vec2::new(0.0, 3.0)),
            CornerRadius::same(style::R_LG + 2),
            Color32::from_black_alpha((16.0 * hover_t) as u8),
        );
    }
    let cover_rect = Rect::from_min_size(card.min, Vec2::splat(cover_size));
    paint_track_cover(ui, app, theme, track, cover_rect, style::R_LG);
    if hover_t > 0.01 {
        ui.painter().rect_filled(
            cover_rect,
            CornerRadius::same(style::R_LG),
            Color32::from_black_alpha((38.0 * hover_t) as u8),
        );
        let button = Rect::from_center_size(cover_rect.center(), Vec2::splat(44.0));
        ui.painter().circle_filled(
            button.center(),
            22.0,
            theme::with_alpha(theme.accent, (235.0 * hover_t) as u8),
        );
        style::paint_icon(
            ui.painter(),
            Icon::Play,
            button.shrink(11.0),
            theme.on_accent,
        );
    }
    let is_current = app
        .state
        .now_playing
        .as_ref()
        .map(|current| current.id == track.id)
        .unwrap_or(false);
    let selected = app.ui.selected_track.as_deref() == Some(track.id.as_str());
    if selected {
        ui.painter().rect_stroke(
            cover_rect,
            CornerRadius::same(style::R_LG),
            Stroke::new(1.5_f32, theme.accent),
            StrokeKind::Inside,
        );
    }
    style::paint_elided(
        ui.painter(),
        Pos2::new(card.left() + 2.0, cover_rect.bottom() + 15.0),
        &track.title,
        style::font(12.5),
        if is_current { theme.accent } else { theme.text },
        card.width() - 4.0,
    );
    let subtitle = if track.artist.trim().is_empty() {
        app.state.tr(Key::UnknownArtist).to_owned()
    } else {
        track.artist.clone()
    };
    style::paint_elided(
        ui.painter(),
        Pos2::new(card.left() + 2.0, cover_rect.bottom() + 30.0),
        &subtitle,
        style::font(11.0),
        theme.text_weak,
        card.width() - 4.0,
    );
    let response = response
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(app.state.tr(Key::DoubleClickPlay));
    if response.clicked() {
        app.ui.selected_track = Some(track.id.clone());
        app.ui.selected_playlist = None;
    }
    if response.double_clicked() {
        if is_current {
            pending.push(Command::TogglePlayPause);
        } else {
            pending.push(play_from(list, index));
        }
    }
}

// ---------------------------------------------------------------------------
// Playlist rows
// ---------------------------------------------------------------------------

fn playlist_glyph(playlist: &Playlist) -> String {
    playlist
        .name
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "♫".to_owned())
}

/// A rich playlist row used on the playlists page (left column).
/// Cover for a playlist: a user-picked image, otherwise the first track's
/// cached cover. No network IO happens in layout.
pub fn playlist_cover_path(app: &EchoPortApp, playlist: &Playlist) -> Option<PathBuf> {
    if let Some(path) = playlist.cover_path.as_ref() {
        if path.is_file() {
            return Some(path.clone());
        }
    }
    let first = app
        .state
        .profile
        .playlist_tracks(&playlist.id)
        .into_iter()
        .next()?;
    cover_for(app, &first)
}

pub fn playlist_list_row(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    playlist: &Playlist,
    subtitle: &str,
    selected: bool,
) -> Response {
    let height = 62.0;
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    let hover_t =
        ui.ctx()
            .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.10);
    let fill = if selected {
        theme::with_alpha(theme.accent_soft, 190)
    } else if hover_t > 0.01 {
        theme::with_alpha(theme.surface_hover, (255.0 * hover_t) as u8)
    } else {
        Color32::TRANSPARENT
    };
    if fill.a() > 0 {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(style::R_MD), fill);
    }
    if selected {
        ui.painter().rect_filled(
            Rect::from_min_size(
                Pos2::new(rect.left(), rect.center().y - 13.0),
                Vec2::new(3.0, 26.0),
            ),
            CornerRadius::same(2),
            theme.accent,
        );
    }
    let cover = Rect::from_min_size(
        Pos2::new(rect.left() + 10.0, rect.center().y - 21.0),
        Vec2::splat(42.0),
    );
    if let Some(path) = playlist_cover_path(app, playlist) {
        app.ui.covers.paint(
            ui,
            theme,
            Some(&path),
            cover,
            style::R_SM,
            &playlist.name,
            &playlist.id,
        );
    } else {
        let (a, b) = style::generated_cover_colors(theme, &playlist.id);
        style::gradient_cover(
            ui.painter(),
            cover,
            style::R_SM,
            a,
            b,
            Some(&playlist_glyph(playlist)),
        );
    }
    let text_left = cover.right() + 12.0;
    let text_right = rect.right() - 12.0;
    style::paint_elided(
        ui.painter(),
        Pos2::new(text_left, rect.center().y - 10.0),
        &playlist.name,
        style::font(13.0),
        theme.text,
        text_right - text_left,
    );
    style::paint_elided(
        ui.painter(),
        Pos2::new(text_left, rect.center().y + 10.0),
        subtitle,
        style::font(11.0),
        theme.text_weak,
        text_right - text_left,
    );
    if response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    response
}

/// Compact playlist row used inside the home "Your playlists" card.
pub fn playlist_home_row(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    playlist: &Playlist,
    subtitle: &str,
    selected: bool,
) -> Response {
    let height = 48.0;
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    let hover_t =
        ui.ctx()
            .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.10);
    let fill = if selected {
        theme::with_alpha(theme.accent_soft, 150)
    } else if hover_t > 0.01 {
        theme::with_alpha(theme.surface_hover, (255.0 * hover_t) as u8)
    } else {
        Color32::TRANSPARENT
    };
    if fill.a() > 0 {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(style::R_SM), fill);
    }
    if selected {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(style::R_SM),
            Stroke::new(1.0_f32, theme::with_alpha(theme.accent, 120)),
            StrokeKind::Inside,
        );
    }
    let cover = Rect::from_center_size(
        Pos2::new(rect.left() + 20.0, rect.center().y),
        Vec2::splat(36.0),
    );
    if let Some(path) = playlist_cover_path(app, playlist) {
        app.ui.covers.paint(
            ui,
            theme,
            Some(&path),
            cover,
            style::R_SM,
            &playlist.name,
            &playlist.id,
        );
    } else {
        let (a, b) = style::generated_cover_colors(theme, &playlist.id);
        style::gradient_cover(
            ui.painter(),
            cover,
            style::R_SM,
            a,
            b,
            Some(&playlist_glyph(playlist)),
        );
    }
    let text_left = cover.right() + 10.0;
    style::paint_elided(
        ui.painter(),
        Pos2::new(text_left, rect.center().y - 8.0),
        &playlist.name,
        style::font(12.5),
        theme.text,
        rect.right() - text_left - 8.0,
    );
    style::paint_elided(
        ui.painter(),
        Pos2::new(text_left, rect.center().y + 9.0),
        subtitle,
        style::font(10.5),
        theme.text_weak,
        rect.right() - text_left - 8.0,
    );
    if response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    response
}

/// A small source shortcut chip row item used on Home: platform dot + name.
#[allow(dead_code)]
pub fn source_chip(ui: &mut Ui, theme: &Theme, platform: Platform, status: &str) -> Response {
    let label = platform.label();
    let font = style::font(12.5);
    let label_width = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font.clone(), theme.text)
        .size()
        .x;
    let status_width = ui
        .painter()
        .layout_no_wrap(status.to_owned(), style::font(10.0), theme.text_weak)
        .size()
        .x;
    let width = label_width.max(status_width) + 34.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 44.0), Sense::click());
    let hover_t =
        ui.ctx()
            .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.12);
    let fill = theme::mix(theme.surface, theme.surface_hover, hover_t);
    ui.painter()
        .rect_filled(rect, CornerRadius::same(style::R_MD), fill);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(style::R_MD),
        Stroke::new(
            1.0_f32,
            theme::mix(theme.border, theme.accent, 0.25 * hover_t),
        ),
        StrokeKind::Inside,
    );
    let dot = Pos2::new(rect.left() + 14.0, rect.center().y);
    ui.painter()
        .circle_filled(dot, 4.5, theme::platform_color(platform));
    style::paint_elided(
        ui.painter(),
        Pos2::new(dot.x + 10.0, rect.center().y - 7.0),
        label,
        font,
        theme.text,
        rect.width() - 34.0,
    );
    style::paint_elided(
        ui.painter(),
        Pos2::new(dot.x + 10.0, rect.center().y + 8.0),
        status,
        style::font(10.0),
        theme.text_weak,
        rect.width() - 34.0,
    );
    if response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    response
}

/// Ensure a page-level scroll area pushes the current page correctly.
pub fn page_scroll<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    // Reserve room for the vertical scrollbar so cards never slide under it.
    let width = (ui.available_width() - 14.0).max(240.0);
    egui::ScrollArea::vertical()
        .id_salt("page-scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_min_width(width);
            ui.set_max_width(width);
            ui.spacing_mut().item_spacing.y = 6.0;
            add_contents(ui)
        })
        .inner
}

// ---------------------------------------------------------------------------
// Queue rows (used in the queue overlay)
// ---------------------------------------------------------------------------

pub fn queue_row(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
    queue: &[Track],
    index: usize,
) -> Response {
    let track = &queue[index];
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 42.0), Sense::click());
    let current = app
        .state
        .now_playing
        .as_ref()
        .map(|now| now.id == track.id)
        .unwrap_or(false);
    let hover_t =
        ui.ctx()
            .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.10);
    if current {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(style::R_SM),
            theme::with_alpha(theme.accent_soft, 170),
        );
    } else if hover_t > 0.01 {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(style::R_SM),
            theme::with_alpha(theme.surface_hover, (200.0 * hover_t) as u8),
        );
    }
    let cover_rect = Rect::from_center_size(
        Pos2::new(rect.left() + 24.0, rect.center().y),
        Vec2::splat(32.0),
    );
    paint_track_cover(ui, app, theme, track, cover_rect, 7);
    let text_left = cover_rect.right() + 9.0;
    let text_right = rect.right() - 34.0;
    style::paint_elided(
        ui.painter(),
        Pos2::new(text_left, rect.center().y - 7.0),
        &track.title,
        style::font(12.0),
        if current { theme.accent } else { theme.text },
        text_right - text_left,
    );
    style::paint_elided(
        ui.painter(),
        Pos2::new(text_left, rect.center().y + 8.0),
        track.display_artist(),
        style::font(10.5),
        theme.text_weak,
        text_right - text_left,
    );
    let remove_rect = Rect::from_center_size(
        Pos2::new(rect.right() - 18.0, rect.center().y),
        Vec2::splat(24.0),
    );
    let remove_response = style::icon_button_at(
        ui,
        remove_rect,
        ("queue-remove", index),
        theme,
        Icon::Close,
        false,
        app.state.tr(Key::RemoveFromQueue),
    );
    if remove_response.clicked() {
        pending.push(Command::RemoveFromQueue(index));
    } else if response.clicked() {
        pending.push(play_from(queue, index));
    }
    response
}

/// A rounded, clickable "chip-like" text button used inside cards.
pub fn inline_link(ui: &mut Ui, theme: &Theme, label: &str) -> Response {
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), style::font(12.0), theme.accent);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(galley.size().x + 4.0, 20.0), Sense::click());
    let t = ui
        .ctx()
        .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.10);
    let color = theme::mix(theme.accent, theme.text, 0.35 * t);
    ui.painter().galley(
        Pos2::new(rect.left() + 2.0, rect.center().y - galley.size().y * 0.5),
        galley,
        color,
    );
    if t > 0.01 {
        ui.painter().hline(
            rect.x_range(),
            rect.bottom() - 3.0,
            Stroke::new(1.0_f32, theme::with_alpha(color, (180.0 * t) as u8)),
        );
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// Rich "now playing" shortcut card used in the sidebar footer.
pub fn sidebar_now_playing(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
) {
    let Some(track) = app.state.now_playing.clone() else {
        return;
    };
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 52.0), Sense::click());
    let hover_t =
        ui.ctx()
            .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.12);
    let fill = theme::mix(style::wash(theme), theme.surface, hover_t * 0.5);
    ui.painter()
        .rect_filled(rect, CornerRadius::same(style::R_MD), fill);
    let cover_rect = Rect::from_center_size(
        Pos2::new(rect.left() + 24.0, rect.center().y),
        Vec2::splat(34.0),
    );
    paint_track_cover(ui, app, theme, &track, cover_rect, style::R_SM);
    let time = ui.ctx().input(|input| input.time);
    let play_state = app.state.playback.state;
    let badge_rect = Rect::from_min_size(
        Pos2::new(cover_rect.right() + 8.0, rect.center().y - 7.0),
        Vec2::new(13.0, 14.0),
    );
    match play_state {
        crate::model::PlaybackState::Playing => {
            style::equalizer(ui.painter(), badge_rect, theme, true, time);
        }
        crate::model::PlaybackState::Loading => {
            style::loading_dots(ui.painter(), badge_rect, theme, time);
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(90));
        }
        crate::model::PlaybackState::Error => {
            style::paint_icon(
                ui.painter(),
                Icon::Info,
                badge_rect.shrink(2.0),
                style::danger_color(theme),
            );
        }
        _ => {}
    }
    let text_left = cover_rect.right()
        + (if matches!(
            play_state,
            crate::model::PlaybackState::Playing
                | crate::model::PlaybackState::Loading
                | crate::model::PlaybackState::Error
        ) {
            26.0
        } else {
            9.0
        });
    style::paint_elided(
        ui.painter(),
        Pos2::new(text_left, rect.center().y - 7.0),
        &track.title,
        style::font(11.5),
        theme.text,
        rect.right() - text_left - 8.0,
    );
    style::paint_elided(
        ui.painter(),
        Pos2::new(text_left, rect.center().y + 7.5),
        track.display_artist(),
        style::font(10.0),
        theme.text_weak,
        rect.right() - text_left - 8.0,
    );
    if response.clicked() {
        app.set_page(Page::NowPlaying);
        app.state.now_playing_tab = NowPlayingTab::Track;
    }
    let _ = pending;
    if response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
}

/// A tiny helper row: icon + title + subtitle + trailing chevron, used by
/// settings / profile cards.
#[allow(dead_code)]
pub fn link_row(ui: &mut Ui, theme: &Theme, icon: Icon, title: &str, subtitle: &str) -> Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 44.0), Sense::click());
    let hover_t =
        ui.ctx()
            .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.10);
    if hover_t > 0.01 {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(style::R_SM),
            theme::with_alpha(theme.surface_hover, (230.0 * hover_t) as u8),
        );
    }
    let icon_rect = Rect::from_center_size(
        Pos2::new(rect.left() + 18.0, rect.center().y),
        Vec2::splat(17.0),
    );
    style::paint_icon(ui.painter(), icon, icon_rect, theme.accent);
    style::paint_elided(
        ui.painter(),
        Pos2::new(rect.left() + 36.0, rect.center().y - 7.0),
        title,
        style::font(12.5),
        theme.text,
        rect.width() - 90.0,
    );
    if !subtitle.is_empty() {
        style::paint_elided(
            ui.painter(),
            Pos2::new(rect.left() + 36.0, rect.center().y + 8.0),
            subtitle,
            style::font(10.5),
            theme.text_weak,
            rect.width() - 90.0,
        );
    }
    style::paint_icon(
        ui.painter(),
        Icon::ChevronRight,
        Rect::from_center_size(
            Pos2::new(rect.right() - 18.0, rect.center().y),
            Vec2::splat(14.0),
        ),
        theme.text_weak,
    );
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// Format a list of platform labels for a card subtitle.
pub fn platforms_label(tracks: &[Track]) -> String {
    let mut seen: Vec<Platform> = Vec::new();
    for track in tracks {
        if !seen.contains(&track.platform) {
            seen.push(track.platform);
        }
    }
    seen.iter()
        .take(3)
        .map(|platform| platform.label())
        .collect::<Vec<_>>()
        .join(" · ")
}

/// A non-interactive hint box.
pub fn callout(ui: &mut Ui, theme: &Theme, icon: Icon, text: &str, accent: bool) {
    let width = ui.available_width();
    let mut frame = egui::Frame::new()
        .fill(if accent {
            style::wash(theme)
        } else {
            theme.surface
        })
        .stroke(Stroke::new(
            1.0_f32,
            theme::mix(theme.border, theme.accent, if accent { 0.25 } else { 0.0 }),
        ))
        .corner_radius(CornerRadius::same(style::R_MD))
        .inner_margin(egui::Margin::symmetric(12, 10));
    if !accent {
        frame = frame.fill(theme::mix(theme.surface, theme.surface_hover, 0.35));
    }
    frame.show(ui, |ui| {
        ui.set_width(width - 24.0);
        ui.horizontal_top(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(15.0, 18.0), Sense::hover());
            style::paint_icon(ui.painter(), icon, rect, theme.accent);
            ui.add_space(3.0);
            ui.label(RichText::new(text).size(11.5).color(theme.text_weak));
        });
    });
}

/// Whether the given text should be treated as a pasted link.
pub fn looks_like_url(text: &str) -> bool {
    let text = text.trim();
    text.starts_with("http://")
        || text.starts_with("https://")
        || text.starts_with("spotify:")
        || (text.contains("://") && !text.contains(' '))
}

// Keep clippy quiet about the helper being available for pages that use it
// conditionally (release build strips it).
#[allow(dead_code)]
pub fn dialog_open(ui_state: &UiState) -> bool {
    ui_state.dialog.is_some()
}
