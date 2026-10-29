# Plugins

EchoPort has two plugin interfaces:

1. **Lua plugins** — drop a `*.lua` file into the plugin directory, no
   compilation needed. This is the friendly interface for users.
2. **Compiled-in Rust plugins** — implement the `echoport::scripting::Plugin`
   trait and register it in the app. This is the extension point for Rust
   authors / downstream crates.

Both receive the same lifecycle events (`ScriptEvent`) and can talk to the app
through commands and toasts. This document covers the contract, the event list
and the API stability promise.

> **Interactive HTML manual:** [docs/plugins.html](plugins.html) — a standalone
> bilingual (中文 / English) plugin development guide with copyable examples.
> Configuration options are documented in [docs/config.html](config.html) and
> [docs/SCRIPTING.md](SCRIPTING.md).

---

## 1. Lua plugins

### Location and lifecycle

| Platform | Directory |
|---|---|
| Linux | `~/.config/echoport/plugins/` |
| macOS | `~/Library/Application Support/echoport/plugins/` |
| Windows | `%APPDATA%\echoport\plugins\` |

* All `*.lua` files in the directory are loaded **sorted by file name**
  (`01_intro.lua` before `02_extra.lua`; hidden files starting with `.` are
  ignored, sub-directories are not descended into).
* Each file is executed once, in a fresh chunk, **protected**: a syntax error or
  a runtime error in one plugin is reported as a warning and the remaining
  plugins still load. `loaded_plugins()` lists only files that executed
  successfully.
* Plugins are loaded **after** `config.lua`, so they can read
  `echoport.config`, `echoport.paths` and `echoport.version`.
* Plugins run **synchronously on the UI thread** whenever an event fires. Keep
  callbacks cheap: no blocking IO, no sleeps, no long loops. Use `echoport.log`
  / `echoport.notify` freely (they only queue work).
* Reloading scripts (**Settings → Reload scripts**) rebuilds the whole Lua state
  and re-executes every plugin; state kept in Lua globals is lost.
* A plugin **cannot override user settings**. `echoport.config` is parsed from
  `config.lua` before plugins run; writing to it later has no effect on
  settings (it is only visible to other scripts).

### Trust note / 安全提示

> Lua plugins are **trusted local scripts**, not a security sandbox. They have
> the full Lua 5.4 standard library available (`io`, `os`, `load`, …) and run
> inside the EchoPort process. Only install plugins you would run as a program.
> EchoPort itself never downloads plugins and never sends their contents
> anywhere. / Lua 插件是受信任的本地脚本，不是沙箱：拥有完整 Lua 标准库
> （包括 `io`、`os`），以当前用户权限运行。只安装你信任的插件。

### Anatomy of a plugin

```lua
-- my_plugin.lua  ->  <config_dir>/echoport/plugins/my_plugin.lua

echoport.log("my_plugin loaded, EchoPort " .. echoport.version)

echoport.on("track_start", function(track)
  if track == nil then return end
  echoport.notify("♪ " .. track.title .. " — " .. track.artist)
end)

echoport.actions.add({
  id = "my_greeting",
  title = "Say hello",
  description = "Example action",
  keys = "Ctrl+Alt+H",           -- optional, registers a key binding
  run = function()
    echoport.notify("hello from my_plugin")
  end,
})

