//! Settings: appearance, language, playback, downloads, media tools, storage
//! and about.

use egui::{
    Align, Color32, CornerRadius, Layout, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui, Vec2,
};

use crate::app::EchoPortApp;
use crate::i18n::{Key, Language};
use crate::model::DownloadFormat;
use crate::state::{Command, Page};
use crate::theme::{self, Palette, Theme, ThemeMode};
use crate::ui::style::{self, Icon, Variant};
use crate::ui::widgets;
use crate::ui::{theme_mode_hint, theme_mode_label};

pub fn draw(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme, pending: &mut Vec<Command>) {
    widgets::page_scroll(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(app.state.tr(Key::Settings))
                        .size(26.0)
                        .color(theme.text),
                );
                ui.label(
                    RichText::new(app.state.tr(Key::AboutHint))
                        .size(11.5)
                        .color(theme.text_weak),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("v{}", app.version()))
                        .size(10.5)
                        .color(theme.text_weak),
                );
            });
        });
        ui.add_space(14.0);
        let available = ui.available_width();
        let palette = if app.state.follow_cover_accent {
            app.state.cover.palette
        } else {
            None
        };
        let gap = 16.0;
        if available < 840.0 {
            scripting(ui, app, theme, pending, available);
            ui.add_space(14.0);
            appearance(ui, app, theme, pending, available, palette.as_ref());
            ui.add_space(14.0);
            playback(ui, app, theme, pending, available);
            ui.add_space(14.0);
            downloads(ui, app, theme, pending, available);
            ui.add_space(14.0);
            tooling(ui, app, theme, pending, available);
            ui.add_space(14.0);
            storage(ui, app, theme, pending, available);
            ui.add_space(14.0);
            about(ui, app, theme, pending, available);
        } else {
            let column_w = (available - gap) / 2.0;
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.allocate_ui_with_layout(
                    Vec2::new(column_w, 1200.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.set_min_width(column_w);
                        appearance(ui, app, theme, pending, column_w, palette.as_ref());
                        ui.add_space(14.0);
                        playback(ui, app, theme, pending, column_w);
                        ui.add_space(14.0);
                        downloads(ui, app, theme, pending, column_w);
                    },
                );
                ui.add_space(gap);
                ui.allocate_ui_with_layout(
                    Vec2::new(column_w, 1200.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.set_min_width(column_w);
                        scripting(ui, app, theme, pending, column_w);
                        ui.add_space(14.0);
                        tooling(ui, app, theme, pending, column_w);
                        ui.add_space(14.0);
                        storage(ui, app, theme, pending, column_w);
                        ui.add_space(14.0);
                        about(ui, app, theme, pending, column_w);
                    },
                );
            });
        }
        ui.add_space(24.0);
        ui.add_space(10.0);
    });
}

fn section_card<R>(
    ui: &mut Ui,
    theme: &Theme,
    icon: Icon,
    title: &str,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> R {
    egui::Frame::new()
        .fill(theme.surface)
        .stroke(Stroke::new(1.0_f32, theme.border))
        .corner_radius(CornerRadius::same(style::R_LG))
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::hover());
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(7), style::wash(theme));
                style::paint_icon(ui.painter(), icon, rect.shrink(5.0), theme.accent);
                ui.add_space(2.0);
                ui.label(RichText::new(title).size(14.0).strong().color(theme.text));
            });
            ui.add_space(10.0);
            add_contents(ui)
        })
        .inner
}

// ---------------------------------------------------------------------------
// Appearance
// ---------------------------------------------------------------------------

