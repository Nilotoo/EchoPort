//! Keyboard chords and the actions they can trigger.
//!
//! Chords are parsed from the same textual form that is used in
//! `config.lua`: `"Space"`, `"Ctrl+ArrowRight"`, `"Ctrl+Shift+K"`, `"Alt+Q"`,
//! `"Cmd+P"`. Parsing is deliberately forgiving (case insensitive modifier
//! names, common key aliases such as `"Esc"` / `"Left"` / `"PgUp"`) but never
//! panics: invalid input simply yields `None` and the caller records a
//! warning.

use egui::Key;

/// A physical key plus the modifier state that has to be held.
///
/// `command` is the platform "primary" modifier: ⌘ on macOS, Ctrl on
/// Windows/Linux. `ctrl` is the literal Control key (which on macOS is a
/// different modifier than ⌘).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyChord {
    pub key: Key,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub command: bool,
}

impl KeyChord {
    /// Parse `"Ctrl+Shift+K"` style specifications.
    ///
    /// Returns `None` for empty strings, modifier-only chords, unknown
    /// modifiers or unknown key names.
    pub fn parse(spec: &str) -> Option<Self> {
        let spec = spec.trim();
        if spec.is_empty() {
            return None;
        }
        let parts: Vec<&str> = spec.split('+').map(str::trim).collect();
        if parts.len() > 5 {
            return None;
        }
        let mut chord = Self {
            key: Key::Space,
            ctrl: false,
            alt: false,
            shift: false,
            command: false,
        };
        let mut key = None;
        for (index, part) in parts.iter().enumerate() {
            if part.is_empty() {
                return None;
            }
            let is_last = index + 1 == parts.len();
            let lower = part.to_ascii_lowercase();
            match lower.as_str() {
                "ctrl" | "control" if !is_last => chord.ctrl = true,
                "alt" | "option" | "opt" if !is_last => chord.alt = true,
                "shift" if !is_last => chord.shift = true,
                "cmd" | "command" | "meta" | "super" | "win" if !is_last => chord.command = true,
                _ => {
                    // The key itself must be the final segment.
                    if !is_last || key.is_some() {
                        return None;
                    }
                    key = Some(parse_key(part)?);
                }
            }
        }
        chord.key = key?;
        Some(chord)
    }

    /// Human readable form, e.g. `"Ctrl+Shift+K"`.
    pub fn label(&self) -> String {
        let mut out = String::new();
        if self.ctrl {
            out.push_str("Ctrl+");
        }
        if self.alt {
            out.push_str("Alt+");
        }
        if self.shift {
            out.push_str("Shift+");
        }
        if self.command {
            out.push_str("Cmd+");
        }
        out.push_str(key_label(self.key));
        out
    }

    /// True when this chord was pressed in `input` during the current frame.
    ///
    /// Modifiers are compared exactly: a `"K"` binding must not fire for
    /// `Ctrl+K`, and `"Ctrl+K"` must not fire for `Ctrl+Shift+K`.
    pub fn matches(&self, input: &egui::InputState) -> bool {
        if !input.key_pressed(self.key) {
            return false;
        }
        let modifiers = input.modifiers;
        if modifiers.alt != self.alt || modifiers.shift != self.shift {
            return false;
        }
        if cfg!(target_os = "macos") {
            // On macOS ⌘ and Control are distinct; require both exactly.
            modifiers.ctrl == self.ctrl && modifiers.command == self.command
        } else {
            // Ctrl and the platform "command" modifier are the same key here,
            // so a chord may name either `Ctrl` or `Cmd`.
            let primary = self.ctrl || self.command;
            !modifiers.mac_cmd && modifiers.ctrl == primary
        }
    }
}

impl std::fmt::Display for KeyChord {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.label())
    }
}

fn key_label(key: Key) -> &'static str {
    match key {
        Key::ArrowDown => "ArrowDown",
        Key::ArrowLeft => "ArrowLeft",
        Key::ArrowRight => "ArrowRight",
        Key::ArrowUp => "ArrowUp",
        Key::Escape => "Escape",
        Key::Backspace => "Backspace",
        Key::Enter => "Enter",
        Key::Tab => "Tab",
        Key::Space => "Space",
        Key::Insert => "Insert",
        Key::Delete => "Delete",
        Key::Home => "Home",
        Key::End => "End",
        Key::PageUp => "PageUp",
        Key::PageDown => "PageDown",
        Key::Backtick => "Backtick",
        Key::Minus => "Minus",
        Key::Plus => "Plus",
        Key::Equals => "Equals",
        Key::Comma => "Comma",
        Key::Period => "Period",
        Key::Slash => "Slash",
        Key::Backslash => "Backslash",
        Key::Semicolon => "Semicolon",
        Key::Quote => "Quote",
        Key::OpenBracket => "OpenBracket",
        Key::CloseBracket => "CloseBracket",
        other => other.name(),
    }
}

