//! Parsing and validation of `config.lua`.
//!
//! The parser is deliberately conservative: every value is type checked,
//! range checked and clamped, and anything suspicious becomes an entry in
//! [`ScriptConfig::warnings`] instead of a panic. Missing values simply stay
//! `None`, which tells the app to keep its profile / built-in default.

use std::path::{Path, PathBuf};

use egui::Color32;
use mlua::{Table, Value};

use crate::i18n::Language;
use crate::model::DownloadFormat;
use crate::state::Page;
use crate::theme::ThemeMode;

use super::keys::{BindingAction, KeyChord};
use super::{ScriptConfig, ScriptPaths};

/// Default `config.lua` written on first run.
///
/// The shipped reference configuration (`examples/config.lua`) is the same
/// file, so there is exactly one template to maintain. It is commented out
/// completely: a fresh install keeps the profile defaults.
pub(crate) const DEFAULT_CONFIG_TEMPLATE: &str = include_str!("../../examples/config.lua");

/// Keys that scripts / plugins may use for their own configuration without a
/// warning.
const FREE_FORM_KEYS: &[&str] = &["plugins", "custom", "hooks"];

/// Result of parsing the `echoport.config` table.
#[derive(Default)]
pub(crate) struct ParsedConfig {
    pub warnings: Vec<String>,
    /// Keyboard entries in a deterministic order: chord spec -> raw target.
    pub keyboard: Vec<(String, Value)>,
}

/// Create the config directory / plugins directory and write the default
/// template when (and only when) `config.lua` does not exist yet.
///
/// Returns warnings (non fatal problems) and whether a template was created.
pub(crate) fn prepare_files(paths: &ScriptPaths) -> (Vec<String>, bool) {
    let mut warnings = Vec::new();
    if let Some(parent) = paths.config_file.parent() {
        if let Err(error) = std::fs::create_dir_all(parent) {
            warnings.push(format!(
                "cannot create config directory {}: {error}",
                parent.display()
            ));
        }
    }
    if let Err(error) = std::fs::create_dir_all(&paths.plugins_dir) {
        warnings.push(format!(
            "cannot create plugin directory {}: {error}",
            paths.plugins_dir.display()
        ));
    }
    let mut created = false;
    if !paths.config_file.exists() {
        match write_new_file(&paths.config_file, DEFAULT_CONFIG_TEMPLATE) {
            Ok(true) => {
                created = true;
                log::info!(
                    "created default Lua configuration at {}",
                    paths.config_file.display()
                );
            }
            Ok(false) => {}
            Err(error) => warnings.push(format!(
                "cannot write default config {}: {error}",
                paths.config_file.display()
            )),
        }
    }
    (warnings, created)
}

fn write_new_file(path: &Path, contents: &str) -> std::io::Result<bool> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => {
            file.write_all(contents.as_bytes())?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(error) => Err(error),
    }
}

