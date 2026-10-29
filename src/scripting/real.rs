//! Lua configuration, key bindings and the plugin interface.
//!
//! This module owns the user facing scripting layer:
//!
//! * `<config>/echoport/config.lua` — startup options, theme settings, keyboard
//!   shortcuts, playback tuning and input/output paths.
//! * `<config>/echoport/plugins/*.lua` — user plugins (loaded in sorted order).
//! * Rust plugins through [`Plugin`] / [`PluginRegistry`] for compiled-in
//!   extensions.
//!
//! The Lua API is documented in `docs/SCRIPTING.md` and `docs/PLUGINS.md`.
//! Nothing in here may panic on user supplied data: every script call is
//! protected and failures are collected as [`ScriptConfig::warnings`].
//!
//! # Lifecycle
//!
//! [`Scripts::load`] creates the config directory, writes a commented default
//! `config.lua` when it is missing, executes it in one fresh Lua state and then
//! loads every `plugins/*.lua` file (sorted by file name, each in its own
//! protected execution). The application later calls [`Scripts::fire`] for
//! lifecycle events, [`Scripts::poll_binding`] once per frame and
//! [`Scripts::run_action`] for custom actions. [`Scripts::reload`] rebuilds the
//! whole Lua state; if the new configuration file fails, the previous
//! configuration and callbacks are kept.
//!
//! The struct is deliberately not `Send`: `mlua::Lua` is thread bound (without
//! the optional `send` feature), so keep one [`Scripts`] inside the UI thread /
//! `EchoPortApp`. Rust [`Plugin`]s are `Send` and can be fed from anywhere.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use mlua::{Lua, MultiValue, RegistryKey, Value};

use crate::i18n::Language;
use crate::model::{DownloadFormat, Track};
use crate::state::{AppEvent, Page};
use crate::theme::ThemeMode;

#[path = "config.rs"]
mod config;
#[path = "lua_api.rs"]
mod lua_api;
#[path = "plugins.rs"]
pub mod plugins;

use crate::scripting::keys::{BindingAction, KeyBinding, KeyChord};
pub use plugins::{Plugin, PluginHost, PluginRegistry};

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

/// Where the Lua configuration and plugin directory live.
///
/// Resolve one with [`ScriptPaths::resolve`]; tests can point both paths at a
/// temporary directory.
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
    /// `<config_dir>/echoport/config.lua` plus the matching `plugins` folder.
    pub fn resolve() -> Self {
        let config_root = dirs::config_dir().unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".config")
        });
        Self::under(config_root.join("echoport"))
    }

    /// Build both paths below a single `echoport` directory (used by tests and
    /// by portable builds).
    pub fn under(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            config_file: root.join("config.lua"),
            plugins_dir: root.join("plugins"),
        }
    }
}

/// Channel used to talk to the application (the same channel as the worker
/// threads use).
pub type EventSender = crossbeam_channel::Sender<AppEvent>;

// ---------------------------------------------------------------------------
// Public configuration vocabulary
// ---------------------------------------------------------------------------

/// Everything the application configuration can override from `config.lua`.
///
/// Every field is optional; `None` means "keep the value from the profile /
/// defaults". Invalid values never end up here — they are reported through
/// [`ScriptConfig::warnings`] instead.
#[derive(Clone, Debug, Default)]
pub struct ScriptConfig {
    pub language: Option<Language>,
    pub theme_mode: Option<ThemeMode>,
    pub follow_cover_accent: Option<bool>,
    pub accent: Option<egui::Color32>,
    pub macaron: Option<[egui::Color32; 5]>,
    pub highlight: Option<egui::Color32>,
    pub startup_page: Option<Page>,
    /// Whole-UI zoom factor (1.0 = 100%). Range `0.8..=2.0`; the Settings
    /// page offers 100% / 110% / 125% / 150% presets.
    pub ui_scale: Option<f32>,
    /// Audio cache cap in MiB. `0` = don't keep played tracks on disk.
    /// Range `0..=10240`; the Settings page offers 0 / 512 / 1024 / 2048 MB.
    pub cache_limit_mb: Option<u32>,
    pub volume: Option<f32>,
    pub seek_step: Option<f64>,
    pub volume_step: Option<f32>,
    pub results_per_platform: Option<usize>,
    pub download_format: Option<DownloadFormat>,
    pub download_dir: Option<PathBuf>,
    pub music_dirs: Option<Vec<PathBuf>>,
    /// Key bindings, in priority order (first match wins).
    pub bindings: Vec<KeyBinding>,
    /// Custom actions registered from Lua.
    pub actions: Vec<ScriptActionInfo>,
    /// Non fatal problems collected while loading / running scripts.
    pub warnings: Vec<String>,
    pub config_file: PathBuf,
    pub plugins_dir: PathBuf,
    /// True when `config.lua` was read and executed successfully.
    pub loaded: bool,
}

