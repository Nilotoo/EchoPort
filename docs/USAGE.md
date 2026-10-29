# EchoPort 使用指南 · Usage Guide

EchoPort（回声港）是一个本地优先的多音源桌面音乐播放器：本地曲库、在线搜索、跨平台歌单、
下载与「歌曲背景」都集中在同一个安静的界面里。**不需要账号，不上传任何数据。**

> 界面支持**中文 / English**，可在「设置 → 语言」或侧边栏快速切换；本指南两种语言都给出。

---

## 中文

### 1. 首次运行

1. 安装并运行 EchoPort（见 [README 安装章节](../README.md)）。
2. 首次启动会创建默认资料「Me」，保存在本机；此时音乐库是空的。
3. 打开 **设置** 确认「媒体工具」：
   * `yt-dlp`：在线播放/搜索/下载需要（未安装时显示「未找到」，可点「重新检测」）。
   * `ffmpeg`：格式转换与部分音源需要。
4. 选择语言与主题（设置 → 外观）。

### 1.1 更新检查、启动动画与 Arch 安装（v0.1.9）

* **更新检查**：启动时匿名请求 GitHub Releases（默认 `Nilotoo/EchoPort`，可用
  `ECHOPORT_UPDATE_REPO=owner/name` 覆盖；仅接受 owner/name 与 https://github.com/ 发布链接），
  之后每 6 小时一次；没有 Release 时回退读取 `VERSION`。
  只比较版本，不上报遥测、不需要 token、不自动下载/安装。About → **Check for updates** 可手动检查；
  发现更新时 About 显示更新行，点 Update 打开 Release 页面。
* **启动动画**：约 0.9 s（检查进行中最多 1.4 s）；发现更新时停留显示 **Stay on this version / Update**。
  `ECHOPORT_SPLASH=1` 强制显示、`ECHOPORT_NO_SPLASH=1` 关闭、`ECHOPORT_NO_UPDATE_CHECK=1` 禁止检查、
  `ECHOPORT_FAKE_UPDATE=9.9.9` 伪造更新用于测试。
* **Arch 用户安装**：源码目录运行 `./scripts/install-arch.sh`（无 sudo）→ `~/.local/bin/echoport` +
  桌面项 + hicolor 32/48/128/256 图标；发布包解包后也可运行 `./install-arch.sh`。
* 仓库 URL 仍为占位假设（等待确认远端/认证），因此真实网络更新检查尚未验证。

### 1.2 安装方式（v0.2.0）

* **Arch pacman 包**：发布资产 `echoport-0.2.0-1-x86_64.pkg.tar.zst`；
  `sudo pacman -U echoport-0.2.0-1-x86_64.pkg.tar.zst`（AUR-ready PKGBUILD 在 `packaging/arch/`）。
* **Windows MSI（per-user）**：`echoport-0.2.0-windows-x86_64.msi`，双击安装（无需管理员），
  开始菜单快捷方式 + 标准卸载；开始菜单中的 **「Uninstall EchoPort (clean app data)」** 会额外删除
  设置/资料/缓存并保留 `%USERPROFILE%\Music\EchoPort` 中已下载音乐。未签名 → SmartScreen 提示。绿色版 zip 仍可用。
* **macOS**：仓库 CI 的 `macos-latest` job 构建 universal `.app`（`EchoPort.app.zip`）；
  本机无 macOS，未在真机验证、ad-hoc 未签名。
* 内嵌文档：设置 → Scripting 的 **Plugin docs / Config reference** 按钮会把双语 HTML 写入数据目录
  并用浏览器打开（不需要随包文件）；`ECHOPORT_OPEN_DOCS` 无额外配置。

### 2. 添加本地音乐

1. 进入 **音乐库** → **添加音乐文件夹**，选择歌曲所在目录（可添加多个）。
2. 点击 **扫描音乐库**；EchoPort 会递归扫描 mp3 / m4a / flac / wav / ogg 等文件，读取标题、
   歌手、专辑、时长和内嵌封面。
3. 双击歌曲即可播放；本地文件不依赖网络，可离线使用。

### 3. 搜索在线音乐

1. 打开 **搜索**，输入歌曲/歌手/专辑名，按 Enter。
2. 结果按相关度分段显示：
   * **最佳匹配 / Best matches**：跨平台最相关的 3 首；
   * **制作人 / 歌手**：每个平台最多 2 位相关艺人，**单击卡片打开艺术家主页**；
   * **各平台列表**：按相关度排序，默认收起为 5 首，可 **展开 / 收起**；**加载更多** 每次追加 12 首（上限 60）。