echoport.bind("Ctrl+Alt+C", function()
  local track = echoport.current_track()
  if track then
    echoport.notify("now: " .. track.title)
  end
end)
```

A complete, working example ships as
[`examples/plugins/example.lua`](../examples/plugins/example.lua).

### Events

Register with `echoport.on(name, callback)`. Multiple handlers per event are
allowed and run in registration order (config.lua first, then plugins in load
order).

| Event | Callback arguments | When |
|---|---|---|
| `startup` | — | app finished starting |
| `track_start` | `track` table | a track started playing |
| `track_end` | `track` table | a track finished |
| `search_done` | `query` string | a multi-platform search finished |
| `download_done` | `track` table, `path` string | a download finished |
| `library_scanned` | `count` number | local library scan finished |
| `playback_paused` | — | user paused |
| `playback_resumed` | — | user resumed |

`track` table:

```lua
{
  id = "youtube:abc123",         -- stable track id
  title = "Song title",
  artist = "Artist",
  album = "Album",               -- nil when unknown
  platform = "youtube",          -- local | spotify | apple_music | netease |
                                 -- soundcloud | bilibili | youtube
  duration = 215.0,              -- seconds, nil when unknown
  url = "https://...",           -- source page, nil when unknown
  local_path = "/music/x.mp3",   -- local tracks only, nil otherwise
}
```

Handlers never receive invalid pointers; a callback error is caught, logged and
appended to the Settings warning list. It does not stop other callbacks.

### API surface available to plugins

| Call | Purpose |
|---|---|
| `echoport.version` | app version string |
| `echoport.paths = { data, cache, config_file, plugins, downloads, music }` | resolved platform paths |
| `echoport.config` | the `config.lua` configuration table (read-only in practice) |
| `echoport.notify(text)` / `echoport.toast(text)` | transient UI toast |
| `echoport.log(text)` | log file / stderr (`log::info`) |
| `echoport.on(event, fn)` | register a lifecycle handler |
| `echoport.actions.add{ id, title, description?, keys?, run }` | register a custom action (shown in Settings, can be hotkeyed) |
| `echoport.bind(chord, action_or_fn)` | bind a key chord to a built-in action, `"action:<id>"` or a function |
| `echoport.command(action)` | trigger a built-in action (`play_pause`, `next`, `previous`, `seek_forward`, `seek_backward`, `volume_up`, `volume_down`, `toggle_queue`, `toggle_favorite`, `toggle_shuffle`, `cycle_repeat`, `open_search`, `open_settings`, `open_home`) |
| `echoport.search(query)` | search all remote platforms |
| `echoport.open_url(url)` | open an absolute URL in the browser |
| `echoport.play_url(url)` | import a platform link and play it |
| `echoport.set_volume(0..1)` | set volume |
| `echoport.current_track()` | last track passed to `set_now_playing`, or `nil` |
| `echoport.reload()` | hint only — reloading is owned by the host (Settings) |

Behaviour details:

* Values are validated. Unknown event names, bad chords, unknown actions and
  invalid action specs produce warnings instead of errors. A binding to a
  custom action that does not exist (checked after all files load) is dropped
  with a warning.
* Duplicate action ids: **last definition wins** and a warning is recorded
  (including across plugins).
* Duplicate key chords: the later binding replaces the earlier one with a
  warning.
* `echoport.actions.add` chords are optional; when present they are equivalent
  to calling `echoport.bind` with `"action:<id>"`.
* Toasts/commands are queued over the app's event channel and applied on the
  next frame; they are safe to call from any handler.

---

## 2. Rust plugins (compiled-in)

The trait lives in `echoport::scripting`:

```rust
pub trait Plugin: Send {
    fn id(&self) -> &str;
    fn on_event(&mut self, event: &ScriptEvent, host: &mut PluginHost);
}
```

`PluginHost` is the handle to the app:

```rust
impl PluginHost {
    pub fn dispatch(&mut self, command: crate::state::Command);
    pub fn notify(&mut self, text: impl Into<String>);
    pub fn sender(&self) -> Option<&Sender<AppEvent>>;
}
```

Example plugin:

```rust
use echoport::scripting::{Plugin, PluginHost, ScriptEvent};
use echoport::state::Command;

struct AutoShuffle;

impl Plugin for AutoShuffle {
    fn id(&self) -> &str {
        "auto-shuffle"
    }

    fn on_event(&mut self, event: &ScriptEvent, host: &mut PluginHost) {
        if let ScriptEvent::TrackStart(track) = event {
            host.notify(format!("now playing: {}", track.title));
            // Commands are queued and applied on the next UI frame.
            host.dispatch(Command::ToggleShuffle);
        }
    }
}
```

Register the plugin in `EchoPortApp` (once, next to the other startup wiring):

```rust
use echoport::scripting::PluginRegistry;