/// Description of a custom action registered by a Lua script.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptActionInfo {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
}

/// A lifecycle event delivered to Lua handlers and Rust plugins.
#[derive(Clone, Debug)]
pub enum ScriptEvent {
    /// The app finished starting up.
    Startup,
    /// A track started playing.
    TrackStart(Track),
    /// A track finished playing.
    TrackEnd(Track),
    /// A search finished (`SearchDone`); carries the query string.
    SearchDone(String),
    /// A download finished; carries the track and the written file.
    DownloadDone(Track, PathBuf),
    /// A local library scan finished; carries the number of tracks found.
    LibraryScanned(usize),
    /// Playback was paused by the user.
    PlaybackPaused,
    /// Playback was resumed by the user.
    PlaybackResumed,
}

impl ScriptEvent {
    /// The Lua event name (`"track_start"`, …).
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

pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");

// ---------------------------------------------------------------------------
// Loaded runtime
// ---------------------------------------------------------------------------

struct Handler {
    file: String,
    func: RegistryKey,
}

struct Action {
    info: ScriptActionInfo,
    func: RegistryKey,
    file: String,
}

struct Binding {
    key_binding: KeyBinding,
    file: String,
}

struct Runtime {
    lua: Lua,
    handlers: HashMap<&'static str, Vec<Handler>>,
    actions: HashMap<String, Action>,
}

/// Intermediate state while loading (`registry keys` need the Lua state to
/// exist first, so the registration list is reconciled after execution).
#[derive(Default)]
struct Parts {
    handlers: HashMap<&'static str, Vec<Handler>>,
    actions: HashMap<String, Action>,
    action_order: Vec<String>,
    bindings: Vec<Binding>,
}

impl Parts {
    fn reconcile(&mut self, registrations: Vec<lua_api::Registration>, warnings: &mut Vec<String>) {
        for registration in registrations {
            match registration {
                lua_api::Registration::Event { event, func, file } => {
                    self.handlers
                        .entry(event)
                        .or_default()
                        .push(Handler { file, func });
                }
                lua_api::Registration::Action {
                    info,
                    func,
                    file,
                    keys,
                } => self.add_action(info, func, file, keys, warnings),
                lua_api::Registration::Binding {
                    chord,
                    action,
                    file,
                } => {
                    self.add_binding(KeyBinding::new(chord, action), file, warnings);
                }
            }
        }
    }

    fn add_action(
        &mut self,
        info: ScriptActionInfo,
        func: RegistryKey,
        file: String,
        keys: Vec<String>,
        warnings: &mut Vec<String>,
    ) {
        let id = info.id.clone();
        if self.actions.contains_key(&id) {
            warnings.push(format!(
                "{file}: duplicate action id {id:?}; keeping the last definition"
            ));
        } else {
            self.action_order.push(id.clone());
        }
        self.actions.insert(
            id.clone(),
            Action {
                info,
                func,
                file: file.clone(),
            },
        );
        for key in keys {
            match KeyChord::parse(&key) {
                Some(chord) => self.add_binding(
                    KeyBinding::new(chord, BindingAction::Custom(id.clone())),
                    file.clone(),
                    warnings,
                ),
                None => warnings.push(format!(
                    "{file}: action {id:?} has an invalid key {key:?} (ignored)"
                )),
            }
        }
    }