/// Resolve the key part of a chord.
fn parse_key(token: &str) -> Option<Key> {
    let token = token.trim();
    if token.is_empty() {
        return None;
    }
    // egui already knows canonical names plus a few aliases ("Left", "Esc").
    if let Some(key) = Key::from_name(token) {
        return Some(key);
    }
    let lower = token.to_ascii_lowercase();
    let canonical: &str = match lower.as_str() {
        "spacebar" => "Space",
        "esc" | "escape" => "Escape",
        "return" | "enter" => "Enter",
        "del" => "Delete",
        "ins" => "Insert",
        "pgup" | "pageup" => "PageUp",
        "pgdown" | "pgdn" | "pagedown" => "PageDown",
        "left" | "arrowleft" => "Left",
        "right" | "arrowright" => "Right",
        "up" | "arrowup" => "Up",
        "down" | "arrowdown" => "Down",
        "backquote" | "grave" => "Backtick",
        "equal" => "Equals",
        "dot" => "Period",
        "apostrophe" => "Quote",
        "bracketleft" => "OpenBracket",
        "bracketright" => "CloseBracket",
        "backslash" => "Backslash",
        "minus" => "Minus",
        "plus" => "Plus",
        "comma" => "Comma",
        "slash" => "Slash",
        "semicolon" => "Semicolon",
        "tab" => "Tab",
        "home" => "Home",
        "end" => "End",
        _ => "",
    };
    if !canonical.is_empty() {
        return Key::from_name(canonical);
    }
    // Single letters/digits: normalize the case.
    if lower.chars().count() == 1 {
        return Key::from_name(&lower.to_ascii_uppercase());
    }
    // Function keys: f1 ... f35.
    if let Some(rest) = lower.strip_prefix('f') {
        if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) {
            return Key::from_name(&format!("F{rest}"));
        }
    }
    None
}

/// An action a key chord (or a Lua `echoport.command`) can trigger.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindingAction {
    PlayPause,
    Next,
    Previous,
    SeekForward,
    SeekBackward,
    VolumeUp,
    VolumeDown,
    ToggleQueue,
    ToggleFavorite,
    ToggleShuffle,
    CycleRepeat,
    OpenSearch,
    OpenSettings,
    OpenHome,
    /// A custom action registered from Lua: `action:<id>`.
    Custom(String),
}

impl BindingAction {
    /// Parse the canonical config names (`"play_pause"`, `"next"`, …) plus a
    /// few friendly aliases. `"action:<id>"` produces [`BindingAction::Custom`].
    pub fn from_key(key: &str) -> Option<Self> {
        let key = key.trim();
        if let Some(id) = key.strip_prefix("action:") {
            let id = id.trim();
            if id.is_empty() {
                return None;
            }
            return Some(Self::Custom(id.to_owned()));
        }
        let normalized = key.to_ascii_lowercase().replace(['-', ' '], "_");
        Some(match normalized.as_str() {
            "play_pause" | "playpause" | "toggle_play_pause" | "toggle_playback" => Self::PlayPause,
            "next" | "next_track" | "skip" => Self::Next,
            "previous" | "prev" | "previous_track" => Self::Previous,
            "seek_forward" | "forward" | "seek_fwd" | "fast_forward" => Self::SeekForward,
            "seek_backward" | "backward" | "seek_back" | "rewind" => Self::SeekBackward,
            "volume_up" | "louder" => Self::VolumeUp,
            "volume_down" | "quieter" => Self::VolumeDown,
            "toggle_queue" | "queue" | "toggle_queue_panel" => Self::ToggleQueue,
            "toggle_favorite" | "toggle_favourite" | "favorite" | "favourite" => {
                Self::ToggleFavorite
            }
            "toggle_shuffle" | "shuffle" => Self::ToggleShuffle,
            "cycle_repeat" | "repeat" => Self::CycleRepeat,
            "open_search" | "search" => Self::OpenSearch,
            "open_settings" | "settings" => Self::OpenSettings,
            "open_home" | "home" => Self::OpenHome,
            _ => return None,
        })
    }

    /// Canonical config name. For [`BindingAction::Custom`] this is the action
    /// id without the `action:` prefix.
    pub fn key(&self) -> &str {
        match self {
            Self::PlayPause => "play_pause",
            Self::Next => "next",
            Self::Previous => "previous",
            Self::SeekForward => "seek_forward",
            Self::SeekBackward => "seek_backward",
            Self::VolumeUp => "volume_up",
            Self::VolumeDown => "volume_down",
            Self::ToggleQueue => "toggle_queue",
            Self::ToggleFavorite => "toggle_favorite",
            Self::ToggleShuffle => "toggle_shuffle",
            Self::CycleRepeat => "cycle_repeat",
            Self::OpenSearch => "open_search",
            Self::OpenSettings => "open_settings",
            Self::OpenHome => "open_home",
            Self::Custom(id) => id.as_str(),
        }
    }

    /// The custom action id, when this is [`BindingAction::Custom`].
    pub fn custom_id(&self) -> Option<&str> {
        match self {
            Self::Custom(id) => Some(id.as_str()),
            _ => None,
        }
    }

    /// The `action:` prefixed form used in `config.lua` bindings.
    pub fn spec(&self) -> String {
        match self {
            Self::Custom(id) => format!("action:{id}"),
            other => other.key().to_owned(),
        }
    }
}

/// A chord bound to an action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyBinding {
    pub chord: KeyChord,
    pub action: BindingAction,
}

impl KeyBinding {
    pub fn new(chord: KeyChord, action: BindingAction) -> Self {
        Self { chord, action }
    }
}
