//! Search across every source, with platform filters, grouped results and the
//! paste-a-link import flow.

use egui::{
    Align, Color32, CornerRadius, CursorIcon, Id, Layout, Pos2, Rect, RichText, Sense, Stroke,
    StrokeKind, Ui, Vec2,
};

use crate::app::EchoPortApp;
use crate::i18n::Key;
use crate::model::{format_duration, Artist, Platform, PlatformResults, SearchResults, Track};
use crate::state::{Command, Page};
use crate::theme::{self, Theme};
use crate::ui::style::{self, Icon, Variant};
use crate::ui::widgets::{self, TrackRow};

pub fn draw(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme, pending: &mut Vec<Command>) {
    ui.add_space(2.0);
    search_bar(ui, app, theme, pending);
    ui.add_space(8.0);
    search_tabs(ui, app, theme);
    ui.add_space(6.0);
    filters(ui, app, theme);
    ui.add_space(12.0);
    if app.state.search.loading {
        loading_state(ui, app, theme);
    } else if let Some(results) = app.state.search.results.clone() {
        results_view(ui, app, theme, pending, &results);
    } else {
        empty_state(ui, app, theme);
    }
}

// ---------------------------------------------------------------------------
// Search bar
// ---------------------------------------------------------------------------

fn search_bar(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme, pending: &mut Vec<Command>) {
    let available = ui.available_width();
    let height = 46.0;
    let is_url = widgets::looks_like_url(&app.state.search.input);
    let action_label = if is_url {
        app.state.tr(Key::ImportUrlCta)
    } else {
        app.state.tr(Key::SearchNav)
    };
    let action_icon = if is_url { Icon::Link } else { Icon::Search };
    let action_w = {
        let text_w = ui
            .painter()
            .layout_no_wrap(action_label.to_owned(), style::font(13.0), theme.text)
            .size()
            .x;
        (text_w + 16.0 + 7.0 + 30.0).max(106.0)
    };
    let gap = 9.0;
    let field_w = (available - action_w - gap).max(140.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let id = Id::new("echoport-search-input");
        let (field_rect, _) = ui.allocate_exact_size(Vec2::new(field_w, height), Sense::hover());
        let focused = ui.ctx().memory(|memory| memory.has_focus(id));
        let hovered = ui.rect_contains_pointer(field_rect);
        ui.painter().rect_filled(
            field_rect,
            CornerRadius::same(style::R_MD + 2),
            theme.surface,
        );
        ui.painter().rect_stroke(
            field_rect,
            CornerRadius::same(style::R_MD + 2),
            Stroke::new(
                if focused { 1.5_f32 } else { 1.0_f32 },
                if focused {
                    theme.accent
                } else if hovered {
                    theme::mix(theme.border, theme.accent, 0.45)
                } else {
                    theme.border
                },
            ),
            StrokeKind::Inside,
        );
        style::paint_icon(
            ui.painter(),
            Icon::Search,
            Rect::from_center_size(
                Pos2::new(field_rect.left() + 20.0, field_rect.center().y),
                Vec2::splat(17.0),
            ),
            theme.text_weak,
        );
        let clear_w = if app.state.search.input.is_empty() {
            0.0
        } else {
            34.0
        };
        let inner = Rect::from_min_max(
            Pos2::new(field_rect.left() + 38.0, field_rect.top() + 12.0),
            Pos2::new(
                field_rect.right() - 14.0 - clear_w,
                field_rect.bottom() - 12.0,
            ),
        );
        let hint = app.state.tr(Key::SearchPlaceholder);
        let _response = ui.put(
            inner,
            egui::TextEdit::singleline(&mut app.state.search.input)
                .id(id)
                .frame(false)
                .hint_text(hint)
                .font(style::font(14.0))
                .text_color(theme.text)
                .desired_width(inner.width()),
        );
        let mut submitted = false;
        if !app.state.search.input.is_empty() {
            let clear_rect = Rect::from_center_size(
                Pos2::new(field_rect.right() - 23.0, field_rect.center().y),
                Vec2::splat(26.0),
            );
            if style::icon_button_at(
                ui,
                clear_rect,
                "search-clear",
                theme,
                Icon::Close,
                false,
                app.state.tr(Key::ClearSearch),
            )
            .clicked()
            {
                app.state.search.input.clear();
                app.state.search.results = None;
            }
        }
        // Primary action sits in a fixed-width slot so nothing can be pushed
        // under the window edge when the clear button appears.
        let action_rect = Rect::from_min_size(
            Pos2::new(field_rect.right() + gap, field_rect.center().y - 17.0),
            Vec2::new(action_w, 34.0),
        );
        ui.allocate_rect(action_rect, Sense::hover());
        if style::button_icon_at(
            ui,
            action_rect,
            "search-action",
            theme,
            action_icon,
            action_label,
            Variant::Primary,
        ) {
            submitted = true;
        }
        if submitted {
            submit(app, pending);
        }
    });
}

