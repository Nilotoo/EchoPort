//! Now Playing: the original quiet, minimal page. Plain theme background,
//! large cover on the left with the macaron palette underneath, and the
//! track controls / details on the right. No tinted backdrop, no decorative
//! animations — just functional loading indicators.

use egui::{Align, Align2, CornerRadius, Layout, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};

use crate::app::EchoPortApp;
use crate::i18n::Key;
use crate::model::{format_duration, LinkKind, PlaybackState, SongBackground, Track};
use crate::state::{Command, NowPlayingTab, Page};
use crate::theme::{self, Theme};
use crate::ui::style::{self, Icon, Variant};
use crate::ui::widgets;

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

pub fn draw(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme, pending: &mut Vec<Command>) {
    header(ui, app, theme, pending);
    ui.add_space(8.0);
    let Some(track) = app.state.now_playing.clone() else {
        style::empty_state(
            ui,
            theme,
            Icon::Music,
            app.state.tr(Key::EmptyStateHint),
            app.state.tr(Key::SearchTryHint),
            Some(app.state.tr(Key::SearchNav)),
        )
        .map(|response| {
            if response.clicked() {
                app.set_page(Page::Search);
            }
        });
        return;
    };
    match app.state.now_playing_tab {
        NowPlayingTab::Track => {
            if ui.available_height() < 520.0 {
                // Very small / zoomed windows: keep the page scrollable so the
                // note and info block never run under the player bar.
                egui::ScrollArea::vertical()
                    .id_salt("np-track-scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        track_view(ui, app, theme, pending, &track);
                        ui.add_space(12.0);
                    });
            } else {
                track_view(ui, app, theme, pending, &track);
            }
        }
        NowPlayingTab::Background => background_view(ui, app, theme, pending, &track),
    }
}

fn header(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme, pending: &mut Vec<Command>) {
    ui.horizontal(|ui| {
        if style::button_icon(
            ui,
            theme,
            Icon::ChevronLeft,
            app.state.tr(Key::GoBack),
            Variant::Ghost,
        )
        .clicked()
        {
            app.set_page(Page::Home);
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let selected = match app.state.now_playing_tab {
                NowPlayingTab::Track => 0,
                NowPlayingTab::Background => 1,
            };
            if let Some(index) = style::segmented(
                ui,
                theme,
                &[
                    app.state.tr(Key::TrackTab),
                    app.state.tr(Key::BackgroundTab),
                ],
                selected,
                30.0,
            ) {
                let next = if index == 0 {
                    NowPlayingTab::Track
                } else {
                    NowPlayingTab::Background
                };
                app.state.now_playing_tab = next;
                if next == NowPlayingTab::Background {
                    let needs_load = app.state.background.track_id.as_deref()
                        != app
                            .state
                            .now_playing
                            .as_ref()
                            .map(|track| track.id.as_str());
                    if needs_load {
                        if let Some(track) = app.state.now_playing.clone() {
                            pending.push(Command::LoadBackground(Box::new(track)));
                        }
                    }
                }
            }
        });
    });
}

fn current_cover_path(app: &EchoPortApp) -> Option<std::path::PathBuf> {
    if app.state.cover.track_id.as_deref()
        == app
            .state
            .now_playing
            .as_ref()
            .map(|track| track.id.as_str())
    {
        app.state.cover.path.clone()
    } else {
        None
    }
}

fn cover_size(ui: &Ui) -> f32 {
    // Original v0.1.0 sizing: quiet and compact even on very large windows.
    let available_h = ui.available_height() - 84.0;
    let available_w = ui.available_width() * 0.44;
    available_h.clamp(190.0, 336.0).min(available_w.max(190.0))
}

/// Gentle scale for the sparse typography on very large windows.
fn page_scale(ui: &Ui) -> f32 {
    (ui.available_height() / 720.0).clamp(0.9, 1.15)
}

// ---------------------------------------------------------------------------
// Track tab
// ---------------------------------------------------------------------------

