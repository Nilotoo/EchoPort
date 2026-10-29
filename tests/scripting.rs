//! Integration tests for the Lua configuration / plugin subsystem.
//!
//! These tests never open a window or need an audio device: they use temporary
//! directories, the public scripting API and a crossbeam channel receiver.

use std::path::{Path, PathBuf};

use crossbeam_channel::{unbounded, Receiver};
use echoport::i18n::Language;
use echoport::model::{DownloadFormat, Platform, Track};
use echoport::scripting::{
    BindingAction, KeyBinding, KeyChord, Plugin, PluginHost, PluginRegistry, ScriptActionInfo,
    ScriptConfig, ScriptEvent, ScriptPaths, Scripts,
};
use echoport::state::{AppEvent, Command, Page};
use echoport::theme::ThemeMode;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup() -> (TempDir, ScriptPaths) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("echoport");
    let paths = ScriptPaths::under(root);
    (dir, paths)
}

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent");
    }
    std::fs::write(path, contents).expect("write file");
}

fn write_plugin(paths: &ScriptPaths, name: &str, contents: &str) {
    write(&paths.plugins_dir.join(name), contents);
}

fn load(
    paths: &ScriptPaths,
) -> (
    Scripts,
    Receiver<AppEvent>,
    crossbeam_channel::Sender<AppEvent>,
) {
    let (tx, rx) = unbounded();
    let scripts = Scripts::load(paths.clone(), tx.clone());
    (scripts, rx, tx)
}

fn track() -> Track {
    Track::new(
        Platform::YouTube,
        "test-video-id",
        "Test Title",
        "Test Artist",
    )
}

fn theme(expected_new: &str, legacy: &str) -> ThemeMode {
    ThemeMode::from_key(expected_new)
        .or_else(|| ThemeMode::from_key(legacy))
        .unwrap_or_else(|| panic!("neither {expected_new:?} nor {legacy:?} is a theme key"))
}

fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

fn drain(rx: &Receiver<AppEvent>) -> Vec<AppEvent> {
    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }
    events
}

// ---------------------------------------------------------------------------
// First run / default template
// ---------------------------------------------------------------------------

#[test]
fn first_run_creates_config_and_plugin_dir_and_never_overwrites() {
    let (_dir, paths) = setup();
    let (scripts, _rx, _tx) = load(&paths);

    assert!(paths.config_file.is_file(), "default config.lua must exist");
    assert!(paths.plugins_dir.is_dir(), "plugins directory must exist");
    assert!(scripts.config().loaded, "shipped template must load");
    assert!(
        scripts.config().warnings.is_empty(),
        "template must not produce warnings: {:?}",
        scripts.config().warnings
    );

    let template = std::fs::read_to_string(&paths.config_file).expect("read template");
    assert_eq!(
        template,
        include_str!("../examples/config.lua"),
        "the first-run template must be exactly examples/config.lua"
    );
    assert!(template.contains("echoport.config"));
    assert!(template.contains("keyboard"));
    assert!(template.contains("actions.add"));
    assert!(template.contains("中文"), "template is bilingual");

    // A later load must never overwrite an existing user configuration.
    let custom = "echoport.config = { volume = 0.9 }\n";
    write(&paths.config_file, custom);
    let (scripts, _rx, _tx) = load(&paths);
    assert!(approx(scripts.config().volume.expect("volume parsed"), 0.9));
    assert_eq!(std::fs::read_to_string(&paths.config_file).unwrap(), custom);

    // Deleting it recreates the template.
    std::fs::remove_file(&paths.config_file).unwrap();
    let (_scripts, _rx, _tx) = load(&paths);
    assert!(paths.config_file.is_file());
}

#[test]
fn example_plugin_parses_and_loads() {
    let (_dir, paths) = setup();
    let example = include_str!("../examples/plugins/example.lua");
    write_plugin(&paths, "example.lua", example);
    let (scripts, _rx, _tx) = load(&paths);

    assert!(
        scripts.config().warnings.is_empty(),
        "example plugin must load cleanly: {:?}",
        scripts.config().warnings
    );
    assert_eq!(scripts.loaded_plugins(), vec!["example.lua".to_owned()]);
    let ids: Vec<&str> = scripts
        .config()
        .actions
        .iter()
        .map(|action| action.id.as_str())
        .collect();
    assert!(ids.contains(&"example_history"), "actions: {ids:?}");
    // The example binds keyboard/bind actions: Ctrl+Alt+R, Ctrl+Alt+G, Ctrl+Alt+L.
    assert!(scripts.config().bindings.len() >= 3);
}

// ---------------------------------------------------------------------------
// Configuration parsing
// ---------------------------------------------------------------------------