fn appearance(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
    width: f32,
    palette: Option<&Palette>,
) {
    section_card(ui, theme, Icon::Info, app.state.tr(Key::Appearance), |ui| {
        ui.label(
            RichText::new(app.state.tr(Key::Theme))
                .size(11.0)
                .color(theme.text_weak),
        );
        ui.add_space(8.0);
        let inner_w = width - 32.0;
        let gap = 10.0;
        let tile_w = (inner_w - gap * 2.0) / 3.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (index, mode) in ThemeMode::ALL.into_iter().enumerate() {
                if index > 0 {
                    ui.add_space(gap);
                }
                let selected = app.state.theme_mode == mode;
                let label = theme_mode_label(&app.state, mode);
                if theme_tile(ui, theme, mode, label, selected, tile_w, palette).clicked()
                    && !selected
                {
                    pending.push(Command::SetThemeMode(mode));
                }
            }
        });
        ui.add_space(8.0);
        ui.label(
            RichText::new(theme_mode_hint(&app.state, app.state.theme_mode))
                .size(10.5)
                .color(theme.text_weak),
        );
        ui.add_space(14.0);
        style::divider(ui, theme);
        ui.add_space(10.0);

        // Follow-cover accent toggle: a separate switch, not a theme mode.
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(app.state.tr(Key::FollowCoverAccent))
                        .size(12.0)
                        .color(theme.text),
                );
                ui.add_space(2.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(app.state.tr(Key::FollowCoverAccentHint))
                            .size(10.0)
                            .color(theme.text_weak),
                    )
                    .wrap(),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if style::switch(
                    ui,
                    theme,
                    app.state.follow_cover_accent,
                    "follow-cover-accent",
                )
                .clicked()
                {
                    pending.push(Command::SetFollowCoverAccent(
                        !app.state.follow_cover_accent,
                    ));
                }
            });
        });
        ui.add_space(14.0);
        style::divider(ui, theme);
        ui.add_space(10.0);
        ui.label(
            RichText::new(app.state.tr(Key::LanguageLabel))
                .size(11.0)
                .color(theme.text_weak),
        );
        ui.add_space(6.0);
        let labels = [Language::En.native_name(), Language::Zh.native_name()];
        let selected = if app.state.lang == Language::En { 0 } else { 1 };
        if let Some(index) = style::segmented(ui, theme, &labels, selected, 30.0) {
            let lang = if index == 0 {
                Language::En
            } else {
                Language::Zh
            };
            if lang != app.state.lang {
                pending.push(Command::SetLanguage(lang));
            }
        }
        ui.add_space(14.0);
        style::divider(ui, theme);
        ui.add_space(10.0);
        ui.label(
            RichText::new(app.state.tr(Key::UiScale))
                .size(11.0)
                .color(theme.text_weak),
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let current = (app.state.ui_scale * 100.0).round() as i32;
            for pct in [100i32, 110, 125, 150] {
                let selected = current == pct;
                if style::chip(ui, theme, &format!("{pct}%"), None, selected).clicked() && !selected
                {
                    pending.push(Command::SetUiScale(pct as f32 / 100.0));
                }
            }
        });
        ui.add_space(2.0);
        ui.label(
            RichText::new(app.state.tr(Key::UiScaleHint))
                .size(10.0)
                .color(theme.text_weak),
        );
    });
}