    fn add_binding(&mut self, binding: KeyBinding, file: String, warnings: &mut Vec<String>) {
        if let Some(existing) = self
            .bindings
            .iter_mut()
            .find(|existing| existing.key_binding.chord == binding.chord)
        {
            warnings.push(format!(
                "{file}: chord {} was already bound to {}; replacing it with {}",
                binding.chord.label(),
                existing.key_binding.action.spec(),
                binding.action.spec()
            ));
            existing.key_binding = binding;
            existing.file = file;
        } else {
            self.bindings.push(Binding {
                key_binding: binding,
                file,
            });
        }
    }

    /// Drop bindings that point at custom actions which were never registered.
    fn finalize_custom_bindings(&mut self, warnings: &mut Vec<String>) {
        let bindings = std::mem::take(&mut self.bindings);
        self.bindings = bindings
            .into_iter()
            .filter(|binding| match binding.key_binding.action.custom_id() {
                Some(id) if !self.actions.contains_key(id) => {
                    warnings.push(format!(
                        "{}: binding {} references unknown action {id:?} (ignored)",
                        binding.file,
                        binding.key_binding.chord.label()
                    ));
                    false
                }
                _ => true,
            })
            .collect();
    }

    fn action_infos(&self) -> Vec<ScriptActionInfo> {
        self.action_order
            .iter()
            .filter_map(|id| self.actions.get(id))
            .map(|action| action.info.clone())
            .collect()
    }
}

/// Result of one full load attempt.
struct Built {
    config: ScriptConfig,
    runtime: Option<Runtime>,
    shared: Option<Rc<lua_api::Shared>>,
    base_warnings: Vec<String>,
    loaded_plugins: Vec<String>,
    /// `Some(message)` when `config.lua` itself raised an error. The initial
    /// load still runs plugins; [`Scripts::reload`] keeps the previous state.
    fatal: Option<String>,
}

// ---------------------------------------------------------------------------
// Scripts
// ---------------------------------------------------------------------------

/// The loaded Lua configuration, key bindings, actions and plugins.
///
/// Create one with [`Scripts::load`] (or [`Scripts::empty`] for demo /
/// screenshot builds that must not touch the user's files) and keep it in the
/// application struct on the UI thread.
pub struct Scripts {
    paths: ScriptPaths,
    config: ScriptConfig,
    runtime: Option<Runtime>,
    shared: Option<Rc<lua_api::Shared>>,
    /// Warnings produced by the loader itself (file errors etc.). Warnings
    /// produced by callbacks live in `shared`; [`Self::sync_warnings`] merges
    /// both into `config.warnings`.
    base_warnings: Vec<String>,
    now_playing: Option<Track>,
    loaded_plugins: Vec<String>,
}

impl Scripts {
    /// Load `config.lua` and all Lua plugins below `paths`.
    ///
    /// Never fails and never panics: an unreadable directory, a syntax error or
    /// a crashing plugin all end up in [`ScriptConfig::warnings`].
    pub fn load(paths: ScriptPaths, tx: EventSender) -> Self {
        let mut scripts = Self::with_paths(paths);
        let built = build(&scripts.paths, tx, None);
        scripts.apply(built);
        scripts
    }

    /// An inert instance without file IO or a Lua state.
    ///
    /// Used by demo / screenshot runs so the output does not depend on the
    /// user's configuration. UI code can keep calling [`Self::fire`] and
    /// [`Self::poll_binding`]; they are cheap no-ops.
    pub fn empty() -> Self {
        Self::with_paths(ScriptPaths::resolve())
    }

    fn with_paths(paths: ScriptPaths) -> Self {
        let config = ScriptConfig {
            config_file: paths.config_file.clone(),
            plugins_dir: paths.plugins_dir.clone(),
            ..ScriptConfig::default()
        };
        Self {
            paths,
            config,
            runtime: None,
            shared: None,
            base_warnings: Vec::new(),
            now_playing: None,
            loaded_plugins: Vec::new(),
        }
    }

