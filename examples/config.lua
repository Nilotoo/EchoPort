-- ===========================================================================
--  EchoPort 配置文件 / EchoPort configuration file
-- ===========================================================================
--  位置 / Location:
--    Linux:   ~/.config/echoport/config.lua
--    macOS:   ~/Library/Application Support/echoport/config.lua
--    Windows: %APPDATA%\echoport\config.lua
--  插件 / Plugins:
--    <config_dir>/echoport/plugins/*.lua  (按文件名排序加载 / sorted by name)
--
--  修改后如何生效 / How to apply changes:
--    设置页 -> 重新加载脚本, 或重启应用.
--    Settings -> "Reload scripts" (or restart EchoPort).
--
--  所有设置都是可选的 / Every setting is optional:
--    * 注释掉 = 使用个人档案里保存的值 / commented out = keep the profile value
--    * 删除本文件 = 下次启动自动重新生成 / delete this file = recreated next run
--    * 非法值不会崩溃, 只会在设置页显示一条警告 / invalid values never crash,
--      they show up as a warning in Settings.
--
--  本文件同时是完整参考配置 (examples/config.lua)。
--  This file is also the full reference configuration (examples/config.lua).
--  本地化完整模板 / Localized full templates:
--    examples/config.zh-CN.lua  (全中文注释 / all-Chinese comments)
--    examples/config.en.lua     (all-English comments)
--    两者选项键完全相同, 复制任意一个替换本文件即可。
--    Both use exactly the same option keys; copy either one over this file.
--  默认状态下所有选项都被注释; 取消注释即可启用。
--  Everything is commented out by default; uncomment what you need.
--  交互式手册 / Interactive manuals: docs/plugins.html · docs/config.html
-- ===========================================================================

echoport.config = {
  -- -------------------------------------------------------------------------
  -- 启动 / Startup
  -- -------------------------------------------------------------------------
  -- 界面语言 / UI language: "en" (English) 或 "zh" (中文)
  -- language = "zh",

  -- 启动页面 / Startup page:
  --   "home" | "search" | "library" | "playlists" | "downloads"
  --   | "now_playing" | "artist" | "profile" | "settings"
  -- startup_page = "home",

  -- 界面缩放 / whole-UI zoom: 0.8..2.0 (设置页预设 / presets: 100%, 110%, 125%, 150%)
  -- ui_scale = 1.0,

  -- 初始音量 0..1 / initial volume 0..1 (default 0.72)
  -- volume = 0.72,

  -- 每个平台请求多少条搜索结果 / results per platform (2..50)
  -- results_per_platform = 12,

  -- -------------------------------------------------------------------------
  -- 外观 / Appearance
  -- -------------------------------------------------------------------------
  -- 主题 / Theme: "pulse" (默认/default), "tide", "ink"
  -- 旧名字也可用 / legacy aliases: blue_white→pulse, album→pulse, midnight→ink
  -- theme = "pulse",

  -- 是否跟随封面取色 / derive accents from the current cover art (true/false)
  -- follow_cover_accent = true,

  -- 强调色 / accent colour, "#RRGGBB" (overrides the theme accent)
  -- accent = "#044BB5",

  -- 荧光高亮色 / fluorescent highlight (playing indicator, badges)
  -- highlight = "#BBFF5C",

  -- 五个马卡龙色 / five macaron pastel colours, exactly five "#RRGGBB"
  -- macaron = {
  --   "#BBCFF5", "#C3E9EA", "#F9E7A6", "#D9CCF2", "#FAD3B8",
  -- },

  -- -------------------------------------------------------------------------
  -- 播放 / Playback
  -- -------------------------------------------------------------------------
  -- 方向键快进/快退步长(秒) / seek step in seconds (1..120)
  -- seek_step = 5.0,

  -- 音量键步长 / volume step (0.01..0.5)
  -- volume_step = 0.05,

  -- -------------------------------------------------------------------------
  -- 输入输出路径 / Input & output paths
  -- 支持 ~ 和 $HOME / `~` and `$VAR` are expanded.
  -- -------------------------------------------------------------------------
  -- 下载格式 / download format: "mp3" | "m4a" | "opus" | "flac"
  -- download_format = "mp3",

  -- 下载目录 / download folder
  -- download_dir = "~/Music/EchoPort",

  -- 本地音乐目录 / local music folders (library scan)
  -- music_dirs = { "~/Music", "~/Downloads/Music" },

  -- 音频缓存上限 (MiB) / audio cache cap in MiB: 0 = 不保留已播放缓存
  --   设置页预设 / Settings presets: 0, 512, 1024, 2048; 0..10240 allowed
  -- cache_limit_mb = 1024,

  -- -------------------------------------------------------------------------
  -- 键盘快捷键 / Keyboard shortcuts
  --   ["按键组合"] = "动作", 例如 / e.g. ["Ctrl+Shift+K"] = "next"
  --   可用修饰键 / modifiers: Ctrl, Alt, Shift, Cmd (Windows/Linux 上 Cmd == Ctrl)
  --   内置动作 / built-in actions:
  --     play_pause, next, previous, seek_forward, seek_backward,
  --     volume_up, volume_down, toggle_queue, toggle_favorite,
  --     toggle_shuffle, cycle_repeat, open_search, open_settings, open_home
  --   自定义动作 / custom actions: "action:<id>" (见文件末尾的 actions.add)
  --   值也可以是函数 / values may also be Lua functions (see echoport.bind below)
  -- -------------------------------------------------------------------------
  -- keyboard = {
  --   ["Space"] = "play_pause",
  --   ["Ctrl+ArrowRight"] = "next",
  --   ["Ctrl+ArrowLeft"] = "previous",
  --   ["Ctrl+Alt+H"] = "toggle_queue",
  --   ["Ctrl+Shift+F"] = "toggle_favorite",
  --   ["Ctrl+Alt+J"] = "action:hello",
  -- },
}

-- ===========================================================================
--  事件钩子 / Event hooks
-- ===========================================================================
--  可用事件 / available events:
--    "startup"          ()                                   应用启动 / app started
--    "track_start"      (track)                              开始播放 / track started
--    "track_end"        (track)                              播放结束 / track ended
--    "search_done"      (query)                              搜索完成 / search finished
--    "download_done"    (track, path)                        下载完成 / download finished
--    "library_scanned"  (count)                              曲库扫描完成 / scan finished
--    "playback_paused"  ()                                   暂停 / paused
--    "playback_resumed" ()                                   继续 / resumed
--
--  track 表 / track table:
--    { id, title, artist, album?, platform, duration?, url?, local_path? }
--    platform = "local" | "spotify" | "apple_music" | "netease"
--             | "soundcloud" | "bilibili" | "youtube"
-- ===========================================================================
-- echoport.on("startup", function()
--   echoport.log("EchoPort " .. echoport.version .. " started, config=" .. echoport.paths.config_file)
--   echoport.toast("欢迎 / Welcome to EchoPort")
-- end)

-- echoport.on("track_start", function(track)
--   if track == nil then return end
--   echoport.notify("♪ " .. track.title .. " — " .. track.artist)
-- end)

-- echoport.on("search_done", function(query)
--   echoport.log("search finished: " .. tostring(query))
-- end)

-- echoport.on("download_done", function(track, path)
--   echoport.toast("saved " .. track.title .. " -> " .. path)
-- end)

-- ===========================================================================
--  自定义动作 / Custom actions
-- ===========================================================================
--  action 会出现在设置页的快捷键列表里, 也可以被 keyboard / echoport.bind 引用。
--  Actions are listed in Settings and can be bound to keys.
-- ===========================================================================
-- echoport.actions.add({
--   id = "hello",
--   title = "说你好 / Say hello",
--   description = "A tiny demo action registered from config.lua",
--   keys = "Ctrl+Alt+K",           -- 可选 / optional; may be a string or a list
--   run = function()
--     echoport.notify("你好, EchoPort! / Hello, EchoPort!")
--   end,
-- })

-- 纯播放逻辑动作 / playback logic action
-- echoport.actions.add({
--   id = "digest",
--   title = "跳到下一首并提示 / Next + toast",
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
-- ===========================================================================
--  下面每一条取消注释后都可直接运行 / every line below is runnable when
--  uncommented:
--
--  local version = echoport.version
--  local paths = echoport.paths
--  local config = echoport.config
--
--  echoport.notify("界面提示 / toast")
--  echoport.toast("界面提示 / toast")
--  echoport.log("写入日志 / log::info")
--  echoport.command("play_pause")
--  echoport.search("坂本龙一")
--  echoport.open_url("https://music.163.com")
--  echoport.play_url("https://www.youtube.com/watch?v=dQw4w9WgXcQ")
--  echoport.set_volume(0.5)
--  local track = echoport.current_track()
--  reload 请从 设置页 -> 重新加载脚本 触发 / reload from Settings instead
--
--  更多说明 / More: docs/SCRIPTING.md 和 / and docs/PLUGINS.md
-- ===========================================================================