3. 用上方的平台标签筛选或排除平台；每首结果可播放、加入队列/歌单、下载、收藏或打开原页面。
4. 搜索会同时请求歌曲和制作人（并行，约 2.5 秒），出现转圈时界面仍可操作，不会卡住。
5. **防连点**：歌曲还在加载时再点播放/三角形会被忽略（同一首只解析一次）；加载中点暂停/继续也不会产生额外请求。

### 3.1 艺术家 / 制作人主页

* 单击搜索页的制作人卡片（或平台来源卡）进入艺术家页：头像、平台标识、来源页链接、**Wikipedia 简介**、
  **热门歌曲**、以及 **我的收听历史**（本机播放过的该艺人歌曲）。
* 简介为空、加载中或出错时都有提示；顶部「Back to search」返回搜索结果。
* 热门歌曲按平台来源解析（Netease 热门接口 / Apple artistId / YouTube·SoundCloud·Bilibili 匹配）；
  仍遵循同样的 DRM 与来源规则。

### 3.2 加载更多 / 展开状态

* 「展开 / Show more」只展示 **已经取回** 的其他结果，不再发请求；「加载更多 / Load more」才向该平台
  追加 12 首（按钮内显示加载点，达到 60 首上限后隐藏）。

### 4. 导入链接（Spotify / Apple Music 等）

* 把 **Spotify** 或 **Apple Music** 的歌曲链接粘贴到搜索框回车即可导入。
* **Apple Music 可以像网易云一样直接搜索**（界面显示「可搜索」）；Spotify 没有匿名搜索，只能粘贴链接导入。
* 两者都不能被 EchoPort 直接播放（受 DRM 保护）：
  * 导入后显示公开元数据与封面；
  * 播放时优先用 yt-dlp 匹配**完整时长**的公开音频（可能匹配到现场/不同版本，属正常限制），
    播放条会短暂显示「正在加载音频…」；不会绕过 DRM。
* 同样支持粘贴 YouTube / SoundCloud / Bilibili / 网易云链接直接导入播放。

### 5. 播放、队列与背景

* 底部播放条常驻：播放/暂停、上一首/下一首、进度拖动、音量、随机/循环、队列；左侧有细分隔线，
  进度条与播放键以内容列为中心，所有按钮共享一条垂直中线。
* **防连点播放**：歌曲加载期间重复点播放会被忽略（只解析一次）；加载中点暂停/继续不会产生额外请求；
  行内三角形按钮保持单击播放/切换。
* **来源页面按钮** 打开弹窗，可 **复制链接** 或 **打开原始页面**；随机播放使用新的双箭头图标（循环按钮旁）。
* **双击交互**：单击选中/高亮歌曲行、卡片、歌单或下载完成项；双击播放歌曲 / 打开歌单；悬停会提示「双击播放 / 双击打开」。
* 打开 **正在播放** 可看到大封面、歌词位置、音量与 **[歌曲 | 背景]** 两个标签页；
  页面保持初版极简设计：纯主题背景，没有封面取色/光晕/模糊大图/漂浮元素/持续动画。
* 封面下方保留 **macaron 调色板小点**，并显示 **Released 发行日期**（Apple/Netease/Spotify/yt-dlp
  导入或本地标签提供时；来源无日期时显示 `—`）。
* **背景** 标签页只显示与当前 **歌手 + 专辑 + 歌曲** 严格相关的内容：歌手简介、专辑背景、歌曲背景三段
  （查不到就留空，不塞无关内容），以及相关采访/解读视频与延伸阅读链接；均为可点击的公开链接。
* 队列面板可查看、删除、清空队列；「下一首播放」可插队。

### 5.1 迷你播放器与后台性能模式（v0.1.7）

* 点击侧边栏顶部的 **EchoPort logo** 可打开/关闭 **迷你播放器**：420×142、始终置顶的浮动窗口，
  含封面（点击恢复主窗口）、标题/歌手、进度条与时间、上一首/播放暂停/下一首、打开主窗口、关闭。
  播放不会中断；主窗口可以被最小化或遮挡。
* **后台性能模式**：主窗口失焦或最小化时 UI 重绘节奏自动降速（播放中 66 ms → 1000 ms、
  有后台任务 90 ms → 1000 ms、布局 50 ms → 关闭），动画冻结但音频继续；回前台自动恢复。
* QA：`ECHOPORT_MINI=1` 启动即打开迷你窗口；`scripts/qa-mini-player.sh` 可在 X11 下截图/自检。

### 6. 歌单（跨平台混排）

