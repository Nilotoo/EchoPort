//! The Lua side: the global `echoport` table and everything it can do.
//!
//! One [`Lua`] state is created per load / reload. Scripts register callbacks
//! which are stored as [`mlua::RegistryKey`]s; the reason is that functions
//! cannot outlive their Lua state, so the owning [`super::Scripts`] rebuilds
//! everything on reload.
//!
//! All closures below are intentionally forgiving with user data: bad argument
//! types are turned into warnings by the protected caller, and out-of-range
//! values are clamped instead of panicking.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use mlua::{Function, Lua, MultiValue, RegistryKey, Table, Value};

use crate::model::{Platform, Track};
use crate::state::{AppEvent, Command};

use super::{EventSender, ScriptActionInfo, ScriptEvent, ScriptPaths, VERSION};
use crate::scripting::keys::{BindingAction, KeyChord};

/// Maximum number of warnings kept in memory (protects against a script that
/// errors on every frame).
pub(crate) const MAX_WARNINGS: usize = 100;

/// Event names accepted by `echoport.on`.
pub(crate) const EVENT_NAMES: &[&str] = &[
    "startup",
    "track_start",
    "track_end",
    "search_done",
    "download_done",
    "library_scanned",
    "playback_paused",
    "playback_resumed",
];

/// A callback / binding registered while a Lua file executes.
pub(crate) enum Registration {
    Event {
        event: &'static str,
        func: RegistryKey,
        file: String,
    },
    Action {
        info: ScriptActionInfo,
        func: RegistryKey,
        file: String,
        /// Chord specs from `actions.add{ keys = ... }`.
        keys: Vec<String>,
    },
    Binding {
        chord: KeyChord,
        action: BindingAction,
        file: String,
    },
}

/// State shared between the host and all Lua callbacks of one Lua instance.
pub(crate) struct Shared {
    pub warnings: RefCell<Vec<String>>,
    pub now_playing: RefCell<Option<Track>>,
    pub registrations: RefCell<Vec<Registration>>,
    /// File currently being executed / called (used to prefix errors).
    current_file: RefCell<String>,
    pub tx: EventSender,
    binding_counter: Cell<u64>,
}

impl Shared {
    pub fn new(tx: EventSender) -> Rc<Self> {
        Rc::new(Self {
            warnings: RefCell::new(Vec::new()),
            now_playing: RefCell::new(None),
            registrations: RefCell::new(Vec::new()),
            current_file: RefCell::new("config.lua".to_owned()),
            tx,
            binding_counter: Cell::new(0),
        })
    }

    pub fn set_file(&self, file: impl Into<String>) {
        *self.current_file.borrow_mut() = file.into();
    }

    pub fn file(&self) -> String {
        self.current_file.borrow().clone()
    }

    /// Append a warning prefixed with the file that caused it.
    pub fn warn(&self, message: impl Into<String>) {
        let message = format!("{}: {}", self.file(), message.into());
        self.push_warning(message);
    }

    /// Append an already formatted warning.
    pub fn push_warning(&self, message: impl Into<String>) {
        let mut warnings = self.warnings.borrow_mut();
        if warnings.len() < MAX_WARNINGS {
            warnings.push(message.into());
        } else if warnings.len() == MAX_WARNINGS {
            warnings.push("… further script warnings suppressed".to_owned());
        }
    }

    pub fn take_registrations(&self) -> Vec<Registration> {
        std::mem::take(&mut *self.registrations.borrow_mut())
    }

    pub(crate) fn next_binding_id(&self) -> String {
        let next = self.binding_counter.get() + 1;
        self.binding_counter.set(next);
        format!("__lua_binding_{next}")
    }
}

/// Directories exposed through `echoport.paths`.
#[derive(Clone, Debug)]
pub(crate) struct PlatformDirs {
    pub data: PathBuf,
    pub cache: PathBuf,
    pub downloads: PathBuf,
    pub music: PathBuf,
}