fn track_view(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
    track: &Track,
) {
    let size = cover_size(ui);
    let available_height = ui.available_height();
    let available_width = ui.available_width();
    let scale = page_scale(ui);
    // Keep cover and info column aligned and vertically balanced on tall
    // (fullscreen) windows without changing the v0.1.0 proportions.
    let group_h = (size + 44.0).max(430.0);
    let lead = ((available_height - group_h) * 0.5).clamp(0.0, 260.0);
    // Lyrics column (when enabled) takes the empty right-hand area; the detail
    // column shrinks a little so the three columns stay on one row.
    let lyrics_on = app.state.profile.settings.lyrics_enabled;
    let lyrics_w = if lyrics_on {
        (available_width * 0.30).clamp(240.0, 620.0)
    } else {
        0.0
    };
    let lyrics_gap = if lyrics_on { 24.0 } else { 0.0 };
    let details_w = (available_width - size - 34.0 - lyrics_w - lyrics_gap).clamp(240.0, 620.0);
    let group_w = 6.0 + size + 34.0 + details_w + lyrics_gap + lyrics_w;
    let side = ((available_width - group_w) * 0.5).max(0.0);
    // When there is room, centre the transport on the content column so its
    // play button lines up with the bottom player bar; otherwise keep the
    // classic left-aligned cluster.
    let content_left = ui.cursor().min.x;
    let content_center = content_left + available_width * 0.5;
    let cover_right = content_left + side + 6.0 + size;
    let cluster_half = 105.0;
    let target = content_center - cluster_half;
    // Prefer the exact content-column centre; when the cover leaves no room
    // (narrow/zoomed windows) get as close to it as possible without
    // overlapping the artwork.
    let best_start = target.max(cover_right + 12.0);
    let fits = best_start + cluster_half * 2.0 <= content_left + available_width - 2.0;
    let transport_x = if lyrics_on {
        None
    } else {
        fits.then_some(best_start)
    };
    ui.horizontal_top(|ui| {
        ui.add_space(side + 6.0);
        ui.vertical(|ui| {
            ui.add_space(lead);
            cover_column(ui, app, theme, track, size);
        });
        ui.add_space(34.0);
        ui.vertical(|ui| {
            ui.set_max_width(details_w);
            ui.add_space(lead);
            track_details(ui, app, theme, pending, track, scale, transport_x);
        });
        if lyrics_on {
            ui.add_space(lyrics_gap);
            ui.vertical(|ui| {
                ui.set_width(lyrics_w);
                ui.add_space(lead.max(4.0));
                lyrics_panel(
                    ui,
                    app,
                    theme,
                    pending,
                    track,
                    lyrics_w,
                    available_height,
                    scale,
                );
            });
        }
    });
}