/// A miniature mock window that shows the actual palette of `mode`:
/// canvas, sidebar, crisp card, accent bar and the strong highlight.
fn theme_tile(
    ui: &mut Ui,
    theme: &Theme,
    mode: ThemeMode,
    label: &str,
    selected: bool,
    width: f32,
    palette: Option<&Palette>,
) -> egui::Response {
    let sample = Theme::for_mode(mode, palette);
    let height = 72.0;
    let label_h = 20.0;
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width, height + label_h), Sense::click());
    let hover =
        ui.ctx()
            .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.12);
    // Outer tile: the canvas colour, with a hairline border.
    let tile = Rect::from_min_size(rect.min, Vec2::new(width, height));
    ui.painter()
        .rect_filled(tile, CornerRadius::same(style::R_MD), sample.bg);
    // Sidebar strip.
    let sidebar = Rect::from_min_max(
        tile.min,
        Pos2::new(tile.left() + tile.width() * 0.30, tile.bottom()),
    );
    ui.painter().rect_filled(
        sidebar,
        CornerRadius {
            nw: style::R_MD,
            sw: style::R_MD,
            ne: 0,
            se: 0,
        },
        sample.panel,
    );
    // Brand block + nav lines in the sidebar.
    ui.painter().rect_filled(
        Rect::from_min_size(
            Pos2::new(sidebar.left() + 6.0, sidebar.top() + 8.0),
            Vec2::new(sidebar.width() * 0.55, 4.0),
        ),
        CornerRadius::same(2),
        sample.text,
    );
    for row in 0..3 {
        let y = sidebar.top() + 20.0 + row as f32 * 9.0;
        let fill = if row == 0 {
            sample.accent_soft
        } else {
            theme::with_alpha(sample.text_weak, 120)
        };
        ui.painter().rect_filled(
            Rect::from_min_size(
                Pos2::new(sidebar.left() + 6.0, y),
                Vec2::new(sidebar.width() - 12.0, 5.0),
            ),
            CornerRadius::same(2),
            fill,
        );
    }
    // Content area: crisp card with an accent bar and highlight pill.
    let content = Rect::from_min_max(
        Pos2::new(sidebar.right() + 6.0, tile.top() + 7.0),
        Pos2::new(tile.right() - 6.0, tile.bottom() - 7.0),
    );
    ui.painter()
        .rect_filled(content, CornerRadius::same(5), sample.surface);
    ui.painter().rect_stroke(
        content,
        CornerRadius::same(5),
        Stroke::new(1.0_f32, sample.border),
        StrokeKind::Inside,
    );
    // Accent bar (progress) + highlight dot.
    ui.painter().rect_filled(
        Rect::from_min_size(
            Pos2::new(content.left() + 6.0, content.top() + 12.0),
            Vec2::new((content.width() - 30.0) * 0.62, 5.0),
        ),
        CornerRadius::same(2),
        sample.accent,
    );
    ui.painter().circle_filled(
        Pos2::new(content.right() - 10.0, content.top() + 14.0),
        4.5,
        sample.highlight,
    );
    // Text lines.
    ui.painter().rect_filled(
        Rect::from_min_size(
            Pos2::new(content.left() + 6.0, content.top() + 26.0),
            Vec2::new(content.width() * 0.55, 4.0),
        ),
        CornerRadius::same(2),
        theme::with_alpha(sample.text, 230),
    );
    ui.painter().rect_filled(
        Rect::from_min_size(
            Pos2::new(content.left() + 6.0, content.top() + 35.0),
            Vec2::new(content.width() * 0.34, 3.0),
        ),
        CornerRadius::same(2),
        theme::with_alpha(sample.text_weak, 200),
    );
    ui.painter().rect_filled(
        Rect::from_min_size(
            Pos2::new(content.left() + 6.0, content.bottom() - 16.0),
            Vec2::new((content.width() - 12.0) * 0.8, 8.0),
        ),
        CornerRadius::same(3),
        sample.accent_soft,
    );
    // Selected / hover outline.
    ui.painter().rect_stroke(
        tile,
        CornerRadius::same(style::R_MD),
        Stroke::new(
            if selected { 2.0_f32 } else { 1.0_f32 },
            if selected {
                theme.accent
            } else if hover > 0.01 {
                theme::mix(theme.border, theme.accent, 0.45)
            } else {
                theme.border
            },
        ),
        StrokeKind::Inside,
    );
    if selected {
        let badge = Rect::from_center_size(
            Pos2::new(tile.right() - 11.0, tile.top() + 11.0),
            Vec2::splat(14.0),
        );
        ui.painter()
            .circle_filled(badge.center(), 7.0, sample.accent);
        style::paint_icon(
            ui.painter(),
            Icon::Check,
            badge.shrink(3.0),
            sample.on_accent,
        );
    }
    style::paint_elided(
        ui.painter(),
        Pos2::new(rect.left() + 2.0, tile.bottom() + 11.0),
        label,
        style::font(10.5),
        if selected {
            theme.accent
        } else {
            theme.text_weak
        },
        width - 4.0,
    );
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

// ---------------------------------------------------------------------------
// Playback
// ---------------------------------------------------------------------------

fn playback(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
    width: f32,
) {
    section_card(
        ui,
        theme,
        Icon::Music,
        app.state.tr(Key::AudioSettings),
        |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(app.state.tr(Key::ResultsPerPlatform))
                            .size(12.0)
                            .color(theme.text),
                    );
                    ui.label(
                        RichText::new(app.state.tr(Key::ResultsHint))
                            .size(10.0)
                            .color(theme.text_weak),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let options = [8usize, 12, 20, 30];
                    for value in options.into_iter().rev() {
                        let selected = app.state.profile.settings.results_per_platform == value;
                        if style::chip(ui, theme, &format!("{value}"), None, selected).clicked()
                            && !selected
                        {
                            pending.push(Command::SetResultsPerPlatform(value));
                        }
                    }
                });
            });
            let _ = width;
        },
    );
}

// ---------------------------------------------------------------------------
// Downloads
// ---------------------------------------------------------------------------