#[test]
fn parses_every_supported_setting() {
    let (_dir, paths) = setup();
    write(
        &paths.config_file,
        r##"
echoport.config = {
  language = "zh",
  theme = "tide",
  follow_cover_accent = true,
  accent = "#044BB5",
  highlight = "#BBFF5C",
  macaron = { "#BBCFF5", "#C3E9EA", "#F9E7A6", "#D9CCF2", "#FAD3B8" },
  startup_page = "settings",
  ui_scale = 1.25,
  cache_limit_mb = 2048,
  volume = 0.5,
  seek_step = 10,
  volume_step = 0.1,
  results_per_platform = 20,
  download_format = "flac",
  download_dir = "~/Music/EchoPortTest",
  music_dirs = { "~/Music", "/tmp/echoport-music" },
  keyboard = {
    ["Space"] = "play_pause",
    ["Ctrl+ArrowRight"] = "next",
    ["Ctrl+Alt+H"] = "action:hello",
    ["Ctrl+Shift+K"] = "open_settings",
  },
}

echoport.actions.add({
  id = "hello",
  title = "Hello",
  description = "demo action",
  run = function()
    echoport.notify("hi")
  end,
})

echoport.on("startup", function()
  echoport.log("started")
end)
"##,
    );

    let (scripts, _rx, _tx) = load(&paths);
    let config = scripts.config();
    assert!(config.loaded);
    assert!(
        config.warnings.is_empty(),
        "valid config produced warnings: {:?}",
        config.warnings
    );
    assert_eq!(config.language, Some(Language::Zh));
    assert_eq!(config.theme_mode, Some(theme("tide", "midnight")));
    assert_eq!(config.follow_cover_accent, Some(true));
    assert_eq!(
        config.accent,
        Some(egui::Color32::from_rgb(0x04, 0x4B, 0xB5))
    );
    assert_eq!(
        config.highlight,
        Some(egui::Color32::from_rgb(0xBB, 0xFF, 0x5C))
    );
    assert_eq!(
        config.macaron,
        Some([
            egui::Color32::from_rgb(0xBB, 0xCF, 0xF5),
            egui::Color32::from_rgb(0xC3, 0xE9, 0xEA),
            egui::Color32::from_rgb(0xF9, 0xE7, 0xA6),
            egui::Color32::from_rgb(0xD9, 0xCC, 0xF2),
            egui::Color32::from_rgb(0xFA, 0xD3, 0xB8),
        ])
    );
    assert_eq!(config.startup_page, Some(Page::Settings));
    assert!(approx(config.ui_scale.unwrap(), 1.25));
    assert_eq!(config.cache_limit_mb, Some(2048));
    assert!(approx(config.volume.unwrap(), 0.5));
    assert_eq!(config.seek_step, Some(10.0));
    assert!(approx(config.volume_step.unwrap(), 0.1));
    assert_eq!(config.results_per_platform, Some(20));
    assert_eq!(config.download_format, Some(DownloadFormat::Flac));

    let home = dirs::home_dir().expect("home dir");
    let download_dir = config.download_dir.as_ref().expect("download_dir");
    assert!(download_dir.starts_with(&home), "{download_dir:?}");
    assert!(download_dir.ends_with(Path::new("Music/EchoPortTest")));
    let music_dirs = config.music_dirs.as_ref().expect("music_dirs");
    assert_eq!(music_dirs.len(), 2);
    assert_eq!(music_dirs[0], home.join("Music"));
    assert_eq!(music_dirs[1], PathBuf::from("/tmp/echoport-music"));

    // Actions.
    assert_eq!(config.actions.len(), 1);
    let hello: &ScriptActionInfo = &config.actions[0];
    assert_eq!(hello.id, "hello");
    assert_eq!(hello.title, "Hello");
    assert_eq!(hello.description.as_deref(), Some("demo action"));

    // Bindings (sorted by chord spec when they come from the keyboard table).
    assert_eq!(config.bindings.len(), 4, "{:?}", config.bindings);
    let find = |spec: &str| -> BindingAction {
        let chord = KeyChord::parse(spec).expect("chord");
        config
            .bindings
            .iter()
            .find(|binding| binding.chord == chord)
            .map(|binding| binding.action.clone())
            .unwrap_or_else(|| panic!("missing binding for {spec}"))
    };
    assert_eq!(find("Space"), BindingAction::PlayPause);
    assert_eq!(find("Ctrl+ArrowRight"), BindingAction::Next);
    assert_eq!(
        find("Ctrl+Alt+H"),
        BindingAction::Custom("hello".to_owned())
    );
    assert_eq!(find("Ctrl+Shift+K"), BindingAction::OpenSettings);
}