/// Resolve the per-platform folders shown in `echoport.paths`.
///
/// The config / plugin locations always come from [`ScriptPaths`] so tests and
/// portable builds stay self-contained; the remaining folders are the normal
/// platform locations.
pub(crate) fn platform_dirs() -> PlatformDirs {
    let data_root = dirs::data_dir().unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".local/share")
    });
    let cache_root = dirs::cache_dir().unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".cache")
    });
    let music = dirs::audio_dir().unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Music")
    });
    PlatformDirs {
        data: data_root.join("echoport"),
        cache: cache_root.join("echoport"),
        downloads: music.join("EchoPort"),
        music,
    }
}

/// Install the `echoport` global table into `lua`.
pub(crate) fn install(lua: &Lua, shared: &Rc<Shared>, paths: &ScriptPaths) -> mlua::Result<()> {
    let echoport = lua.create_table()?;
    echoport.set("version", VERSION)?;

    let dirs = platform_dirs();
    let paths_table = lua.create_table()?;
    paths_table.set("data", dirs.data.to_string_lossy().as_ref())?;
    paths_table.set("cache", dirs.cache.to_string_lossy().as_ref())?;
    paths_table.set("config_file", paths.config_file.to_string_lossy().as_ref())?;
    paths_table.set("plugins", paths.plugins_dir.to_string_lossy().as_ref())?;
    paths_table.set("downloads", dirs.downloads.to_string_lossy().as_ref())?;
    paths_table.set("music", dirs.music.to_string_lossy().as_ref())?;
    echoport.set("paths", paths_table)?;

    // An empty table so `echoport.config.language = "zh"` works too; config.lua
    // usually replaces it wholesale.
    echoport.set("config", lua.create_table()?)?;

    let toast = {
        let shared = shared.clone();
        lua.create_function(move |_, value: Value| {
            let text = value_text(&value);
            if !text.trim().is_empty() {
                let _ = shared.tx.send(AppEvent::ScriptToast(text));
            }
            Ok(())
        })?
    };
    echoport.set("notify", toast.clone())?;
    echoport.set("toast", toast)?;

    {
        let shared = shared.clone();
        echoport.set(
            "log",
            lua.create_function(move |_, value: Value| {
                log::info!("[lua {}] {}", shared.file(), value_text(&value));
                Ok(())
            })?,
        )?;
    }

    {
        let shared = shared.clone();
        echoport.set(
            "command",
            lua.create_function(move |_, name: String| {
                let name = name.trim();
                match BindingAction::from_key(name) {
                    Some(action) => {
                        let _ = shared.tx.send(AppEvent::ScriptBinding(action));
                    }
                    None => shared.warn(format!(
                        "echoport.command({name:?}) is not a known action; \
                         see docs/SCRIPTING.md for the list"
                    )),
                }
                Ok(())
            })?,
        )?;
    }

    {
        let shared = shared.clone();
        echoport.set(
            "search",
            lua.create_function(move |_, query: String| {
                let query = query.trim().to_owned();
                if query.is_empty() {
                    shared.warn("echoport.search() needs a non-empty query");
                    return Ok(());
                }
                let _ = shared.tx.send(AppEvent::RunCommand(Command::Search {
                    query,
                    platforms: Platform::REMOTE.to_vec(),
                }));
                Ok(())
            })?,
        )?;
    }

    {
        let shared = shared.clone();
        echoport.set(
            "open_url",
            lua.create_function(move |_, url: String| {
                let url = url.trim().to_owned();
                if url.is_empty() {
                    shared.warn("echoport.open_url() needs a non-empty URL");
                } else if !url.contains("://") {
                    shared.warn(format!(
                        "echoport.open_url({url:?}) must be an absolute URL (https://...)"
                    ));
                } else {
                    let _ = shared.tx.send(AppEvent::RunCommand(Command::OpenUrl(url)));
                }
                Ok(())
            })?,
        )?;
    }

    {
        let shared = shared.clone();
        echoport.set(
            "play_url",
            lua.create_function(move |_, url: String| {
                let url = url.trim().to_owned();
                if url.is_empty() {
                    shared.warn("echoport.play_url() needs a non-empty URL");
                } else if !url.contains("://") {
                    shared.warn(format!(
                        "echoport.play_url({url:?}) must be an absolute URL (https://...)"
                    ));
                } else {
                    let _ = shared
                        .tx
                        .send(AppEvent::RunCommand(Command::ImportLink(url)));
                }
                Ok(())
            })?,
        )?;
    }

    {
        let shared = shared.clone();
        echoport.set(
            "set_volume",
            lua.create_function(move |_, volume: f64| {
                if !volume.is_finite() {
                    shared.warn("echoport.set_volume() needs a finite number");
                    return Ok(());
                }
                let clamped = volume.clamp(0.0, 1.0) as f32;
                if f64::from(clamped) != volume {
                    shared.warn(format!(
                        "echoport.set_volume({volume}) is outside 0..1; clamped to {clamped}"
                    ));
                }
                let _ = shared
                    .tx
                    .send(AppEvent::RunCommand(Command::SetVolume(clamped)));
                Ok(())
            })?,
        )?;
    }

    {
        let shared = shared.clone();
        echoport.set(
            "current_track",
            lua.create_function(move |lua, ()| current_track_value(lua, &shared))?,
        )?;
    }

    {
        let shared = shared.clone();
        echoport.set(
            "reload",
            lua.create_function(move |_, ()| {
                let message =
                    "echoport.reload(): use Settings → Reload scripts (the host owns the Lua state)";
                log::info!("[lua {}] {message}", shared.file());
                shared.push_warning(message);
                Ok(())
            })?,
        )?;
    }

    {
        let shared = shared.clone();
        echoport.set(
            "on",
            lua.create_function(move |lua, (event, func): (String, Function)| {
                let Some(name) = canonical_event(&event) else {
                    shared.warn(format!(
                        "echoport.on({event:?}) — unknown event; expected one of {}",
                        EVENT_NAMES.join(", ")
                    ));
                    return Ok(());
                };
                match lua.create_registry_value(func) {
                    Ok(key) => shared.registrations.borrow_mut().push(Registration::Event {
                        event: name,
                        func: key,
                        file: shared.file(),
                    }),
                    Err(error) => {
                        shared.warn(format!("echoport.on: cannot store callback: {error}"))
                    }
                }
                Ok(())
            })?,
        )?;
    }

    {
        let shared = shared.clone();
        echoport.set(
            "bind",
            lua.create_function(move |lua, (chord_spec, target): (String, Value)| {
                let file = shared.file();
                let Some(chord) = KeyChord::parse(&chord_spec) else {
                    shared.warn(format!(
                        "echoport.bind({chord_spec:?}) — chord is not understood (ignored)"
                    ));
                    return Ok(());
                };
                match target {
                    Value::String(action) => {
                        let action_name = action.to_string_lossy();
                        match BindingAction::from_key(&action_name) {
                            Some(action) => {
                                shared.registrations.borrow_mut().push(Registration::Binding {
                                    chord,
                                    action,
                                    file,
                                });
                            }
                            None => shared.warn(format!(
                                "echoport.bind({chord_spec:?}, {action_name:?}) — unknown action (ignored)"
                            )),
                        }
                    }
                    Value::Function(func) => {
                        let id = shared.next_binding_id();
                        match lua.create_registry_value(func) {
                            Ok(key) => {
                                let info = ScriptActionInfo {
                                    id: id.clone(),
                                    title: format!("Lua callback ({chord_spec})"),
                                    description: Some(format!(
                                        "Inline function bound to {chord_spec} with echoport.bind"
                                    )),
                                };
                                let mut registrations = shared.registrations.borrow_mut();
                                registrations.push(Registration::Action {
                                    info,
                                    func: key,
                                    file: file.clone(),
                                    keys: Vec::new(),
                                });
                                registrations.push(Registration::Binding {
                                    chord,
                                    action: BindingAction::Custom(id),
                                    file,
                                });
                            }
                            Err(error) => shared.warn(format!(
                                "echoport.bind({chord_spec:?}): cannot store callback: {error}"
                            )),
                        }
                    }
                    other => shared.warn(format!(
                        "echoport.bind({chord_spec:?}) expects an action name or a function \
                         (got {})",
                        other.type_name()
                    )),
                }
                Ok(())
            })?,
        )?;
    }

    let actions = lua.create_table()?;
    {
        let shared = shared.clone();
        actions.set(
            "add",
            lua.create_function(move |lua, spec: Table| {
                add_action(lua, &shared, spec);
                Ok(())
            })?,
        )?;
    }
    echoport.set("actions", actions)?;

    lua.globals().set("echoport", echoport)?;
    Ok(())
}