fn submit(app: &mut EchoPortApp, pending: &mut Vec<Command>) {
    let mut query = app.state.search.input.trim().to_owned();
    if query.is_empty() {
        return;
    }
    if widgets::looks_like_url(&query) {
        pending.push(Command::ImportLink(query));
        return;
    }
    // A plain query: search every selected platform.
    let platforms: Vec<Platform> = Platform::REMOTE
        .into_iter()
        .filter(|platform| app.state.search.platforms.contains(platform))
        .collect();
    if platforms.is_empty() {
        query = query.trim().to_owned();
        pending.push(Command::Search {
            query: query.clone(),
            platforms: Platform::REMOTE.to_vec(),
        });
        app.state.search.platforms = Platform::REMOTE.into_iter().collect();
        return;
    }
    pending.push(Command::Search { query, platforms });
}

// ---------------------------------------------------------------------------
// Result category tabs
// ---------------------------------------------------------------------------

fn search_tabs(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme) {
    use crate::state::SearchTab;
    ui.horizontal(|ui| {
        for (tab, key) in [
            (SearchTab::All, Key::All),
            (SearchTab::Artists, Key::Artists),
            (SearchTab::Albums, Key::Albums),
        ] {
            let selected = app.state.search.tab == tab;
            if style::chip(ui, theme, app.state.tr(key), None, selected).clicked() && !selected {
                app.state.search.tab = tab;
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Platform filter chips
// ---------------------------------------------------------------------------

fn filters(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme) {
    ui.horizontal_wrapped(|ui| {
        let all_selected = Platform::REMOTE
            .iter()
            .all(|platform| app.state.search.platforms.contains(platform));
        if style::chip(ui, theme, app.state.tr(Key::All), None, all_selected).clicked() {
            if all_selected {
                app.state.search.platforms.clear();
            } else {
                app.state.search.platforms = Platform::REMOTE.into_iter().collect();
            }
        }
        ui.add_space(4.0);
        for platform in Platform::REMOTE {
            let selected = app.state.search.platforms.contains(&platform);
            if style::chip(
                ui,
                theme,
                platform.label(),
                Some(theme::platform_color(platform)),
                selected,
            )
            .clicked()
            {
                if !selected {
                    app.state.search.platforms.insert(platform);
                } else {
                    app.state.search.platforms.remove(&platform);
                }
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Loading skeleton
// ---------------------------------------------------------------------------

fn loading_state(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme) {
    let time = ui.ctx().input(|input| input.time);
    ui.add_space(8.0);
    style::loading_pulse(ui, theme, app.state.tr(Key::Searching), time);
    ui.add_space(12.0);
    for group in 0..3 {
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 168.0), Sense::hover());
        ui.painter()
            .rect_filled(rect, CornerRadius::same(style::R_LG), theme.surface);
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(style::R_LG),
            Stroke::new(1.0_f32, theme.border),
            StrokeKind::Inside,
        );
        let pulse = 0.5 + 0.5 * ((time as f32 * 2.4) - group as f32 * 0.8).sin();
        let bar_color = theme::with_alpha(theme.border, (120.0 + 70.0 * pulse) as u8);
        let header = Rect::from_min_size(
            Pos2::new(rect.left() + 16.0, rect.top() + 16.0),
            Vec2::new(120.0, 10.0),
        );
        ui.painter()
            .rect_filled(header, CornerRadius::same(4), bar_color);
        for row in 0..3 {
            let y = rect.top() + 42.0 + row as f32 * 40.0;
            let thumb = Rect::from_min_size(Pos2::new(rect.left() + 16.0, y), Vec2::splat(28.0));
            ui.painter().rect_filled(
                thumb,
                CornerRadius::same(6),
                theme::with_alpha(theme.border, 90),
            );
            let line = Rect::from_min_size(
                Pos2::new(rect.left() + 54.0, y + 4.0),
                Vec2::new((rect.width() * 0.4).min(240.0), 9.0),
            );
            ui.painter()
                .rect_filled(line, CornerRadius::same(4), bar_color);
            let line2 = Rect::from_min_size(
                Pos2::new(rect.left() + 54.0, y + 18.0),
                Vec2::new((rect.width() * 0.24).min(150.0), 7.0),
            );
            ui.painter().rect_filled(
                line2,
                CornerRadius::same(4),
                theme::with_alpha(theme.border, 80),
            );
        }
        ui.add_space(12.0);
    }
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(120));
}

// ---------------------------------------------------------------------------
// Results
// ---------------------------------------------------------------------------

/// Maximum number of tracks we keep per platform (Load more stops here).
const MAX_PLATFORM_TRACKS: usize = 60;
/// Collapsed platform lists show this many rows.
const COLLAPSED_ROWS: usize = 5;

fn query_tokens(query: &str) -> Vec<String> {
    query
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
        .collect()
}

fn score_text(query: &str, title: &str, artist: &str) -> i32 {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return 0;
    }
    let title = title.to_lowercase();
    let artist = artist.to_lowercase();
    let mut score = 0;
    if artist == query {
        score += 120;
    } else if artist.starts_with(&query) {
        score += 85;
    } else if artist.contains(&query) {
        score += 55;
    }
    if title == query {
        score += 100;
    } else if title.starts_with(&query) {
        score += 70;
    } else if title.contains(&query) {
        score += 45;
    }
    for token in query_tokens(&query) {
        if artist.contains(&token) {
            score += 8;
        }
        if title.contains(&token) {
            score += 6;
        }
    }
    score
}

fn score_artist(query: &str, name: &str) -> i32 {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return 0;
    }
    let name = name.to_lowercase();
    let mut score = 0;
    if name == query {
        score += 140;
    } else if name.starts_with(&query) {
        score += 95;
    } else if name.contains(&query) {
        score += 65;
    }
    for token in query_tokens(&query) {
        if name.contains(&token) {
            score += 10;
        }
    }
    score
}

fn platform_order(platform: Platform) -> usize {
    Platform::REMOTE
        .iter()
        .position(|candidate| *candidate == platform)
        .unwrap_or(usize::MAX)
}

/// Top tracks across every platform, ranked by how well they match the query.
fn best_matches(query: &str, results: &[PlatformResults]) -> Vec<Track> {
    let mut scored: Vec<(i32, usize, Track)> = Vec::new();
    for group in results {
        for track in &group.tracks {
            let score = score_text(query, &track.title, &track.artist);
            if score > 0 {
                scored.push((score, platform_order(group.platform), track.clone()));
            }
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut seen = std::collections::HashSet::new();
    scored
        .into_iter()
        .filter(|(_, _, track)| seen.insert(track.id.clone()))
        .take(3)
        .map(|(_, _, track)| track)
        .collect()
}

/// Platforms ordered by their best relevance (exact artist matches first).
fn ranked_groups<'a>(query: &str, results: &'a [PlatformResults]) -> Vec<&'a PlatformResults> {
    let mut groups: Vec<&PlatformResults> = results.iter().collect();
    groups.sort_by(|a, b| {
        let score_a = a
            .tracks
            .iter()
            .map(|track| score_text(query, &track.title, &track.artist))
            .max()
            .unwrap_or(0);
        let score_b = b
            .tracks
            .iter()
            .map(|track| score_text(query, &track.title, &track.artist))
            .max()
            .unwrap_or(0);
        score_b
            .cmp(&score_a)
            .then(platform_order(a.platform).cmp(&platform_order(b.platform)))
    });
    groups
}

/// Ranked producer/artist groups, keeping the first two per platform.
fn ranked_artists(
    query: &str,
    artists: &[crate::model::PlatformArtistResults],
) -> Vec<(Platform, Vec<Artist>)> {
    let mut ranked: Vec<(i32, usize, Platform, Vec<Artist>)> = artists
        .iter()
        .filter(|group| !group.artists.is_empty())
        .map(|group| {
            let mut names: Vec<Artist> = group.artists.clone();
            names.sort_by(|a, b| score_artist(query, &b.name).cmp(&score_artist(query, &a.name)));
            let best = names
                .first()
                .map(|artist| score_artist(query, &artist.name))
                .unwrap_or(0);
            names.truncate(2);
            (best, platform_order(group.platform), group.platform, names)
        })
        .collect();
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    ranked
        .into_iter()
        .map(|(_, _, platform, artists)| (platform, artists))
        .collect()
}

fn section_heading(ui: &mut Ui, theme: &Theme, title: &str, count: Option<usize>) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).size(15.0).strong().color(theme.text));
        if let Some(count) = count {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{count}"))
                        .size(11.0)
                        .color(theme.text_weak),
                );
            });
        }
    });
    ui.add_space(2.0);
}