#[test]
fn invalid_values_warn_but_never_panic() {
    let (_dir, paths) = setup();
    write(
        &paths.config_file,
        r##"
echoport.config = {
  language = 42,
  theme = "neon",
  follow_cover_accent = "yes",
  accent = "red",
  highlight = 12,
  macaron = { "#111111", "#222222" },
  startup_page = "moon",
  ui_scale = 9,
  cache_limit_mb = -5,
  volume = "loud",
  seek_step = -5,
  volume_step = 99,
  results_per_platform = 0,
  download_format = "wav",
  download_dir = 12,
  music_dirs = "not-a-table",
  keyboard = {
    ["NotARealKey"] = "play_pause",
    ["Ctrl+Q"] = "explode",
    ["Ctrl+Alt+Q"] = 5,
  },
  unknown_extra = true,
}

echoport.actions.add({ id = "", title = "no id", run = function() end })
echoport.actions.add({ id = "no_run", title = "no run" })
echoport.on("not_an_event", function() end)
"##,
    );

    let (scripts, _rx, _tx) = load(&paths);
    let config = scripts.config();
    assert!(config.loaded, "a config with bad values still loads");
    assert!(
        config.warnings.len() >= 10,
        "expected many warnings, got {:?}",
        config.warnings
    );
    assert!(config.language.is_none());
    assert!(config.theme_mode.is_none());
    assert!(config.follow_cover_accent.is_none());
    assert!(config.accent.is_none());
    assert!(config.macaron.is_none());
    assert!(config.startup_page.is_none());
    assert!(approx(config.ui_scale.unwrap(), 2.0));
    assert_eq!(config.cache_limit_mb, Some(0));
    assert!(config.volume.is_none());
    // Out of range numeric values are clamped (documented behaviour).
    assert_eq!(config.seek_step, Some(1.0));
    assert!(approx(config.volume_step.unwrap(), 0.5));
    assert_eq!(config.results_per_platform, Some(2));
    assert!(config.download_format.is_none());
    assert!(config.download_dir.is_none());
    assert!(config.music_dirs.is_none());
    assert!(config.bindings.is_empty(), "{:?}", config.bindings);
    // The invalid actions are rejected.
    assert!(config.actions.is_empty());

    let joined = config.warnings.join("\n");
    for expected in [
        "theme",
        "seek_step",
        "keyboard",
        "not_an_event",
        "unknown_extra",
    ] {
        assert!(
            joined.contains(expected),
            "missing warning about {expected}: {joined}"
        );
    }
}

#[test]
fn ui_scale_cache_limit_and_artist_startup_page() {
    let (_dir, paths) = setup();
    write(
        &paths.config_file,
        r#"
echoport.config = {
  startup_page = "artist",
  ui_scale = 1.1,
  cache_limit_mb = 0,
}
"#,
    );
    let (scripts, _rx, _tx) = load(&paths);
    let config = scripts.config();
    assert!(config.loaded);
    assert!(
        config.warnings.is_empty(),
        "valid ui options produced warnings: {:?}",
        config.warnings
    );
    assert_eq!(config.startup_page, Some(Page::Artist));
    assert!(approx(config.ui_scale.unwrap(), 1.1));
    assert_eq!(config.cache_limit_mb, Some(0));

    // Out-of-range values are clamped with a warning (never rejected).
    write(
        &paths.config_file,
        r#"
echoport.config = {
  ui_scale = 0.1,
  cache_limit_mb = 999999,
}
"#,
    );
    let (scripts, _rx, _tx) = load(&paths);
    let config = scripts.config();
    assert!(approx(config.ui_scale.unwrap(), 0.8));
    assert_eq!(config.cache_limit_mb, Some(10240));
    assert!(
        config
            .warnings
            .iter()
            .any(|warning| warning.contains("ui_scale")),
        "{:?}",
        config.warnings
    );
    assert!(
        config
            .warnings
            .iter()
            .any(|warning| warning.contains("cache_limit_mb")),
        "{:?}",
        config.warnings
    );
}

#[test]
fn theme_keys_and_legacy_aliases_match_theme_module() {
    let (_dir, paths) = setup();
    // Every name here must resolve exactly like `ThemeMode::from_key`, plus the
    // documented friendly extras ("default" -> pulse, "dark" -> ink).
    for (name, expected) in [
        ("pulse", ThemeMode::from_key("pulse")),
        ("tide", ThemeMode::from_key("tide")),
        ("ink", ThemeMode::from_key("ink")),
        ("blue_white", ThemeMode::from_key("blue_white")),
        ("album", ThemeMode::from_key("album")),
        ("porcelain", ThemeMode::from_key("porcelain")),
        ("midnight", ThemeMode::from_key("midnight")),
        ("default", ThemeMode::from_key("pulse")),
        ("dark", ThemeMode::from_key("ink")),
    ] {
        let expected = expected.expect("theme module must know these names");
        let config = format!("echoport.config = {{ theme = \"{name}\" }}\n");
        write(&paths.config_file, &config);
        let (scripts, _rx, _tx) = load(&paths);
        assert_eq!(
            scripts.config().theme_mode,
            Some(expected),
            "theme {name:?} should map to {expected:?} (warnings: {:?})",
            scripts.config().warnings
        );
    }
}

// ---------------------------------------------------------------------------
// Plugins
// ---------------------------------------------------------------------------