fn add_action(lua: &Lua, shared: &Rc<Shared>, spec: Table) {
    let file = shared.file();

    let id = match spec.get::<Option<String>>("id") {
        Ok(Some(id)) if !id.trim().is_empty() => id.trim().to_owned(),
        _ => {
            shared.warn("echoport.actions.add: `id` must be a non-empty string (action ignored)");
            return;
        }
    };
    if id.starts_with("__") {
        shared.warn(format!(
            "echoport.actions.add: id {id:?} uses a reserved prefix (action ignored)"
        ));
        return;
    }
    let title = match spec.get::<Option<String>>("title") {
        Ok(Some(title)) if !title.trim().is_empty() => title,
        _ => {
            shared.warn(format!(
                "echoport.actions.add({id:?}): `title` is missing; using the id"
            ));
            id.clone()
        }
    };
    let description = match spec.get::<Option<String>>("description") {
        Ok(description) => description,
        Err(_) => {
            shared.warn(format!(
                "echoport.actions.add({id:?}): `description` must be a string (ignored)"
            ));
            None
        }
    };
    let run = match spec.get::<Option<Function>>("run") {
        Ok(Some(run)) => run,
        Ok(None) => {
            shared.warn(format!(
                "echoport.actions.add({id:?}): `run` function is missing (action ignored)"
            ));
            return;
        }
        Err(_) => {
            shared.warn(format!(
                "echoport.actions.add({id:?}): `run` must be a function (action ignored)"
            ));
            return;
        }
    };
    let func = match lua.create_registry_value(run) {
        Ok(func) => func,
        Err(error) => {
            shared.warn(format!(
                "echoport.actions.add({id:?}): cannot store callback: {error}"
            ));
            return;
        }
    };
    let keys = match spec.get::<Option<Value>>("keys") {
        Ok(Some(Value::String(key))) => vec![key.to_string_lossy()],
        Ok(Some(Value::Table(list))) => {
            let mut keys = Vec::new();
            for (index, item) in list.sequence_values::<Value>().enumerate() {
                match item {
                    Ok(Value::String(key)) => keys.push(key.to_string_lossy()),
                    Ok(other) => shared.warn(format!(
                        "echoport.actions.add({id:?}): keys[{index}] must be a chord string \
                         (got {})",
                        other.type_name()
                    )),
                    Err(error) => shared.warn(format!(
                        "echoport.actions.add({id:?}): cannot read keys[{index}]: {error}"
                    )),
                }
            }
            keys
        }
        Ok(Some(other)) => {
            shared.warn(format!(
                "echoport.actions.add({id:?}): `keys` must be a string or a list of strings \
                 (got {})",
                other.type_name()
            ));
            Vec::new()
        }
        Ok(None) => Vec::new(),
        Err(_) => {
            shared.warn(format!(
                "echoport.actions.add({id:?}): `keys` must be a string or a list of strings (ignored)"
            ));
            Vec::new()
        }
    };
    shared
        .registrations
        .borrow_mut()
        .push(Registration::Action {
            info: ScriptActionInfo {
                id,
                title,
                description,
            },
            func,
            file,
            keys,
        });
}