fn results_view(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
    results: &SearchResults,
) {
    if app.ui.search_seen_query.as_deref() != Some(results.query.as_str()) {
        app.ui.search_expanded.clear();
        app.ui.search_seen_query = Some(results.query.clone());
    }
    let tab = app.state.search.tab;
    let total_tracks: usize = results.results.iter().map(|group| group.tracks.len()).sum();
    let has_artists = results
        .artists
        .iter()
        .any(|group| !group.artists.is_empty());
    let has_albums = results.albums.iter().any(|group| !group.albums.is_empty());
    let total = total_tracks + if has_artists { 1 } else { 0 } + if has_albums { 1 } else { 0 };
    egui::ScrollArea::vertical()
        .id_salt("search-results")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add_space(2.0);
            if total == 0 && results.results.iter().all(|group| group.error.is_none()) {
                style::empty_state(
                    ui,
                    theme,
                    Icon::Search,
                    app.state.tr(Key::SearchNoResults),
                    app.state.tr(Key::NoResultsHint),
                    None,
                );
                return;
            }
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{} “{}”",
                        app.state.tr(Key::ResultsFor),
                        results.query
                    ))
                    .size(12.0)
                    .color(theme.text_weak),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if style::button(ui, theme, app.state.tr(Key::ClearSearch), Variant::Ghost)
                        .clicked()
                    {
                        app.state.search.results = None;
                        app.state.search.input.clear();
                    }
                });
            });
            ui.add_space(6.0);

            match tab {
                crate::state::SearchTab::Artists => {
                    artists_section(ui, app, theme, pending, results);
                }
                crate::state::SearchTab::Albums => {
                    albums_section(ui, app, theme, pending, results);
                }
                crate::state::SearchTab::All => {
                    let best = best_matches(&results.query, &results.results);
                    if !best.is_empty() {
                        section_heading(
                            ui,
                            theme,
                            app.state.tr(Key::BestMatches),
                            Some(best.len()),
                        );
                        ui.add_space(2.0);
                        for (index, track) in best.iter().enumerate() {
                            best_row(ui, app, theme, pending, track, &best, index);
                        }
                        ui.add_space(14.0);
                    }

                    artists_section(ui, app, theme, pending, results);

                    albums_section(ui, app, theme, pending, results);

                    for group in ranked_groups(&results.query, &results.results) {
                        platform_group(ui, app, theme, pending, group);
                        ui.add_space(12.0);
                    }
                }
            }
            ui.add_space(20.0);
        });
}