fn lyrics_panel(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
    track: &Track,
    width: f32,
    height: f32,
    scale: f32,
) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height.max(260.0)), Sense::hover());
    let header = Rect::from_min_max(rect.min, Pos2::new(rect.right(), rect.top() + 30.0));
    ui.painter().text(
        Pos2::new(header.left(), header.center().y),
        Align2::LEFT_CENTER,
        app.state.tr(Key::Lyrics),
        style::font(13.0 * scale),
        style::weak_text(theme),
    );
    if let Some(data) = app.state.lyrics.data.as_ref() {
        ui.painter().text(
            Pos2::new(header.right(), header.center().y),
            Align2::RIGHT_CENTER,
            &data.source,
            style::font(9.0),
            style::weak_text(theme),
        );
    }
    let content = Rect::from_min_max(
        Pos2::new(rect.left(), header.bottom() + 8.0),
        Pos2::new(rect.right(), rect.bottom()),
    );
    let painter = ui.painter().with_clip_rect(content);
    let centered = |text: &str, color: egui::Color32, y: f32, size: f32| {
        painter.text(
            Pos2::new(content.center().x, y),
            Align2::CENTER_CENTER,
            text,
            style::font(size),
            color,
        );
    };
    let lyric_center_y = content.top() + content.height() * 0.26;
    if app.state.lyrics.loading {
        centered(
            app.state.tr(Key::LyricsLoading),
            style::weak_text(theme),
            lyric_center_y,
            13.0,
        );
        return;
    }
    let Some(data) = app.state.lyrics.data.as_ref() else {
        let message = app
            .state
            .lyrics
            .error
            .as_deref()
            .unwrap_or_else(|| app.state.tr(Key::NoLyrics));
        centered(message, style::weak_text(theme), lyric_center_y, 12.0);
        return;
    };
    if data.lines.is_empty() && data.plain.is_empty() {
        centered(
            app.state.tr(Key::NoLyrics),
            style::weak_text(theme),
            lyric_center_y,
            12.0,
        );
        return;
    }

    let position = app
        .ui
        .scrub_preview
        .unwrap_or(app.state.playback.position_secs);
    let max_text_width = (content.width() - 18.0).max(80.0);
    if !data.lines.is_empty() {
        let active = data.active_line(position);
        let target = active.unwrap_or(0) as f32;
        if app.ui.lyrics_track.as_deref() != Some(track.id.as_str()) {
            app.ui.lyrics_scroll_pos = target;
        }
        let dt = ui.ctx().input(|input| input.stable_dt).min(0.1);
        let diff = target - app.ui.lyrics_scroll_pos;
        app.ui.lyrics_scroll_pos += diff * (dt * 11.0).min(1.0);
        if diff.abs() > 0.015 {
            ui.ctx().request_repaint();
        }
        app.ui.lyrics_track = Some(track.id.clone());

        let current = active.unwrap_or(0) as isize;
        let base_line_h = (content.height() / 6.0).clamp(34.0, 54.0) * scale;
        let shift = (current as f32 - app.ui.lyrics_scroll_pos) * base_line_h;
        let current_center = lyric_center_y + shift;
        let mut rows: Vec<(i32, std::sync::Arc<egui::Galley>, egui::Color32)> = Vec::new();
        for offset in -2i32..=2 {
            let index = current + offset as isize;
            if index < 0 || index as usize >= data.lines.len() {
                continue;
            }
            let (size, alpha, color) = match offset.abs() {
                0 => (23.0 * scale, 1.0, theme.accent),
                1 => (16.0 * scale, 0.62, theme.text),
                _ => (12.5 * scale, 0.28, theme.text_weak),
            };
            let color = theme::with_alpha(color, (alpha * 255.0) as u8);
            let galley = painter.layout(
                data.lines[index as usize].text.clone(),
                style::font(size),
                color,
                max_text_width,
            );
            rows.push((offset, galley, color));
        }
        let Some(current_row) = rows.iter().find(|(offset, _, _)| *offset == 0) else {
            return;
        };
        let current_h = current_row.1.size().y;
        let mut clicked_seek = None;
        let mut above_bottom = current_center - current_h * 0.5 - 8.0;
        for (offset, galley, color) in rows.iter().filter(|(offset, _, _)| *offset < 0).rev() {
            let y = above_bottom - galley.size().y * 0.5;
            let pos = Pos2::new(
                content.center().x - galley.size().x * 0.5,
                y - galley.size().y * 0.5,
            );
            painter.galley(pos, galley.clone(), *color);
            let hit = Rect::from_center_size(
                Pos2::new(content.center().x, y),
                Vec2::new(content.width(), galley.size().y.max(base_line_h)),
            );
            let index = (current + *offset as isize) as usize;
            let response = ui.interact(
                hit,
                ui.id().with(("lyrics-line", track.id.as_str(), index)),
                Sense::click(),
            );
            if response.clicked() {
                clicked_seek = Some(data.lines[index].time_secs);
            }
            above_bottom = y - galley.size().y * 0.5 - 8.0;
        }
        let current_pos = Pos2::new(
            content.center().x - current_row.1.size().x * 0.5,
            current_center - current_h * 0.5,
        );
        painter.galley(current_pos, current_row.1.clone(), current_row.2);
        let mut below_top = current_center + current_h * 0.5 + 8.0;
        for (offset, galley, color) in rows.iter().filter(|(offset, _, _)| *offset > 0) {
            let y = below_top + galley.size().y * 0.5;
            let pos = Pos2::new(content.center().x - galley.size().x * 0.5, below_top);
            painter.galley(pos, galley.clone(), *color);
            let hit = Rect::from_center_size(
                Pos2::new(content.center().x, y),
                Vec2::new(content.width(), galley.size().y.max(base_line_h)),
            );
            let index = (current + *offset as isize) as usize;
            let response = ui.interact(
                hit,
                ui.id().with(("lyrics-line", track.id.as_str(), index)),
                Sense::click(),
            );
            if response.clicked() {
                clicked_seek = Some(data.lines[index].time_secs);
            }
            below_top += galley.size().y + 8.0;
        }
        if let Some(seconds) = clicked_seek {
            pending.push(Command::Seek(seconds));
        }
    } else {
        // Plain lyrics have no timestamps; wrap and centre the first five lines.
        let mut rows = Vec::new();
        for line in data.plain.iter().take(5) {
            let galley = painter.layout(
                line.clone(),
                style::font(14.0 * scale),
                theme.text,
                max_text_width,
            );
            rows.push(galley);
        }
        if rows.is_empty() {
            return;
        }
        let total_h: f32 = rows.iter().map(|row| row.size().y).sum::<f32>()
            + 10.0 * (rows.len().saturating_sub(1) as f32);
        let mut y = lyric_center_y - total_h * 0.5;
        for galley in rows {
            let pos = Pos2::new(content.center().x - galley.size().x * 0.5, y);
            painter.galley(pos, galley.clone(), theme.text);
            y += galley.size().y + 10.0;
        }
    }
}