    /// The parsed configuration (apply this at startup / after reload).
    pub fn config(&self) -> &ScriptConfig {
        &self.config
    }

    /// Rebuild the whole Lua state, re-run `config.lua` and reload all plugins.
    ///
    /// If `config.lua` now fails to load, the previous configuration and
    /// callbacks are kept and the failure is reported through
    /// [`ScriptConfig::warnings`].
    pub fn reload(&mut self, tx: EventSender) {
        let previous_config = self.config.clone();
        let built = build(&self.paths, tx, self.now_playing.clone());
        if built.fatal.is_some() && self.runtime.is_some() {
            // Keep the previous Lua state (plugins, handlers, actions) and the
            // previous settings; only report why the reload failed.
            self.base_warnings = built.base_warnings;
            self.config = previous_config;
            self.sync_warnings();
            log::warn!("Lua configuration reload failed; keeping the previous configuration");
            return;
        }
        self.apply(built);
        log::info!(
            "Lua scripts loaded: {} plugin(s), {} action(s), {} binding(s)",
            self.loaded_plugins.len(),
            self.config.actions.len(),
            self.config.bindings.len()
        );
    }

    /// Deliver a lifecycle event to the matching Lua handlers.
    ///
    /// Errors inside a handler are appended to the warning list and logged; the
    /// remaining handlers still run.
    pub fn fire(&mut self, event: &ScriptEvent, _tx: &EventSender) {
        if let Some(runtime) = self.runtime.as_ref() {
            if let Some(handlers) = runtime.handlers.get(event.name()) {
                for handler in handlers {
                    let outcome = lua_api::event_args(&runtime.lua, event)
                        .map_err(lua_api::format_lua_error)
                        .and_then(|args| lua_api::call_function(&runtime.lua, &handler.func, args));
                    if let Err(message) = outcome {
                        log::warn!("[lua] {}: {message}", handler.file);
                        if let Some(shared) = &self.shared {
                            shared.push_warning(format!("{}: {message}", handler.file));
                        }
                    }
                }
            }
        }
        self.sync_warnings();
    }

    /// Run a custom action registered with `echoport.actions.add`.
    ///
    /// Returns `true` when an action with this id existed. A Lua error inside
    /// the action yields a warning, not a panic.
    pub fn run_action(&mut self, action_id: &str, _tx: &EventSender) -> bool {
        let mut found = false;
        if let Some(runtime) = self.runtime.as_ref() {
            if let Some(action) = runtime.actions.get(action_id) {
                found = true;
                let outcome = lua_api::call_function(&runtime.lua, &action.func, MultiValue::new());
                if let Err(message) = outcome {
                    log::warn!(
                        "[lua] {}: action `{action_id}` failed: {message}",
                        action.file
                    );
                    if let Some(shared) = &self.shared {
                        shared.push_warning(format!(
                            "{}: action {action_id:?} failed: {message}",
                            action.file
                        ));
                    }
                }
            }
        }
        self.sync_warnings();
        found
    }

    /// Poll the keyboard for the first matching key binding.
    ///
    /// Call once per frame, before/next to the app's own shortcuts. The poll is
    /// suppressed while a text field has keyboard focus so typing a space in
    /// the search box does not toggle playback. The caller dispatches the
    /// returned action (for [`BindingAction::Custom`], call [`Self::run_action`]
    /// with the id).
    pub fn poll_binding(&self, ctx: &egui::Context) -> Option<BindingAction> {
        if ctx.wants_keyboard_input() {
            return None;
        }
        ctx.input(|input| {
            self.config
                .bindings
                .iter()
                .find(|binding| binding.chord.matches(input))
                .map(|binding| binding.action.clone())
        })
    }

    /// Remember the current track (exposed to Lua as `echoport.current_track()`).
    pub fn set_now_playing(&mut self, track: Option<Track>) {
        self.now_playing = track.clone();
        if let Some(shared) = &self.shared {
            *shared.now_playing.borrow_mut() = track;
        }
    }

    pub fn config_file(&self) -> &Path {
        &self.paths.config_file
    }

