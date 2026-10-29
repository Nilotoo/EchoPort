# Lua 配置 / Scripting

EchoPort 的可选配置层用 Lua 编写：启动选项、主题、快捷键、播放参数、输入/输出
路径，以及自定义事件钩子（hooks）和动作（actions）。Lua 5.4（mlua + vendored），
无需安装 Lua。

> **定位：本地受信任脚本。** 配置文件和插件都可以使用完整的 Lua 标准库（包括
> `io`/`os`），不会联网，也没有沙箱。只运行你信任的脚本，见
> [PLUGINS.md](PLUGINS.md#安全提示--trust-note)。
>
> **交互式 HTML 手册 / Interactive HTML manuals:** [config.html](config.html)
> （全部配置选项 / every option）· [plugins.html](plugins.html)（插件开发 /
> plugin development）。

---

## 1. 文件位置

| 平台 | 配置文件 | 插件目录 |
|---|---|---|
| Linux | `~/.config/echoport/config.lua` | `~/.config/echoport/plugins/*.lua` |
| macOS | `~/Library/Application Support/echoport/config.lua` | `…/echoport/plugins/*.lua` |
| Windows | `%APPDATA%\echoport\config.lua` | `%APPDATA%\echoport\plugins\*.lua` |

* 首次启动且文件不存在时，会写入一份**全注释的双语模板**；已有文件永远不会被
  覆盖（可以放心手改）。
* 删除配置文件后，下次启动会重新生成模板。
* 修改后：**设置 → 重新加载脚本**（或重启应用）生效。
* 可参考仓库里的完整示例 `examples/config.lua`（与内置模板相同）。
* 本地化完整模板：`examples/config.zh-CN.lua`（全中文注释）与
  `examples/config.en.lua`（全英文注释），选项键完全一致，复制任意一个替换
  `config.lua` 即可。

重新加载失败时（例如 Lua 语法错误）会**保留上一次成功的配置和插件**，并在设置页
显示一条警告，不会让应用崩溃。

---

## 2. 完整配置参考

所有字段都可选；注释掉 = 使用个人档案/内置默认值。非法值不会崩溃，只会产生警告。

| 键 | 类型 | 默认 | 说明 |
|---|---|---|---|
| `language` | `"en"` / `"zh"` | 系统语言 | 界面语言 |
| `theme` | `"pulse"` / `"tide"` / `"ink"` | `pulse` | 主题；旧别名 `blue_white` / `album` / `porcelain` → `pulse`，`midnight` → `ink` |
| `follow_cover_accent` | `bool` | 个人档案值 | 是否跟随封面取色 |
| `accent` | `"#RRGGBB"` | 主题色 | 强调色 |
| `highlight` | `"#RRGGBB"` | 主题值 | 荧光高亮色（播放指示、徽章） |
| `macaron` | 5 个 `"#RRGGBB"` 的表 | 内置马卡龙色 | 五个装饰马卡龙色，必须正好 5 个 |
| `startup_page` | `"home"` `"search"` `"library"` `"playlists"` `"downloads"` `"now_playing"` `"artist"` `"profile"` `"settings"` | `home` | 启动页面 |
| `ui_scale` | `0.8..2.0` 数字 | `1.0` | 界面缩放；设置页预设 100% / 110% / 125% / 150%（越界截断并告警） |
| `volume` | `0..1` 数字 | 档案音量 | 初始音量（越界会截断并告警） |
| `seek_step` | 秒，`1..120` | `5.0` | `seek_forward` / `seek_backward` 的步长 |
| `volume_step` | `0.01..0.5` | `0.05` | `volume_up` / `volume_down` 的步长 |
| `results_per_platform` | `2..50` 整数 | `12` | 每个平台请求的结果数 |
| `download_format` | `"mp3"` / `"m4a"` / `"opus"` / `"flac"` | `mp3` | 下载格式 |
| `download_dir` | 路径字符串 | `~/Music/EchoPort` | 下载目录，支持 `~`、`$HOME`、`${VAR}` |
| `music_dirs` | 路径字符串的表 | `{ "~/Music" }` | 本地曲库扫描目录 |
| `cache_limit_mb` | `0..10240` 整数 | `1024` | 音频缓存上限（MiB）；`0` = 不保留已播放缓存；设置页预设 0 / 512 / 1024 / 2048 |
| `keyboard` | `{ [按键] = 动作 }` | 内置快捷键 | 见下节；值也可以是函数 |

`echoport.config` 中的其他键（除 `plugins`、`custom`、`hooks` 这三个自由命名空间外）
会得到一条 `unknown setting` 警告并被忽略。

### 2.1 键盘快捷键

格式：`["按键组合"] = "动作"`。修饰键可用 `Ctrl` / `Alt` / `Shift` / `Cmd`
（Windows/Linux 上 `Cmd` 等价于 `Ctrl`）。

内置动作：`play_pause`、`next`、`previous`、`seek_forward`、`seek_backward`、
`volume_up`、`volume_down`、`toggle_queue`、`toggle_favorite`、`toggle_shuffle`、
`cycle_repeat`、`open_search`、`open_settings`、`open_home`。

自定义动作写作 `"action:<id>"`（`id` 来自 `echoport.actions.add`）。

```lua
echoport.config = {
  keyboard = {
    ["Space"] = "play_pause",
    ["Ctrl+ArrowRight"] = "next",
    ["Ctrl+ArrowLeft"] = "previous",
    ["Ctrl+Shift+F"] = "toggle_favorite",
    ["Ctrl+Alt+J"] = "action:hello",
  },
}
```

按键名与 egui 一致，常用别名都接受（大小写不敏感）：`Space`、`Esc`/`Escape`、
`Enter`、`Tab`、`Backspace`、`Delete`、`Insert`、`Home`、`End`、`PageUp`/`PgUp`、
`PageDown`/`PgDn`、`ArrowLeft`/`Left`、`ArrowRight`、`ArrowUp`/`Up`、
`ArrowDown`/`Down`、`A`–`Z`、`0`–`9`、`F1`–`F35`、`Comma`、`Period`、`Slash`、
`Backslash`、`Minus`、`Equals`、`Backtick` 等。无法识别的按键/动作会产生警告并被忽略。

---

## 3. Lua API 参考

全局表 `echoport`：

### 字段

| 字段 | 说明 |
|---|---|
| `echoport.version` | 应用版本号字符串，如 `"0.1.6"` |
| `echoport.paths` | `{ data, cache, config_file, plugins, downloads, music }` |
| `echoport.config` | 你在 `config.lua` 里写的配置表（加载后读取一次；插件可读） |

### 事件钩子

```lua
echoport.on(事件名, function(参数...) ... end)
```

| 事件 | 回调参数 |
|---|---|
| `"startup"` | — |
| `"track_start"` | `track` 表 |
| `"track_end"` | `track` 表 |
| `"search_done"` | `query` 字符串 |
| `"download_done"` | `track` 表、`path` 字符串 |
| `"library_scanned"` | `count` 数字 |
| `"playback_paused"` | — |
| `"playback_resumed"` | — |

track 表字段（缺省字段可能是 `nil`，请判空）：

```lua
{
  id = "youtube:abc123",
  title = "曲名",
  artist = "艺术家",
  album = "专辑",            -- 可能为 nil
  platform = "youtube",      -- local/spotify/apple_music/netease/soundcloud/bilibili/youtube
  duration = 215.0,          -- 秒，可能为 nil
  url = "https://...",       -- 来源页面，可能为 nil
  local_path = "/path/x.mp3",-- 仅本地曲目，可能为 nil
}
```

同一事件可以注册多个回调，按注册顺序执行。一个回调报错只产生警告，其它回调和
插件继续运行。

### 动作（actions）

```lua
echoport.actions.add({
  id = "hello",                       -- 必填，唯一
  title = "说你好 / Say hello",        -- 必填（缺失时用 id 并告警）
  description = "可选说明",            -- 可选
  keys = "Ctrl+Alt+H",                -- 可选，字符串或字符串表；自动注册键位
  run = function()                    -- 必填
    echoport.notify("Hello!")
  end,
})
```

注册的动作出现在 `ScriptConfig.actions` 里（设置页可展示、可绑定），由
`Scripts::run_action(id)` 执行；带 `keys` 的动作会自动获得一个
`Custom(id)` 键位绑定。

### 键位绑定

```lua
echoport.bind("Ctrl+Alt+N", "next")              -- 绑定到内置动作
echoport.bind("Ctrl+Alt+D", function()           -- 绑定到函数
  echoport.notify("自定义绑定被触发")
end)
echoport.bind("Ctrl+Alt+H", "action:hello")      -- 绑定到自定义动作
```

函数形式会自动包装成一个隐藏的 custom action（同样通过 `run_action` 执行）。
绑定到不存在的 `action:<id>` 会在加载结束时给出警告并被忽略。

### 命令与播放

```lua
echoport.command("play_pause")   -- 内置动作名，见 2.1 列表；也可以 "action:hello"
echoport.search("坂本龙一")       -- 搜索所有远程平台
echoport.open_url("https://example.com")
echoport.play_url("https://www.youtube.com/watch?v=...")  -- 导入链接并播放
echoport.set_volume(0.5)         -- 0..1，越界自动截断
echoport.current_track()         -- 当前/最近播放的 track 表，没有则返回 nil
```

`echoport.command` / 自定义动作 / 键位绑定通过应用事件通道派发（`AppEvent::ScriptBinding`），
在下一帧由应用执行。

### 提示与日志

```lua
echoport.notify("界面上的轻提示")   -- 也写作 echoport.toast(...)
echoport.log("只写入日志，不打扰用户")
```

### 其它

```lua
echoport.reload()  -- 仅提示：请从 设置 → 重新加载脚本 触发重载
```

---

## 4. 可复制示例

### 4.1 主题与启动

```lua
echoport.config = {
  language = "zh",
  theme = "ink",
  follow_cover_accent = false,
  accent = "#2DACB5",
  highlight = "#BBFF5C",
  startup_page = "now_playing",
  volume = 0.6,
}
```

### 4.2 快捷键

```lua
echoport.config = {
  keyboard = {
    ["Space"] = "play_pause",
    ["Ctrl+ArrowRight"] = "next",
    ["Ctrl+ArrowLeft"] = "previous",
    ["Ctrl+Alt+Q"] = "toggle_queue",
  },
}
```

### 4.3 自定义动作 + 键位

```lua
echoport.actions.add({
  id = "focus_mode",
  title = "专注模式 / Focus mode",
  description = "音量降到 30% 并暂停",
  keys = "Ctrl+Alt+F",
  run = function()
    echoport.set_volume(0.3)
    echoport.command("play_pause")
    echoport.notify("专注模式已开启")
  end,
})

echoport.bind("Ctrl+Alt+N", function()
  local track = echoport.current_track()
  if track then
    echoport.notify("正在播放：" .. track.title .. " — " .. track.artist)
  else
    echoport.notify("还没有播放任何歌曲")
  end
end)
```

### 4.4 播放逻辑钩子

```lua
echoport.on("startup", function()
  echoport.log("EchoPort " .. echoport.version .. " 已启动")
end)

echoport.on("track_start", function(track)
  if track == nil then return end
  echoport.notify("♪ " .. track.title .. " — " .. track.artist)
end)

echoport.on("library_scanned", function(count)
  echoport.toast("曲库扫描完成：" .. tostring(count) .. " 首")
end)
```

### 4.5 搜索 / 打开链接

```lua
echoport.actions.add({
  id = "search_artist",
  title = "搜索当前艺术家",
  run = function()
    local track = echoport.current_track()
    if track then
      echoport.search(track.artist)
    end
  end,
})
echoport.bind("Ctrl+Alt+S", function()
  local track = echoport.current_track()
  if track and track.url then
    echoport.open_url(track.url)
  end
end)
```

---

## 5. 与 Rust 的集成（供应用/插件作者参考）

启动时：

```rust
let paths = echoport::scripting::ScriptPaths::resolve();
let mut scripts = echoport::scripting::Scripts::load(paths, tx.clone());
let config = scripts.config(); // Option 字段，None = 保留档案值
if let Some(language) = config.language { /* 应用到 AppState */ }
// ... theme_mode / accent / macaron / highlight / startup_page / volume / ...
```

每帧：

```rust
if let Some(action) = scripts.poll_binding(ctx) {
    match action {
        BindingAction::Custom(id) => { scripts.run_action(&id, &tx); }
        other => app.run_binding_action(other),
    }
}
```

事件（在相应状态变化时调用）：

```rust
scripts.fire(&ScriptEvent::Startup, &tx);
scripts.fire(&ScriptEvent::TrackStart(track.clone()), &tx);
scripts.fire(&ScriptEvent::SearchDone(query.clone()), &tx);
scripts.fire(&ScriptEvent::DownloadDone(track.clone(), path.clone()), &tx);
scripts.fire(&ScriptEvent::LibraryScanned(count), &tx);
scripts.fire(&ScriptEvent::PlaybackPaused, &tx);
scripts.set_now_playing(Some(track.clone()));
```

`Scripts` 不含 `Send`（Lua 状态绑定线程），请放在 UI 线程；演示/截图模式可以用
`Scripts::empty()` 完全跳过文件 IO。

---

## 6. 故障排查

| 现象 | 处理 |
|---|---|
| 配置改了没生效 | 设置 → 重新加载脚本；看设置页的 Script warnings |
| 有 "syntax error" 警告 | 检查 Lua 语法（逗号、`end`、引号）；应用会保留上次配置 |
| 快捷键不响应 | 动作名拼写；输入框聚焦时快捷键会被抑制；是否与其他绑定冲突（后注册覆盖先前） |
| `action:<id>` 警告 unknown | 先 `actions.add` 注册，或检查 id 大小写 |
| 主题/颜色不生效 | 颜色必须是 `"#RRGGBB"`；`macaron` 必须正好 5 个 |
| 插件没加载 | 文件名必须以 `.lua` 结尾；看 warnings 里 `<file>: <error>` |
| 想恢复默认 | 删除 `config.lua`（插件不动），重启/重载 |

---

## English summary

EchoPort can be configured with an optional Lua 5.4 file at
`<config_dir>/echoport/config.lua` (written as a commented bilingual template on
first run, never overwritten; full single-language editions live in
`examples/config.zh-CN.lua` / `examples/config.en.lua`) plus Lua plugins in the
sibling `plugins/` directory. Options include UI language/theme/zoom
(`ui_scale`), cover-accent following, custom colours, startup page, volume,
seek/volume steps, search results, download format/folder, music folders and the
audio cache cap (`cache_limit_mb`). The global `echoport` table provides `config`, `paths`, `version`,
lifecycle hooks (`on`), custom actions (`actions.add` + `run_action`), key
bindings (`bind`), playback commands (`command`, `search`, `open_url`,
`play_url`, `set_volume`, `current_track`) and UI toasts/logging (`notify`,
`toast`, `log`). Every value is validated and clamped; invalid entries become
warnings in the Settings page instead of panics. Lua errors are caught per
callback and per plugin file, so one broken plugin never blocks the others.
Reload from Settings; a failed reload keeps the previous configuration. Scripts
are trusted local code with the full Lua standard library — treat plugins like
any other program you install. See [PLUGINS.md](PLUGINS.md) for the plugin
contract and the compiled-in Rust `Plugin` trait. Interactive HTML editions:
[config.html](config.html) and [plugins.html](plugins.html).