1. 进入 **歌单** → **新建歌单**，输入名称。
2. 在搜索结果或歌曲行菜单里选择 **添加到歌单**；本地与各平台歌曲可以混在同一个歌单。
3. 歌单支持播放全部 / 随机播放全部、重命名、删除、移除歌曲。
4. 歌单只保存「歌曲 ID + 元数据」，全部在本地 JSON 中。
5. **离线分享**：歌单页工具栏可 **导入歌单**，歌单详情页可 **导出歌单**——导出的是 JSON **元数据**
   （曲目 ID、标题、艺术家、专辑、来源页面链接，**不含音频**）；对方导入后按自己的音源解析播放。

### 7. 下载

1. 在歌曲行的菜单或 **正在播放** 页点击 **下载**；下载任务会出现在 **下载** 页。
2. 在设置中选择音频格式（MP3 / M4A / Opus / FLAC）与下载目录（默认 `~/Music/EchoPort`）。
3. 下载由 yt-dlp 完成，需要网络与可用的 yt-dlp；完成后可「打开目录」。

### 8. 个人资料（Profiles）、统计与历史

* **个人资料** 页管理多个本机资料：新建、切换、删除。每个资料有独立的歌单、收藏、历史与设置。
* **头像与简介**：可为每个资料选择图片头像（256px PNG，内嵌在资料 JSON）或移除；未设置时显示按资料名
  生成的柔和纯色字母头像。
* **按需编辑（v0.1.8）**：资料页默认**不显示编辑卡片**；点击右上角 **Edit profile**（与 **+ New profile**
  并排）展开。卡片内 **Save** 提交简介并关闭；**Cancel** 或 **×** 关闭且不应用简介草稿；头像选择/移除
  即时生效。切换资料会收起旧编辑器。QA：`ECHOPORT_PROFILE_EDIT=1` 启动即展开编辑器。
* **收听历史（v0.1.8）**：卡片显示最近 **12 首**（原 8 首），整页滚动、无内部视口裁剪。
* **资料导出 / 导入**：资料页支持导出可移植 JSON（头像 data URL、简介、歌单、收藏、设置）与导入，
  可用于备份或迁移到另一台电脑（不含音频文件）。
* 资料数据保存在 `$XDG_DATA_HOME/echoport/profiles/<名称>.json`（默认
  `~/.local/share/echoport/profiles/`）；可手动备份或拷贝到另一台电脑。

### 9. 主题、Lua 配置与插件

* **设置 → 外观** 选择三种主题之一（详细脚本/配置选项见 HTML 手册
  [config.html](config.html) 与 [plugins.html](plugins.html)，模板见
  `examples/config.zh-CN.lua` / `examples/config.en.lua`；`config.lua` 支持 `ui_scale` 与 `cache_limit_mb`）：
  * **Pulse**（默认）：瓷白画布 + 亮蓝主色 + 荧光黄绿强调；
  * **Tide**：浅青画布 + 蓝绿主色 + 深蓝文字；
  * **Ink**：低饱和深灰夜间配色。
* 打开 **跟随封面强调色**（默认开启）后，强调色 / 高亮色会取自当前歌曲封面，
  背景与正文仍保持主题配色，兼顾氛围与可读性。
* 主题切换有轻量过渡动画；界面空闲时不持续重绘，省电省 CPU。
* **界面缩放**：在「设置 → 外观 → 界面缩放」选择 100% / 110% / 125% / 150%（适合高分屏或远距离观看），
  选择随资料保存；缩放后各页面（含最大化和 980×620 小窗口）仍保持完整边界、无控件裁切。
