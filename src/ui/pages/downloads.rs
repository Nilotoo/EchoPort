//! Downloads: active jobs with animated progress, completed list and format /
//! folder controls.

use egui::{
    Align, CornerRadius, Layout, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui, Vec2,
};

use crate::app::EchoPortApp;
use crate::i18n::Key;
use crate::model::{DownloadFormat, DownloadJob, JobState};
use crate::state::{Command, Page};
use crate::theme::{self, Theme};
use crate::ui::style::{self, Icon, Variant};
use crate::ui::widgets;

pub fn draw(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme, pending: &mut Vec<Command>) {
    widgets::page_scroll(ui, |ui| {
        header(ui, app, theme, pending);
        ui.add_space(12.0);
        controls(ui, app, theme, pending);
        ui.add_space(16.0);
        let jobs = app.state.downloads.jobs.clone();
        if jobs.is_empty() {
            style::empty_state(
                ui,
                theme,
                Icon::Download,
                app.state.tr(Key::DownloadsEmpty),
                app.state.tr(Key::DownloadsEmptyHint),
                None,
            );
            return;
        }
        let active: Vec<&DownloadJob> = jobs
            .iter()
            .filter(|job| matches!(job.state, JobState::Queued | JobState::Running))
            .collect();
        let finished: Vec<&DownloadJob> = jobs
            .iter()
            .filter(|job| matches!(job.state, JobState::Done | JobState::Failed))
            .collect();
        if !active.is_empty() {
            section_title(ui, theme, app.state.tr(Key::DownloadsActive), active.len());
            for job in active {
                job_row(ui, app, theme, pending, job, &[], None);
                ui.add_space(8.0);
            }
            ui.add_space(8.0);
        }
        if !finished.is_empty() {
            section_title(
                ui,
                theme,
                app.state.tr(Key::DownloadsCompleted),
                finished.len(),
            );
            let done_tracks: Vec<crate::model::Track> = finished
                .iter()
                .filter(|job| job.state == JobState::Done)
                .map(|job| job.track.clone())
                .collect();
            let mut done_index = 0usize;
            for job in finished {
                let play_pos = if job.state == JobState::Done {
                    let index = done_index;
                    done_index += 1;
                    Some(index)
                } else {
                    None
                };
                job_row(ui, app, theme, pending, job, &done_tracks, play_pos);
                ui.add_space(8.0);
            }
        }
        ui.add_space(8.0);
    });
}

fn header(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme, pending: &mut Vec<Command>) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(
                RichText::new(app.state.tr(Key::DownloadCenter))
                    .size(26.0)
                    .color(theme.text),
            );
            ui.label(
                RichText::new(app.state.tr(Key::DownloadsEmptyHint))
                    .size(11.5)
                    .color(theme.text_weak),
            );
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if style::button_icon(
                ui,
                theme,
                Icon::Folder,
                app.state.tr(Key::OpenFolder),
                Variant::Secondary,
            )
            .clicked()
            {
                pending.push(Command::OpenDownloadFolder);
            }
            let _ = Page::Downloads;
        });
    });
}

fn controls(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme, pending: &mut Vec<Command>) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 96.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(style::R_LG), theme.surface);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(style::R_LG),
        Stroke::new(1.0_f32, theme.border),
        StrokeKind::Inside,
    );
    let inner = rect.shrink2(Vec2::new(18.0, 14.0));
    style::absolute(ui, inner, |ui| {
        ui.set_min_size(inner.size());
        // Format row.
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(app.state.tr(Key::DownloadFormat))
                    .size(11.0)
                    .color(theme.text_weak),
            );
            ui.add_space(6.0);
            for format in DownloadFormat::ALL {
                let selected = app.state.profile.settings.download_format == format;
                if style::chip(ui, theme, format.label(), None, selected).clicked() && !selected {
                    pending.push(Command::SetDownloadFormat(format));
                }
            }
        });
        ui.add_space(10.0);
        // Folder row.
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(app.state.tr(Key::DownloadFolder))
                    .size(11.0)
                    .color(theme.text_weak),
            );
            ui.add_space(6.0);
            let dir = app
                .state
                .profile
                .settings
                .download_dir
                .to_string_lossy()
                .into_owned();
            let available = ui.available_width();
            style::paint_elided(
                ui.painter(),
                Pos2::new(ui.cursor().min.x, ui.cursor().min.y + 8.0),
                &dir,
                style::font(11.0),
                theme.text,
                available - 220.0,
            );
            ui.add_space(available - 218.0);
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
    });
}

fn section_title(ui: &mut Ui, theme: &Theme, label: &str, count: usize) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(15.0).strong().color(theme.text));
        ui.label(
            RichText::new(format!("{count}"))
                .size(10.5)
                .color(theme.text_weak),
        );
    });
    ui.add_space(6.0);
}