fn cover_column(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme, track: &Track, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let path = current_cover_path(app);
    app.ui.covers.paint_fading(
        ui,
        theme,
        path.as_deref(),
        rect,
        style::R_XL,
        &track.title,
        &track.id,
        &track.id,
    );
    if app.state.cover.loading {
        let time = ui.ctx().input(|input| input.time) as f32;
        let alpha = (110.0 + 90.0 * (time * 3.0).sin()) as u8;
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(style::R_XL),
            Stroke::new(1.5_f32, theme::with_alpha(theme.accent, alpha)),
            egui::StrokeKind::Inside,
        );
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(90));
    }
    ui.add_space(14.0);
    // Macaron palette dots (from the cover when available) plus the release
    // date (or just the dots when the date is unknown).
    let palette = app.state.cover.palette;
    let dots = palette
        .map(|palette| palette.macarons)
        .unwrap_or(theme.macaron);
    ui.horizontal(|ui| {
        ui.add_space(2.0);
        for color in dots {
            let (dot, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
            ui.painter().circle_filled(dot.center(), 4.0, color);
            ui.painter().circle_stroke(
                dot.center(),
                5.5,
                Stroke::new(1.0_f32, theme::with_alpha(theme.border, 120)),
            );
            ui.add_space(2.0);
        }
        if let Some(label) = track.release_label() {
            ui.add_space(8.0);
            style::paint_elided(
                ui.painter(),
                Pos2::new(ui.cursor().min.x, ui.cursor().min.y + 8.0),
                &format!("{} {}", app.state.tr(Key::Released), label),
                style::font(10.0),
                style::weak_text(theme),
                220.0,
            );
        }
        ui.add_space(10.0);
    });
}