let mut rust_plugins = PluginRegistry::with_sender(tx.clone());
rust_plugins.register(Box::new(AutoShuffle));

// Later, next to the Lua dispatch point:
rust_plugins.fire(&ScriptEvent::TrackStart(track.clone()));
```

Rules and semantics:

* `PluginRegistry::new()` creates an empty registry without an app channel
  (commands/toasts are only logged); use `with_sender(tx)` /
  `set_sender(tx)` so plugins can reach the UI.
* Registration order is preserved. Registering a second plugin with the same
  `id()` replaces the first one and logs a warning.
* `on_event` runs synchronously on the UI thread. Keep it cheap; use
  `PluginHost::dispatch` / `notify` to hand work to the app. Do not block.
* Rust plugins are trusted, compiled code: panics are not caught (unlike Lua
  callbacks). Wrap risky work in `std::panic::catch_unwind` yourself if needed.
* `PluginRegistry` is plain `Send` data and can be stored anywhere; the Lua
  `Scripts` struct deliberately is **not** `Send` (Lua state is thread-bound).

---

## 3. API stability promise

* **Lua API**: the names documented above (`echoport.on`, `actions.add`,
  `bind`, `notify`, `toast`, `log`, `command`, `search`, `open_url`,
  `play_url`, `set_volume`, `current_track`, `paths`, `version`, `config`, the
  eight event names and the track table fields) are a stable public contract.
  Additions are backwards compatible; deprecations will be documented here and
  in `docs/SCRIPTING.md` for at least one minor release.
* **Track tables**: fields are only ever added, never removed or renamed.
* **Rust extension point**: the `Plugin` trait signature and `PluginHost`
  methods (`dispatch`, `notify`) are stable. The `PluginRegistry` helpers
  (`new`, `with_sender`, `set_sender`, `register`, `fire`, `plugins`,
  `is_empty`, `len`) are stable.
* Internal types (`src/scripting/config.rs`, `lua_api.rs`, private runtime
  structs) are **not** public API and may change.
* Event payloads: `ScriptEvent` may gain variants in a minor release; match
  with a `_ => {}` arm in Rust plugins to stay forward compatible.

---

## 中文摘要

EchoPort 提供两种插件：

1. **Lua 插件**：把 `*.lua` 放进 `<配置目录>/echoport/plugins/` 即可，无需编译。
   按文件名排序加载，每个文件独立执行；一个插件报错只产生警告，不影响其他插件。
   插件可以注册事件钩子（`startup`、`track_start`、`track_end`、`search_done`、
   `download_done`、`library_scanned`、`playback_paused`、`playback_resumed`）、
   自定义动作（会出现在设置页、可绑快捷键）、键位绑定，并调用
   `notify`/`log`/`command`/`search`/`open_url`/`play_url`/`set_volume`/
   `current_track` 等 API。插件在 UI 线程同步执行，回调要保持轻量。
2. **编译进程序的 Rust 插件**：实现 `echoport::scripting::Plugin` trait
   （`id()` + `on_event()`），通过 `PluginRegistry` 注册，用 `PluginHost`
   派发命令/提示。

安全提示：Lua 插件是**受信任的本地脚本**，拥有完整 Lua 标准库（含 `io`/`os`），
不是沙箱；只安装你信任的插件。插件不能覆盖用户的 `config.lua` 设置。

API 稳定性：上文 Lua API、`Plugin`/`PluginHost`/`PluginRegistry` 以及 track 表
字段属于稳定公共契约，只会向后兼容地扩展；`src/scripting` 内部类型不是公共 API。

更多资源：交互式 HTML 手册 [docs/plugins.html](plugins.html)（中英双语）与
[docs/config.html](config.html)（全部配置选项）；完整示例
`examples/plugins/example.lua`、`examples/config.zh-CN.lua`、`examples/config.en.lua`。