fn downloads(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
    width: f32,
) {
    section_card(
        ui,
        theme,
        Icon::Download,
        app.state.tr(Key::DownloadSettings),
        |ui| {
            ui.label(
                RichText::new(app.state.tr(Key::DownloadFormat))
                    .size(11.0)
                    .color(theme.text_weak),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                for format in DownloadFormat::ALL {
                    let selected = app.state.profile.settings.download_format == format;
                    if style::chip(ui, theme, format.label(), None, selected).clicked() && !selected
                    {
                        pending.push(Command::SetDownloadFormat(format));
                    }
                }
            });
            ui.add_space(12.0);
            ui.label(
                RichText::new(app.state.tr(Key::DownloadFolder))
                    .size(11.0)
                    .color(theme.text_weak),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let dir = app
                    .state
                    .profile
                    .settings
                    .download_dir
                    .to_string_lossy()
                    .into_owned();
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    let label = egui::Label::new(RichText::new(dir).size(11.0).color(theme.text))
                        .truncate();
                    ui.add_sized(
                        Vec2::new((ui.available_width() - 200.0).max(80.0), 30.0),
                        label,
                    );
                });
                if style::button_sized(
                    ui,
                    theme,
                    app.state.tr(Key::ChooseFolder),
                    Variant::Secondary,
                    30.0,
                )
                .clicked()
                {
                    pending.push(Command::PickDownloadFolder);
                }
                if style::button_sized(
                    ui,
                    theme,
                    app.state.tr(Key::OpenFolder),
                    Variant::Ghost,
                    30.0,
                )
                .clicked()
                {
                    pending.push(Command::OpenDownloadFolder);
                }
            });
            let _ = width;
        },
    );
}

// ---------------------------------------------------------------------------
// Media tools
// ---------------------------------------------------------------------------

fn tooling(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
    width: f32,
) {
    section_card(
        ui,
        theme,
        Icon::Settings,
        app.state.tr(Key::ToolingStatus),
        |ui| {
            let tooling = app.state.tooling.clone();
            tool_row(
                ui,
                app,
                theme,
                "yt-dlp",
                if tooling.ytdlp_ready() {
                    Key::ToolAvailable
                } else {
                    Key::ToolMissing
                },
                tooling.ytdlp_version.clone(),
                width,
            );
            ui.add_space(6.0);
            tool_row(
                ui,
                app,
                theme,
                "ffmpeg",
                if tooling.ffmpeg_ready() {
                    Key::ToolAvailable
                } else {
                    Key::ToolMissing
                },
                tooling
                    .ffmpeg_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned()),
                width,
            );
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if style::button_icon(
                    ui,
                    theme,
                    Icon::Refresh,
                    app.state.tr(Key::DetectTools),
                    Variant::Secondary,
                )
                .clicked()
                {
                    pending.push(Command::DetectTools);
                }
            });
            ui.add_space(8.0);
            style::hint_line(ui, theme, app.state.tr(Key::ToolingHint));
            if !tooling.ytdlp_ready() {
                ui.add_space(4.0);
                style::hint_line(ui, theme, app.state.tr(Key::InstallYtDlpHint));
            }
        },
    );
}

fn tool_row(
    ui: &mut Ui,
    _app: &EchoPortApp,
    theme: &Theme,
    name: &str,
    state_key: Key,
    version: Option<String>,
    width: f32,
) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width - 64.0, 36.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(style::R_SM), style::wash(theme));
    style::paint_elided(
        ui.painter(),
        Pos2::new(rect.left() + 12.0, rect.center().y),
        name,
        style::font(11.5),
        theme.text,
        80.0,
    );
    let ready = matches!(state_key, Key::ToolAvailable);
    let color = if ready {
        style::success_color(theme)
    } else {
        style::danger_color(theme)
    };
    let label = _app.state.tr(state_key);
    let suffix = version.unwrap_or_else(|| label.to_owned());
    style::paint_elided_right(
        ui.painter(),
        Pos2::new(rect.right() - 12.0, rect.center().y),
        &suffix,
        style::font(10.0),
        color,
        rect.width() - 100.0,
    );
}

// ---------------------------------------------------------------------------
// Scripting & plugins
// ---------------------------------------------------------------------------