fn track_details(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
    track: &Track,
    scale: f32,
    transport_x: Option<f32>,
) {
    // Source + album/year.
    ui.horizontal(|ui| {
        let (dot, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
        ui.painter()
            .circle_filled(dot.center(), 4.0, theme::platform_color(track.platform));
        ui.add_space(2.0);
        ui.label(
            RichText::new(track.platform.label())
                .size(11.0 * scale)
                .color(style::weak_text(theme)),
        );
        if let Some(album) = &track.album {
            ui.label(
                RichText::new("·")
                    .size(11.0 * scale)
                    .color(style::weak_text(theme)),
            );
            ui.label(
                RichText::new(album)
                    .size(11.0 * scale)
                    .color(style::weak_text(theme)),
            );
        }
        if let Some(year) = track.year {
            ui.label(
                RichText::new(format!("· {year}"))
                    .size(11.0 * scale)
                    .color(style::weak_text(theme)),
            );
        }
    });
    ui.add_space(6.0);
    ui.add(
        egui::Label::new(
            RichText::new(&track.title)
                .size(30.0 * scale)
                .color(theme.text),
        )
        .wrap(),
    );
    ui.add_space(2.0);
    ui.label(
        RichText::new(track.display_artist())
            .size(15.0 * scale)
            .color(style::weak_text(theme)),
    );
    // Playback state stays consistent with the player bar/sidebar without
    // adding a loud pill: only a quiet loading/error line.
    match app.state.playback.state {
        PlaybackState::Loading => {
            let time = ui.ctx().input(|input| input.time);
            ui.horizontal(|ui| {
                let (dots, _) = ui.allocate_exact_size(Vec2::new(26.0, 14.0), Sense::hover());
                style::loading_dots(ui.painter(), dots, theme, time);
                ui.add_space(2.0);
                ui.label(
                    RichText::new(app.state.tr(Key::LoadingAudio))
                        .size(10.0)
                        .color(style::weak_text(theme)),
                );
            });
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(90));
        }
        PlaybackState::Error => {
            ui.label(
                RichText::new(
                    app.state
                        .playback
                        .error
                        .clone()
                        .unwrap_or_else(|| app.state.tr(Key::PlaybackError).to_owned()),
                )
                .size(10.0)
                .color(style::danger_color(theme)),
            );
        }
        _ => {}
    }
    ui.add_space(12.0);

    // Transport controls (same cluster as the bottom player bar).
    let playing = app.state.playback.state.is_playing();
    let sizes = [32.0, 36.0, 54.0, 36.0, 32.0, 32.0];
    let gaps = [4.0, 6.0, 6.0, 6.0, 4.0];
    let cluster_w: f32 = sizes.iter().sum::<f32>() + gaps.iter().sum::<f32>();
    if let Some(x) = transport_x {
        // Align the page's play button with the bottom bar transport while
        // keeping the normal vertical flow (no double cursor advance).
        let top = ui.cursor().min.y;
        let cluster = Rect::from_min_size(Pos2::new(x, top), Vec2::new(cluster_w, sizes[2]));
        ui.scope_builder(egui::UiBuilder::new().max_rect(cluster), |ui| {
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                widgets::transport_row(
                    ui, app, theme, pending, playing, "np", sizes[2], &sizes, &gaps,
                )
            });
        });
    } else {
        ui.horizontal(|ui| {
            widgets::transport_row(
                ui, app, theme, pending, playing, "np", sizes[2], &sizes, &gaps,
            )
        });
    }
    ui.add_space(8.0);

    // Scrubber.
    let duration = app
        .state
        .playback
        .duration_secs
        .or(track.duration_secs)
        .unwrap_or(0.0);
    let position = app
        .ui
        .scrub_preview
        .unwrap_or(app.state.playback.position_secs);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format_duration(position))
                .size(10.5)
                .color(style::weak_text(theme)),
        );
        ui.add_space(8.0);
        let width = (ui.available_width() - 64.0).max(120.0);
        let (response, new_position) = ui
            .allocate_ui_with_layout(
                Vec2::new(width, 18.0),
                Layout::left_to_right(Align::Center),
                |ui| style::scrubber(ui, theme, position, duration, "np-scrub"),
            )
            .inner;
        let pointer_down = ui.input(|input| input.pointer.primary_down());
        if response.dragged() {
            app.ui.scrub_preview = new_position;
        } else if app.ui.scrub_preview.is_some() && !pointer_down {
            if let Some(seconds) = new_position.or(app.ui.scrub_preview) {
                pending.push(Command::Seek(seconds));
            }
            app.ui.scrub_preview = None;
        } else if response.clicked() {
            if let Some(seconds) = new_position {
                pending.push(Command::Seek(seconds));
            }
        }
        ui.add_space(8.0);
        ui.label(
            RichText::new(format_duration(duration))
                .size(10.5)
                .color(style::weak_text(theme)),
        );
    });
    ui.add_space(10.0);

    // Actions.
    ui.horizontal(|ui| {
        let favorite = app.state.is_favorite(&track.id);
        if style::icon_only(
            ui,
            theme,
            if favorite {
                Icon::HeartFill
            } else {
                Icon::Heart
            },
            34.0,
            if favorite {
                Variant::Tonal
            } else {
                Variant::Secondary
            },
            app.state.tr(if favorite {
                Key::RemoveFromFavorites
            } else {
                Key::AddToFavorites
            }),
        )
        .clicked()
        {
            pending.push(Command::ToggleFavorite(Box::new(track.clone())));
        }
        if style::icon_only(
            ui,
            theme,
            Icon::Download,
            34.0,
            Variant::Secondary,
            app.state.tr(Key::Download),
        )
        .clicked()
        {
            pending.push(Command::Download(vec![track.clone()]));
        }
        if style::icon_only(
            ui,
            theme,
            Icon::Queue,
            34.0,
            Variant::Secondary,
            app.state.tr(Key::AddToQueue),
        )
        .clicked()
        {
            pending.push(Command::AddToQueue(vec![track.clone()]));
        }
        if track.page_url.is_some()
            && style::icon_only(
                ui,
                theme,
                Icon::External,
                34.0,
                Variant::Secondary,
                app.state.tr(Key::OpenSourcePage),
            )
            .clicked()
        {
            if let Some(url) = &track.page_url {
                pending.push(Command::OpenUrl(url.clone()));
            }
        }
        if style::icon_only(
            ui,
            theme,
            Icon::Info,
            34.0,
            Variant::Secondary,
            app.state.tr(Key::SongBackground),
        )
        .clicked()
        {
            pending.push(Command::LoadBackground(Box::new(track.clone())));
        }
    });
    ui.add_space(16.0);

    // Info block, matching the original minimal look.
    let details_width = ui.available_width().min(420.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(details_width, 76.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(style::R_MD), style::wash(theme));
    let cell_w = rect.width() / 2.0;
    let details = [
        (
            app.state.tr(Key::AlbumLabel),
            track
                .album
                .clone()
                .unwrap_or_else(|| app.state.tr(Key::UnknownAlbum).to_owned()),
        ),
        (app.state.tr(Key::DurationLabel), format_duration(duration)),
        (
            app.state.tr(Key::SourceLabel),
            track.platform.label().to_owned(),
        ),
        (
            app.state.tr(Key::YearLabel),
            track
                .year
                .map(|year| year.to_string())
                .unwrap_or_else(|| "—".to_owned()),
        ),
    ];
    for (index, (label, value)) in details.iter().enumerate() {
        let column = index % 2;
        let row = index / 2;
        let x = rect.left() + 16.0 + column as f32 * cell_w;
        let y = rect.top() + 22.0 + row as f32 * 34.0;
        style::paint_elided(
            ui.painter(),
            Pos2::new(x, y - 8.0),
            label,
            style::font(9.5),
            style::weak_text(theme),
            cell_w - 24.0,
        );
        style::paint_elided(
            ui.painter(),
            Pos2::new(x, y + 7.0),
            value,
            style::font(11.5),
            theme.text,
            cell_w - 24.0,
        );
    }
    ui.add_space(8.0);
    ui.label(
        RichText::new(app.state.tr(Key::MatchedAudioNote))
            .size(9.5)
            .color(style::weak_text(theme)),
    );
}