#[test]
fn plugin_errors_are_isolated_and_valid_plugins_run() {
    let (_dir, paths) = setup();
    write_plugin(&paths, "01_broken.lua", "this is not lua (");
    write_plugin(
        &paths,
        "02_good.lua",
        r#"
echoport.actions.add({
  id = "ping",
  title = "Ping",
  run = function()
    echoport.notify("pong")
  end,
})

echoport.on("track_start", function(track)
  if track then
    echoport.notify("track:" .. track.title)
  end
end)

echoport.on("download_done", function(track, path)
  echoport.notify("saved:" .. track.title .. ":" .. path)
end)
"#,
    );
    write_plugin(
        &paths,
        "03_also_broken.lua",
        "error('exploded at load time')",
    );

    let (mut scripts, rx, tx) = load(&paths);
    assert_eq!(
        scripts.loaded_plugins(),
        vec!["02_good.lua".to_owned()],
        "only the valid plugin loads"
    );
    let joined = scripts.config().warnings.join("\n");
    assert!(joined.contains("01_broken.lua"), "{joined}");
    assert!(joined.contains("03_also_broken.lua"), "{joined}");
    assert!(scripts.config().loaded, "config still loads");

    assert!(scripts.run_action("ping", &tx));
    let events = drain(&rx);
    assert!(
        matches!(events.as_slice(), [AppEvent::ScriptToast(text)] if text == "pong"),
        "{events:?}"
    );

    scripts.set_now_playing(Some(track()));
    scripts.fire(&ScriptEvent::TrackStart(track()), &tx);
    let events = drain(&rx);
    assert!(
        matches!(events.as_slice(), [AppEvent::ScriptToast(text)] if text == "track:Test Title"),
        "{events:?}"
    );

    scripts.fire(
        &ScriptEvent::DownloadDone(track(), PathBuf::from("/tmp/song.mp3")),
        &tx,
    );
    let events = drain(&rx);
    assert!(
        matches!(events.as_slice(), [AppEvent::ScriptToast(text)] if text == "saved:Test Title:/tmp/song.mp3"),
        "{events:?}"
    );

    // Unknown action ids are simply reported.
    assert!(!scripts.run_action("does_not_exist", &tx));
}

#[test]
fn plugins_load_in_sorted_file_name_order() {
    let (_dir, paths) = setup();
    write_plugin(
        &paths,
        "b_second.lua",
        "_G.echoport_test_order = (_G.echoport_test_order or \"b\") .. \"b\"",
    );
    write_plugin(&paths, "a_first.lua", "_G.echoport_test_order = \"a\"");
    write_plugin(
        &paths,
        "c_report.lua",
        r#"
echoport.actions.add({
  id = "order",
  title = "order",
  run = function()
    echoport.notify(_G.echoport_test_order)
  end,
})
"#,
    );

    let (mut scripts, rx, tx) = load(&paths);
    assert_eq!(
        scripts.loaded_plugins(),
        vec![
            "a_first.lua".to_owned(),
            "b_second.lua".to_owned(),
            "c_report.lua".to_owned()
        ]
    );
    assert!(scripts.run_action("order", &tx));
    let events = drain(&rx);
    assert!(
        matches!(events.as_slice(), [AppEvent::ScriptToast(text)] if text == "ab"),
        "{events:?}"
    );
}

#[test]
fn keyword_keyboard_function_bindings_and_custom_action_validation() {
    let (_dir, paths) = setup();
    write(
        &paths.config_file,
        r#"
echoport.config = {
  keyboard = {
    ["Ctrl+Alt+F"] = function()
      echoport.notify("keyboard-function")
    end,
  },
}

echoport.bind("Ctrl+Alt+B", function()
  echoport.notify("bind-function")
end)
echoport.bind("Ctrl+Alt+N", "next")
echoport.bind("Ctrl+Alt+X", "action:missing_action")
"#,
    );

    let (mut scripts, rx, tx) = load(&paths);
    let config = scripts.config();
    assert!(config.loaded);
    let joined = config.warnings.join("\n");
    assert!(joined.contains("missing_action"), "{joined}");

    // Two hidden actions were created (keyboard function + echoport.bind).
    let keyboard_id = config
        .actions
        .iter()
        .find(|action| {
            action
                .description
                .as_deref()
                .is_some_and(|description| description.contains("Ctrl+Alt+F"))
        })
        .expect("keyboard function action")
        .id
        .clone();
    let bind_id = config
        .actions
        .iter()
        .find(|action| {
            action
                .description
                .as_deref()
                .is_some_and(|description| description.contains("Ctrl+Alt+B"))
        })
        .expect("bind function action")
        .id
        .clone();

    // The keyboard function binding survives; the dangling custom action does not.
    assert_eq!(config.bindings.len(), 3, "{:?}", config.bindings);
    let find = |spec: &str| -> BindingAction {
        let chord = KeyChord::parse(spec).unwrap();
        config
            .bindings
            .iter()
            .find(|binding| binding.chord == chord)
            .map(|binding| binding.action.clone())
            .unwrap_or_else(|| panic!("missing binding for {spec}"))
    };
    assert_eq!(find("Ctrl+Alt+N"), BindingAction::Next);
    assert_eq!(
        find("Ctrl+Alt+F"),
        BindingAction::Custom(keyboard_id.clone())
    );
    assert_eq!(find("Ctrl+Alt+B"), BindingAction::Custom(bind_id.clone()));
    assert!(config
        .bindings
        .iter()
        .all(|binding| { binding.chord != KeyChord::parse("Ctrl+Alt+X").unwrap() }));

    assert!(scripts.run_action(&keyboard_id, &tx));
    let events = drain(&rx);
    assert!(
        matches!(events.as_slice(), [AppEvent::ScriptToast(text)] if text == "keyboard-function"),
        "{events:?}"
    );
    assert!(scripts.run_action(&bind_id, &tx));
    let events = drain(&rx);
    assert!(
        matches!(events.as_slice(), [AppEvent::ScriptToast(text)] if text == "bind-function"),
        "{events:?}"
    );
}