fn artists_section(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
    results: &SearchResults,
) {
    let artists = ranked_artists(&results.query, &results.artists);
    if artists.is_empty() {
        style::empty_state(
            ui,
            theme,
            Icon::Profile,
            app.state.tr(Key::SearchNoResults),
            app.state.tr(Key::NoResultsHint),
            None,
        );
        return;
    }
    section_heading(ui, theme, app.state.tr(Key::ProducersArtists), None);
    style::hint_line(ui, theme, app.state.tr(Key::SearchArtistsHint));
    ui.add_space(6.0);
    for (platform, artists) in &artists {
        ui.push_id(("search-artists", platform.key()), |ui| {
            platform_subheading(ui, theme, *platform);
            let card_w = ((ui.available_width() - 20.0) / 3.0).clamp(170.0, 280.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                for artist in artists {
                    artist_card(ui, app, theme, pending, artist, card_w);
                }
            });
            ui.add_space(8.0);
        });
    }
    ui.add_space(6.0);
}

fn album_relevance(query: &str, album: &crate::model::Album) -> i32 {
    score_text(query, &album.title, &album.artist)
}

fn albums_section(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
    results: &SearchResults,
) {
    let mut albums: Vec<crate::model::Album> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for group in &results.albums {
        for album in &group.albums {
            if seen.insert(album.id.clone()) {
                albums.push(album.clone());
            }
        }
    }
    albums.sort_by(|a, b| {
        album_relevance(&results.query, b)
            .cmp(&album_relevance(&results.query, a))
            .then(a.platform.key().cmp(b.platform.key()))
            .then(a.title.cmp(&b.title))
    });
    if albums.is_empty() {
        style::empty_state(
            ui,
            theme,
            Icon::Music,
            app.state.tr(Key::SearchNoResults),
            app.state.tr(Key::NoResultsHint),
            None,
        );
        return;
    }
    section_heading(ui, theme, app.state.tr(Key::Albums), Some(albums.len()));
    ui.add_space(6.0);
    let width = ((ui.available_width() - 20.0) / 2.0).clamp(220.0, 360.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        for album in &albums {
            widgets::album_card(ui, app, theme, pending, album, width);
        }
    });
    ui.add_space(14.0);
}