fn canonical_event(name: &str) -> Option<&'static str> {
    let normalized = name.trim().to_ascii_lowercase().replace(['-', ' '], "_");
    EVENT_NAMES
        .iter()
        .copied()
        .find(|candidate| *candidate == normalized.as_str())
}

/// Convert any Lua value into a human readable string (for notify/log).
fn value_text(value: &Value) -> String {
    match value {
        Value::Nil => "nil".to_owned(),
        Value::Boolean(flag) => flag.to_string(),
        Value::Integer(number) => number.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => text.to_string_lossy(),
        other => format!("<{}>", other.type_name()),
    }
}

pub(crate) fn track_table(lua: &Lua, track: &Track) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("id", track.id.as_str())?;
    table.set("title", track.title.as_str())?;
    table.set("artist", track.artist.as_str())?;
    table.set("platform", track.platform.key())?;
    if let Some(album) = &track.album {
        table.set("album", album.as_str())?;
    }
    if let Some(duration) = track.duration_secs {
        table.set("duration", duration)?;
    }
    if let Some(url) = &track.page_url {
        table.set("url", url.as_str())?;
    }
    if let Some(path) = &track.local_path {
        table.set("local_path", path.to_string_lossy().as_ref())?;
    }
    Ok(table)
}

/// Build the argument list passed to a Lua event handler.
pub(crate) fn event_args(lua: &Lua, event: &ScriptEvent) -> mlua::Result<MultiValue> {
    let mut args = Vec::new();
    match event {
        ScriptEvent::Startup | ScriptEvent::PlaybackPaused | ScriptEvent::PlaybackResumed => {}
        ScriptEvent::TrackStart(track) | ScriptEvent::TrackEnd(track) => {
            args.push(Value::Table(track_table(lua, track)?));
        }
        ScriptEvent::SearchDone(query) => {
            args.push(Value::String(lua.create_string(query)?));
        }
        ScriptEvent::DownloadDone(track, path) => {
            args.push(Value::Table(track_table(lua, track)?));
            args.push(Value::String(
                lua.create_string(path.to_string_lossy().as_ref())?,
            ));
        }
        ScriptEvent::LibraryScanned(count) => {
            args.push(Value::Integer(*count as i64));
        }
    }
    Ok(MultiValue::from_vec(args))
}

