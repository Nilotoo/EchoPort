//! Page implementations. Every page is a pure function of `AppState` plus the
//! ephemeral `UiState`; actions are collected as `Command`s and dispatched by
//! the shell after the frame has been laid out.

mod artist;
mod downloads;
mod home;
mod library;
mod now_playing;
mod playlists;
mod profile;
mod search;
mod settings;

use egui::{Pos2, Rect, Ui, Vec2};

use crate::app::EchoPortApp;
use crate::state::{Command, Page};
use crate::theme::Theme;
use crate::ui::style;

/// Widest content column on very large windows; keeps lines readable instead
/// of stretching everything edge to edge.
const MAX_CONTENT_WIDTH: f32 = 1560.0;

pub fn draw(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme, pending: &mut Vec<Command>) {
    let full = ui.max_rect();
    if full.width() > MAX_CONTENT_WIDTH + 8.0 {
        let rect = Rect::from_min_size(
            Pos2::new(
                full.left() + (full.width() - MAX_CONTENT_WIDTH) * 0.5,
                full.top(),
            ),
            Vec2::new(MAX_CONTENT_WIDTH, full.height()),
        );
        style::absolute(ui, rect, |ui| draw_page(ui, app, theme, pending));
    } else {
        draw_page(ui, app, theme, pending);
    }
}

fn draw_page(ui: &mut Ui, app: &mut EchoPortApp, theme: &Theme, pending: &mut Vec<Command>) {
    match app.state.page {
        Page::Home => home::draw(ui, app, theme, pending),
        Page::Search => search::draw(ui, app, theme, pending),
        Page::Library => library::draw(ui, app, theme, pending),
        Page::Playlists => playlists::draw(ui, app, theme, pending),
        Page::Downloads => downloads::draw(ui, app, theme, pending),
        Page::NowPlaying => now_playing::draw(ui, app, theme, pending),
        Page::Artist => artist::draw(ui, app, theme, pending),
        Page::Profile => profile::draw(ui, app, theme, pending),
        Page::Settings => settings::draw(ui, app, theme, pending),
    }
}