#[test]
fn duplicate_action_ids_keep_the_last_definition() {
    let (_dir, paths) = setup();
    write_plugin(
        &paths,
        "one.lua",
        r#"
echoport.actions.add({ id = "dup", title = "first", run = function() echoport.notify("first") end })
"#,
    );
    write_plugin(
        &paths,
        "two.lua",
        r#"
echoport.actions.add({ id = "dup", title = "second", run = function() echoport.notify("second") end })
"#,
    );
    let (mut scripts, rx, tx) = load(&paths);
    assert_eq!(scripts.config().actions.len(), 1);
    assert_eq!(scripts.config().actions[0].title, "second");
    assert!(
        scripts
            .config()
            .warnings
            .iter()
            .any(|warning| warning.contains("duplicate action id")),
        "{:?}",
        scripts.config().warnings
    );
    assert!(scripts.run_action("dup", &tx));
    let events = drain(&rx);
    assert!(
        matches!(events.as_slice(), [AppEvent::ScriptToast(text)] if text == "second"),
        "{events:?}"
    );
}

// ---------------------------------------------------------------------------
// Events and commands
// ---------------------------------------------------------------------------

#[test]
fn lua_commands_emit_app_events() {
    let (_dir, paths) = setup();
    write(
        &paths.config_file,
        r#"
echoport.actions.add({
  id = "api_calls",
  title = "API calls",
  run = function()
    echoport.search("query")
    echoport.open_url("https://example.com")
    echoport.play_url("https://example.com/track")
    echoport.set_volume(0.25)
    echoport.command("play_pause")
    echoport.toast("done")
  end,
})
"#,
    );
    let (mut scripts, rx, tx) = load(&paths);
    assert!(scripts.run_action("api_calls", &tx));
    let events = drain(&rx);
    assert_eq!(events.len(), 6, "{events:?}");
    match &events[0] {
        AppEvent::RunCommand(Command::Search { query, platforms }) => {
            assert_eq!(query, "query");
            assert_eq!(*platforms, Platform::REMOTE.to_vec());
        }
        other => panic!("unexpected first event: {other:?}"),
    }
    assert!(matches!(
        &events[1],
        AppEvent::RunCommand(Command::OpenUrl(url)) if url == "https://example.com"
    ));
    assert!(matches!(
        &events[2],
        AppEvent::RunCommand(Command::ImportLink(url)) if url == "https://example.com/track"
    ));
    assert!(matches!(
        &events[3],
        AppEvent::RunCommand(Command::SetVolume(volume)) if approx(*volume, 0.25)
    ));
    assert!(matches!(
        &events[4],
        AppEvent::ScriptBinding(BindingAction::PlayPause)
    ));
    assert!(matches!(&events[5], AppEvent::ScriptToast(text) if text == "done"));
}

#[test]
fn current_track_is_exposed_to_lua() {
    let (_dir, paths) = setup();
    write(
        &paths.config_file,
        r#"
echoport.on("startup", function()
  local track = echoport.current_track()
  if track then
    echoport.notify("current:" .. track.title .. "@" .. track.platform)
  else
    echoport.notify("current:none")
  end
end)
"#,
    );
    let (mut scripts, rx, tx) = load(&paths);

    scripts.fire(&ScriptEvent::Startup, &tx);
    let events = drain(&rx);
    assert!(
        matches!(events.as_slice(), [AppEvent::ScriptToast(text)] if text == "current:none"),
        "{events:?}"
    );

    scripts.set_now_playing(Some(track()));
    scripts.fire(&ScriptEvent::Startup, &tx);
    let events = drain(&rx);
    assert!(
        matches!(events.as_slice(), [AppEvent::ScriptToast(text)] if text == "current:Test Title@youtube"),
        "{events:?}"
    );

    scripts.set_now_playing(None);
    scripts.fire(&ScriptEvent::Startup, &tx);
    let events = drain(&rx);
    assert!(
        matches!(events.as_slice(), [AppEvent::ScriptToast(text)] if text == "current:none"),
        "{events:?}"
    );
}

