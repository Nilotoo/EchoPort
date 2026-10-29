-- ===========================================================================
--  EchoPort 配置文件 · 中文完整版 / EchoPort configuration · full Chinese edition
-- ===========================================================================
--  文件位置 / Location:
--    Linux:   ~/.config/echoport/config.lua
--    macOS:   ~/Library/Application Support/echoport/config.lua
--    Windows: %APPDATA%\echoport\config.lua
--  插件目录 / Plugins:
--    <config_dir>/echoport/plugins/*.lua   （按文件名排序加载 / sorted by name）
--
--  本文件是 examples/config.zh-CN.lua 的完整中文注释版本，与
--  examples/config.en.lua 的选项键完全一致；复制到上面任意位置即可使用。
--  This is the full Chinese-commented edition; it has exactly the same option
--  keys as examples/config.en.lua. Copy it to the location above to use it.
--
--  修改后如何生效 / How to apply changes:
--    设置页 → 重新加载脚本，或重启 EchoPort。
--    Settings → "Reload scripts" (or restart EchoPort).
--
--  所有设置都是可选的 / Every setting is optional:
--    · 注释掉 = 保留个人档案里保存的值（本文件默认全部注释）
--    · 删除文件 = 下次启动自动重新生成默认模板
--    · 非法值不会崩溃：会被截断/忽略，并在设置页显示一条警告
--
--  交互式手册 / Interactive manuals:
--    docs/config.html   （本文件所有选项的中英文对照 / option reference）
--    docs/plugins.html  （Lua 插件开发指南 / plugin development guide）
-- ===========================================================================

echoport.config = {
  -- -------------------------------------------------------------------------
  --  启动 / Startup
  -- -------------------------------------------------------------------------
  -- 界面语言："zh"（中文）或 "en"（English）。默认跟随系统语言。
  -- language = "zh",

  -- 启动页面 / startup page:
  --   "home" | "search" | "library" | "playlists" | "downloads"
  --   | "now_playing" | "artist" | "profile" | "settings"
  -- startup_page = "home",

  -- 界面缩放 / whole-UI zoom: 0.8 ~ 2.0
  --   设置页预设 / Settings presets: 100%, 110%, 125%, 150%
  -- ui_scale = 1.0,

  -- 初始音量 / initial volume: 0 ~ 1
  -- volume = 0.72,

  -- -------------------------------------------------------------------------
  --  外观 / Appearance
  -- -------------------------------------------------------------------------
  -- 主题 / theme: "pulse"（默认/default）| "tide" | "ink"
  --   旧名字兼容 / legacy aliases: blue_white→pulse, album→pulse, midnight→ink
  -- theme = "pulse",

  -- 是否跟随当前封面取色 / derive accents from the current cover art
  -- follow_cover_accent = false,

  -- 强调色 / accent colour: "#RRGGBB"
  -- accent = "#044BB5",

  -- 荧光高亮色 / fluorescent highlight: "#RRGGBB"
  -- highlight = "#BBFF5C",

  -- 五个马卡龙色 / five macaron pastel colours: 必须正好 5 个 "#RRGGBB"
  -- macaron = {
  --   "#BBCFF5", "#C3E9EA", "#F9E7A6", "#D9CCF2", "#FAD3B8",
  -- },

  -- -------------------------------------------------------------------------
  --  播放 / Playback
  -- -------------------------------------------------------------------------
  -- 快进/快退步长（秒），范围 1 ~ 120
  -- seek step in seconds, range 1 ~ 120
  -- seek_step = 5.0,

  -- 音量键步长，范围 0.01 ~ 0.5
  -- volume button step, range 0.01 ~ 0.5
  -- volume_step = 0.05,

  -- -------------------------------------------------------------------------
  --  搜索、缓存与下载 / Search, cache and downloads
  -- -------------------------------------------------------------------------
  -- 每个平台的搜索结果数量，范围 2 ~ 50
  -- results requested per platform, range 2 ~ 50
  -- results_per_platform = 12,

  -- 下载格式 / download format: "mp3" | "m4a" | "opus" | "flac"
  -- download_format = "mp3",

  -- 下载目录 / download folder（支持 ~ 和 $VAR / supports ~ and $VAR）
  -- download_dir = "~/Music/EchoPort",

  -- 本地曲库扫描目录 / local music folders scanned by the library
  -- music_dirs = { "~/Music", "~/Downloads/Music" },

  -- 音频缓存上限（MiB）/ audio cache cap in MiB
  --   0 = 不保留已播放缓存；设置页预设 0 / 512 / 1024 / 2048；范围 0 ~ 10240
  --   0 = keep no played tracks; Settings presets 0 / 512 / 1024 / 2048; range 0 ~ 10240
  -- cache_limit_mb = 1024,

  -- -------------------------------------------------------------------------
  --  键盘快捷键 / Keyboard shortcuts
  --   写法 / syntax: ["按键组合"] = "动作"
  --   修饰键 / modifiers: Ctrl, Alt, Shift, Cmd（Windows/Linux 上 Cmd == Ctrl）
  --   内置动作 / built-in actions:
  --     play_pause, next, previous, seek_forward, seek_backward,
  --     volume_up, volume_down, toggle_queue, toggle_favorite,
  --     toggle_shuffle, cycle_repeat, open_search, open_settings, open_home
  --   自定义动作 / custom actions: "action:<id>"（见文件后面的 actions.add）
  --   值也可以写函数 / values may also be functions（见文件后面的 echoport.bind）
  -- -------------------------------------------------------------------------
  -- keyboard = {
  --   ["Space"] = "play_pause",
  --   ["Ctrl+ArrowRight"] = "next",
  --   ["Ctrl+ArrowLeft"] = "previous",
  --   ["Ctrl+Alt+H"] = "toggle_queue",
  --   ["Ctrl+Shift+F"] = "toggle_favorite",
  --   ["Ctrl+Alt+J"] = "action:hello",
  --   ["Ctrl+Alt+M"] = function()
  --     echoport.notify("键盘函数回调 / keyboard function fired")
  --   end,
  -- },
}

-- ===========================================================================
--  事件钩子 / Event hooks
-- ===========================================================================
--  可用事件 / available events:
--    "startup"          ()                          应用启动 / app started
--    "track_start"      (track)                     开始播放 / track started
--    "track_end"        (track)                     播放结束 / track ended
--    "search_done"      (query)                     搜索完成 / search finished
--    "download_done"    (track, path)               下载完成 / download finished
--    "library_scanned"  (count)                     曲库扫描完成 / scan finished
--    "playback_paused"  ()                          暂停 / paused
--    "playback_resumed" ()                          继续 / resumed
--
--  track 表 / track table:
--    { id, title, artist, album?, platform, duration?, url?, local_path? }
--    platform = "local" | "spotify" | "apple_music" | "netease"
--             | "soundcloud" | "bilibili" | "youtube"
--  字段可能为 nil，请判空 / fields may be nil — always nil-check.
-- ===========================================================================
-- echoport.on("startup", function()
--   echoport.log("EchoPort " .. echoport.version .. " 已启动 / started")
--   echoport.toast("欢迎使用 EchoPort / welcome")
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
--   echoport.log("搜索完成 / search finished: " .. tostring(query))
-- end)

-- echoport.on("download_done", function(track, path)
--   if track ~= nil then
--     echoport.toast("已保存 / saved: " .. track.title .. " -> " .. path)
--   end
-- end)

-- echoport.on("library_scanned", function(count)
--   echoport.toast("曲库扫描完成 / library scanned: " .. tostring(count))
-- end)

-- echoport.on("playback_paused", function()
--   echoport.log("暂停 / paused")
-- end)

-- echoport.on("playback_resumed", function()
--   echoport.log("继续 / resumed")
-- end)

-- ===========================================================================
--  自定义动作 / Custom actions
-- ===========================================================================
--  action 会出现在设置页的脚本动作列表里，也可以被 keyboard 或 echoport.bind 引用。
--  Actions are listed in Settings and can be referenced by keyboard bindings.
-- ===========================================================================
-- echoport.actions.add({
--   id = "hello",
--   title = "说你好 / Say hello",
--   description = "来自 config.lua 的示例动作 / example action from config.lua",
--   keys = "Ctrl+Alt+K",   -- 可选 / optional; 也可以是字符串表 / or a list of strings
--   run = function()
--     echoport.notify("你好, EchoPort! / Hello, EchoPort!")
--   end,
-- })

-- 一个“播放逻辑”动作：下一首并提示 / playback-logic action: next + toast
-- echoport.actions.add({
--   id = "next_and_toast",
--   title = "下一首并提示 / Next + toast",
--   run = function()
--     echoport.command("next")
--     echoport.notify("下一首 / next track")
--   end,
-- })

-- ===========================================================================
--  自定义键位绑定 / Custom key bindings
--  用法 / usage: echoport.bind(按键, 动作名 或 函数) / bind(chord, action_name_or_function)
-- ===========================================================================
-- echoport.bind("Ctrl+Alt+N", "next")
-- echoport.bind("Ctrl+Alt+D", function()
--   echoport.notify("自定义绑定 / custom binding fired")
-- end)

-- ===========================================================================
--  可用 Lua API 速查 / Lua API cheat sheet
--  下面每一行取消注释后都可以直接运行 / every line is runnable when uncommented
-- ===========================================================================
-- local version = echoport.version
-- local paths = echoport.paths
-- local config = echoport.config
--
-- echoport.notify("界面轻提示 / toast")
-- echoport.toast("界面轻提示 / toast")
-- echoport.log("只写入日志 / log::info only")
-- echoport.command("play_pause")
-- echoport.search("坂本龙一")
-- echoport.open_url("https://music.163.com")
-- echoport.play_url("https://www.youtube.com/watch?v=dQw4w9WgXcQ")
-- echoport.set_volume(0.5)
-- local track = echoport.current_track()
-- reload 请从 设置页 → 重新加载脚本 触发 / reload from Settings instead
--
-- ===========================================================================
--  安全与限制 / Security and limits
-- ===========================================================================
--  · 配置文件以你的用户权限在本机执行，属于“受信任的本地脚本”，不是沙箱。
--    Config files run locally with your user permissions; trusted code, no sandbox.
--  · echoport.* 不提供任何网络 API；Lua 标准库完整可用（含 io/os）。
--    No networking API is exposed by echoport.*; the full Lua stdlib is available.
--  · 所有回调都受保护：单个脚本报错只会产生警告，不会让应用崩溃。
--    Every callback is protected: a script error becomes a warning, never a crash.
--  · 最多在设置页显示 100 条脚本警告，避免刷屏。
--    At most 100 script warnings are shown in Settings.
--  · 插件文件位于 plugins/ 目录，按文件名排序加载，每个文件独立执行。
--    Plugins live in plugins/ and load in sorted order, isolated per file.
--  · 更多说明 / more: docs/plugins.html · docs/SCRIPTING.md · docs/PLUGINS.md
-- ===========================================================================