/// Read `echoport.current_track()` from the shared now-playing slot.
pub(crate) fn current_track_value(lua: &Lua, shared: &Rc<Shared>) -> mlua::Result<Value> {
    let track = shared.now_playing.borrow();
    match track.as_ref() {
        Some(track) => Ok(Value::Table(track_table(lua, track)?)),
        None => Ok(Value::Nil),
    }
}

/// Call a stored Lua function, turning errors into `Err(message)`.
pub(crate) fn call_function(lua: &Lua, key: &RegistryKey, args: MultiValue) -> Result<(), String> {
    let func: Function = lua
        .registry_value(key)
        .map_err(|error| format!("Lua callback is gone: {error}"))?;
    func.call::<()>(args).map_err(format_lua_error)
}

/// Flatten a Lua error into a one line message (tracebacks are noisy in the UI).
pub(crate) fn format_lua_error(error: mlua::Error) -> String {
    let text = error.to_string();
    let text = text.trim();
    if text.is_empty() {
        "Lua error".to_owned()
    } else {
        // Keep the first line: full tracebacks would flood the warning list.
        text.lines().next().unwrap_or(text).to_owned()
    }
}

/// Read the `echoport.config` table after config.lua has executed.
pub(crate) fn config_value(lua: &Lua) -> Result<Option<Table>, String> {
    let globals = lua.globals();
    let echoport: Value = globals
        .get("echoport")
        .map_err(|error| format!("cannot read the echoport table: {error}"))?;
    let Value::Table(echoport) = echoport else {
        return Err("the global `echoport` table was removed or replaced".to_owned());
    };
    let config: Value = echoport
        .get("config")
        .map_err(|error| format!("cannot read echoport.config: {error}"))?;
    match config {
        Value::Nil => Ok(None),
        Value::Table(table) => Ok(Some(table)),
        other => Err(format!(
            "echoport.config must be a table (got {})",
            other.type_name()
        )),
    }
}
