-- ===========================================================================
--  EchoPort 示例插件 / EchoPort example plugin
-- ===========================================================================
--  安装 / Install:
--    把这个文件复制到 / copy this file to
--      <config_dir>/echoport/plugins/example.lua
--    Linux:   ~/.config/echoport/plugins/example.lua
--    macOS:   ~/Library/Application Support/echoport/plugins/example.lua
--    Windows: %APPDATA%\echoport\plugins\example.lua
--
--  然后在 设置 -> 重新加载脚本 (Settings -> Reload scripts) 生效。
--  Then use Settings -> "Reload scripts" (or restart EchoPort).
--
--  插件按文件名排序加载, 每个文件独立执行: 一个插件报错不会影响其他插件。
--  Plugins load in sorted file name order; each file runs isolated, so one
--  broken plugin does not stop the others.
--
--  安全提示 / Trust note: Lua plugins are trusted local scripts with the full
--  Lua standard library (io/os available). Only install plugins you trust.
-- ===========================================================================

local version = echoport.version or "?"
echoport.log(string.format("example plugin loaded on EchoPort %s", version))

-- ---------------------------------------------------------------------------
--  事件钩子 / Event hooks
-- ---------------------------------------------------------------------------
echoport.on("startup", function()
  echoport.log("example plugin: startup event")
end)

echoport.on("track_start", function(track)
  if track == nil then
    return
  end
  -- 注意: notify 会在界面上弹出一条轻提示 / show a toast
  echoport.notify("♪ " .. track.title .. " — " .. track.artist)
  echoport.log(string.format(
    "track_start platform=%s duration=%s",
    tostring(track.platform),
    tostring(track.duration)
  ))
end)

echoport.on("track_end", function(track)
  if track ~= nil then
    echoport.log("track_end: " .. track.title)
  end
end)

echoport.on("search_done", function(query)
  echoport.log("search_done: " .. tostring(query))
end)

-- 其他可用事件 / other events:
--   "download_done"    function(track, path) end
--   "library_scanned"  function(count) end
--   "playback_paused"  function() end
--   "playback_resumed" function() end

-- ---------------------------------------------------------------------------
--  自定义动作 / Custom action
--  (设置页会列出这些动作, 也可以绑定快捷键)
--  (these show up in Settings and can be bound to a key)
-- ---------------------------------------------------------------------------
local history = {}

echoport.actions.add({
  id = "example_history",
  title = "显示最近播放 / Show listening history",
  description = "Lists tracks remembered by this session (example plugin)",
  keys = "Ctrl+Alt+L",
  run = function()
    if #history == 0 then
      echoport.notify("example plugin: no tracks seen yet")
      return
    end
    local lines = {}
    for index, title in ipairs(history) do
      if index > 5 then
        break
      end
      lines[#lines + 1] = title
    end
    echoport.notify("recent: " .. table.concat(lines, " | "))
  end,
})

-- 把最近播放记在插件自己的表里 (不写磁盘 / no disk IO)
echoport.on("track_start", function(track)
  if track == nil then
    return
  end
  table.insert(history, 1, track.title)
  if #history > 20 then
    table.remove(history)
  end
end)

-- ---------------------------------------------------------------------------
--  自定义键位绑定 / Custom key bindings
-- ---------------------------------------------------------------------------
-- 绑定到内置动作 / bind to a built-in action:
echoport.bind("Ctrl+Alt+R", "cycle_repeat")

-- 绑定到一个函数 / bind to a function:
echoport.bind("Ctrl+Alt+G", function()
  local track = echoport.current_track()
  if track then
    echoport.notify("now playing: " .. track.title .. " — " .. track.artist)
  else
    echoport.notify("nothing playing yet")
  end
end)

-- ---------------------------------------------------------------------------
--  可以在这里调用的小工具 / little helpers you can call from here
-- ---------------------------------------------------------------------------
-- echoport.command("play_pause")          -- 内置动作 / built-in action
-- echoport.command("next")
-- echoport.search("query")                -- 搜索全部平台
-- echoport.open_url("https://example.com")
-- echoport.play_url("https://www.youtube.com/watch?v=...")
-- echoport.set_volume(0.5)
-- echoport.log("message")                 -- 日志 / log only
-- echoport.toast("message")               -- 界面提示 / toast