// ---------------------------------------------------------------------------
// Background tab
// ---------------------------------------------------------------------------

fn background_view(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
    track: &Track,
) {
    let size = cover_size(ui).min(248.0);
    let available_height = ui.available_height();
    ui.horizontal_top(|ui| {
        ui.add_space(6.0);
        ui.vertical(|ui| {
            ui.add_space(((available_height - size - 44.0) * 0.5).max(0.0));
            cover_column(ui, app, theme, track, size);
        });
        ui.add_space(34.0);
        let width = ui.available_width();
        ui.allocate_ui_with_layout(
            Vec2::new(width, available_height),
            Layout::top_down(Align::Min),
            |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("np-background")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.add_space(4.0);
                        background_content(ui, app, theme, pending, track);
                        ui.add_space(20.0);
                    });
            },
        );
    });
}

fn background_content(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
    track: &Track,
) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(app.state.tr(Key::SongBackground))
                .size(20.0)
                .color(theme.text),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if track.page_url.is_some()
                && style::button(ui, theme, app.state.tr(Key::SourcePage), Variant::Ghost).clicked()
            {
                if let Some(url) = &track.page_url {
                    pending.push(Command::OpenUrl(url.clone()));
                }
            }
        });
    });
    ui.add_space(8.0);
    if app.state.background.loading {
        let time = ui.ctx().input(|input| input.time);
        style::loading_pulse(ui, theme, app.state.tr(Key::BackgroundLoading), time);
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(90));
        return;
    }
    if let Some(error) = &app.state.background.error {
        widgets::callout(ui, theme, Icon::Info, error, false);
        ui.add_space(8.0);
    }
    let Some(data) = app.state.background.data.clone() else {
        style::empty_state(
            ui,
            theme,
            Icon::Info,
            app.state.tr(Key::BackgroundEmpty),
            app.state.tr(Key::SearchTryHint),
            None,
        );
        return;
    };
    let SongBackground {
        bio,
        album,
        song,
        links,
        videos,
        ..
    } = data;
    if let Some(bio) = bio {
        background_card(ui, theme, app.state.tr(Key::ArtistBio), |ui| {
            ui.label(RichText::new(&bio.title).size(15.0).color(theme.text));
            ui.add_space(4.0);
            ui.label(
                RichText::new(&bio.extract)
                    .size(12.0)
                    .color(style::weak_text(theme)),
            );
            if !bio.url.is_empty() {
                ui.add_space(8.0);
                if widgets::inline_link(ui, theme, app.state.tr(Key::OpenWikipedia)).clicked() {
                    pending.push(Command::OpenUrl(bio.url.clone()));
                }
            }
        });
    }
    if let Some(album) = album {
        background_card(ui, theme, app.state.tr(Key::AlbumBackground), |ui| {
            ui.label(RichText::new(&album.title).size(15.0).color(theme.text));
            ui.add_space(4.0);
            ui.label(
                RichText::new(&album.extract)
                    .size(12.0)
                    .color(style::weak_text(theme)),
            );
            if !album.url.is_empty() {
                ui.add_space(8.0);
                if widgets::inline_link(ui, theme, app.state.tr(Key::OpenReference)).clicked() {
                    pending.push(Command::OpenUrl(album.url.clone()));
                }
            }
        });
    }
    if let Some(song) = song {
        background_card(ui, theme, app.state.tr(Key::SongBackgroundSection), |ui| {
            ui.label(RichText::new(&song.title).size(15.0).color(theme.text));
            ui.add_space(4.0);
            ui.label(
                RichText::new(&song.extract)
                    .size(12.0)
                    .color(style::weak_text(theme)),
            );
            if !song.url.is_empty() {
                ui.add_space(8.0);
                if widgets::inline_link(ui, theme, app.state.tr(Key::OpenReference)).clicked() {
                    pending.push(Command::OpenUrl(song.url.clone()));
                }
            }
        });
    }
    if !videos.is_empty() {
        background_card(ui, theme, app.state.tr(Key::Interviews), |ui| {
            for link in &videos {
                if background_link_row(ui, theme, link).clicked() {
                    pending.push(Command::OpenUrl(link.url.clone()));
                }
            }
        });
    }
    if !links.is_empty() {
        background_card(ui, theme, app.state.tr(Key::FurtherReading), |ui| {
            for link in &links {
                if background_link_row(ui, theme, link).clicked() {
                    pending.push(Command::OpenUrl(link.url.clone()));
                }
            }
        });
    }
}

