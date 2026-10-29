-- ===========================================================================
--  EchoPort configuration · full English edition
--  (same option keys as examples/config.zh-CN.lua)
-- ===========================================================================
--  Location:
--    Linux:   ~/.config/echoport/config.lua
--    macOS:   ~/Library/Application Support/echoport/config.lua
--    Windows: %APPDATA%\echoport\config.lua
--  Plugins:
--    <config_dir>/echoport/plugins/*.lua   (loaded sorted by file name)
--
--  This is the full English-commented edition. It has exactly the same option
--  keys as examples/config.zh-CN.lua; copy either file to the location above.
--
--  How to apply changes:
--    Settings → "Reload scripts", or restart EchoPort.
--
--  Every setting is optional:
--    · commented out = keep the value stored in your profile (all are commented
--      out by default in this file)
--    · delete the file = a fresh default template is written on the next start
--    · invalid values never crash: they are clamped / ignored and reported as a
--      warning in Settings
--
--  Interactive manuals:
--    docs/config.html   (every option below, bilingual / 中英对照)
--    docs/plugins.html  (Lua plugin development guide)
-- ===========================================================================

echoport.config = {
  -- -------------------------------------------------------------------------
  --  Startup
  -- -------------------------------------------------------------------------
  -- UI language: "en" (English) or "zh" (Chinese). Defaults to the system
  -- language. / 界面语言：中英文均可。
  -- language = "en",

  -- Startup page:
  --   "home" | "search" | "library" | "playlists" | "downloads"
  --   | "now_playing" | "artist" | "profile" | "settings"
  -- startup_page = "home",

  -- Whole-UI zoom: 0.8 ~ 2.0
  --   Settings presets: 100%, 110%, 125%, 150%
  -- ui_scale = 1.0,

  -- Initial volume: 0 ~ 1
  -- volume = 0.72,

  -- -------------------------------------------------------------------------
  --  Appearance
  -- -------------------------------------------------------------------------
  -- Theme: "pulse" (default) | "tide" | "ink"
  --   Legacy aliases still work: blue_white→pulse, album→pulse, midnight→ink
  -- theme = "pulse",

  -- Derive accents from the current cover art (true / false)
  -- follow_cover_accent = false,

  -- Accent colour: "#RRGGBB"
  -- accent = "#044BB5",

  -- Fluorescent highlight colour: "#RRGGBB"
  -- highlight = "#BBFF5C",

  -- Five macaron pastel colours: exactly five "#RRGGBB" strings
  -- macaron = {
  --   "#BBCFF5", "#C3E9EA", "#F9E7A6", "#D9CCF2", "#FAD3B8",
  -- },

  -- -------------------------------------------------------------------------
  --  Playback
  -- -------------------------------------------------------------------------
  -- Seek step in seconds, range 1 ~ 120
  -- seek_step = 5.0,

  -- Volume button step, range 0.01 ~ 0.5
  -- volume_step = 0.05,

  -- -------------------------------------------------------------------------
  --  Search, cache and downloads
  -- -------------------------------------------------------------------------
  -- Search results requested per platform, range 2 ~ 50
  -- results_per_platform = 12,

  -- Download format: "mp3" | "m4a" | "opus" | "flac"
  -- download_format = "mp3",

  -- Download folder (supports ~ and $VAR)
  -- download_dir = "~/Music/EchoPort",

  -- Local music folders scanned by the library
  -- music_dirs = { "~/Music", "~/Downloads/Music" },

  -- Audio cache cap in MiB
  --   0 = keep no played tracks; Settings presets 0 / 512 / 1024 / 2048;
  --   allowed range 0 ~ 10240
  -- cache_limit_mb = 1024,

  -- -------------------------------------------------------------------------
  --  Keyboard shortcuts
  --   Syntax: ["chord"] = "action"
  --   Modifiers: Ctrl, Alt, Shift, Cmd (on Windows/Linux Cmd == Ctrl)
  --   Built-in actions:
  --     play_pause, next, previous, seek_forward, seek_backward,
  --     volume_up, volume_down, toggle_queue, toggle_favorite,
  --     toggle_shuffle, cycle_repeat, open_search, open_settings, open_home
  --   Custom actions: "action:<id>" (see actions.add later in this file)
  --   Values may also be Lua functions (see echoport.bind later in this file)
  -- -------------------------------------------------------------------------
  -- keyboard = {
  --   ["Space"] = "play_pause",
  --   ["Ctrl+ArrowRight"] = "next",
  --   ["Ctrl+ArrowLeft"] = "previous",
  --   ["Ctrl+Alt+H"] = "toggle_queue",
  --   ["Ctrl+Shift+F"] = "toggle_favorite",
  --   ["Ctrl+Alt+J"] = "action:hello",
  --   ["Ctrl+Alt+M"] = function()
  --     echoport.notify("keyboard function fired / 键盘函数回调")
  --   end,
  -- },
}

-- ===========================================================================
--  Event hooks
-- ===========================================================================
--  Available events:
--    "startup"          ()                          app finished starting
--    "track_start"      (track)                     a track started playing
--    "track_end"        (track)                     a track finished
--    "search_done"      (query)                     a search finished
--    "download_done"    (track, path)               a download finished
--    "library_scanned"  (count)                     local library scan finished
--    "playback_paused"  ()                          paused
--    "playback_resumed" ()                          resumed
--
--  track table:
--    { id, title, artist, album?, platform, duration?, url?, local_path? }
--    platform = "local" | "spotify" | "apple_music" | "netease"
--             | "soundcloud" | "bilibili" | "youtube"
--  Fields may be nil — always nil-check.
-- ===========================================================================
-- echoport.on("startup", function()
--   echoport.log("EchoPort " .. echoport.version .. " started")
--   echoport.toast("Welcome to EchoPort / 欢迎使用 EchoPort")
-- end)

-- echoport.on("track_start", function(track)
--   if track == nil then return end
--   echoport.notify("♪ " .. track.title .. " — " .. track.artist)
--   echoport.log("playing on " .. track.platform)
-- end)

-- echoport.on("track_end", function(track)
--   if track ~= nil then
--     echoport.log("track_end: " .. track.title)
--   end
-- end)

-- echoport.on("search_done", function(query)
--   echoport.log("search finished: " .. tostring(query))
-- end)

-- echoport.on("download_done", function(track, path)
--   if track ~= nil then
--     echoport.toast("saved: " .. track.title .. " -> " .. path)
--   end
-- end)

-- echoport.on("library_scanned", function(count)
--   echoport.toast("library scanned: " .. tostring(count) .. " tracks")
-- end)

-- echoport.on("playback_paused", function()
--   echoport.log("paused")
-- end)

-- echoport.on("playback_resumed", function()
--   echoport.log("resumed")
-- end)

-- ===========================================================================
--  Custom actions
-- ===========================================================================
--  Actions appear in the Settings script-action list and can be referenced by
--  keyboard bindings or echoport.bind. / 动作会出现在设置页，可绑定快捷键。
-- ===========================================================================
-- echoport.actions.add({
--   id = "hello",
--   title = "Say hello",
--   description = "Example action from config.lua / 来自 config.lua 的示例动作",
--   keys = "Ctrl+Alt+K",   -- optional; may be a string or a list of strings
--   run = function()
--     echoport.notify("Hello, EchoPort!")
--   end,
-- })

-- A playback-logic action: next track + toast
-- echoport.actions.add({
--   id = "next_and_toast",
--   title = "Next + toast",
--   run = function()
--     echoport.command("next")
--     echoport.notify("next track")
--   end,
-- })

-- ===========================================================================
--  Custom key bindings
--  usage: echoport.bind(chord, action_name_or_function)
-- ===========================================================================
-- echoport.bind("Ctrl+Alt+N", "next")
-- echoport.bind("Ctrl+Alt+D", function()
--   echoport.notify("custom binding fired / 自定义绑定")
-- end)

-- ===========================================================================
--  Lua API cheat sheet
--  Every line below is runnable when uncommented.
-- ===========================================================================
-- local version = echoport.version
-- local paths = echoport.paths
-- local config = echoport.config
--
-- echoport.notify("toast")
-- echoport.toast("toast")
-- echoport.log("log::info only")
-- echoport.command("play_pause")
-- echoport.search("Ryuichi Sakamoto")
-- echoport.open_url("https://music.163.com")
-- echoport.play_url("https://www.youtube.com/watch?v=dQw4w9WgXcQ")
-- echoport.set_volume(0.5)
-- local track = echoport.current_track()
-- reload from Settings -> "Reload scripts" instead (echoport.reload is a hint)
--
-- ===========================================================================
--  Security and limits
-- ===========================================================================
--  · Config files run locally with your user permissions: trusted code, not a
--    sandbox. / 配置文件以当前用户权限在本机执行，属于受信任脚本，不是沙箱。
--  · echoport.* exposes no networking API; the full Lua standard library
--    (including io/os) is available.
--  · Every callback is protected: a script error becomes a warning in Settings,
--    never a crash. / 所有回调受保护，脚本报错只产生警告，不会崩溃。
--  · At most 100 script warnings are shown in Settings.
--  · Plugin files live in plugins/, load sorted by file name and are isolated
--    per file (one broken plugin does not stop the others).
--  · More: docs/plugins.html · docs/SCRIPTING.md · docs/PLUGINS.md
-- ===========================================================================