fn job_row(
    ui: &mut Ui,
    app: &mut EchoPortApp,
    theme: &Theme,
    pending: &mut Vec<Command>,
    job: &DownloadJob,
    done_tracks: &[crate::model::Track],
    play_pos: Option<usize>,
) {
    let active = matches!(job.state, JobState::Queued | JobState::Running);
    let width = ui.available_width();
    let height = 78.0;
    let (rect, body) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    let selected =
        play_pos.is_some() && app.ui.selected_track.as_deref() == Some(job.track.id.as_str());
    let fill = if selected {
        theme::with_alpha(theme.accent_soft, 110)
    } else {
        theme.surface
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(style::R_MD), fill);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(style::R_MD),
        Stroke::new(1.0_f32, theme.border),
        StrokeKind::Inside,
    );
    let cover_rect = Rect::from_center_size(
        Pos2::new(rect.left() + 34.0, rect.center().y),
        Vec2::splat(42.0),
    );
    widgets::paint_track_cover(ui, app, theme, &job.track, cover_rect, style::R_SM);
    let text_left = cover_rect.right() + 12.0;
    let reserve = if job.state == JobState::Failed {
        196.0
    } else {
        132.0
    };
    let text_right = (rect.right() - reserve).max(text_left + 80.0);
    style::paint_elided(
        ui.painter(),
        Pos2::new(text_left, rect.center().y - 18.0),
        &job.track.title,
        style::font(13.0),
        theme.text,
        text_right - text_left,
    );
    style::paint_elided(
        ui.painter(),
        Pos2::new(text_left, rect.center().y - 2.0),
        job.track.display_artist(),
        style::font(10.5),
        theme.text_weak,
        text_right - text_left,
    );
    let bar_rect = Rect::from_min_size(
        Pos2::new(text_left, rect.center().y + 12.0),
        Vec2::new((text_right - text_left - 46.0).max(60.0), 5.0),
    );
    if active {
        let progress = job.progress.clamp(0.0, 1.0);
        let animated = ui.ctx().animate_value_with_time(
            ui.id().with(("dl-progress", &job.track.id)),
            progress,
            0.25,
        );
        ui.painter().rect_filled(
            bar_rect,
            CornerRadius::same(3),
            theme::with_alpha(theme.border, 190),
        );
        if job.state == JobState::Queued {
            let time = ui.ctx().input(|input| input.time) as f32;
            let width = bar_rect.width() * 0.22;
            let offset = (bar_rect.width() - width) * (0.5 + 0.5 * (time * 1.1).sin());
            ui.painter().rect_filled(
                Rect::from_min_size(
                    Pos2::new(bar_rect.left() + offset, bar_rect.top()),
                    Vec2::new(width, bar_rect.height()),
                ),
                CornerRadius::same(3),
                theme::with_alpha(theme.highlight, 210),
            );
        } else {
            ui.painter().rect_filled(
                Rect::from_min_size(
                    bar_rect.min,
                    Vec2::new(bar_rect.width() * animated, bar_rect.height()),
                ),
                CornerRadius::same(3),
                theme.accent,
            );
        }
        style::paint_elided_right(
            ui.painter(),
            Pos2::new(bar_rect.right() + 42.0, bar_rect.center().y),
            &format!("{:.0}%", progress * 100.0),
            style::font(10.0),
            theme.text_weak,
            40.0,
        );
        let stage = if job.stage.is_empty() {
            app.state.tr(if job.state == JobState::Queued {
                Key::QueuedJob
            } else {
                Key::Downloading
            })
        } else {
            job.stage.as_str()
        };
        style::paint_elided(
            ui.painter(),
            Pos2::new(text_left, rect.center().y + 26.0),
            stage,
            style::font(9.5),
            theme.text_weak,
            (text_right - text_left - 30.0).max(60.0),
        );
    }
    // Right side: status badge, plus retry for failures.
    let (state_label, color) = match job.state {
        JobState::Queued => (app.state.tr(Key::QueuedJob), theme.text_weak),
        JobState::Running => (app.state.tr(Key::Downloading), theme.accent),
        JobState::Done => (app.state.tr(Key::JobDone), style::success_color(theme)),
        JobState::Failed => (app.state.tr(Key::FailedJob), style::danger_color(theme)),
    };
    let badge_width = 106.0;
    let badge_rect = Rect::from_center_size(
        Pos2::new(rect.right() - 18.0 - badge_width * 0.5, rect.center().y),
        Vec2::new(badge_width, 26.0),
    );
    ui.painter().rect_filled(
        badge_rect,
        CornerRadius::same(13),
        theme::with_alpha(color, 30),
    );
    style::paint_icon(
        ui.painter(),
        if job.state == JobState::Done {
            Icon::Check
        } else if job.state == JobState::Failed {
            Icon::Info
        } else {
            Icon::Clock
        },
        Rect::from_center_size(
            Pos2::new(badge_rect.left() + 16.0, badge_rect.center().y),
            Vec2::splat(13.0),
        ),
        color,
    );
    style::paint_elided(
        ui.painter(),
        Pos2::new(badge_rect.left() + 27.0, badge_rect.center().y),
        state_label,
        style::font(9.5),
        color,
        badge_width - 32.0,
    );
    if job.state == JobState::Failed {
        let retry_rect = Rect::from_center_size(
            Pos2::new(badge_rect.left() - 8.0 - 58.0, rect.center().y),
            Vec2::new(58.0, 28.0),
        );
        if style::button_at(
            ui,
            retry_rect,
            ("dl-retry", &job.track.id),
            theme,
            app.state.tr(Key::Retry),
            Variant::Secondary,
        ) {
            pending.push(Command::Download(vec![job.track.clone()]));
        }
        if let Some(error) = &job.error {
            style::paint_elided_right(
                ui.painter(),
                Pos2::new(rect.right() - 120.0, rect.center().y + 18.0),
                error,
                style::font(9.5),
                style::danger_color(theme),
                160.0,
            );
        }
    }
    if job.state == JobState::Done {
        if let Some(path) = &job.path {
            style::paint_elided_right(
                ui.painter(),
                Pos2::new(badge_rect.left() - 14.0, rect.center().y),
                &path.to_string_lossy(),
                style::font(9.5),
                theme.text_weak,
                220.0,
            );
        }
    }
    if let Some(index) = play_pos {
        let body = body
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(app.state.tr(Key::DoubleClickPlay));
        if body.clicked() {
            app.ui.selected_track = Some(job.track.id.clone());
            app.ui.selected_playlist = None;
        }
        if body.double_clicked() {
            pending.push(widgets::play_from(done_tracks, index));
        }
    }
    // A small progress sanity load: unused variable guard.
    let _ = active;
}