fn background_card(ui: &mut Ui, theme: &Theme, title: &str, add_contents: impl FnOnce(&mut Ui)) {
    ui.label(RichText::new(title).size(13.5).strong().color(theme.text));
    ui.add_space(4.0);
    let width = ui.available_width();
    egui::Frame::new()
        .fill(theme.surface)
        .stroke(Stroke::new(1.0_f32, theme.border))
        .corner_radius(CornerRadius::same(style::R_MD))
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(width - 28.0);
            add_contents(ui);
        });
    ui.add_space(12.0);
}

fn background_link_row(
    ui: &mut Ui,
    theme: &Theme,
    link: &crate::model::BackgroundLink,
) -> egui::Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 44.0), Sense::click());
    let hover =
        ui.ctx()
            .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.10);
    if hover > 0.01 {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(style::R_SM),
            theme::with_alpha(theme.surface_hover, (230.0 * hover) as u8),
        );
    }
    let color = theme::platform_color(match link.kind {
        LinkKind::YouTube => crate::model::Platform::YouTube,
        LinkKind::Genius => crate::model::Platform::Spotify,
        _ => crate::model::Platform::Local,
    });
    let icon = match link.kind {
        LinkKind::YouTube => Icon::Play,
        LinkKind::Wikipedia => Icon::Search,
        LinkKind::Genius => Icon::Music,
        _ => Icon::External,
    };
    style::paint_icon(
        ui.painter(),
        icon,
        Rect::from_center_size(
            Pos2::new(rect.left() + 15.0, rect.center().y),
            Vec2::splat(15.0),
        ),
        color,
    );
    style::paint_elided(
        ui.painter(),
        Pos2::new(rect.left() + 32.0, rect.center().y - 7.0),
        &link.title,
        style::font(12.0),
        theme.text,
        rect.width() - 70.0,
    );
    if let Some(subtitle) = &link.subtitle {
        style::paint_elided(
            ui.painter(),
            Pos2::new(rect.left() + 32.0, rect.center().y + 8.0),
            subtitle,
            style::font(10.0),
            style::weak_text(theme),
            rect.width() - 70.0,
        );
    }
    style::paint_icon(
        ui.painter(),
        Icon::ChevronRight,
        Rect::from_center_size(
            Pos2::new(rect.right() - 16.0, rect.center().y),
            Vec2::splat(13.0),
        ),
        style::weak_text(theme),
    );
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}