* **Lua 配置**：`<配置目录>/echoport/config.lua` 可自定义启动选项、快捷键、路径等；
  **插件**：`<配置目录>/echoport/plugins/*.lua` 按文件名顺序加载。
  Linux 默认在 `~/.config/echoport/`，Windows 在 `%APPDATA%\echoport\`。
  详见 [脚本说明](SCRIPTING.md) 与 [插件接口](PLUGINS.md)。

### 10. 快捷键

| 按键 | 功能 |
|---|---|
| `Space` | 播放 / 暂停 |
| `←` / `→` | 后退 / 前进 5 秒 |
| `N` | 下一首 |
| `B` | 上一首 |
| `Esc` | 关闭弹窗 / 浮层（当前页面无浮层时无效果） |
| `Enter` | 搜索框中提交搜索 |

> 输入框获得焦点时，播放快捷键不会触发，避免打字误操作。
> 快捷键可以在 `config.lua` 的 `keyboard = { ... }` 中自定义或覆盖，见 [脚本说明](SCRIPTING.md)。

### 11. 数据位置与清理

| 内容 | 位置 |
|---|---|
| 资料、歌单、收藏、设置 | `$XDG_DATA_HOME/echoport/profiles/*.json`（默认 `~/.local/share/echoport`） |
| 封面缓存 | `$XDG_CACHE_HOME/echoport/covers`（默认 `~/.cache/echoport/covers`） |
| 音频缓存 | `$XDG_CACHE_HOME/echoport/audio`（默认 `~/.cache/echoport/audio`） |
| Lua 配置 / 插件 | `$XDG_CONFIG_HOME/echoport`（默认 `~/.config/echoport`）：`config.lua`、`plugins/*.lua` |
| 下载的音乐 | 设置中的下载目录（默认 `~/Music/EchoPort`） |

设置页可查看缓存大小并一键清除缓存；删除某个资料文件即删除该资料（请先关闭应用）。
在线播放的音频**只缓存在磁盘**（`…/echoport/audio`，不常驻内存），只预取下一首；
**磁盘缓存上限** 可在设置 → 存储选择 **无缓存 / 512 MB / 1 GB（默认）/ 2 GB**，改动立即生效：
启动时按上限最旧优先清理，最近 120 秒内使用的文件受保护（受保护/临时文件仍计入总量）。
**无缓存模式**会在切歌时删除上一首、启动时清空旧缓存，但**下载目录中的音乐永远不会被自动删除**。
封面纹理内存中最多 96 张、每张 ≤320 px。详见 [README 的「缓存策略」](../README.md#缓存策略磁盘占用与隐私)。

### 12. 常见问题

* **在线歌曲不能播放** → 检查 yt-dlp 是否安装/更新（`yt-dlp -U`），以及网络/地区限制；
  试试候选结果里的其他来源。
* **没有声音** → 设置里选择音频设备；系统音量与播放器音量都要检查。
* **中文显示为方框** → 安装中文字体（Linux：`noto-fonts-cjk` / `fonts-noto-cjk`；Windows/macOS 自带），
  或用 `ECHOPORT_CJK_FONT=/path/font.ttc` 指定字体后重启。
* **下载失败** → 确认下载目录可写、磁盘空间足够，并更新 yt-dlp；错误信息会显示在下载条目上。
* **缓存会占多少空间？会不会上传？** → 流媒体缓存只存在本机磁盘，上限可在设置里选 **无缓存 / 512MB / 1GB（默认）/ 2GB**，
  自动清理最旧文件；不占常驻内存、不会上传。封面纹理内存上限 96 张（≤320 px）。设置页可查看并一键清除，
  清除不会删除你的下载文件。详见 [README「缓存策略」](../README.md#缓存策略磁盘占用与隐私)。
* **Bilibili / SoundCloud 报错 412 / 567 或播放失败** → 应用会先自动尝试 **YouTube 公开音频匹配**；只有全部来源都失败时才会显示双语提示，建议换个来源或稍后重试。
* **Spotify 搜不到歌** → 这是有意设计：Spotify 无匿名搜索，请粘贴歌曲链接导入。

---

### 13. 资源占用与最低配置

> v0.1.3 发布二进制实测（v0.1.4 复核数值范围一致；本机：Arch Linux / 32 核 KVM / 无 GPU，Mesa llvmpipe 软渲染 / 无声卡）。
> 详细数据与测量方法见 [QA_REPORT](QA_REPORT.md) 和 [`docs/QA_REPORT.md` §6](../docs/QA_REPORT.md)。

| 项目 | 实测 |
|---|---|
| 二进制 / tar.gz | 15,364,248 B（≈14.7 MiB，LTO fat）/ 7,063,887 B（≈6.74 MiB，含文档/示例/Arch 安装脚本） |
| 启动：窗口出现 / 首帧 | 163–183 ms（中位 177 ms）/ 425–439 ms |
| 空闲 CPU / RSS / PSS | 0.000–0.125 % / ≈254.7 MiB / ≈127.9 MiB |
| 空闲线程 / FD | 68 / 10 |
| 播放 1 首流媒体（含封面） | RSS ≈288.8 MiB（PSS ≈146–159；线程 73；FD 39） |
| Demo 动画 + 模拟播放 | CPU ≈156 %（软渲染）/ RSS ≈274 MiB |
| 缓存增长（1 GiB 上限，5 首） | → 20.6 MiB / 5 文件；下载目录不变 |
| 无缓存模式 | 启动清旧缓存、切歌删上一首；下载目录不动 |
| 资料 JSON / config.lua | 496 B / 9,296 B |

**最低配置建议**：x86_64 + SSE4.2、双核 1.5 GHz+（软渲染建议 4 核）；**512 MB–1 GB 可用内存**；
**OpenGL 3.3+**（无 GPU 可用 llvmpipe 软渲染，较慢）；程序约 15 MiB + 缓存 0–2 GiB 可调 + 可选
yt-dlp/ffmpeg；Linux x86_64（**预编译包 glibc ≥ 2.44**，老发行版请源码重建）、Windows 10+/macOS 11+ 为构建目标；
需要音频输出设备（无声卡时播放会报错）与联网（在线音源）。

---

## English

EchoPort is a local-first, multi-source desktop music player: a local library,
online search, mixed-platform playlists, downloads and song background, all in
one quiet interface. **No account, and nothing is uploaded.**

### 1. First run

1. Install and start EchoPort (see the install section in the [README](../README.md)).
2. On first launch a local profile "Me" is created; the library starts empty.
3. Open **Settings** and check **Media tools**:
   * `yt-dlp` — needed for online playback, search and downloads (shows
     "Not found" with a **Detect again** button when missing).
   * `ffmpeg` — needed for format conversion and some sources.
4. Pick a language and theme (Settings → Appearance).

### 1.1 Update check, startup animation & Arch install (v0.1.9)

* **Update check**: anonymous GitHub Releases lookup at startup and every 6 hours (default
  `nilotoo/EchoPort`, override with `ECHOPORT_UPDATE_REPO=owner/name`; falls back to `VERSION` when no
  Release exists). Version comparison only — no telemetry, no token, no auto-download/install.
  About → **Check for updates** runs it manually; an available update shows a row whose Update button
  opens the release page.
* **Startup animation**: ~0.9 s (up to 1.4 s while a check is in flight); an update found during the
  splash keeps it open with **Stay on this version / Update**. `ECHOPORT_SPLASH=1` forces it,
  `ECHOPORT_NO_SPLASH=1` disables it, `ECHOPORT_NO_UPDATE_CHECK=1` disables checking, and
  `ECHOPORT_FAKE_UPDATE=9.9.9` fakes an update for tests.
* **Arch users**: run `./scripts/install-arch.sh` (no sudo) from the source tree, or `./install-arch.sh`
  from the extracted tarball; it installs the binary, desktop entry and hicolor 32/48/128/256 icons.
* The repo URL is still a placeholder pending confirmation, so a real-network update check is untested.

### 2. Add local music

1. Go to **Library** → **Add music folder** and pick a folder (repeat for more).
2. Press **Scan library**; EchoPort walks it recursively for mp3 / m4a / flac /
   wav / ogg files and reads title, artist, album, duration and embedded cover.
3. Play any row. Local files need no network and work offline.

### 3. Search online music

1. Open **Search**, type a song/artist/album and press Enter.
2. Results are ranked into sections:
   * **Best matches**: the top 3 tracks across platforms;
   * **Producers & artists**: up to 2 relevant artists per platform; **single-click a card to open its page**;
   * **per-platform lists**: relevance-ordered, collapsed to 5 tracks with **Show more / Show less**;
     **Load more** appends 12 tracks per click (capped at 60).
3. Use the platform chips to filter; each row can play, queue, add to playlist,
   download, favourite or open its source page.
4. Tracks and artists are fetched in parallel (~2.5 s total); the UI stays responsive while loading.
5. **Play-spam protection**: clicking play/the triangle again while a track is loading is ignored (one resolve
   per track); pause/resume during loading does nothing extra.

### 3.1 Artist / producer pages

* Single-click a producer card on the search page (or a platform source card) to open the artist page: avatar,
  platform badge, source-page link, **Wikipedia bio**, **Popular tracks** and **Your listening history**
  (locally played tracks from that artist).
* Loading / error / no-bio states have clear hints; "Back to search" returns to the results.
* Popular tracks use the platform source (Netease hot songs / Apple `artistId` / YouTube·SoundCloud·Bilibili
  matching) under the same DRM and source rules.

### 3.2 Show more vs Load more

* "Show more" only reveals results **already fetched** (no new request); "Load more" asks that platform for
  12 more tracks (inline loading dots; hidden at the 60-track cap).

### 4. Import links (Spotify / Apple Music, …)

* Paste a **Spotify** or **Apple Music** track link into the search box and press
  Enter to import it.
* **Apple Music is searchable just like Netease** (shown as "Searchable" in the UI); Spotify has no
  anonymous search, so paste a track link to import.
* Neither service can be played directly by EchoPort (DRM protected). After import you get:
  * public metadata and cover art,
  * **full-length** public audio matched via yt-dlp (usually YouTube; it may match a
    live/alternate version, which is an intended limitation) — the player bar briefly
    shows "Loading audio…" while resolving; DRM is never bypassed.
* Links from YouTube / SoundCloud / Bilibili / Netease import and play the same way.

### 5. Playback, queue and background

* The player bar is always visible: play/pause, previous/next, scrubbing,
  volume, shuffle/repeat and the queue; the left divider frames the transport, the scrubber is centred on
  the content column together with the play button, and all transport buttons share one vertical centre line.
* **Play-spam protection**: repeat play clicks while a track is loading are ignored (one resolve); pause/resume
  during loading is a no-op; row triangles stay single-click play/toggle.
* The **source-page button** opens a popup with **Copy link / Open original page**; shuffle has its own
  crossed-arrow icon next to repeat.
* **Double-click UX**: single click selects/highlights rows, cards, playlists or finished downloads;
  double-click plays the song / opens the playlist; hover tooltips say "Double-click to play/open".
* **Now playing** shows a large cover and two tabs: **[Track | Background]**. The page keeps the
  original minimal design: a plain theme background with no cover tint, glow, blurred backdrop,
  floating decoration or continuous animation. Under the cover the macaron palette dots are shown
  together with a **Released date** (from Apple/Netease/Spotify/yt-dlp imports or local tags; an em
  dash when the source has none).
* The **Background** tab only shows content strictly about this **artist + album + song**: artist bio,
  album background and song background sections (empty when nothing relevant is found — no unrelated
  noise), plus interviews and further reading links — all public links.
* The queue panel lets you inspect, remove and clear upcoming tracks; **Play
  next** inserts a track.

### 5.1 Mini player & background performance mode (v0.1.7)

* Click the **EchoPort logo** at the top of the sidebar to toggle the **mini player** — a 420×142
  always-on-top window with cover (click = restore main), title/artist, progress + times,
  previous/play-pause/next, open-main and close. Playback continues while the main window is hidden.
* **Background performance mode**: when the main window is unfocused/minimized the UI repaint cadence
  is throttled automatically (playing 66 ms → 1000 ms, busy 90 ms → 1000 ms, layout 50 ms → off);
  animations freeze but audio keeps playing; foreground cadence returns when focused again.
* QA: `ECHOPORT_MINI=1` auto-opens it; `scripts/qa-mini-player.sh` captures the window on X11.

### 6. Playlists (mixed platforms)

1. **Playlists** → **New playlist**, give it a name.
2. Use a row menu → **Add to playlist**; local files and songs from every
   platform can live in the same playlist.
3. Play all / shuffle all, rename, delete and remove tracks.
4. A playlist only stores track ids + metadata, entirely in local JSON.
5. **Offline sharing**: the Playlists toolbar can **Import playlist**, and a playlist's detail page can
   **Export playlist** — the file is JSON **metadata** (track ids, title, artist, album, source-page
   links, **no audio**); the other side resolves playback from its own sources.

### 7. Downloads

1. Use a row menu or **Now playing** → **Download**; jobs appear on the
   **Downloads** page.
2. Choose the audio format (MP3 / M4A / Opus / FLAC) and folder in Settings
   (default `~/Music/EchoPort`).
3. Downloads are performed by yt-dlp; network and a working yt-dlp are required.
   **Open folder** reveals finished files.

### 8. Profiles, stats and history

* **Profile** manages multiple local profiles: create, switch, delete. Every
  profile has its own playlists, favourites, history and settings.
* **Avatar & bio**: choose a 256 px PNG picture (embedded in the profile JSON) or remove it; without
  one a deterministic pastel letter avatar is shown.
* **On-demand editor (v0.1.8)**: the profile page hides the edit card by default; click **Edit profile**
  (top right, next to **+ New profile**) to open it. **Save** commits the bio and closes; **Cancel** or
  **×** closes without applying the bio draft; avatar choose/remove applies immediately. Switching
  profiles closes a stale editor. QA: `ECHOPORT_PROFILE_EDIT=1` opens the editor at startup.
* **Listening history (v0.1.8)**: the card shows the latest **12 tracks** (was 8) with page scrolling
  instead of inner-viewport clipping.
* **Profile export/import**: export a portable JSON profile (avatar data URL, bio, playlists,
  favourites, settings) and import it back — no audio files included.
* Profiles are stored at `$XDG_DATA_HOME/echoport/profiles/<name>.json`
  (default `~/.local/share/echoport/profiles/`) — easy to back up or copy.

### 9. Themes, Lua config & plugins

* **Settings → Appearance** offers three themes:
  * **Pulse** (default) — porcelain-white canvas, vivid blue primary, fluorescent-lime highlight;
  * **Tide** — light aqua canvas, teal primary, deep-navy text;
  * **Ink** — low-saturation dark slate.
* **Follow cover accent** (on by default) tints the accent / highlight colours from
  the current cover while background and body text keep the theme palette.
* Theme changes animate briefly; the UI does not repaint continuously while idle.
* Script/config options are documented in the bilingual HTML manuals [config.html](config.html) and
  [plugins.html](plugins.html); templates: `examples/config.zh-CN.lua` / `config.en.lua`;
  `config.lua` supports `ui_scale` and `cache_limit_mb`.
* **Interface scale**: Settings → Appearance → Interface scale offers 100% / 110% / 125% / 150%
  (for HiDPI or distant viewing), persisted per profile. All pages keep their boundaries with no
  clipped controls after scaling, including maximized and 980×620 windows.
* **Lua config**: `<config dir>/echoport/config.lua` customises startup options,
  shortcuts and paths; **plugins**: `<config dir>/echoport/plugins/*.lua` load in
  file-name order. On Linux the default is `~/.config/echoport/`, on Windows
  `%APPDATA%\echoport\`. See [Scripting](SCRIPTING.md) and [Plugins](PLUGINS.md).

### 10. Keyboard shortcuts

| Key | Action |
|---|---|
| `Space` | Play / pause |
| `←` / `→` | Seek −5 s / +5 s |
| `N` | Next track |
| `B` | Previous track |
| `Esc` | Close pop-ups / overlays (no-op when none are open) |
| `Enter` | Submit a search from the search field |

> Playback shortcuts are ignored while a text field has keyboard focus.
> Shortcuts can be customised/overridden in `config.lua` under `keyboard = { ... }`
> (see [Scripting](SCRIPTING.md)).

### 11. Data locations & cleanup

| Content | Location |
|---|---|
| Profiles, playlists, favourites, settings | `$XDG_DATA_HOME/echoport/profiles/*.json` (default `~/.local/share/echoport`) |
| Cover cache | `$XDG_CACHE_HOME/echoport/covers` (default `~/.cache/echoport/covers`) |
| Audio cache | `$XDG_CACHE_HOME/echoport/audio` (default `~/.cache/echoport/audio`) |
| Lua config / plugins | `$XDG_CONFIG_HOME/echoport` (default `~/.config/echoport`): `config.lua`, `plugins/*.lua` |
| Downloaded music | the download folder in Settings (default `~/Music/EchoPort`) |

Settings shows the cache size and can clear it. Deleting a profile file removes
that profile (close the app first). Streamed audio is cached **on disk only**
(`…/echoport/audio`, never kept in RAM); only the next track is prefetched; in-memory cover textures
are capped at 96 × ≤320 px.
**Disk cache limit** (Settings → Storage) offers **No cache / 512 MB / 1 GB (default) / 2 GB** and applies
immediately: startup prunes oldest-mtime files down to the cap, files touched in the last 120 s are
protected (protected/temporary files still count toward the total).
**No-cache mode** deletes the previous track when switching and purges old cache at startup, but music in
the download folder is user-owned and **never auto-deleted** —
see the [README cache policy](../README.md#cache-policy-disk-use--privacy).

### 12. Troubleshooting

* **Online tracks will not play** → check that yt-dlp is installed and current
  (`yt-dlp -U`), and for region blocks; try another source/candidate.
* **No sound** → pick an audio device in Settings; check both system and player volume.
* **Chinese text renders as boxes** → install a CJK font (Linux:
  `noto-fonts-cjk` / `fonts-noto-cjk`; Windows/macOS ship one), or restart with
  `ECHOPORT_CJK_FONT=/path/font.ttc`.
* **Download failed** → make sure the download folder is writable and has space,
  and update yt-dlp; the error is shown on the job row.
* **How much disk does the cache use, and is it uploaded?** → Streamed audio is cached
  on local disk only; the cap is selectable (No cache / 512MB / 1GB default / 2GB) with oldest-first
  pruning; never kept in RAM and never uploaded. In-memory cover textures are capped
  at 96 × ≤320 px. Settings shows the size and can clear it without touching your
  downloads — see the [README cache policy](../README.md#cache-policy-disk-use--privacy).
* **Bilibili / SoundCloud says HTTP 412 / 567 or playback fails** → the app first retries with a
  **YouTube public-audio match**; the bilingual hint only appears when every source fails — then try
  another source or retry later.
* **Spotify search finds nothing** → by design: Spotify has no anonymous search;
  paste a Spotify track link instead.

---

### 13. Resource usage & minimum configuration

> Measured on the v0.1.3 release binary, re-checked on v0.1.4 (same range; this host: Arch Linux / 32-core KVM / no GPU — Mesa llvmpipe
> software rendering / no sound card). Full data and method: [QA_REPORT](QA_REPORT.md) and
> [`docs/QA_REPORT.md` §6](../docs/QA_REPORT.md).

| Item | Measured |
|---|---|
| Binary / tarball | 15,364,248 B (≈14.7 MiB, LTO fat) / 7,063,887 B (≈6.74 MiB, docs/examples/Arch installer) |
| Startup: window / first frame | 163–183 ms (median 177 ms) / 425–439 ms |
| Idle CPU / RSS / PSS | 0.000–0.125 % / ≈254.7 MiB / ≈127.9 MiB |
| Idle threads / FDs | 68 / 10 |
| Playing one streamed track (incl. cover) | RSS ≈288.8 MiB (PSS ≈146–159; threads 73; FDs 39) |
| Demo animation + simulated playback | CPU ≈156 % (software GL) / RSS ≈274 MiB |
| Cache growth (1 GiB cap, 5 tracks) | → 20.6 MiB / 5 files; download folder unchanged |
| No-cache mode | old cache purged at startup, previous track removed on switch; downloads untouched |
| Profile JSON / config.lua | 496 B / 9,296 B |

**Minimum estimate**: x86_64 + SSE4.2, dual-core 1.5 GHz+ (4+ cores with software GL); **512 MB–1 GB free
RAM**; **OpenGL 3.3+** (llvmpipe software fallback works but is slower); ~15 MiB app + 0–2 GiB configurable
cache + optional yt-dlp/ffmpeg; Linux x86_64 (**prebuilt tarball glibc ≥ 2.44**; older distros rebuild from
source), Windows 10+/macOS 11+ are build targets; an audio output device (playback errors without one) and
network for online sources.


### 14. Windows 10 / 11 说明

* 运行要求：Windows 10（1903+）/ Windows 11 x86_64，**OpenGL 3.3+ 显卡驱动**，音频输出设备（WASAPI）；
  无需 WebView2/.NET/安装器。
* v0.1.6 起：GUI 子系统（无控制台窗口）、exe 内嵌多尺寸图标（`.rsrc`）、外部工具静默启动；
  提供 `dist/echoport-0.1.9-windows-x86_64.zip`（exe + README-WINDOWS + ico + 文档/示例）。
  **交叉构建、未签名、未在 Windows 实机运行**。
* 解压 zip 后双击 `echoport.exe`；未签名 exe 会被 SmartScreen 提示，选择「更多信息 → 仍要运行」。
* 在线播放/下载需要 `yt-dlp.exe` 与 `ffmpeg.exe`（放到 `PATH` 或 exe 旁边，或在设置中指定路径）。
* 数据：`%APPDATA%\echoport`（资料/config.lua）、`%LOCALAPPDATA%\echoport\cache`（封面/音频缓存）、
  `%USERPROFILE%\Music\EchoPort`（下载，可在设置修改）。
* 界面缩放 / DPI：winit/egui 处理系统 DPI；设置 → 外观 可选 100/110/125/150%。
* 构建：Windows 上原生 MSVC 构建（推荐，`scripts/build-windows.ps1`），或 Linux 上
  `scripts/package-windows-cross.sh` 交叉构建（CI/验证用，未在 Windows 实机验证）。

---

更多细节 / More details: [PROVIDERS.md](PROVIDERS.md) · [ARCHITECTURE.md](ARCHITECTURE.md) · [QA_REPORT.md](QA_REPORT.md) [PROVIDERS.md](PROVIDERS.md) · [ARCHITECTURE.md](ARCHITECTURE.md) · [QA_REPORT.md](QA_REPORT.md)

### 14. Windows 10 / 11 notes

* Requirements: Windows 10 (1903+) / Windows 11 x86_64, an **OpenGL 3.3+ driver**, and an audio
  output device (WASAPI); no WebView2/.NET/installer needed.
* Since v0.1.6: GUI subsystem (no console window), embedded multi-size exe icon (`.rsrc`), quiet child
  tools; `dist/echoport-0.1.9-windows-x86_64.zip` ships exe + README-WINDOWS + ico + docs/examples.
  **Cross-built, unsigned, not executed on Windows.**
* Unzip and double-click `echoport.exe`; for the unsigned binary SmartScreen needs
  "More info → Run anyway".
* Online playback/downloads need `yt-dlp.exe` and `ffmpeg.exe` on `PATH` or next to the exe (or set
  explicit paths in Settings / `ECHOPORT_YT_DLP` / `ECHOPORT_FFMPEG`).
* Data: `%APPDATA%\echoport` (profiles + config.lua), `%LOCALAPPDATA%\echoport\cache`
  (covers/audio), `%USERPROFILE%\Music\EchoPort` (downloads, configurable).
* DPI is handled by winit/egui; the in-app UI scale (100/110/125/150 %) is in Settings → Appearance.
* Build natively on Windows with MSVC (`scripts/build-windows.ps1`, recommended) or cross-build on
  Linux (`scripts/package-windows-cross.sh`, CI/verification only — not run on real Windows here).