fn platform_subheading(ui: &mut Ui, theme: &Theme, platform: Platform) {
    ui.horizontal(|ui| {
        let (dot, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
        ui.painter()
            .circle_filled(dot.center(), 3.5, theme::platform_color(platform));
        ui.add_space(2.0);
        ui.label(
            RichText::new(platform.label())
                .size(11.0)
                .strong()
                .color(style::weak_text(theme)),
        );
    });
    ui.add_space(4.0);
}

fn artist_card(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
    artist: &Artist,
    width: f32,
) {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 58.0), Sense::click());
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
            theme::mix(theme.border, theme.accent, 0.25 * hover),
        ),
        StrokeKind::Inside,
    );
    let avatar = Rect::from_center_size(
        Pos2::new(rect.left() + 8.0 + 19.0, rect.center().y),
        Vec2::splat(38.0),
    );
    widgets::paint_artist_avatar(ui, app, theme, artist, avatar);
    let text_left = avatar.right() + 10.0;
    style::paint_elided(
        ui.painter(),
        Pos2::new(text_left, rect.center().y - 8.0),
        &artist.name,
        style::font(12.5),
        theme.text,
        rect.right() - text_left - 8.0,
    );
    ui.painter().circle_filled(
        Pos2::new(text_left + 3.0, rect.center().y + 11.0),
        2.6,
        theme::platform_color(artist.platform),
    );
    style::paint_elided(
        ui.painter(),
        Pos2::new(text_left + 9.0, rect.center().y + 9.0),
        artist.platform.label(),
        style::font(10.0),
        style::weak_text(theme),
        rect.right() - text_left - 14.0,
    );
    let response = response
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(app.state.tr(Key::ArtistPage));
    if response.clicked() {
        pending.push(Command::OpenArtist(Box::new(artist.clone())));
    }
}

fn best_row(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
    track: &Track,
    list: &[Track],
    index: usize,
) {
    let width = ui.available_width();
    let (rect, body) = ui.allocate_exact_size(Vec2::new(width, 46.0), Sense::click());
    let is_current = app
        .state
        .now_playing
        .as_ref()
        .map(|current| current.id == track.id)
        .unwrap_or(false);
    let selected = app.ui.selected_track.as_deref() == Some(track.id.as_str());
    let hover = ui
        .ctx()
        .animate_bool_with_time(body.id.with("hover"), body.hovered(), 0.10);
    let fill = if is_current {
        theme::with_alpha(theme.accent_soft, 170)
    } else if selected {
        theme::with_alpha(theme.accent_soft, 96)
    } else if hover > 0.01 {
        theme::with_alpha(theme.surface_hover, (220.0 * hover) as u8)
    } else {
        Color32::TRANSPARENT
    };
    if fill.a() > 0 {
        ui.painter().rect_filled(
            rect.shrink2(Vec2::new(2.0, 1.0)),
            CornerRadius::same(style::R_SM),
            fill,
        );
    }
    // Single-click triangle (play / pause for the current track).
    let play_rect = Rect::from_center_size(
        Pos2::new(rect.left() + 22.0, rect.center().y),
        Vec2::splat(28.0),
    );
    let play = style::icon_button_at(
        ui,
        play_rect,
        ("best-play", track.id.as_str()),
        theme,
        if is_current && app.state.playback.state.is_playing() {
            Icon::Pause
        } else {
            Icon::Play
        },
        is_current,
        "",
    );
    if play.clicked() {
        if is_current {
            pending.push(Command::TogglePlayPause);
        } else {
            pending.push(widgets::play_from(list, index));
        }
    }
    let text_left = rect.left() + 74.0;
    ui.painter().circle_filled(
        Pos2::new(rect.left() + 58.0, rect.center().y),
        3.5,
        theme::platform_color(track.platform),
    );
    style::paint_elided(
        ui.painter(),
        Pos2::new(text_left, rect.center().y - 8.0),
        &track.title,
        style::font(13.0),
        if is_current { theme.accent } else { theme.text },
        rect.right() - text_left - 90.0,
    );
    style::paint_elided(
        ui.painter(),
        Pos2::new(text_left, rect.center().y + 9.0),
        &format!("{} · {}", track.display_artist(), track.platform.label()),
        style::font(10.5),
        style::weak_text(theme),
        rect.right() - text_left - 90.0,
    );
    if let Some(duration) = track.duration_secs {
        style::paint_elided_right(
            ui.painter(),
            Pos2::new(rect.right() - 8.0, rect.center().y),
            &format_duration(duration),
            style::font(11.0),
            theme.text_weak,
            48.0,
        );
    }
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
            pending.push(widgets::play_from(list, index));
        }
    }
}

