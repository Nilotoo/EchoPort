//! EchoPort — a quiet, local-first port for music from many sources.
//!
//! The crate is split into small layers so the GUI, the audio engine and the
//! provider catalogues can evolve independently:
//!
//! * [`model`] / [`state`] — plain data shared by everything
//! * [`audio`] — cpal + symphonia playback engine
//! * [`resolver`] / [`download`] — yt-dlp powered playback and downloads
//! * [`providers`] / [`art`] / [`background`] — online catalogues and covers
//! * [`ui`] / [`theme`] / [`i18n`] — the egui interface
//! * [`store`] — JSON profiles under the platform data directory

pub mod app;
pub mod art;
pub mod audio;
pub mod background;
pub mod download;
pub mod fonts;
pub mod i18n;
pub mod library;
pub mod model;
pub mod net;
pub mod process;
pub mod providers;
pub mod resolver;
pub mod scripting;
pub mod state;
pub mod store;
pub mod theme;
pub mod ui;
pub mod update;