fn scripting(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
    width: f32,
) {
    let script = app.state.script.clone();
    section_card(
        ui,
        theme,
        Icon::Settings,
        app.state.tr(Key::ScriptingPlugins),
        |ui| {
            if !script.loaded {
                style::hint_line(ui, theme, app.state.tr(Key::ScriptNotLoaded));
                ui.add_space(10.0);
            } else {
                // Config + plugins folder paths.
                path_row(
                    ui,
                    theme,
                    app.state.tr(Key::ScriptConfigFile),
                    &script.config_file,
                );
                ui.add_space(3.0);
                path_row(
                    ui,
                    theme,
                    app.state.tr(Key::ScriptPluginsFolder),
                    &script.plugins_dir,
                );
                ui.add_space(10.0);
                // Plugin names.
                ui.label(
                    RichText::new(app.state.tr(Key::ScriptPlugins))
                        .size(10.0)
                        .color(theme.text_weak),
                );
                ui.add_space(2.0);
                if script.plugins.is_empty() {
                    ui.label(
                        RichText::new(app.state.tr(Key::NoPluginsLoaded))
                            .size(11.0)
                            .color(style::weak_text(theme)),
                    );
                } else {
                    ui.horizontal_wrapped(|ui| {
                        for name in &script.plugins {
                            plugin_badge(ui, theme, name);
                            ui.add_space(4.0);
                        }
                    });
                }
                ui.add_space(10.0);
                // Custom actions with Run buttons.
                ui.label(
                    RichText::new(app.state.tr(Key::ScriptActions))
                        .size(10.0)
                        .color(theme.text_weak),
                );
                ui.add_space(3.0);
                if script.actions.is_empty() {
                    ui.label(
                        RichText::new(app.state.tr(Key::NoScriptActions))
                            .size(11.0)
                            .color(style::weak_text(theme)),
                    );
                } else {
                    for action in &script.actions {
                        if script_action_row(ui, app, theme, action) {
                            pending.push(Command::RunScriptAction(action.id.clone()));
                        }
                        ui.add_space(2.0);
                    }
                }
                // Custom keybindings.
                if !script.bindings.is_empty() {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(app.state.tr(Key::ScriptBindings))
                            .size(10.0)
                            .color(theme.text_weak),
                    );
                    ui.add_space(2.0);
                    for binding in &script.bindings {
                        ui.label(
                            RichText::new(format!(
                                "{}  →  {}",
                                binding.chord.label(),
                                binding.action.key()
                            ))
                            .monospace()
                            .size(10.0)
                            .color(style::weak_text(theme)),
                        );
                    }
                }
                // Warnings (first three, then a count).
                if !script.warnings.is_empty() {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        style::paint_icon(
                            ui.painter(),
                            Icon::Info,
                            Rect::from_center_size(
                                Pos2::new(ui.cursor().min.x + 7.0, ui.cursor().min.y + 8.0),
                                Vec2::splat(13.0),
                            ),
                            style::danger_color(theme),
                        );
                        ui.add_space(16.0);
                        ui.label(
                            RichText::new(format!(
                                "{} ({})",
                                app.state.tr(Key::ScriptWarnings),
                                script.warnings.len()
                            ))
                            .size(10.0)
                            .color(style::danger_color(theme)),
                        );
                    });
                    for warning in script.warnings.iter().take(3) {
                        style::paint_elided(
                            ui.painter(),
                            Pos2::new(ui.cursor().min.x + 16.0, ui.cursor().min.y + 6.0),
                            warning,
                            style::font(10.0),
                            style::danger_color(theme),
                            width - 64.0,
                        );
                        ui.add_space(13.0);
                    }
                    if script.warnings.len() > 3 {
                        ui.label(
                            RichText::new(format!("+{}", script.warnings.len() - 3))
                                .size(10.0)
                                .color(style::weak_text(theme)),
                        );
                    }
                }
                ui.add_space(10.0);
            }
            ui.horizontal(|ui| {
                if style::button_icon(
                    ui,
                    theme,
                    Icon::Refresh,
                    app.state.tr(Key::ReloadScripts),
                    Variant::Tonal,
                )
                .clicked()
                {
                    pending.push(Command::ReloadScripts);
                }
                ui.add_space(2.0);
                if style::button_icon(
                    ui,
                    theme,
                    Icon::Folder,
                    app.state.tr(Key::OpenScriptsFolder),
                    Variant::Secondary,
                )
                .clicked()
                {
                    pending.push(Command::OpenScriptsFolder);
                }
            });
            ui.add_space(6.0);
            // Embedded bilingual manuals: no network and no extra install step.
            ui.horizontal_wrapped(|ui| {
                if style::button_icon(
                    ui,
                    theme,
                    Icon::Info,
                    app.state.tr(Key::PluginDocs),
                    Variant::Ghost,
                )
                .clicked()
                {
                    pending.push(Command::OpenPluginDocs);
                }
                if style::button_icon(
                    ui,
                    theme,
                    Icon::Settings,
                    app.state.tr(Key::ConfigDocs),
                    Variant::Ghost,
                )
                .clicked()
                {
                    pending.push(Command::OpenConfigDocs);
                }
            });
            ui.add_space(8.0);
            style::hint_line(ui, theme, app.state.tr(Key::ScriptingHint));
        },
    );
}