fn platform_group(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
    group: &PlatformResults,
) {
    let hint = if group.error.is_some() {
        group_hint(group.platform)
    } else {
        None
    };
    ui.push_id(("platform-group", group.platform.key()), |ui| {
        style::card(theme).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            // Header row.
            ui.horizontal(|ui| {
                let (dot, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
                ui.painter().circle_filled(
                    dot.center(),
                    4.5,
                    theme::platform_color(group.platform),
                );
                ui.add_space(2.0);
                ui.label(
                    RichText::new(group.platform.label())
                        .size(14.0)
                        .strong()
                        .color(theme.text),
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new(style::tracks_count(&app.state, group.tracks.len()))
                        .size(10.5)
                        .color(theme.text_weak),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if hint.is_some() {
                        ui.label(
                            RichText::new(app.state.tr(Key::SourceUnavailable))
                                .size(9.5)
                                .color(theme.text_weak),
                        );
                    }
                });
            });
            if let Some(artists) = app.state.search.results.as_ref().and_then(|results| {
                results
                    .artists
                    .iter()
                    .find(|artists| artists.platform == group.platform)
                    .map(|artists| artists.artists.clone())
            }) {
                if !artists.is_empty() {
                    ui.add_space(2.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        for artist in artists.iter().take(2) {
                            artist_chip(ui, app, theme, pending, artist);
                        }
                    });
                }
            }
            ui.painter().hline(
                ui.max_rect().x_range(),
                ui.cursor().min.y + 2.0,
                Stroke::new(1.0_f32, theme.border),
            );
            ui.add_space(4.0);
            if group.tracks.is_empty() {
                let text = if let Some(key) = hint {
                    app.state.tr(key)
                } else if let Some(error) = &group.error {
                    error.as_str()
                } else {
                    app.state.tr(Key::SearchNoResults)
                };
                widgets::callout(ui, theme, Icon::Info, text, true);
                return;
            }
            let expanded = app.ui.search_expanded.contains(&group.platform);
            let visible = if expanded {
                group.tracks.len()
            } else {
                group.tracks.len().min(COLLAPSED_ROWS)
            };
            for (index, track) in group.tracks.iter().take(visible).enumerate() {
                let mut row = TrackRow::new(&group.tracks, index);
                row.height = 48.0;
                row.show_album = false;
                row.salt = group.platform.key();
                widgets::track_row(ui, app, theme, pending, track, row);
            }
            // Collapse / expand the already-fetched rows.
            if group.tracks.len() > COLLAPSED_ROWS {
                ui.add_space(2.0);
                let label = if expanded {
                    app.state.tr(Key::ShowLessTracks)
                } else {
                    app.state.tr(Key::ShowMoreTracks)
                };
                if style::button(ui, theme, label, Variant::Ghost).clicked() {
                    if expanded {
                        app.ui.search_expanded.remove(&group.platform);
                    } else {
                        app.ui.search_expanded.insert(group.platform);
                    }
                }
            }
            // Load more from the network, with an inline spinner.
            if group.tracks.len() < MAX_PLATFORM_TRACKS {
                ui.add_space(2.0);
                let loading = app.state.search.loading_more == Some(group.platform);
                if loading {
                    ui.horizontal(|ui| {
                        let time = ui.ctx().input(|input| input.time);
                        let (dots, _) =
                            ui.allocate_exact_size(Vec2::new(22.0, 12.0), Sense::hover());
                        style::loading_dots(ui.painter(), dots, theme, time);
                        ui.add_space(2.0);
                        ui.label(
                            RichText::new(app.state.tr(Key::LoadMoreTracks))
                                .size(11.0)
                                .color(theme.text_weak),
                        );
                    });
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_millis(90));
                } else if style::button(
                    ui,
                    theme,
                    app.state.tr(Key::LoadMoreTracks),
                    Variant::Secondary,
                )
                .clicked()
                {
                    pending.push(Command::SearchMore(group.platform));
                }
            }
            if let Some(key) = hint {
                style::hint_line(ui, theme, app.state.tr(key));
            }
        });
    });
}

