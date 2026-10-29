//! Compiled-in Rust plugin extension point.
//!
//! Lua plugins live in `<config>/echoport/plugins/*.lua`; this module is the
//! equivalent interface for plugins that are compiled into EchoPort itself
//! (or for Rust crates that want to embed EchoPort). The [`Plugin`] trait is
//! intentionally tiny and stable:
//!
//! ```no_run
//! use echoport::scripting::{Plugin, PluginHost, ScriptEvent};
//! use echoport::state::Command;
//!
//! struct Logger;
//!
//! impl Plugin for Logger {
//!     fn id(&self) -> &str {
//!         "logger"
//!     }
//!
//!     fn on_event(&mut self, event: &ScriptEvent, host: &mut PluginHost) {
//!         if let ScriptEvent::TrackStart(track) = event {
//!             host.notify(format!("now playing: {}", track.title));
//!             host.dispatch(Command::ToggleShuffle);
//!         }
//!     }
//! }
//! ```
//!
//! Plugins run on the UI thread, synchronously, so `on_event` must stay cheap
//! and must never block (no network, no heavy IO). Commands and toasts are
//! queued over the app event channel and applied on the next frame.

use crossbeam_channel::Sender;

use crate::state::{AppEvent, Command};

use super::ScriptEvent;

/// A compiled-in plugin.
pub trait Plugin: Send {
    /// Stable, unique identifier (used for logging and duplicate detection).
    fn id(&self) -> &str;

    /// Called for every [`ScriptEvent`] fired by the host.
    fn on_event(&mut self, event: &ScriptEvent, host: &mut PluginHost);
}

/// Host handle given to a plugin so it can talk to the application.
pub struct PluginHost {
    sender: Option<Sender<AppEvent>>,
}

impl PluginHost {
    fn new(sender: Option<Sender<AppEvent>>) -> Self {
        Self { sender }
    }

    /// Queue a command for the app (executed on the next frame).
    pub fn dispatch(&mut self, command: Command) {
        match &self.sender {
            Some(sender) => {
                let _ = sender.send(AppEvent::RunCommand(command));
            }
            None => log::warn!("plugin dispatched a command but no app channel is connected"),
        }
    }

    /// Queue a toast for the user.
    pub fn notify(&mut self, text: impl Into<String>) {
        let text = text.into();
        match &self.sender {
            Some(sender) => {
                let _ = sender.send(AppEvent::ScriptToast(text));
            }
            None => log::info!("plugin notification: {text}"),
        }
    }

    /// The underlying event channel, if one is connected.
    pub fn sender(&self) -> Option<&Sender<AppEvent>> {
        self.sender.as_ref()
    }
}

/// A set of compiled-in plugins.
///
/// Create it once in the app, connect the event channel and call [`Self::fire`]
/// wherever [`crate::scripting::Scripts::fire`] is called:
///
/// ```no_run
/// # use echoport::scripting::{PluginRegistry, ScriptEvent};
/// # fn demo(tx: echoport::scripting::EventSender) {
/// let mut registry = PluginRegistry::with_sender(tx);
/// // registry.register(Box::new(MyPlugin));
/// registry.fire(&ScriptEvent::Startup);
/// # }
/// ```
#[derive(Default)]
pub struct PluginRegistry {
    plugins: Vec<Box<dyn Plugin>>,
    sender: Option<Sender<AppEvent>>,
}

impl PluginRegistry {
    /// Create an empty registry without an app event channel.
    ///
    /// Use [`Self::with_sender`] (or [`Self::set_sender`]) when plugins should
    /// be able to dispatch commands / toasts.
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
            sender: None,
        }
    }

    /// Create an empty registry connected to the app event channel.
    pub fn with_sender(sender: Sender<AppEvent>) -> Self {
        Self {
            plugins: Vec::new(),
            sender: Some(sender),
        }
    }

    /// Connect (or replace) the app event channel.
    pub fn set_sender(&mut self, sender: Sender<AppEvent>) {
        self.sender = Some(sender);
    }

    /// Register a plugin. If a plugin with the same id is already registered it
    /// is replaced and a warning is logged (last registration wins).
    pub fn register(&mut self, plugin: Box<dyn Plugin>) {
        let id = plugin.id().to_owned();
        if let Some(index) = self.plugins.iter().position(|other| other.id() == id) {
            log::warn!("plugin `{id}` registered twice; replacing the earlier registration");
            self.plugins.remove(index);
        }
        self.plugins.push(plugin);
    }

    /// Fire an event to all registered plugins, in registration order.
    ///
    /// A panic inside a plugin is not caught: compiled-in plugins are trusted
    /// code, unlike Lua plugins which are sandboxed by `pcall` semantics.
    pub fn fire(&mut self, event: &ScriptEvent) {
        if self.plugins.is_empty() {
            return;
        }
        let sender = self.sender.clone();
        for plugin in self.plugins.iter_mut() {
            let mut host = PluginHost::new(sender.clone());
            plugin.on_event(event, &mut host);
        }
    }

    /// Ids of the registered plugins, in registration order.
    pub fn plugins(&self) -> Vec<String> {
        self.plugins
            .iter()
            .map(|plugin| plugin.id().to_owned())
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    pub fn len(&self) -> usize {
        self.plugins.len()
    }
}