fn path_row(ui: &mut Ui, theme: &Theme, label: &str, path: &std::path::Path) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(10.0).color(theme.text_weak));
        ui.add_space(4.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add(
                egui::Label::new(
                    RichText::new(path.to_string_lossy().into_owned())
                        .monospace()
                        .size(9.5)
                        .color(style::weak_text(theme)),
                )
                .truncate(),
            );
        });
    });
}

fn plugin_badge(ui: &mut Ui, theme: &Theme, name: &str) {
    let font = style::font(10.0);
    let width = ui
        .painter()
        .layout_no_wrap(name.to_owned(), font.clone(), theme.text)
        .size()
        .x
        + 36.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 22.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(11), style::wash(theme));
    style::paint_icon(
        ui.painter(),
        Icon::Playlist,
        Rect::from_center_size(
            Pos2::new(rect.left() + 11.0, rect.center().y),
            Vec2::splat(11.0),
        ),
        theme.accent,
    );
    style::paint_elided(
        ui.painter(),
        Pos2::new(rect.left() + 19.0, rect.center().y),
        name,
        font,
        theme.text,
        width - 28.0,
    );
}

fn script_action_row(
    ui: &mut Ui,
    app: &EchoPortApp,
    theme: &Theme,
    action: &crate::scripting::ScriptActionInfo,
) -> bool {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 38.0), Sense::hover());
    let hover =
        ui.ctx()
            .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.10);
    if hover > 0.01 {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(style::R_SM),
            theme::with_alpha(theme.surface_hover, (220.0 * hover) as u8),
        );
    }
    style::paint_icon(
        ui.painter(),
        Icon::Play,
        Rect::from_center_size(
            Pos2::new(rect.left() + 14.0, rect.center().y),
            Vec2::splat(13.0),
        ),
        theme.accent,
    );
    let text_left = rect.left() + 28.0;
    let text_right = rect.right() - 62.0;
    style::paint_elided(
        ui.painter(),
        Pos2::new(text_left, rect.center().y - 7.0),
        &action.title,
        style::font(11.0),
        theme.text,
        text_right - text_left,
    );
    if let Some(description) = &action.description {
        style::paint_elided(
            ui.painter(),
            Pos2::new(text_left, rect.center().y + 8.0),
            description,
            style::font(9.5),
            style::weak_text(theme),
            text_right - text_left,
        );
    }
    let run_rect = Rect::from_center_size(
        Pos2::new(rect.right() - 30.0, rect.center().y),
        Vec2::new(52.0, 26.0),
    );
    style::button_at(
        ui,
        run_rect,
        ("script-run", action.id.as_str()),
        theme,
        app.state.tr(Key::RunAction),
        Variant::Tonal,
    )
}

// ---------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------

fn storage(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
    width: f32,
) {
    section_card(
        ui,
        theme,
        Icon::Download,
        app.state.tr(Key::StorageSection),
        |ui| {
            let size = app.ui.cache_size.unwrap_or(0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(app.state.tr(Key::CacheSize))
                            .size(12.0)
                            .color(theme.text),
                    );
                    ui.label(
                        RichText::new(format_bytes(size))
                            .size(17.0)
                            .color(theme.text),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if style::button(ui, theme, app.state.tr(Key::ClearCache), Variant::Secondary)
                        .clicked()
                    {
                        pending.push(Command::ClearCache);
                        app.ui.cache_size = Some(0);
                    }
                });
            });
            ui.add_space(10.0);
            ui.label(
                RichText::new(app.state.tr(Key::CacheLimitTitle))
                    .size(11.0)
                    .color(theme.text_weak),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let current = app.state.profile.settings.cache_limit_mb;
                for (value, label) in [
                    (0u32, app.state.tr(Key::CacheNone).to_owned()),
                    (512, "512 MB".to_owned()),
                    (1024, "1 GB".to_owned()),
                    (2048, "2 GB".to_owned()),
                ] {
                    let selected = current == value;
                    if style::chip(ui, theme, &label, None, selected).clicked() && !selected {
                        pending.push(Command::SetCacheLimit(value));
                        app.ui.cache_size = Some(size);
                    }
                }
            });
            ui.add_space(6.0);
            style::hint_line(ui, theme, app.state.tr(Key::CacheHint));
            style::hint_line(ui, theme, app.state.tr(Key::CacheLimitHint));
            let _ = width;
        },
    );
}

fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let bytes = bytes as f64;
    if bytes >= GB {
        format!("{:.1} GB", bytes / GB)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes / MB)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes / KB)
    } else {
        format!("{bytes:.0} B")
    }
}

// ---------------------------------------------------------------------------
// About
// ---------------------------------------------------------------------------

fn about(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
    width: f32,
) {
    section_card(ui, theme, Icon::Info, app.state.tr(Key::About), |ui| {
        ui.horizontal(|ui| {
            let font = style::font(20.0);
            let galley = ui
                .painter()
                .layout_no_wrap("EchoPort".to_owned(), font, theme.text);
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(galley.size().x + 4.0, 26.0), Sense::hover());
            ui.painter().galley(
                Pos2::new(rect.left(), rect.center().y - galley.size().y * 0.5),
                galley.clone(),
                theme.text,
            );
            ui.label(
                RichText::new(format!("v{}", app.version()))
                    .size(10.5)
                    .color(theme.text_weak),
            );
        });
        ui.add_space(4.0);
        ui.label(
            RichText::new(app.state.tr(Key::BuiltWithRust))
                .size(11.0)
                .color(theme.text_weak),
        );
        ui.add_space(6.0);
        ui.label(
            RichText::new(app.state.tr(Key::Credits))
                .size(10.5)
                .color(theme.text_weak),
        );
        ui.label(
            RichText::new(app.state.tr(Key::IconCredits))
                .size(10.5)
                .color(theme.text_weak),
        );
        ui.label(
            RichText::new(app.state.tr(Key::MadeBy))
                .size(10.5)
                .color(theme.text_weak),
        );
        ui.add_space(12.0);
        ui.label(
            RichText::new(app.state.tr(Key::KeyboardShortcuts))
                .size(11.0)
                .color(theme.text_weak),
        );
        ui.add_space(4.0);
        ui.label(
            RichText::new(app.state.tr(Key::KeyboardHint))
                .size(10.5)
                .color(theme.text_weak),
        );
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if let Some(info) = app.state.updates.available.as_ref() {
                ui.label(
                    RichText::new(format!(
                        "{} · v{}",
                        app.state.tr(Key::UpdateAvailable),
                        info.latest
                    ))
                    .size(10.5)
                    .color(theme.accent),
                );
                if style::button(ui, theme, app.state.tr(Key::UpdateNow), Variant::Primary)
                    .clicked()
                {
                    pending.push(Command::OpenUpdatePage);
                }
            } else if app.state.updates.checking {
                ui.label(
                    RichText::new(app.state.tr(Key::CheckingForUpdates))
                        .size(10.5)
                        .color(theme.text_weak),
                );
            } else {
                if style::button(
                    ui,
                    theme,
                    app.state.tr(Key::CheckForUpdates),
                    Variant::Secondary,
                )
                .clicked()
                {
                    pending.push(Command::CheckForUpdates);
                }
                if app.state.updates.error.is_some() {
                    ui.label(
                        RichText::new(app.state.tr(Key::UpdateCheckFailed))
                            .size(10.0)
                            .color(theme.text_weak),
                    );
                } else if app.state.updates.last_checked.is_some() {
                    ui.label(
                        RichText::new(app.state.tr(Key::UpToDate))
                            .size(10.0)
                            .color(theme.text_weak),
                    );
                }
            }
        });
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if style::button(ui, theme, app.state.tr(Key::OpenDataFolder), Variant::Ghost).clicked()
            {
                pending.push(Command::OpenDataFolder);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if style::button(ui, theme, app.state.tr(Key::Quit), Variant::Ghost).clicked() {
                    pending.push(Command::Quit);
                }
            });
        });
        let _ = (width, Page::Settings);
    });
}

/// Unused type anchor so the palette parameter stays discoverable to callers.
#[allow(dead_code)]
fn _palette_anchor(_palette: Option<&Palette>) -> Color32 {
    Color32::TRANSPARENT
}