#[test]
fn script_paths_and_version_are_exposed() {
    let (_dir, paths) = setup();
    let config_path = paths.config_file.to_string_lossy().into_owned();
    write(
        &paths.config_file,
        &format!(
            r#"
echoport.actions.add({{
  id = "meta",
  title = "meta",
  run = function()
    echoport.notify(echoport.version .. "|" .. echoport.paths.config_file)
  end,
}})
"#
        ),
    );
    let (mut scripts, rx, tx) = load(&paths);
    assert!(scripts.run_action("meta", &tx));
    let events = drain(&rx);
    let expected = format!("{}|{}", env!("CARGO_PKG_VERSION"), config_path);
    assert!(
        matches!(events.as_slice(), [AppEvent::ScriptToast(text)] if text == &expected),
        "expected ScriptToast({expected:?}), got {events:?}"
    );
}

// ---------------------------------------------------------------------------
// Reload
// ---------------------------------------------------------------------------

#[test]
fn reload_applies_new_config_and_keeps_previous_on_error() {
    let (_dir, paths) = setup();
    write(
        &paths.config_file,
        r#"
echoport.config = { volume = 0.42 }
echoport.actions.add({ id = "keep", title = "keep", run = function() echoport.notify("kept") end })
"#,
    );
    let (mut scripts, rx, tx) = load(&paths);
    assert!(approx(scripts.config().volume.unwrap(), 0.42));

    // Broken syntax: reload must keep the previous settings and actions.
    write(&paths.config_file, "echoport.config = {");
    scripts.reload(tx.clone());
    assert!(approx(scripts.config().volume.unwrap(), 0.42));
    assert!(
        !scripts.config().warnings.is_empty(),
        "failed reload must report a warning"
    );
    assert!(scripts.run_action("keep", &tx));
    let events = drain(&rx);
    assert!(
        matches!(events.as_slice(), [AppEvent::ScriptToast(text)] if text == "kept"),
        "{events:?}"
    );

    // A fixed file is applied.
    write(&paths.config_file, "echoport.config = { volume = 0.11 }");
    scripts.reload(tx.clone());
    assert!(approx(scripts.config().volume.unwrap(), 0.11));
    assert!(scripts.config().loaded);
    assert!(
        !scripts.run_action("keep", &tx),
        "old action should be gone"
    );
}

// ---------------------------------------------------------------------------
// Key chords / binding actions
// ---------------------------------------------------------------------------

#[test]
fn keychord_parse_label_and_binding_action_names() {
    let chord = KeyChord::parse("Ctrl+Shift+K").expect("parse");
    assert_eq!(chord.key, egui::Key::K);
    assert!(chord.ctrl && chord.shift && !chord.alt && !chord.command);
    assert_eq!(chord.label(), "Ctrl+Shift+K");

    let arrow = KeyChord::parse("Ctrl+ArrowRight").expect("parse");
    assert_eq!(arrow.key, egui::Key::ArrowRight);
    assert_eq!(arrow.label(), "Ctrl+ArrowRight");

    let command = KeyChord::parse("Cmd+P").expect("parse");
    assert!(command.command && !command.ctrl);
    assert_eq!(command.label(), "Cmd+P");

    let lower = KeyChord::parse("alt+q").expect("parse lowercase modifiers");
    assert_eq!(lower.key, egui::Key::Q);
    assert!(lower.alt);
    assert_eq!(KeyChord::parse("Space").unwrap().key, egui::Key::Space);
    assert_eq!(KeyChord::parse("Esc").unwrap().key, egui::Key::Escape);
    assert_eq!(KeyChord::parse("F12").unwrap().key, egui::Key::F12);
    assert_eq!(KeyChord::parse("pgup").unwrap().key, egui::Key::PageUp);
    assert_eq!(KeyChord::parse("5").unwrap().key, egui::Key::Num5);

    assert!(KeyChord::parse("").is_none());
    assert!(KeyChord::parse("Ctrl+").is_none());
    assert!(KeyChord::parse("Ctrl+Shift").is_none());
    assert!(KeyChord::parse("Hyper+K").is_none());
    assert!(KeyChord::parse("Ctrl+NotAKey").is_none());

    for (name, expected) in [
        ("play_pause", BindingAction::PlayPause),
        ("next", BindingAction::Next),
        ("previous", BindingAction::Previous),
        ("seek_forward", BindingAction::SeekForward),
        ("seek_backward", BindingAction::SeekBackward),
        ("volume_up", BindingAction::VolumeUp),
        ("volume_down", BindingAction::VolumeDown),
        ("toggle_queue", BindingAction::ToggleQueue),
        ("toggle_favorite", BindingAction::ToggleFavorite),
        ("toggle_shuffle", BindingAction::ToggleShuffle),
        ("cycle_repeat", BindingAction::CycleRepeat),
        ("open_search", BindingAction::OpenSearch),
        ("open_settings", BindingAction::OpenSettings),
        ("open_home", BindingAction::OpenHome),
        ("action:hello", BindingAction::Custom("hello".to_owned())),
    ] {
        assert_eq!(
            BindingAction::from_key(name),
            Some(expected.clone()),
            "{name}"
        );
        assert_eq!(expected.key(), name.strip_prefix("action:").unwrap_or(name));
    }
    assert_eq!(
        BindingAction::Custom("hello".to_owned()).spec(),
        "action:hello"
    );
    assert_eq!(
        BindingAction::Custom("hello".to_owned()).custom_id(),
        Some("hello")
    );
    assert!(BindingAction::from_key("nonsense").is_none());
    assert!(BindingAction::from_key("action:").is_none());

    let binding = KeyBinding::new(chord, BindingAction::Next);
    assert_eq!(binding.chord, chord);
    assert_eq!(binding.action, BindingAction::Next);
}

