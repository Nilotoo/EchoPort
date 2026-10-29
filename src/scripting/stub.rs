//! Lean-build no-op scripting backend.
//!
//! It deliberately exposes the same public types/methods as the Lua backend so
//! the rest of the application remains feature-agnostic; no Lua runtime is
//! linked, no config file is parsed and no custom actions/bindings exist.

use std::path::{Path, PathBuf};

use crossbeam_channel::Sender;

use crate::i18n::Language;
use crate::model::{DownloadFormat, Track};
use crate::state::{AppEvent, Page};
use crate::theme::ThemeMode;

use super::keys::{BindingAction, KeyBinding};

pub type EventSender = Sender<AppEvent>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptPaths {
    pub config_file: PathBuf,
    pub plugins_dir: PathBuf,
}

impl Default for ScriptPaths {
    fn default() -> Self {
        Self::resolve()
    }
}

impl ScriptPaths {
    pub fn resolve() -> Self {
        let root = dirs::config_dir()
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(".config")
            })
            .join("echoport");
        Self::under(root)
    }

    pub fn under(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            config_file: root.join("config.lua"),
            plugins_dir: root.join("plugins"),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ScriptConfig {
    pub language: Option<Language>,
    pub theme_mode: Option<ThemeMode>,
    pub follow_cover_accent: Option<bool>,
    pub accent: Option<egui::Color32>,
    pub macaron: Option<[egui::Color32; 5]>,
    pub highlight: Option<egui::Color32>,
    pub startup_page: Option<Page>,
    pub ui_scale: Option<f32>,
    pub cache_limit_mb: Option<u32>,
    pub volume: Option<f32>,
    pub seek_step: Option<f64>,
    pub volume_step: Option<f32>,
    pub results_per_platform: Option<usize>,
    pub download_format: Option<DownloadFormat>,
    pub download_dir: Option<PathBuf>,
    pub music_dirs: Option<Vec<PathBuf>>,
    pub bindings: Vec<KeyBinding>,
    pub actions: Vec<ScriptActionInfo>,
    pub warnings: Vec<String>,
    pub config_file: PathBuf,
    pub plugins_dir: PathBuf,
    pub loaded: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptActionInfo {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
}

#[derive(Clone, Debug)]
pub enum ScriptEvent {
    Startup,
    TrackStart(Track),
    TrackEnd(Track),
    SearchDone(String),
    DownloadDone(Track, PathBuf),
    LibraryScanned(usize),
    PlaybackPaused,
    PlaybackResumed,
}

#[allow(dead_code)]
impl ScriptEvent {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Startup => "startup",
            Self::TrackStart(_) => "track_start",
            Self::TrackEnd(_) => "track_end",
            Self::SearchDone(_) => "search_done",
            Self::DownloadDone(_, _) => "download_done",
            Self::LibraryScanned(_) => "library_scanned",
            Self::PlaybackPaused => "playback_paused",
            Self::PlaybackResumed => "playback_resumed",
        }
    }
}

pub trait Plugin: Send {
    fn id(&self) -> &str;
    fn on_event(&mut self, event: &ScriptEvent, host: &mut PluginHost);
}

#[allow(dead_code)]
pub struct PluginHost {
    sender: Option<EventSender>,
}

impl PluginHost {
    fn new(sender: Option<EventSender>) -> Self {
        Self { sender }
    }
}

#[derive(Default)]
pub struct PluginRegistry {
    plugins: Vec<Box<dyn Plugin>>,
    sender: Option<EventSender>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_sender(sender: EventSender) -> Self {
        Self {
            plugins: Vec::new(),
            sender: Some(sender),
        }
    }

    pub fn set_sender(&mut self, sender: EventSender) {
        self.sender = Some(sender);
    }

    pub fn register(&mut self, plugin: Box<dyn Plugin>) {
        self.plugins.push(plugin);
    }

    pub fn fire(&mut self, event: &ScriptEvent) {
        if self.plugins.is_empty() {
            return;
        }
        for plugin in self.plugins.iter_mut() {
            let mut host = PluginHost::new(self.sender.clone());
            plugin.on_event(event, &mut host);
        }
    }

    pub fn plugins(&self) -> Vec<String> {
        self.plugins
            .iter()
            .map(|plugin| plugin.id().to_owned())
            .collect()
    }
}

pub struct Scripts {
    config: ScriptConfig,
}

impl Default for Scripts {
    fn default() -> Self {
        Self::empty()
    }
}

impl Scripts {
    pub fn empty() -> Self {
        Self {
            config: ScriptConfig::default(),
        }
    }

    pub fn load(paths: ScriptPaths, _tx: EventSender) -> Self {
        let mut config = ScriptConfig::default();
        config.config_file = paths.config_file;
        config.plugins_dir = paths.plugins_dir;
        config.loaded = false;
        config
            .warnings
            .push("Lean build: Lua scripting/plugins are not included.".to_owned());
        Self { config }
    }

    pub fn config(&self) -> &ScriptConfig {
        &self.config
    }

    pub fn reload(&mut self, _tx: EventSender) {}

    pub fn fire(&mut self, _event: &ScriptEvent, _tx: &EventSender) {}

    pub fn run_action(&mut self, _action_id: &str, _tx: &EventSender) -> bool {
        false
    }

    pub fn poll_binding(&self, _ctx: &egui::Context) -> Option<BindingAction> {
        None
    }

    pub fn set_now_playing(&mut self, _track: Option<Track>) {}

    pub fn config_file(&self) -> &Path {
        &self.config.config_file
    }

    pub fn plugins_dir(&self) -> &Path {
        &self.config.plugins_dir
    }

    pub fn loaded_plugins(&self) -> Vec<String> {
        Vec::new()
    }
}