/// Parse the `echoport.config` table into `config`.
pub(crate) fn parse(config: &mut ScriptConfig, table: &Table) -> ParsedConfig {
    let mut parsed = ParsedConfig::default();

    for pair in table.pairs::<Value, Value>() {
        let (key, value) = match pair {
            Ok(pair) => pair,
            Err(error) => {
                parsed
                    .warnings
                    .push(format!("config.lua: cannot read setting: {error}"));
                continue;
            }
        };
        let Value::String(name) = &key else {
            parsed.warnings.push(format!(
                "config.lua: ignored setting with non-string key ({})",
                key.type_name()
            ));
            continue;
        };
        let name = name.to_string_lossy();
        match name.as_str() {
            "language" => {
                if let Some(value) = as_string(&value) {
                    match parse_language(&value) {
                        Some(language) => config.language = Some(language),
                        None => parsed.warnings.push(format!(
                            "config.lua: language {value:?} is not supported (use \"en\" or \"zh\")"
                        )),
                    }
                } else {
                    parsed
                        .warnings
                        .push(wrong_type("language", &value, "\"en\" or \"zh\""));
                }
            }
            "theme" => {
                if let Some(value) = as_string(&value) {
                    match parse_theme(&value) {
                        Some(theme) => config.theme_mode = Some(theme),
                        None => parsed.warnings.push(format!(
                            "config.lua: theme {value:?} is not supported \
                             (use \"pulse\", \"tide\" or \"ink\")"
                        )),
                    }
                } else {
                    parsed.warnings.push(wrong_type(
                        "theme",
                        &value,
                        "\"pulse\", \"tide\" or \"ink\"",
                    ));
                }
            }
            "follow_cover_accent" => match as_bool(&value) {
                Some(flag) => config.follow_cover_accent = Some(flag),
                None => {
                    parsed
                        .warnings
                        .push(wrong_type("follow_cover_accent", &value, "true or false"))
                }
            },
            "accent" => match as_string(&value).and_then(|text| parse_color(&text)) {
                Some(color) => config.accent = Some(color),
                None => parsed
                    .warnings
                    .push(wrong_type("accent", &value, "a \"#RRGGBB\" colour")),
            },
            "highlight" => match as_string(&value).and_then(|text| parse_color(&text)) {
                Some(color) => config.highlight = Some(color),
                None => {
                    parsed
                        .warnings
                        .push(wrong_type("highlight", &value, "a \"#RRGGBB\" colour"))
                }
            },
            "macaron" => match parse_macaron(&value) {
                Ok(colors) => config.macaron = Some(colors),
                Err(message) => parsed.warnings.push(format!("config.lua: {message}")),
            },
            "startup_page" => {
                if let Some(value) = as_string(&value) {
                    match parse_page(&value) {
                        Some(page) => config.startup_page = Some(page),
                        None => parsed.warnings.push(format!(
                            "config.lua: startup_page {value:?} is not a known page \
                             (home, search, library, playlists, downloads, now_playing, artist, profile, settings)"
                        )),
                    }
                } else {
                    parsed
                        .warnings
                        .push(wrong_type("startup_page", &value, "a page name"));
                }
            }
            "ui_scale" => match as_number(&value) {
                Some(number) if number.is_finite() => {
                    // The app clamps the live zoom to 0.8..=2.0 (Command::SetUiScale
                    // and AppState::new use the same bound); match that here.
                    let clamped = number.clamp(0.8, 2.0) as f32;
                    if (f64::from(clamped) - number).abs() > 1e-6 {
                        parsed.warnings.push(format!(
                            "config.lua: ui_scale {number} is outside 0.8..2.0; clamped to {clamped}"
                        ));
                    }
                    config.ui_scale = Some(clamped);
                }
                _ => parsed.warnings.push(wrong_type(
                    "ui_scale",
                    &value,
                    "a number between 0.8 and 2.0 (1.0, 1.1, 1.25, 1.5 are the UI presets)",
                )),
            },
            "cache_limit_mb" => match as_number(&value) {
                Some(number) if number.is_finite() => {
                    let megabytes = number.round().clamp(0.0, 10240.0) as u32;
                    if (f64::from(megabytes) - number).abs() > 0.5 {
                        parsed.warnings.push(format!(
                            "config.lua: cache_limit_mb {number} is outside 0..10240 or not an \
                             integer; using {megabytes}"
                        ));
                    }
                    config.cache_limit_mb = Some(megabytes);
                }
                _ => parsed.warnings.push(wrong_type(
                    "cache_limit_mb",
                    &value,
                    "an integer number of MiB (0 = keep no played tracks; UI presets: 0, 512, 1024, 2048)",
                )),
            },
            "volume" => {
                if let Some(number) = as_number(&value) {
                    let clamped = number.clamp(0.0, 1.0);
                    if clamped != number {
                        parsed.warnings.push(format!(
                            "config.lua: volume {number} is outside 0..1; clamped to {clamped}"
                        ));
                    }
                    config.volume = Some(clamped as f32);
                } else {
                    parsed
                        .warnings
                        .push(wrong_type("volume", &value, "a number between 0 and 1"));
                }
            }
            "seek_step" => match as_number(&value) {
                Some(number) if number.is_finite() => {
                    let clamped = number.clamp(1.0, 120.0);
                    if clamped != number {
                        parsed.warnings.push(format!(
                            "config.lua: seek_step {number} is outside 1..120; clamped to {clamped}"
                        ));
                    }
                    config.seek_step = Some(clamped);
                }
                _ => parsed.warnings.push(wrong_type(
                    "seek_step",
                    &value,
                    "a number of seconds (1..120)",
                )),
            },
            "volume_step" => match as_number(&value) {
                Some(number) if number.is_finite() => {
                    let clamped = number.clamp(0.01, 0.5);
                    if clamped != number {
                        parsed.warnings.push(format!(
                            "config.lua: volume_step {number} is outside 0.01..0.5; clamped to {clamped}"
                        ));
                    }
                    config.volume_step = Some(clamped as f32);
                }
                _ => parsed.warnings.push(wrong_type(
                    "volume_step",
                    &value,
                    "a number between 0.01 and 0.5",
                )),
            },
            "results_per_platform" => match as_number(&value) {
                Some(number) if number.is_finite() => {
                    let count = number.round().clamp(2.0, 50.0) as usize;
                    if count as f64 != number {
                        parsed.warnings.push(format!(
                            "config.lua: results_per_platform {number} is outside 2..50 or not an                              integer; using {count}"
                        ));
                    }
                    config.results_per_platform = Some(count);
                }
                Some(number) => parsed.warnings.push(format!(
                    "config.lua: results_per_platform {number} is not a finite number; ignored"
                )),
                None => parsed.warnings.push(wrong_type(
                    "results_per_platform",
                    &value,
                    "a positive integer",
                )),
            },
            "download_format" => {
                match as_string(&value).and_then(|text| parse_download_format(&text)) {
                    Some(format) => config.download_format = Some(format),
                    None => parsed.warnings.push(wrong_type(
                        "download_format",
                        &value,
                        "\"mp3\", \"m4a\", \"opus\" or \"flac\"",
                    )),
                }
            }
            "download_dir" => match as_string(&value) {
                Some(text) if !text.trim().is_empty() => {
                    config.download_dir = Some(expand_path(&text));
                }
                _ => parsed.warnings.push(wrong_type(
                    "download_dir",
                    &value,
                    "a non-empty path string",
                )),
            },
            "music_dirs" => match parse_music_dirs(&value) {
                Ok(dirs) => config.music_dirs = Some(dirs),
                Err(message) => parsed.warnings.push(format!("config.lua: {message}")),
            },
            "keyboard" => match &value {
                Value::Table(keyboard) => {
                    parsed.keyboard = parse_keyboard(keyboard, &mut parsed.warnings);
                }
                other => parsed.warnings.push(wrong_type(
                    "keyboard",
                    other,
                    "a table of { chord = action }",
                )),
            },
            other if FREE_FORM_KEYS.contains(&other) => {}
            other => parsed.warnings.push(format!(
                "config.lua: unknown setting {other:?} (ignored; see docs/SCRIPTING.md)"
            )),
        }
    }

    parsed
}