#[test]
fn keychord_matches_modifiers_exactly() {
    let chord = KeyChord::parse("Ctrl+Shift+K").expect("parse");
    let context = egui::Context::default();

    let matches_with = |modifiers: egui::Modifiers, key: egui::Key| -> bool {
        let input = egui::RawInput {
            modifiers,
            events: vec![egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        };
        let mut matched = false;
        let _ = context.run(input, |context| {
            // egui may run several passes; accumulate so a press in any pass
            // counts.
            matched |= context.input(|input| chord.matches(input));
        });
        matched
    };

    let ctrl_shift = egui::Modifiers {
        ctrl: true,
        shift: true,
        command: true, // on Windows/Linux egui mirrors ctrl into command
        ..Default::default()
    };
    assert!(matches_with(ctrl_shift, egui::Key::K));
    assert!(!matches_with(ctrl_shift, egui::Key::J));

    let ctrl_only = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    assert!(
        !matches_with(ctrl_only, egui::Key::K),
        "shift must be required"
    );

    let shift_only = egui::Modifiers {
        shift: true,
        ..Default::default()
    };
    assert!(
        !matches_with(shift_only, egui::Key::K),
        "ctrl must be required"
    );
}

// ---------------------------------------------------------------------------
// Empty / demo mode
// ---------------------------------------------------------------------------

#[test]
fn empty_scripts_are_inert() {
    let mut scripts = Scripts::empty();
    assert!(!scripts.config().loaded);
    assert!(scripts.config().warnings.is_empty());
    assert!(scripts.loaded_plugins().is_empty());
    assert!(scripts.config_file().to_string_lossy().contains("echoport"));
    assert!(scripts.plugins_dir().to_string_lossy().contains("plugins"));

    let context = egui::Context::default();
    assert!(scripts.poll_binding(&context).is_none());

    let (tx, rx) = unbounded();
    scripts.fire(&ScriptEvent::Startup, &tx);
    assert!(drain(&rx).is_empty());
}

// ---------------------------------------------------------------------------
// poll_binding
// ---------------------------------------------------------------------------

#[test]
fn poll_binding_uses_config_bindings_and_exact_modifiers() {
    let (_dir, paths) = setup();
    write(
        &paths.config_file,
        r#"
echoport.config = {
  keyboard = {
    ["Space"] = "play_pause",
    ["Ctrl+Shift+K"] = "open_settings",
  },
}
"#,
    );
    let (scripts, _rx, _tx) = load(&paths);
    let context = egui::Context::default();

    let poll = |modifiers: egui::Modifiers, key: Option<egui::Key>| -> Option<BindingAction> {
        let events = match key {
            Some(key) => vec![egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed: true,
                repeat: false,
                modifiers,
            }],
            None => Vec::new(),
        };
        let input = egui::RawInput {
            modifiers,
            events,
            ..Default::default()
        };
        let mut result = None;
        let _ = context.run(input, |context| {
            if result.is_none() {
                result = scripts.poll_binding(context);
            }
        });
        result
    };

    let none = egui::Modifiers::default();
    assert_eq!(
        poll(none, Some(egui::Key::Space)),
        Some(BindingAction::PlayPause)
    );
    // No key press -> no action.
    assert_eq!(poll(none, None), None);
    // Exact modifier match: Ctrl+Shift+K only, not Ctrl+K / plain K.
    let ctrl = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    assert_eq!(poll(ctrl, Some(egui::Key::K)), None);
    let ctrl_shift = egui::Modifiers {
        ctrl: true,
        shift: true,
        command: true,
        ..Default::default()
    };
    assert_eq!(
        poll(ctrl_shift, Some(egui::Key::K)),
        Some(BindingAction::OpenSettings)
    );
    // Space is not bound with modifiers.
    assert_eq!(poll(ctrl, Some(egui::Key::Space)), None);
}

// ---------------------------------------------------------------------------
// Rust plugin extension point
// ---------------------------------------------------------------------------

struct DemoPlugin;

impl Plugin for DemoPlugin {
    fn id(&self) -> &str {
        "demo"
    }

    fn on_event(&mut self, event: &ScriptEvent, host: &mut PluginHost) {
        if matches!(event, ScriptEvent::Startup) {
            host.dispatch(Command::ToggleShuffle);
            host.notify("hello from rust plugin");
        }
    }
}