    pub fn plugins_dir(&self) -> &Path {
        &self.paths.plugins_dir
    }

    /// File names of the plugins that loaded without an error (sorted).
    pub fn loaded_plugins(&self) -> Vec<String> {
        self.loaded_plugins.clone()
    }

    fn apply(&mut self, built: Built) {
        self.config = built.config;
        self.runtime = built.runtime;
        self.shared = built.shared;
        self.base_warnings = built.base_warnings;
        self.loaded_plugins = built.loaded_plugins;
        if let Some(shared) = &self.shared {
            *shared.now_playing.borrow_mut() = self.now_playing.clone();
        }
        self.sync_warnings();
    }

    fn sync_warnings(&mut self) {
        let mut warnings = self.base_warnings.clone();
        if let Some(shared) = &self.shared {
            warnings.extend(shared.warnings.borrow().iter().cloned());
        }
        cap_warnings(&mut warnings);
        self.config.warnings = warnings;
    }
}

fn cap_warnings(warnings: &mut Vec<String>) {
    if warnings.len() > lua_api::MAX_WARNINGS {
        warnings.truncate(lua_api::MAX_WARNINGS);
        warnings.push("… further script warnings suppressed".to_owned());
    }
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

fn build(paths: &ScriptPaths, tx: EventSender, now_playing: Option<Track>) -> Built {
    let mut warnings = Vec::new();
    let (mut preparation, _created) = config::prepare_files(paths);
    warnings.append(&mut preparation);

    let mut config = ScriptConfig {
        config_file: paths.config_file.clone(),
        plugins_dir: paths.plugins_dir.clone(),
        ..ScriptConfig::default()
    };

    let lua = Lua::new();
    let shared = lua_api::Shared::new(tx);
    shared.set_file("config.lua");
    *shared.now_playing.borrow_mut() = now_playing;

    if let Err(error) = lua_api::install(&lua, &shared, paths) {
        warnings.push(format!(
            "internal error: cannot install the Lua scripting API: {error}"
        ));
        return Built {
            config,
            runtime: None,
            shared: Some(shared),
            base_warnings: warnings,
            loaded_plugins: Vec::new(),
            fatal: Some("Lua scripting API could not be installed".to_owned()),
        };
    }

    let mut parts = Parts::default();
    let mut fatal = None;

    match std::fs::read_to_string(&paths.config_file) {
        Err(error) => warnings.push(format!(
            "config.lua: cannot read {}: {error}",
            paths.config_file.display()
        )),
        Ok(source) => {
            let chunk_name = paths.config_file.to_string_lossy().into_owned();
            match lua.load(&source).set_name(chunk_name).exec() {
                Ok(()) => {
                    config.loaded = true;
                    match lua_api::config_value(&lua) {
                        Ok(Some(table)) => {
                            let parsed = config::parse(&mut config, &table);
                            warnings.extend(parsed.warnings);
                            let mut registrations = keyboard_registrations(
                                &lua,
                                &shared,
                                parsed.keyboard,
                                &mut warnings,
                            );
                            registrations.extend(shared.take_registrations());
                            parts.reconcile(registrations, &mut warnings);
                        }
                        Ok(None) => {
                            parts.reconcile(shared.take_registrations(), &mut warnings);
                        }
                        Err(error) => {
                            warnings.push(format!("config.lua: {error}"));
                            // Keep callbacks that were registered directly on
                            // the echoport table; a broken config table should
                            // not throw them away.
                            parts.reconcile(shared.take_registrations(), &mut warnings);
                        }
                    }
                }
                Err(error) => {
                    // Syntax / runtime error in config.lua: fatal for reloads.
                    let message = format!("config.lua: {}", lua_api::format_lua_error(error));
                    warnings.push(message.clone());
                    fatal = Some(message);
                    // Discard half-registered callbacks from the broken file.
                    shared.take_registrations();
                }
            }
        }
    }

    // Plugins: sorted by file name, each executed in its own protected call.
    let mut loaded_plugins = Vec::new();
    for file in plugin_files(&paths.plugins_dir, &mut warnings) {
        let file_name = file
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("plugin.lua")
            .to_owned();
        shared.set_file(file_name.clone());
        match std::fs::read_to_string(&file) {
            Err(error) => warnings.push(format!("{file_name}: cannot read plugin: {error}")),
            Ok(source) => {
                let chunk_name = format!("@{}", file.display());
                match lua.load(&source).set_name(chunk_name).exec() {
                    Ok(()) => loaded_plugins.push(file_name),
                    Err(error) => {
                        warnings.push(format!("{file_name}: {}", lua_api::format_lua_error(error)))
                    }
                }
            }
        }
    }
    shared.set_file("config.lua");
    parts.reconcile(shared.take_registrations(), &mut warnings);
    parts.finalize_custom_bindings(&mut warnings);

    config.actions = parts.action_infos();
    config.bindings = parts
        .bindings
        .iter()
        .map(|binding| binding.key_binding.clone())
        .collect();
    let runtime = Runtime {
        lua,
        handlers: parts.handlers,
        actions: parts.actions,
    };

    Built {
        config,
        runtime: Some(runtime),
        shared: Some(shared),
        base_warnings: warnings,
        loaded_plugins,
        fatal,
    }
}

/// List `*.lua` files in `dir`, sorted by file name (hidden files skipped).
fn plugin_files(dir: &Path, warnings: &mut Vec<String>) -> Vec<PathBuf> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            warnings.push(format!(
                "plugins: cannot read directory {}: {error}",
                dir.display()
            ));
            return Vec::new();
        }
    };
    let mut names = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        if !name.to_ascii_lowercase().ends_with(".lua") {
            continue;
        }
        names.push(name.to_owned());
    }
    names.sort();
    names.into_iter().map(|name| dir.join(name)).collect()
}