fn parse_keyboard(table: &Table, warnings: &mut Vec<String>) -> Vec<(String, Value)> {
    let mut entries = Vec::new();
    for pair in table.pairs::<Value, Value>() {
        let (key, value) = match pair {
            Ok(pair) => pair,
            Err(error) => {
                warnings.push(format!("config.lua: cannot read keyboard entry: {error}"));
                continue;
            }
        };
        let Value::String(chord) = &key else {
            warnings.push(format!(
                "config.lua: keyboard keys must be strings (got {})",
                key.type_name()
            ));
            continue;
        };
        let chord = chord.to_string_lossy();
        if KeyChord::parse(&chord).is_none() {
            warnings.push(format!(
                "config.lua: keyboard chord {chord:?} is not understood (ignored)"
            ));
            continue;
        }
        match &value {
            Value::String(action) => {
                if BindingAction::from_key(&action.to_string_lossy()).is_none() {
                    warnings.push(format!(
                        "config.lua: keyboard[{chord:?}] action {:?} is not known (ignored)",
                        action.to_string_lossy()
                    ));
                    continue;
                }
            }
            Value::Function(_) => {}
            other => {
                warnings.push(format!(
                    "config.lua: keyboard[{chord:?}] must be an action name or a function \
                     (got {})",
                    other.type_name()
                ));
                continue;
            }
        }
        entries.push((chord, value));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries
}

fn parse_language(value: &str) -> Option<Language> {
    match value.trim().to_ascii_lowercase().as_str() {
        "en" | "english" | "eng" => Some(Language::En),
        "zh" | "chinese" | "cmn" | "中文" | "汉语" => Some(Language::Zh),
        _ => None,
    }
}

fn parse_theme(value: &str) -> Option<ThemeMode> {
    let normalized = value.trim().to_ascii_lowercase().replace([' ', '-'], "_");
    // `ThemeMode::from_key` is the single source of truth and already accepts
    // the pre-redesign aliases (blue_white / album / porcelain -> pulse,
    // midnight -> ink, ...).
    if let Some(mode) = ThemeMode::from_key(&normalized) {
        return Some(mode);
    }
    // A few friendly extras that are not theme-module aliases.
    let key = match normalized.as_str() {
        "default" | "light" | "blue" | "day" => "pulse",
        "teal" | "aqua" | "sea" => "tide",
        "dark" | "night" | "black" => "ink",
        "cover" | "cover_accent" | "art" => "pulse",
        _ => return None,
    };
    ThemeMode::from_key(key)
}

fn parse_page(value: &str) -> Option<Page> {
    let normalized = value.trim().to_ascii_lowercase().replace([' ', '-'], "_");
    let normalized = match normalized.as_str() {
        "nowplaying" | "now" | "playing" => "now_playing",
        "home" | "search" | "library" | "playlists" | "downloads" | "now_playing" | "artist"
        | "profile" | "settings" => normalized.as_str(),
        _ => return None,
    };
    Page::PRIMARY
        .into_iter()
        .find(|page| page.key() == normalized)
}

fn parse_download_format(value: &str) -> Option<DownloadFormat> {
    let normalized = value.trim().to_ascii_lowercase();
    if let Some(format) = DownloadFormat::from_key(&normalized) {
        return Some(format);
    }
    match normalized.as_str() {
        // Friendly aliases.
        "aac" | "m4a (aac)" => Some(DownloadFormat::M4a),
        "mpeg" | "mp3 (mpeg)" => Some(DownloadFormat::Mp3),
        "ogg" | "vorbis" => Some(DownloadFormat::Opus),
        _ => None,
    }
}

fn parse_macaron(value: &Value) -> Result<[Color32; 5], String> {
    let Value::Table(table) = value else {
        return Err(wrong_type(
            "macaron",
            value,
            "a table of five \"#RRGGBB\" colours",
        ));
    };
    let mut colors = [Color32::WHITE; 5];
    let mut count = 0;
    for item in table.sequence_values::<Value>() {
        let item = match item {
            Ok(item) => item,
            Err(error) => return Err(format!("cannot read macaron[{count}]: {error}")),
        };
        if count >= 5 {
            return Err("macaron has more than five colours (expected exactly five)".to_owned());
        }
        match as_string(&item).and_then(|text| parse_color(&text)) {
            Some(color) => colors[count] = color,
            None => {
                return Err(format!(
                    "macaron[{count}] is not a \"#RRGGBB\" colour (got {})",
                    item.type_name()
                ))
            }
        }
        count += 1;
    }
    if count != 5 {
        return Err(format!(
            "macaron needs exactly five colours (found {count})"
        ));
    }
    Ok(colors)
}

fn parse_music_dirs(value: &Value) -> Result<Vec<PathBuf>, String> {
    let Value::Table(table) = value else {
        return Err(wrong_type("music_dirs", value, "a table of path strings"));
    };
    let mut dirs = Vec::new();
    for (index, item) in table.sequence_values::<Value>().enumerate() {
        let item = match item {
            Ok(item) => item,
            Err(error) => return Err(format!("cannot read music_dirs[{index}]: {error}")),
        };
        match as_string(&item) {
            Some(text) if !text.trim().is_empty() => dirs.push(expand_path(&text)),
            _ => {
                return Err(format!(
                    "music_dirs[{index}] must be a non-empty path string (got {})",
                    item.type_name()
                ))
            }
        }
    }
    if dirs.is_empty() {
        return Err("music_dirs must contain at least one path".to_owned());
    }
    Ok(dirs)
}

/// Parse `"#RRGGBB"`, `"#RGB"` or `"#RRGGBBAA"` (with or without `#`).
pub(crate) fn parse_color(text: &str) -> Option<Color32> {
    let hex = text.trim().trim_start_matches('#').trim_start_matches("0x");
    let digits: Vec<u8> = hex
        .chars()
        .map(|c| c.to_digit(16).map(|d| d as u8))
        .collect::<Option<Vec<u8>>>()?;
    match digits.len() {
        3 => Some(Color32::from_rgb(
            digits[0] * 17,
            digits[1] * 17,
            digits[2] * 17,
        )),
        6 => Some(Color32::from_rgb(
            digits[0] * 16 + digits[1],
            digits[2] * 16 + digits[3],
            digits[4] * 16 + digits[5],
        )),
        8 => Some(Color32::from_rgba_unmultiplied(
            digits[0] * 16 + digits[1],
            digits[2] * 16 + digits[3],
            digits[4] * 16 + digits[5],
            digits[6] * 16 + digits[7],
        )),
        _ => None,
    }
}

/// Expand a leading `~` (POSIX or Windows style) and `$VAR` / `${VAR}`
/// environment variables. Unknown variables are left untouched.
pub(crate) fn expand_path(raw: &str) -> PathBuf {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return PathBuf::new();
    }
    let expanded = if trimmed == "~" {
        home_dir().to_string_lossy().into_owned()
    } else if let Some(rest) = trimmed
        .strip_prefix("~/")
        .or_else(|| trimmed.strip_prefix("~\\"))
    {
        home_dir().join(rest).to_string_lossy().into_owned()
    } else {
        expand_env(trimmed)
    };
    PathBuf::from(expanded)
}

fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

fn expand_env(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '$' {
            out.push(ch);
            continue;
        }
        let mut name = String::new();
        if chars.peek() == Some(&'{') {
            chars.next();
            for next in chars.by_ref() {
                if next == '}' {
                    break;
                }
                name.push(next);
            }
        } else {
            while let Some(next) = chars.peek() {
                if next.is_ascii_alphanumeric() || *next == '_' {
                    name.push(*next);
                    chars.next();
                } else {
                    break;
                }
            }
        }
        if name.is_empty() {
            out.push('$');
            continue;
        }
        match std::env::var(&name) {
            Ok(value) => out.push_str(&value),
            Err(_) => {
                out.push_str("${");
                out.push_str(&name);
                out.push('}');
            }
        }
    }
    out
}

fn as_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.to_string_lossy()),
        _ => None,
    }
}

fn as_number(value: &Value) -> Option<f64> {
    match value {
        Value::Integer(number) => Some(*number as f64),
        Value::Number(number) => Some(*number),
        _ => None,
    }
}

fn as_bool(value: &Value) -> Option<bool> {
    match value {
        Value::Boolean(flag) => Some(*flag),
        _ => None,
    }
}

fn wrong_type(key: &str, value: &Value, expected: &str) -> String {
    format!(
        "config.lua: {key} must be {expected} (got {})",
        value.type_name()
    )
}