struct QuietPlugin;

impl Plugin for QuietPlugin {
    fn id(&self) -> &str {
        "quiet"
    }

    fn on_event(&mut self, _event: &ScriptEvent, _host: &mut PluginHost) {}
}

#[test]
fn plugin_registry_dispatches_commands_and_toasts() {
    let (tx, rx) = unbounded();
    let mut registry = PluginRegistry::with_sender(tx);
    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);

    registry.register(Box::new(QuietPlugin));
    registry.register(Box::new(DemoPlugin));
    assert_eq!(
        registry.plugins(),
        vec!["quiet".to_owned(), "demo".to_owned()]
    );
    assert!(!registry.is_empty());
    assert_eq!(registry.len(), 2);

    registry.fire(&ScriptEvent::Startup);
    let events = drain(&rx);
    assert!(
        matches!(events.as_slice(), [
            AppEvent::RunCommand(Command::ToggleShuffle),
            AppEvent::ScriptToast(text),
        ] if text == "hello from rust plugin"),
        "{events:?}"
    );

    // Re-registering an id replaces the old plugin (last wins).
    registry.register(Box::new(DemoPlugin));
    assert_eq!(
        registry.plugins(),
        vec!["quiet".to_owned(), "demo".to_owned()]
    );

    // Without a channel the registry still works (notifications are logged).
    let mut detached = PluginRegistry::new();
    detached.register(Box::new(DemoPlugin));
    detached.fire(&ScriptEvent::Startup);
    assert_eq!(detached.plugins(), vec!["demo".to_owned()]);
}

// ---------------------------------------------------------------------------
// Untouched defaults
// ---------------------------------------------------------------------------

#[test]
fn script_config_default_is_empty_and_cheap() {
    let config = ScriptConfig::default();
    assert!(config.language.is_none());
    assert!(config.theme_mode.is_none());
    assert!(config.bindings.is_empty());
    assert!(config.actions.is_empty());
    assert!(config.warnings.is_empty());
    assert!(!config.loaded);
    assert!(config.config_file.as_os_str().is_empty());
}

// ---------------------------------------------------------------------------
// The shipped template must stay usable when uncommented
// ---------------------------------------------------------------------------

/// Heuristic: a line in the template is "commented Lua" when, after removing
/// the `-- ` prefix, it starts with one of these code shapes.
fn looks_like_lua(code: &str) -> bool {
    let code = code.trim_start();
    // Keyboard example lines: `["Space"] = "play_pause"` but not the Chinese
    // prose line that happens to start with `["按键组合"]`.
    if let Some(rest) = code.strip_prefix("[\"") {
        return rest
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric());
    }
    const PREFIXES: &[&str] = &[
        "language =",
        "theme =",
        "follow_cover_accent =",
        "accent =",
        "highlight =",
        "macaron =",
        "startup_page =",
        "ui_scale =",
        "cache_limit_mb =",
        "volume =",
        "seek_step =",
        "volume_step =",
        "results_per_platform =",
        "download_format =",
        "download_dir =",
        "music_dirs =",
        "keyboard =",
        "\"#",
        "}",
        "echoport.",
        "local ",
        "id =",
        "title =",
        "description =",
        "keys =",
        "run =",
        "end,",
        "end)",
        "})",
        "},",
        "if ",
    ];
    PREFIXES.iter().any(|prefix| code.starts_with(prefix))
}

#[test]
fn template_code_examples_are_valid_when_uncommented() {
    let template = include_str!("../examples/config.lua");
    let mut uncommented = String::new();
    for line in template.lines() {
        let trimmed = line.trim_start();
        if let Some(code) = trimmed.strip_prefix("-- ") {
            if looks_like_lua(code) {
                uncommented.push_str(&line[..line.len() - trimmed.len()]);
                uncommented.push_str(code);
                uncommented.push('\n');
                continue;
            }
        }
        uncommented.push_str(line);
        uncommented.push('\n');
    }

    let (_dir, paths) = setup();
    write(&paths.config_file, &uncommented);
    let (scripts, _rx, _tx) = load(&paths);
    assert!(
        scripts.config().loaded,
        "fully uncommented template must load: {:?}",
        scripts.config().warnings
    );
    assert!(
        scripts.config().warnings.is_empty(),
        "fully uncommented template produced warnings: {:?}",
        scripts.config().warnings
    );
    assert!(approx(scripts.config().ui_scale.unwrap(), 1.0));
    assert_eq!(scripts.config().cache_limit_mb, Some(1024));
    let action_ids: Vec<&str> = scripts
        .config()
        .actions
        .iter()
        .map(|action| action.id.as_str())
        .collect();
    assert!(action_ids.contains(&"hello"), "actions: {action_ids:?}");
    assert!(action_ids.contains(&"digest"), "actions: {action_ids:?}");
    assert!(
        scripts.config().bindings.len() >= 8,
        "keyboard + bind examples should register bindings: {:?}",
        scripts.config().bindings
    );
}