fn group_hint(platform: Platform) -> Option<Key> {
    match platform {
        Platform::Spotify => Some(Key::SpotifySearchHint),
        _ => None,
    }
}

fn artist_chip(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
    artist: &Artist,
) {
    let font = style::font(11.0);
    let text_w = ui
        .painter()
        .layout_no_wrap(artist.name.clone(), font.clone(), theme.text)
        .size()
        .x;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(text_w + 58.0, 30.0), Sense::click());
    let hover =
        ui.ctx()
            .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.12);
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(15),
        theme::mix(theme.surface, theme.surface_hover, hover),
    );
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(15),
        Stroke::new(
            1.0_f32,
            theme::mix(theme.border, theme.accent, 0.25 * hover),
        ),
        StrokeKind::Inside,
    );
    let avatar = Rect::from_center_size(
        Pos2::new(rect.left() + 15.0, rect.center().y),
        Vec2::splat(22.0),
    );
    widgets::paint_artist_avatar(ui, app, theme, artist, avatar);
    style::paint_elided(
        ui.painter(),
        Pos2::new(avatar.right() + 6.0, rect.center().y - 6.0),
        &artist.name,
        font,
        theme.text,
        rect.right() - avatar.right() - 10.0,
    );
    if response
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(app.state.tr(Key::ArtistPage))
        .clicked()
    {
        pending.push(Command::OpenArtist(Box::new(artist.clone())));
    }
}

// ---------------------------------------------------------------------------
// Empty state (nothing searched yet)
// ---------------------------------------------------------------------------

fn empty_state(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme) {
    egui::ScrollArea::vertical()
        .id_salt("search-empty")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(160.0, 110.0), Sense::hover());
                let painter = ui.painter();
                painter.circle_filled(
                    rect.center() + Vec2::new(-44.0, -14.0),
                    16.0,
                    theme::with_alpha(theme.highlight, 150),
                );
                painter.circle_filled(
                    rect.center() + Vec2::new(42.0, 18.0),
                    12.0,
                    theme::with_alpha(theme.accent, 60),
                );
                painter.circle_stroke(
                    rect.center() + Vec2::new(26.0, -34.0),
                    8.0,
                    Stroke::new(2.0_f32, theme::with_alpha(theme.accent, 140)),
                );
                let glass =
                    Rect::from_center_size(rect.center() + Vec2::new(0.0, 4.0), Vec2::splat(62.0));
                painter.circle_filled(glass.center(), 31.0, style::wash(theme));
                style::paint_icon(painter, Icon::Globe, glass.shrink(16.0), theme.accent);
                ui.add_space(10.0);
                ui.label(
                    RichText::new(app.state.tr(Key::SearchHint))
                        .size(17.0)
                        .color(theme.text),
                );
                ui.add_space(3.0);
                ui.label(
                    RichText::new(app.state.tr(Key::SearchTryHint))
                        .size(12.0)
                        .color(theme.text_weak),
                );
            });
            ui.add_space(22.0);
            let width = (ui.available_width() - 12.0).min(760.0);
            ui.vertical_centered(|ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(width, 104.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                        widgets::callout(
                            ui,
                            theme,
                            Icon::Link,
                            app.state.tr(Key::SpotifySearchHint),
                            true,
                        );
                    },
                );
            });
            ui.add_space(10.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(app.state.tr(Key::ImportLinkHint))
                        .size(11.0)
                        .color(theme.text_weak),
                );
            });
        });
    let _ = Page::Search;
    let _ = CursorIcon::PointingHand;
}