/// Convert the `keyboard` table entries into registrations.
///
/// String targets map to built-in actions; function targets are wrapped in an
/// automatically generated custom action (so the app can run them through the
/// same [`Scripts::run_action`] path as named actions).
fn keyboard_registrations(
    lua: &Lua,
    shared: &Rc<lua_api::Shared>,
    entries: Vec<(String, Value)>,
    warnings: &mut Vec<String>,
) -> Vec<lua_api::Registration> {
    let mut registrations = Vec::new();
    for (chord_spec, value) in entries {
        let Some(chord) = KeyChord::parse(&chord_spec) else {
            warnings.push(format!(
                "config.lua: keyboard chord {chord_spec:?} is not understood (ignored)"
            ));
            continue;
        };
        match value {
            Value::String(action) => {
                let action_name = action.to_string_lossy();
                match BindingAction::from_key(&action_name) {
                    Some(action) => {
                        registrations.push(lua_api::Registration::Binding {
                            chord,
                            action,
                            file: "config.lua".to_owned(),
                        });
                    }
                    None => warnings.push(format!(
                        "config.lua: keyboard[{chord_spec:?}] action {action_name:?} is not known (ignored)"
                    )),
                }
            }
            Value::Function(func) => match lua.create_registry_value(func) {
                Ok(key) => {
                    let id = shared.next_binding_id();
                    registrations.push(lua_api::Registration::Action {
                        info: ScriptActionInfo {
                            id: id.clone(),
                            title: format!("Lua callback ({chord_spec})"),
                            description: Some(format!(
                                "Keyboard callback from config.lua bound to {chord_spec}"
                            )),
                        },
                        func: key,
                        file: "config.lua".to_owned(),
                        keys: Vec::new(),
                    });
                    registrations.push(lua_api::Registration::Binding {
                        chord,
                        action: BindingAction::Custom(id),
                        file: "config.lua".to_owned(),
                    });
                }
                Err(error) => warnings.push(format!(
                    "config.lua: keyboard[{chord_spec:?}]: cannot store callback: {error}"
                )),
            },
            other => warnings.push(format!(
                "config.lua: keyboard[{chord_spec:?}] must be an action name or a function (got {})",
                other.type_name()
            )),
        }
    }
    registrations
}
