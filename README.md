# EchoPort · 回声港

**一个安静、轻量的多音源桌面音乐播放器。** 本地曲库、在线搜索、跨平台歌单、下载与歌曲背景，
都停靠在同一个港口里。**无需账号，不上传任何数据。**

**A quiet, lightweight, local-first desktop music player** for your local files and music from
many web sources — with mixed-platform playlists, downloads and song background.
**No account. No cloud. No telemetry.**

`Rust 1.85+` · `egui 0.32` · `Linux / Windows / macOS` · `GPL-3.0` · `v0.2.3`
· 中文 | [English](#english)

---

## 📸 截图

全部截图使用隔离的临时 XDG 目录与中性 Demo 资料（Me），不含个人资料、历史或本机路径。

| Home | Search |
|---|---|
| ![Home](screenshots/qa-home.png) | ![Search](screenshots/qa-search.png) |

| Library | Now playing |
|---|---|
| ![Library](screenshots/qa-library.png) | ![Now playing](screenshots/qa-now_playing.png) |

| Playlists | Downloads |
|---|---|
| ![Playlists](screenshots/qa-playlists.png) | ![Downloads](screenshots/qa-downloads.png) |

| Settings | Profile（按需编辑：收起态） |
|---|---|
| ![Settings](screenshots/qa-settings.png) | ![Profile collapsed](screenshots/qa18-profile-collapsed.png) |

| Profile（编辑器展开：Save / Cancel / ×） | Mini player（迷你播放器） |
|---|---|
| ![Profile editing](screenshots/qa18-profile-editing.png) | ![Mini player](screenshots/qa17-mini-player.png) |

| Startup splash（启动动画） | Splash update prompt（更新提示） |
|---|---|
| ![Splash](screenshots/qa19-splash.png) | ![Splash update](screenshots/qa19-splash-update.png) |

| Embedded Plugin docs / Config reference（内嵌文档按钮） | Settings（设置） |
|---|---|
| ![Docs buttons](screenshots/qa20-settings-docs-buttons.png) | ![Settings](screenshots/qa-settings.png) |

| New ringless icon（无外圈图标） | Maximized home（最大化） |
|---|---|
| ![Icon preview](screenshots/qa19-icon-preview.png) | ![Home maximized](screenshots/qa-home-maximized.png) |

| Tide theme · Home | Ink theme · Home |
|---|---|
| ![Tide home](screenshots/qa-tide-home.png) | ![Ink home](screenshots/qa-ink-home.png) |

| 125% UI scale · Home |  |
|---|---|
| ![125% scale](screenshots/qa-ui-scale-125-home.png) | |

## ✨ 功能

* **多音源播放器** — YouTube、SoundCloud、Bilibili、网易云、Apple Music 目录 + 本地文件。
  Apple Music 与网易云一样在界面中显示为「可搜索」；Spotify / Apple Music 链接导入后，
  **完整播放优先通过 yt-dlp 匹配公开音频**（Apple/Spotify 自身流媒体受 DRM 保护，不绕过）。
* **极简正在播放页（初版设计）** — 纯主题背景：没有封面取色、光晕、模糊大图、漂浮元素或持续动画；
  大封面 + 歌曲信息 + 播放控制，封面下保留 macaron 调色板小点并显示 **Released 发行日期**（若来源提供）。
* **界面缩放** — 设置 → 外观里可选 **100% / 110% / 125% / 150%**，适配高分屏与远距离观看；选择会随资料持久化。
* **跨平台歌单** — 同一个歌单可以混排本地文件与各平台歌曲；播放全部 / 随机播放全部。
* **歌单离线分享** — 歌单可导出/导入为 JSON **元数据**（曲目 ID、标题、艺术家、专辑、来源页面链接等，**不含音频**）；
  对方导入后仍按自己的音源解析播放。
* **双击交互** — 单击选中/高亮列表行、卡片或歌单；双击播放歌曲 / 打开歌单（播放按钮、播放控制等仍为单击），
  悬停会显示「双击播放 / 双击打开」提示。
* **来源页面与随机播放** — 播放控制区新增「来源页面」按钮（弹窗可 **复制链接** 或 **打开原始页面**）；
  随机播放使用独立的双箭头图标，位于循环按钮旁。
* **下载** — 通过外部 yt-dlp 下载（MP3 / M4A / Opus / FLAC），带进度、完成提示与「打开目录」。
* **浏览与搜索** — 一次搜索全部已选平台；结果按相关度分为 **最佳匹配 / 制作人·歌手 / 各平台列表**，
  支持展开收起与 **加载更多**，点制作人卡片进入艺术家主页（简介/热门/我的历史）；支持平台筛选、粘贴链接导入。
* **防连点播放** — 歌曲加载期间重复点播放会被忽略（同一首只解析一次），不产生重复请求。
* **歌曲背景（严格相关）** — 正在播放页「背景」标签只显示与当前 **歌手 + 专辑 + 歌曲** 真正相关的内容：
  歌手简介、专辑背景、歌曲背景三段（查不到就留空，不塞无关内容），以及相关采访/解说与来源页面链接。
* **三种主题 + 跟随封面强调色** — 默认 **Pulse**（瓷白 + 亮蓝 + 荧光黄绿）、**Tide**（浅青 + 蓝绿 + 深蓝）、
  **Ink**（低饱和深灰）；可选让强调色跟随当前封面。
* **Lua 配置与插件** — `config.lua` 自定义启动选项与快捷键，`plugins/*.lua` 扩展功能
  （见 [脚本说明](docs/SCRIPTING.md) / [插件接口](docs/PLUGINS.md)）。
* **本地资料（Profiles）** — 多个本机资料互相隔离；支持头像（图片或柔和字母占位）、简介、
  **资料导出/导入**（可移植 JSON，含头像/歌单/设置）；歌单 / 收藏 / 历史 / 设置全部保存在本机 JSON。
* **中文 / English** — 界面与文档双语，可随时切换；CJK 字体自动寻找系统字体。
* **轻量** — 原生 Rust + egui（OpenGL），无内置浏览器、无 Electron、空闲时几乎不占用 CPU。

---

## 🌐 平台支持

图例：✅ 可用 · ⚠️ 部分可用 / 尽力而为 · ❌ 有意不支持

| 平台 | 搜索 | 播放 | 封面 / 元数据 | 粘贴链接导入 | 说明 |
|---|---|---|---|---|---|
| **本地文件** | 文件夹扫描 | ✅ 原生解码 | ✅ 内嵌标签/封面 | — | mp3 / m4a / aac / alac / flac / wav / ogg，可离线 |
| **YouTube** | ✅ | ✅ 完整（经 yt-dlp） | ✅ | ✅ | 通用音频后端；请保持 yt-dlp 最新 |
| **SoundCloud** | ✅ | ⚠️ 部分曲目受 DRM/HLS 限制；失败时回退 YouTube 公开匹配 | ✅ | ✅ | 原生来源均失败后才提示 |
| **Bilibili** | ⚠️ | ⚠️ 仅音频，可能地区受限；失败时回退 YouTube 公开匹配 | ✅ | ✅ | 412/567 先自动尝试 YouTube 匹配；全部失败才提示更换来源/重试 |
| **网易云音乐 Netease** | ✅ 公开 API | ⚠️ 经 yt-dlp，128 kbps mp3 | ✅ | ✅ | 地区/版权变化，部分司法辖区合法性存疑 |
| **Apple Music** | ✅ 公开 iTunes Search API（界面显示「可搜索」，与网易云一致） | ✅ 完整（经 yt-dlp 匹配公开音频） | ✅ | ✅ | Apple 自身音频受 FairPlay DRM 保护，不绕过 |
| **Spotify** | ❌ 无匿名搜索 | ✅ 完整（经 yt-dlp 匹配公开音频；30 秒预览后备） | ✅（来自链接） | ✅ 仅链接 | 官方接口需要令牌且为 Widevine DRM，不绕过 |
| **YouTube Music** | 经普通 YouTube | 经 YouTube | ✅ | ✅ | 使用普通 YouTube 链接即可 |

> 以上是**如实**的支持范围：EchoPort 不做 DRM 破解、不保存账号令牌、不内置音乐内容。
> 各平台接口与地区限制随时可能变化，失败时界面会给出提示而不是崩溃。
> 详细原因与排错见 [docs/PROVIDERS.md](docs/PROVIDERS.md)。

---

## 📦 安装

### 依赖

| 依赖 | 是否必需 | 说明 |
|---|---|---|
| **Rust 1.85+** | 构建必需 | 推荐用 [rustup](https://rustup.rs) 安装 stable 工具链 |
| **C 编译器** | 构建必需 | Lua 运行时（mlua）内置编译，需要 `cc`；无需系统 Lua、无需 cmake |
| **yt-dlp** | 在线功能必需 | 在线搜索 / 播放 / 下载都由它完成；建议保持最新（`yt-dlp -U`）。本项目验证版本 2026.10.21 |
| **ffmpeg** | 部分功能必需 | 格式转换、HLS/部分音源、下载转码。验证版本 9.0.2 |
| **中文字体** | 可选 | 缺少 CJK 字体时中文会显示方框；Linux 安装 `noto-fonts-cjk` 即可 |
| **图形与音频运行时** | 必需 | Wayland 或 X11 + OpenGL（Mesa 软件渲染亦可）；PipeWire / PulseAudio / ALSA 任选其一 |

yt-dlp 与 ffmpeg 是**外部工具**，EchoPort 不打包它们（它们各自遵循自己的许可证与条款）。
EchoPort 会自动在 `PATH`、`~/.local/bin`、`/usr/local/bin`、`/opt/homebrew/bin` 等位置寻找，
也可以在「设置 → 媒体工具」中手动指定路径。

### Linux：从源码构建

```bash
# 以 Arch Linux 为例（其他发行版把包名换成对应的 -dev 包）
sudo pacman -S --needed base-devel pkgconf rustup mesa libxkbcommon wayland libxcb noto-fonts-cjk
rustup default stable
# 可选，但强烈建议：
sudo pacman -S --needed yt-dlp ffmpeg

git clone <仓库地址> EchoPort   # 或直接进入已有的仓库目录
cd EchoPort
cargo build --release           # 也可以运行 ./scripts/build.sh
./target/release/echoport
```

Debian/Ubuntu 的构建依赖大致为：`build-essential pkg-config libgl1-mesa-dev libxkbcommon-dev`
（Wayland/X11 与音频运行库按桌面环境选择）。包名因发行版而异，以发行版文档为准。
C 编译器用于编译内置 Lua 5.4（mlua vendored），不需要安装系统 Lua 或 cmake。

### Linux：预编译包（tar.gz）

`dist/echoport-<版本>-linux-x86_64.tar.gz`（由 `scripts/package-linux.sh` 生成，可随 Release 分发）
内含：`echoport` 二进制、`echoport.desktop`、`echoport.png`、`README.md`、`INSTALL.txt`。安装：

```bash
tar -xzf echoport-0.2.3-linux-x86_64.tar.gz
cd echoport-0.2.3-linux-x86_64
mkdir -p ~/.local/bin ~/.local/share/applications ~/.local/share/icons/hicolor/256x256/apps
install -m755 echoport        ~/.local/bin/echoport
install -m644 echoport.desktop ~/.local/share/applications/echoport.desktop
install -m644 echoport.png     ~/.local/share/icons/hicolor/256x256/apps/echoport.png
echoport   # 或从应用菜单启动 EchoPort
```

* **Arch 用户**：源码目录运行 `./scripts/install-arch.sh`（无 sudo，自动安装到 `~/.local`）；
  发布包提供 pacman 包：`sudo pacman -U echoport-0.2.3-1-x86_64.pkg.tar.zst`（AUR PKGBUILD 见
  `packaging/arch/`，依赖 glibc/gcc-libs，可选 yt-dlp/ffmpeg）。

* **发布资产 / release assets**：`echoport-0.2.3-linux-x86_64.tar.gz`（绿色版）、
  `echoport-0.2.3-1-x86_64.pkg.tar.zst`（Arch pacman）、`echoport-0.2.3-windows-x86_64.zip`（绿色版）、
  `echoport-0.2.3-windows-x86_64.msi`（per-user 安装）、macOS `EchoPort.app.zip`（CI 构建），
  每个文件附 `.sha256`。

> 发布包**不含** yt-dlp / ffmpeg，请按上面的依赖说明另外安装。当前项目处于 0.2.3
> 开发阶段，尚未提供官方托管的二进制下载；请按源码构建或使用脚本自行打包。

### Windows 10 / 11（x86_64）

**运行要求**

* Windows 10（1903+）或 Windows 11，x86_64；需要有 **OpenGL 3.3+ 的显卡驱动**
  （当前 AMD/Intel/NVIDIA 驱动都提供；Microsoft 基本显示适配器 / 部分 RDP 会话只有 OpenGL 1.1，
  窗口无法创建）；需要一个音频输出设备（WASAPI）。
* 无需 WebView2、.NET 或安装器：解压即用（未签名 exe）。

**原生构建（推荐）**

```powershell
rustup default stable-msvc    # Rust 1.85+；需安装 VS Build Tools「使用 C++ 的桌面开发」
cd EchoPort
cargo build --release         # target\release\echoport.exe
powershell -ExecutionPolicy Bypass -File scripts\build-windows.ps1
```

产物：`dist\echoport-<版本>-windows-x86_64.zip`（`echoport.exe`、`README-WINDOWS.txt`、
`echoport.ico`、文档与 Lua 示例）。

**Linux 交叉构建（CI / 验证用，非官方发行路径）**

```bash
rustup target add x86_64-pc-windows-gnu
# Arch: sudo pacman -S mingw-w64-gcc    Ubuntu: gcc-mingw-w64-x86-64-posix binutils-mingw-w64-x86-64
./scripts/package-windows-cross.sh       # dist/echoport-<版本>-windows-x86_64.zip
```

交叉构建产物是未签名 PE，**未在 Windows 真机运行验证**；正式发布请在 Windows 上原生构建。

**v0.2.3 构建状态**

* 交叉构建的 PE 已通过静态检查：`Subsystem 00000002 (Windows GUI)`（无控制台窗口）、包含
  `.rsrc` 图标资源目录（多尺寸 `assets/echoport.ico`）、外部工具子进程使用 `CREATE_NO_WINDOW`。
* 产物：绿色版 `dist/echoport-0.2.3-windows-x86_64.zip` 与 per-user MSI
  `dist/echoport-0.2.3-windows-x86_64.msi`（无需管理员、开始菜单快捷方式 + 标准卸载）；MSI 还带
  **「Uninstall EchoPort (clean app data)」** 一键干净卸载（删除设置/资料/缓存，保留已下载音乐）。
  **安装器（setup.exe）、MSI 和 zip 都内置 yt-dlp.exe + ffmpeg.exe**，普通用户无需另装依赖。
  **推荐下载 `-setup.exe` 安装器**（有安装目录/桌面快捷方式选择、开始菜单、卸载器）；MSI 是静默 per-user 安装（无界面选项）；
  zip 仅适合便携使用。
  MSI 在 Linux 用
  msitools/wixl 交叉构建，**未在 Windows 真机执行**，未签名 → SmartScreen 提示。
  **未签名、未在 Windows 实机运行**；首次运行请留意 SmartScreen 与 OpenGL 驱动要求。

**运行与数据**

* 双击 `echoport.exe`；未签名 → SmartScreen 提示时选「更多信息 → 仍要运行」。
* 在线播放/下载需要 `yt-dlp.exe` 与 `ffmpeg.exe`：放到 `PATH` 或 `echoport.exe` 旁边；
  也可在设置里指定路径，或设置 `ECHOPORT_YT_DLP` / `ECHOPORT_FFMPEG`。
* 数据位置见下方「数据与缓存位置」表（`%APPDATA%\echoport`、
  `%LOCALAPPDATA%\echoport\cache`）；下载默认 `%USERPROFILE%\Music\EchoPort`。
* DPI 缩放由 winit/egui 处理；界面缩放可在 **设置 → 外观** 选 100 / 110 / 125 / 150%。
* 已知限制：未签名 / 无安装器 / 无自动更新；依赖 OpenGL 驱动；yt-dlp/ffmpeg 不随包；
  **本轮 Windows 支持为源码审计 + 交叉构建 PE，尚未在 Windows 实机运行验证**。

### macOS

* 必须在 macOS 上构建（Apple 工具链不允许从 Linux 交叉编译）；GitHub Actions `macos-latest` job
  构建 universal `.app` 并打包 `EchoPort.app.zip`（CI-only、未在真机验证、ad-hoc 未签名）：

  ```bash
  cargo build --release           # 产物 target/release/echoport
  ```

* 通用二进制（Intel + Apple Silicon）：

  ```bash
  rustup target add aarch64-apple-darwin x86_64-apple-darwin
  cargo build --release --target aarch64-apple-darwin
  cargo build --release --target x86_64-apple-darwin
  lipo -create -output echoport \
    target/aarch64-apple-darwin/release/echoport \
    target/x86_64-apple-darwin/release/echoport
  ```

* 当前没有签名/公证的 `.app` 包；如需双击启动可自行制作 app bundle，或直接运行二进制。
* `yt-dlp` / `ffmpeg` 可用 Homebrew 安装：`brew install yt-dlp ffmpeg`。

---

## ▶️ 运行

```bash
./target/release/echoport     # 正常启动（真实网络 + 音频）
ECHOPORT_DEMO=1 cargo run     # 离线演示模式：假数据 + 生成的封面，无需网络
# 演示/截图变量：ECHOPORT_PAGE、ECHOPORT_THEME=pulse|tide|ink（旧值 blue_white|midnight|album 仍兼容）
# ECHOPORT_FOLLOW_COVER=1 让强调色跟随封面
```

首次启动会创建本机资料「Me」。第一次在线播放需要 yt-dlp 下载音频，通常要几秒钟；
之后同一首歌会命中缓存。详细操作见 [使用指南](docs/USAGE.md)。

---

## 📁 数据与缓存位置

| 内容 | Linux | macOS | Windows |
|---|---|---|---|
| 资料 / 歌单 / 收藏 / 设置 | `$XDG_DATA_HOME/echoport/profiles/*.json`（默认 `~/.local/share/echoport`） | `~/Library/Application Support/echoport` | `%APPDATA%\echoport\profiles\*.json` |
| 封面缓存 | `$XDG_CACHE_HOME/echoport/covers`（默认 `~/.cache/echoport`） | `~/Library/Caches/echoport` | `%LOCALAPPDATA%\echoport\cache\covers` |
| 音频缓存 | `$XDG_CACHE_HOME/echoport/audio`（默认 `~/.cache/echoport`） | `~/Library/Caches/echoport` | `%LOCALAPPDATA%\echoport\cache\audio` |
| Lua 配置 / 插件 | `$XDG_CONFIG_HOME/echoport`（默认 `~/.config/echoport`）：`config.lua`、`plugins/*.lua` | `~/Library/Application Support/echoport` | `%APPDATA%\echoport` |
| 下载的音乐 | 设置中的下载目录（默认 `~/Music/EchoPort`） | 同左 | `%USERPROFILE%\Music\EchoPort` |

资料是**纯 JSON**，可手动查看、备份、迁移；删除即删除资料（请先退出应用）。

### 缓存策略（磁盘占用与隐私）

* **上限可在设置里选**：设置 → 存储 → **磁盘缓存上限** 提供 **无缓存 / 512 MB / 1 GB（默认）/ 2 GB** 四档，
  改动立即生效（会按上限清理最旧文件）。
* **无缓存模式（0）**：不保留播放过的流媒体音频——切歌时删除上一首的缓存文件，启动时清空旧缓存；
  正在播放的当前曲目在播放期间仍需落盘（播放器从文件解码）。**下载目录中的音乐永远不会被删除。**
* **只在磁盘，不在内存**：在线播放的音频由 yt-dlp 下载到
  `$XDG_CACHE_HOME/echoport/audio`（默认 `~/.cache/echoport/audio`），播放器从文件解码，
  不会把整首歌常驻内存。
* **自动清理**：启动时按当前上限（默认 **1 GiB**）把音频缓存目录（递归）**最旧优先删除**；
  最近 **120 秒**内被访问的文件（当前播放、预取的下一首）受保护不删，`*.part`/`*.tmp`/`*.ytdl`
  临时文件也永不删除——但这些受保护/临时文件仍计入目录总量，因此正在播放时缓存可能短暂高于上限。
  QA/CI 可用环境变量 `ECHOPORT_CACHE_LIMIT_MB` 覆盖（GUI 设置优先）。
* **只预取下一首**：播放时最多预取队列中的下一首，不会批量下载整个歌单。
* **下载不在此列**：通过「下载」保存的音乐在设置中的下载目录（默认 `~/Music/EchoPort`），
  是用户文件，EchoPort **永远不会自动删除**。
* **封面纹理缓存**：内存中最多 **96** 张封面纹理（LRU 淘汰），每张最长边 ≤ **320 px**，
  长时间浏览也不会无限占用内存/显存；封面文件本身缓存在 `echoport/covers`。
* **手动清理**：设置页显示缓存总大小（音频 + 封面），可一键清除；清除不会影响音乐库、
  歌单和已下载文件。缓存目录权限属于当前用户，EchoPort 不会上传缓存内容，也不缓存账号信息
  （项目本身无账号）。

---

---

## 💻 资源占用与最低配置（v0.1.3 实测，v0.1.4 复核数值一致）

> 在本机（Arch Linux / 32 核 KVM / **无 GPU，Mesa llvmpipe 软件渲染** / 无声卡）用 `scripts/qa-resources.sh`
> 对**发布二进制**实测；详细方法与原始输出见 [docs/QA_REPORT.md](docs/QA_REPORT.md) 与
> [`docs/QA_REPORT.md` §6](docs/QA_REPORT.md)。

### 当前占用

| 项目 | 实测 |
|---|---|
| 发布二进制 | **15,364,248 B（≈14.7 MiB，stripped，LTO fat）**，sha256 `a004315e…` |
| 发布包 `.tar.gz` | **7,063,887 B（≈6.74 MiB）**；sha256 见 release 资产 / `dist/*.tar.gz.sha256` |
| 启动：进程→窗口出现 / 首帧 | **163–183 ms（中位 177 ms）** / **425–439 ms**（截图模式第 40 帧含写盘） |
| 空闲 CPU（8 s 采样 ×5） | **0.000–0.125 %**（本机软件渲染；历史偶发重绘异常见 QA_REPORT） |
| 空闲内存 | RSS **≈254.7 MiB**（PSS 127.9；私有 RssAnon 81.2；共享 RssFile 167.6） |
| 空闲线程 / 打开 FD | **68 / 10** |
| 播放 1 首流媒体（含封面） | RSS **≈288.8 MiB**（PSS ≈146–159；RssAnon ≈108；线程 73；FD 39）——本机无声卡，解析/缓存/封面已加载 |
| Demo 动画 + 模拟播放 | CPU **≈156 %**（软渲染）；RSS ≈274 MiB（PSS 147.5） |
| 缓存增长（1 GiB 上限，5 首） | 0 → 3.3 → 8.8 → 12.7 → 16.3 → **20.6 MiB / 5 文件**；下载目录字节数不变 |
| 无缓存模式 | 启动清空旧缓存；切歌删除上一首缓存；**下载目录不动** |
| 资料 JSON / `config.lua` 模板 | **496 B / 9,296 B** |
| GL 上下文 | Mesa 26.2.5 **llvmpipe**（LLVM 23.1.3），OpenGL 4.6，`Accelerated: no` |

### 最低配置建议（估算）

| 项目 | 建议 |
|---|---|
| CPU | x86_64 + SSE4.2，**双核 1.5 GHz+**（无 GPU、用软渲染时建议 4 核以上；动画/播放时软渲染较吃 CPU） |
| 内存 | **512 MB–1 GB 可用**（私有内存 ~80–110 MB + GL/驱动共享开销；软件渲染更高） |
| GPU | **OpenGL 3.3+**；无 GPU 时 Mesa llvmpipe 软渲染可运行（较慢、较耗 CPU，本机即此模式） |
| 磁盘 | 程序约 **15 MiB**（安装占用约 20–70 MB 含文档/desktop）；缓存 **0–2 GiB 可调**；可选 yt-dlp/ffmpeg；下载另计 |
| 系统 | Linux x86_64。**预编译包需 glibc ≥ 2.44**（本机构建基线；老发行版请用 Rust 1.85+ 源码重建）。Windows 10+ / macOS 11+ 为构建目标（本轮未实测） |
| 音频 | ALSA / PipeWire / PulseAudio 输出设备（无声卡时播放会显示错误；QA 主机如此） |
| 网络 | 在线搜索/播放需 HTTPS；本地播放可离线；在线音源需要 yt-dlp/ffmpeg（可选） |

---

## 🔒 隐私

* **无账号**：EchoPort 从不要求登录，不保存账号、Cookie 或令牌。
* **无遥测**：不发送任何统计、崩溃报告或使用数据。
* **无云端**：所有资料、歌单、收藏、历史、设置都只保存在本机 JSON 中。
* **网络只在需要时使用**：只有你主动搜索、导入链接、播放、下载或查看歌曲背景时，才会请求对应平台的公开接口。
* **无广告、无追踪脚本**，界面由原生 Rust 绘制。
* 封面与音频缓存在本机，可随时清除。

---

## ⚖️ 法律声明

* EchoPort 只是播放器与工具编排者：**它不托管、不打包、不分发任何音乐内容**。
* 在线播放与下载由你本机安装的 **yt-dlp / ffmpeg** 完成；使用即表示你同意目标网站的服务条款
  （YouTube、SoundCloud、Bilibili、网易云等）。**请只下载你有权下载的内容。**
* **不绕过 DRM**：EchoPort 不包含 Widevine / FairPlay 或任何解密代码，也不保存账号凭证。
  Spotify / Apple Music 等受 DRM 保护的服务只提供公开元数据与公开音频匹配，不绕过 DRM。
* 部分音源存在**地区限制**或版权变动（如 Bilibili、网易云），能否使用取决于你所在地区与时间。
* 软件以 GPL-3.0 许可证发布（见 [LICENSE](LICENSE)）；许可证只覆盖代码，不覆盖你访问的内容。

---

## ⌨️ 快捷键

| 按键 | 功能 |
|---|---|
| `Space` | 播放 / 暂停 |
| `←` / `→` | 后退 / 前进 5 秒 |
| `N` | 下一首 |
| `B` | 上一首 |
| `Esc` | 关闭弹窗 / 浮层 |
| `Enter` | 在搜索框提交搜索 |

文本输入框获得焦点时，播放类快捷键不会触发。

---

## ❓ 常见问题

* **在线歌曲无法播放** — 确认已安装并更新 yt-dlp（`yt-dlp -U`）；部分音源受地区限制，换一个候选来源试试。
* **提示找不到 yt-dlp** — 安装后到「设置 → 媒体工具」点「重新检测」，或手动指定路径。
* **没有声音** — 在「设置 → 播放 → 音频设备」中选择设备；同时检查系统音量与播放器音量。
* **中文显示成方框** — 安装中文字体（Linux：`noto-fonts-cjk`），或设置环境变量
  `ECHOPORT_CJK_FONT=/path/font.ttc` 后重启。
* **Spotify 搜不到歌** — 这是有意设计：Spotify 无匿名搜索，请粘贴歌曲链接导入。
* **界面太大/太小** — 设置 → 外观 → 界面缩放，可选 100% / 110% / 125% / 150%（随资料保存）。
* **Apple Music 播放的是匹配版本** — Apple/Spotify 自身音频受 DRM 保护；EchoPort 用 yt-dlp 匹配公开音频（通常为完整时长，但可能是现场/翻唱等不同版本），不会绕过 DRM。
* **下载失败** — 检查下载目录是否可写、磁盘空间是否足够，并更新 yt-dlp；错误会显示在下载条目上。
* **远程/无 GPU 环境下窗口白屏或卡住** — 试试软件渲染或 X11 后端：
  `LIBGL_ALWAYS_SOFTWARE=1 WAYLAND_DISPLAY= ./target/release/echoport`
  （详细说明见 [QA_REPORT.md](docs/QA_REPORT.md)）。

---

## 🔐 v0.2.3：安全/隐私加固、精简与安装包

* **安全加固**：更新检查只接受 `owner/name` 仓库与 **https://github.com/** 发布链接；版本字符串白名单
  清洗；`OpenUrl` 只允许 http/https；资料/歌单导入限制 8 MB 文件与头像/简介/名称/歌单/曲目数量上限。
  个人路径已从源码/文档/截图中清理。
* **精简与内存**：release 启用 `lto="fat"` + strip + codegen-units=1，Linux 二进制
  **15,364,248 B**（v0.1.9 为 16,130,920 B，−4.75%）；封面纹理缓存 96→64 张、320→256 px，
  纹理预算约 39 MB→17 MB。公开仓库截图/日志精简到 18 张当前截图（不含历史证据/本机日志）。
* **内嵌文档**：设置 → Scripting 新增 **Plugin docs / Config reference** 按钮；双语 HTML 内嵌在二进制中，
  点击时写入数据目录并用浏览器打开（无需随包附带 HTML 文件）。
* **启动提示**：更新提示的 Stay/Update 按钮改为居中。
* **安装包**：新增 **Arch pacman 包**（`echoport-<ver>-1-x86_64.pkg.tar.zst`，AUR PKGBUILD 见
  `packaging/arch/`）、**Windows MSI**（per-user，无需管理员，开始菜单快捷方式 + 卸载）、
  **macOS universal .app（CI 构建）**；GitHub Actions 发布 Linux tarball + Arch 包、Windows zip + MSI、
  macOS zip 及各自 sha256。
* **更新检查仓库**：默认目标现为 `https://github.com/Nilotoo/EchoPort`；仓库尚未 push，真实在线更新
  检查仍未验证（fake/本地路径 + 单元测试覆盖）。
* 版本与发布包更新为 **v0.2.3**。

---

## 🔔 v0.1.9：更新检查、启动动画、无外圈图标与 Arch 用户安装

* **内置更新检查（匿名、可关闭）**：启动时检查一次、之后每 **6 小时**检查 GitHub Releases
  （公开仓库，默认常量 `nilotoo/EchoPort`，可用 `ECHOPORT_UPDATE_REPO=owner/name` 覆盖；没有
  Release 时回退读取 main/master 的 `VERSION`）；只做版本比较，**不上报遥测、不需要 token、
  不自动下载/安装**。About 卡片显示更新行，「Check for updates」可手动检查。
  `ECHOPORT_FAKE_UPDATE=9.9.9` 可伪造更新；`ECHOPORT_NO_UPDATE_CHECK=1` 关闭。
* **启动动画（约 0.9 s，检查进行中最多 1.4 s）**：无外圈应用标记 + EchoPort 字标 + 细进度线；
  启动期间发现更新则停留显示「Update available vX」+ **Stay on this version / Update**
  （Update 自动下载并校验新版本；Windows/Linux 会静默安装并重启；macOS 暂为打开发布页）。`ECHOPORT_SPLASH=1` 强制显示、
  `ECHOPORT_NO_SPLASH=1` 关闭。
* **新应用图标（去掉外圈）**：`assets/icon.png`（256）、`icon-128.png`、`icon-48.png`、`icon-32.png`
  与多尺寸 `echoport.ico` 重绘；运行时窗口图标读取 `assets/icon.png`，启动动画使用同一标记；
  Windows 包使用新 ICO（PE `.rsrc` 已核对）。
* **Arch 用户本地安装**：`scripts/install-arch.sh`（无 sudo）安装到 `~/.local/bin/echoport`、
  `~/.local/share/applications/echoport.desktop` 与 hicolor 32/48/128/256 图标；本机已实际安装并通过
  `desktop-file-validate`。发布包内含该脚本与 32/48/128/256 图标。
* **发布管理**：`VERSION`（0.1.9）；`.github/workflows/release.yml` 在 `v*` tag 时构建 Linux tarball +
  Windows zip 并发布 GitHub Release；Cargo repository/homepage 指向
  `https://github.com/nilotoo/EchoPort`。
* ⚠️ **诚实说明**：仓库 URL 仍为 `nilotoo/EchoPort` 占位假设（等待用户确认远端与认证），本轮
  **未对真实 GitHub 做在线更新检查**；更新逻辑与版本比较用 fake/local 路径验证，真实网络行为待
  URL/Release 就绪后确认。

---

## 🔧 v0.1.8：资料编辑器按需展开 + 收听历史加高

* **资料编辑器默认收起**：右上角「Edit profile」按钮（与「+ New profile」并排）点击后才展开编辑卡片，
  页面初始只显示资料卡片/统计/收听历史；卡片内提供 **Close（×）/ Save / Cancel**：
  Save 提交 Bio 并关闭，Cancel 与 × 关闭且**不应用** Bio 草稿；头像选择/移除仍即时生效。
  切换资料会自动收起旧编辑器，且草稿同步逻辑保持不变。
* **收听历史卡片加高**：显示最近 **12 首**（原 8 首），整页滚动、列表不再被内部视口裁剪，一屏能看到更多行。
* QA 钩子：`ECHOPORT_PROFILE_EDIT=1` 启动即展开编辑器（用于截图）。
* 版本与发布包更新为 v0.1.8（Linux tar.gz + Windows zip）。

---

## 🩹 v0.1.7：迷你播放器 / 后台性能模式 / 图标修正

* **迷你播放器窗口（Mini player）**：点击侧边栏顶部的 **EchoPort 品牌/logo** 打开或关闭一个
  420×142 的**始终置顶**浮动窗口：封面、标题/歌手、进度条与时间、上一首/播放暂停/下一首、
  「打开主窗口」与关闭按钮；点击封面也会恢复主窗口。它作为独立 egui viewport 共享应用状态，
  主窗口可以最小化/被遮挡，播放继续。`ECHOPORT_MINI=1` 可直接打开用于 QA。
* **后台性能模式（自动）**：主窗口失焦或最小化时，UI 重绘节奏自动降速（播放中 66 ms → 1000 ms、
  有后台任务 90 ms → 1000 ms、布局 50 ms → 关闭），动画实际冻结，**音频播放不受影响**；
  回到前台自动恢复原节奏。
* **图标修正**：设置页「Reload scripts」与「Detect again」的重载图标改为**连续弧线 + 相切箭头**
  （按用户要求旋转约 30°，不再脱节）；随机播放图标改为从图标包 SVG 路径派生的**交叉箭头**，
  在 16–20 px 下仍是干净的 X（来源按钮仍为 #34 分享，搜索空状态仍为 #25 地球）。
* 版本与发布包更新为 v0.1.7（Linux tar.gz + Windows zip）。

---

## 🩹 v0.1.6：资料头像 / 发行日期 / Windows 10·11 支持

* **资料头像与简介**：每个本机资料可选图片头像（256px PNG，嵌入资料 JSON 的 data URL）与简介；
  未设置头像时显示按资料名生成的柔和纯色字母头像；资料页可 **选择/移除头像、编辑简介并保存**。
* **资料导出/导入**：资料页支持导出为可移植 JSON（含头像 data URL、简介、歌单、收藏与设置）与导入，
  方便备份或迁移到另一台电脑。
* **正在播放页**：封面下方恢复 macaron 调色板小点，原「Macaron palette」文字改为 **Released <日期>**
  （来自 Apple/Netease/Spotify/yt-dlp 导入或本地标签；来源没有日期时显示占位符）。
* **图标完善**：来源页面快捷按钮改用图标包 #34（分享），搜索页空状态使用 #25（地球）。
* **Windows 10 / 11 支持**：修复 GUI 子系统（不再出现黑色控制台窗口）、外部工具子进程
  `CREATE_NO_WINDOW`（不再闪控制台）、exe 内嵌多尺寸图标（`.rsrc`）、工具探测（PATH、exe 旁、
  winget/scoop/Python Scripts/Program Files 等）；提供 `assets/echoport.ico`、
  `packaging/README-WINDOWS.md`、`scripts/build-windows.ps1` 与 `scripts/package-windows-cross.sh`。
  本轮 Windows 产物为**交叉构建、未签名、未在 Windows 实机运行**（详见 Windows 小节）。
* **双语 HTML 手册**：新增 `docs/plugins.html` 与 `docs/config.html`（中英切换），
  并提供 `examples/config.zh-CN.lua` / `examples/config.en.lua` / `examples/config.lua`；
  config.lua 现支持 `ui_scale`（0.8–2.0）与 `cache_limit_mb`（0–10240），启动页支持 `artist`。

---

## 🩹 v0.1.5 新搜索体验与艺术家页

* **搜索升级（相关性排序 + 多段结果）**：搜索页按相关度显示 **最佳匹配**（跨平台前 3）、
  **制作人 / 歌手** 卡片（每个平台前 2，点开即进入艺术家页），以及按相关度排序的各平台歌曲列表；
  结果默认收起为 5 首，可 **展开 / 收起**；**加载更多** 每次追加 12 首（上限 60，按钮内显示加载点）。
* **艺术家 / 制作人主页**：头像、平台标识、来源页链接、**Wikipedia 简介**、**热门歌曲**（Netease 热门接口 /
  Apple artistId lookup / YouTube·SoundCloud·Bilibili 匹配），以及 **我的收听历史**（本机播放过的该艺人歌曲）；
  加载/错误/无简介状态均有提示，可一键返回搜索。
* **搜索更快**：歌曲与制作人结果 **并行请求**（原先串行约 5 s，现在约 2.5 s）。
* **防连点播放**：歌曲仍在加载时重复点击播放/三角形会被忽略（同一首只解析一次）；
  加载中点暂停/继续也不会产生额外请求；行内播放三角形保持单击播放/切换。
* **Provider 扩展**：Netease 公开歌手搜索 + 歌手热门歌曲、Apple iTunes 歌手搜索 + artistId 查询、
  YouTube/SoundCloud/Bilibili 从结果派生制作人卡片；Spotify 保持无匿名搜索（给链接导入提示）；
  Bilibili 412 在歌手搜索/播放时也会转化为友好提示并尝试 YouTube 回退。

---

## 🩹 v0.1.4 修复

* **播放控制垂直居中**：底部播放条与「正在播放」页的 transport 按钮（来源/上一首/播放/下一首/循环/随机）
  统一垂直居中到同一条中线上（此前左侧两个按钮偏上）。
* **移除多余右侧分隔线**：底部播放条只保留 mini-player 与 transport 之间的左侧细分隔线，右侧分隔线已删除。
* **右侧工具左移**：喜欢/队列/音量/信息等右侧工具整体左移（实测均匀左移 40 px），980×620、125%/150% 下不裁切。
* **Bilibili / SoundCloud 回退**：这两类来源在自身地址/搜索失败后，最后会尝试 **YouTube 公开音频匹配**，
  因此受 412/反爬限制的结果仍可能播放；若所有来源都失败，才显示友好的 412/567 双语提示。
  硬规则不变：音频始终由 yt-dlp 下载，绝不把媒体 URL 交给 HTTP 客户端。

---

## 🩹 v0.1.3 修复与改进

* **底部播放条对齐 + 分隔线**：进度条宽度计算修正，进度条中心、播放键中心与内容列中心一致（1200×800 实测 891.0 vs 891.25 px；
  最大化 0.0 px 偏差）；播放条左右各加一条低透明度细分隔线。
* **正在播放页更干净**：移除封面下方的小圆点调色板与「Macaron palette」文字，保留 v0.1.0 极简布局与信息块。
* **Apple Music 显示为「可搜索」**：与网易云一致，界面不再显示「试听/匹配」提示；解析逻辑不变（仍完整匹配优先）。
* **双击交互**：单击选中/高亮歌曲行、卡片、歌单与下载完成项；双击播放歌曲 / 打开歌单；悬停提示「双击播放 / 双击打开」。
* **来源页面按钮 + 新随机播放图标**：播放控制区的来源按钮弹出「复制链接 / 打开原始页面」；随机播放改为独立双箭头图标（循环按钮旁）。
* **缓存上限选项**：设置 → 存储新增 **无缓存 / 512MB / 1GB（默认）/ 2GB**，无缓存模式会立即删除已播放音频（下载目录不受影响）。
* **歌单离线分享**：歌单可导出/导入 JSON 元数据（含曲目 ID/标题/艺术家/专辑/来源链接，不含音频），支持逐歌单导出与工具栏导入。
* **更严格的歌曲背景**：背景标签只保留与当前歌手 + 专辑 + 歌曲相关的内容（歌手简介、专辑背景、歌曲背景三段），查不到就留空。
* **Bilibili 提示更友好**：412/567 等搜索/播放错误会显示双语提示，建议更换来源或稍后重试。
* **关于页**：新增「Made by nilotoo.」署名（图标 MIT-0 署名保留）。

---

## 🩹 v0.1.2 修复一览

* **封面圆角统一**：真实封面、生成占位封面、歌单封面、侧栏迷你封面、播放条、卡片、列表与正在播放页现在共用同一套圆角几何与裁剪；
  占位封面的渐变/字母被裁进圆角，不再出现「方形占位图嵌在圆角卡片里」。
* **列表播放/暂停图标修正**：图标集的 play 三角按 +90° 旋转为右向，所有图标按居中正方形等比缩放，不再被压成细长的「A」；
  暂停方块保持正方形居中；hover / 当前曲目 / 暂停 / 播放 / loading 等状态、菜单、播放条和正在播放页一致（100–150% 缩放）。
* **底部播放条对齐**：播放条 transport 与进度条以**内容列中心**（排除侧栏）对齐，并与「正在播放」页的按钮顺序/间距/尺寸一致；
  1200×800 与最大化下播放键 x 坐标与 NowPlaying 页面一致（实测偏差 ≈0.1 px / 0.0 px）。

---

## 🩹 v0.1.1 修复一览

* **Apple Music / Spotify 完整播放**：优先用 yt-dlp 匹配公开音频（完整时长），官方 30 秒试听仅作后备；
  自身流媒体仍受 DRM 保护、不绕过。
* **播放状态一致性**：切换歌曲立即停止上一首；播放条 / 侧边栏 / 正在播放页统一显示当前曲目与加载状态；
  进度条在播放时持续前进（修复偶发不走的问题）。
* **正在播放页恢复初版极简设计**：纯主题背景（去掉封面取色、光晕、模糊大图、漂浮色块与持续动画）；
  最大化 2560×1600、980×620 均自适应。
* **界面缩放**：设置里新增 100% / 110% / 125% / 150% 四档（持久化），高分屏/大窗口布局与边界已审计。
* **封面与搜索框**：重新设计确定性生成的占位封面（去掉神秘圆环）；搜索框有文字/清除按钮时搜索按钮不再被裁切。
* **资料页**：收听历史行不再溢出/裁切圆角卡片。
* **后台信息**：维基百科后备链接改为正常 `/w/index.php` 搜索页（不再打开 API 帮助页），
  并新增 MusicBrainz 歌手搜索与 YouTube 采访搜索后备。
* **缓存**：启动时自动清理流媒体音频缓存（默认 2 GiB、最旧优先、保护 120 秒内文件），
  封面纹理内存缓存上限 96 张（LRU、最长边 ≤320 px）；下载目录中的用户文件永不自动删除。
* **Lua 脚本/插件**：配置与插件系统、设置页「脚本与插件」卡片、自定义快捷键与动作（见 docs/SCRIPTING.md）。

---

## 🗺️ 路线图

- [ ] 歌词显示（本地 `.lrc` / 公开歌词源，可选）
- [ ] 导入/导出 `.m3u` 歌单，更完整的曲库管理（排序、专辑视图、搜索）
- [ ] 系统媒体控制（MPRIS / 媒体键）与托盘常驻
- [ ] 无缝播放 / 淡入淡出、更细的音量控制
- [ ] Windows / macOS 官方发行包（签名、`.app` bundle、安装器）
- [ ] 更多目录来源（如 Deezer 作为搜索回退）与可配置音源
- [ ] 便携模式（数据目录跟随可执行文件）与资料导入/导出
- [ ] 无障碍与键盘导航的持续改进

> 欢迎 issue / PR；本项目坚持「无账号、无 DRM 绕过、本地优先」的原则。

---

## 🧑‍💻 开发者

```bash
cargo check                      # 快速类型检查
cargo test                       # 单元 + 集成测试（持久化 / i18n / 主题 / 歌单 / 模型）
cargo build --release            # 发布构建（LTO、strip、panic=abort）
./scripts/build.sh               # 同上，带环境检查与体积报告
./scripts/package-linux.sh       # 生成 dist/echoport-<版本>-linux-x86_64.tar.gz
./scripts/run-demo.sh            # 离线演示模式生成全部截图（screenshots/qa-*.png）
python3 scripts/gen-icon.py      # 重新生成 assets/icon.png（256）与 icon-128.png
./scripts/qa-cache-prune.sh      # QA：验证音频缓存自动清理策略（伪造缓存 + 真实启动裁剪）
cargo test --test live_network -- --ignored --nocapture   # QA：真实网络的 Apple/搜索/后台链接回归测试
```

Lua 配置模板见 `examples/config.lua`，插件示例见 `examples/plugins/example.lua`；
说明见 [脚本配置](docs/SCRIPTING.md) 与 [插件接口](docs/PLUGINS.md)。

内置演示/截图模式（无需网络、无需音频设备）：

```bash
ECHOPORT_DEMO=1 cargo run
ECHOPORT_DEMO=1 ECHOPORT_SHOT=$PWD/screenshots/demo.png ECHOPORT_PAGE=home cargo run
# ECHOPORT_PAGE: home|search|library|playlists|downloads|now_playing|profile|settings
# ECHOPORT_THEME: pulse|tide|ink（旧值 blue_white|midnight|album 仍兼容）
# ECHOPORT_FOLLOW_COVER=1 强调色跟随封面；ECHOPORT_LIVE=1 截图时保留真实网络路径
```

架构、接口与线程模型见 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)，终验数据见
[docs/QA_REPORT.md](docs/QA_REPORT.md)。请勿在 `src/**` 内放置硬编码 UI 文案，所有用户可见字符串
必须走 `i18n::t`（中英双语）。

---

## 📚 文档

* [使用指南 Usage Guide](docs/USAGE.md) — 首次运行、导入、歌单、下载、快捷键、排错
* [脚本 HTML 手册 Scripting HTML manual](docs/plugins.html) · [配置 HTML 手册 Config HTML manual](docs/config.html) — 中英切换
* [配置模板 Templates](examples/) — `config.lua` / `config.zh-CN.lua` / `config.en.lua`
* [音源支持 Providers](docs/PROVIDERS.md) — 各平台能力、限制与原因（含 DRM/地区说明）
* [架构说明 Architecture](docs/ARCHITECTURE.md) — 模块、线程、数据流、持久化 schema
* [脚本配置 Scripting](docs/SCRIPTING.md) · [插件接口 Plugins](docs/PLUGINS.md) — Lua 配置与扩展
* [质量报告 QA Report](docs/QA_REPORT.md) — 构建、测试、性能与截图证据

## 界面图标 / Icon credits

应用内界面图标（导航、操作、播放控制）使用项目所有者提供的图标集：
作者 **Singular Programmer**，**MIT-0** 许可。资源与许可证见
[`assets/icons/`](assets/icons/README.md)（白/黑两套 PNG+SVG，程序内嵌白色 48px 变体并按主题着色）。
应用/启动器图标（`assets/icon.png`）目前仍是 EchoPort 品牌图标，未替换。

## 许可证

GPL-3.0 © EchoPort contributors. 见 [LICENSE](LICENSE)。

**Made by nilotoo.**

---

<a id="english"></a>
# EchoPort (English)

**A quiet, lightweight, local-first desktop music player.** Your local library, online search,
mixed-platform playlists, downloads and song background all dock in one place.
**No account, nothing uploaded.**

`Rust 1.85+` · `egui 0.32` · `Linux / Windows / macOS` · `GPL-3.0` · `v0.2.3`
· English | [中文](#echoport--回声港)

## 📸 Screenshots

All screenshots use an isolated temporary XDG environment and the neutral demo profile (Me); no personal profiles, history or machine paths are shown.

| Home | Search |
|---|---|
| ![Home](screenshots/qa-home.png) | ![Search](screenshots/qa-search.png) |

| Library | Now playing |
|---|---|
| ![Library](screenshots/qa-library.png) | ![Now playing](screenshots/qa-now_playing.png) |

| Playlists | Downloads |
|---|---|
| ![Playlists](screenshots/qa-playlists.png) | ![Downloads](screenshots/qa-downloads.png) |

| Settings | Profile (on-demand editor, collapsed) |
|---|---|
| ![Settings](screenshots/qa-settings.png) | ![Profile collapsed](screenshots/qa18-profile-collapsed.png) |

| Profile (editor open: Save / Cancel / ×) | Mini player |
|---|---|
| ![Profile editing](screenshots/qa18-profile-editing.png) | ![Mini player](screenshots/qa17-mini-player.png) |

| Startup splash | Splash update prompt |
|---|---|
| ![Splash](screenshots/qa19-splash.png) | ![Splash update](screenshots/qa19-splash-update.png) |

| Embedded Plugin docs / Config reference | Settings |
|---|---|
| ![Docs buttons](screenshots/qa20-settings-docs-buttons.png) | ![Settings](screenshots/qa-settings.png) |

| New ringless icon | Maximized home |
|---|---|
| ![Icon preview](screenshots/qa19-icon-preview.png) | ![Home maximized](screenshots/qa-home-maximized.png) |

| Tide theme · Home | Ink theme · Home |
|---|---|
| ![Tide home](screenshots/qa-tide-home.png) | ![Ink home](screenshots/qa-ink-home.png) |

| 125% UI scale · Home |  |
|---|---|
| ![125% scale](screenshots/qa-ui-scale-125-home.png) | |

## ✨ Features

* **Multi-source player** — YouTube, SoundCloud, Bilibili, Netease, the Apple Music catalogue and
  local files. Apple Music is shown as **Searchable** in the UI like Netease; Spotify / Apple Music
  links are imported and **full playback is matched to public audio via yt-dlp** (the services' own
  streams are DRM-protected and are not bypassed).
* **Minimal Now playing (original design)** — plain theme background (no cover tint, glow, blurred
  wallpaper, floating decoration or animation); large cover + info + controls, with the macaron palette
  dots and a **Released date** under the cover when the source provides one.
* **Interface scale** — Settings → Appearance offers **100% / 110% / 125% / 150%** for HiDPI or
  distant viewing; the choice is persisted per profile.
* **Mixed-platform playlists** — one playlist can contain local files and songs from every source;
  play all / shuffle all.
* **Offline playlist sharing** — export/import a playlist as a JSON **metadata** file (track ids, title,
  artist, album, source-page links — **no audio**); the other side resolves playback from its own sources.
* **Double-click interaction** — single click selects/highlights rows, cards and playlists; double-click
  plays the song / opens the playlist (explicit play buttons and transport stay single-click); hover
  tooltips say "Double-click to play/open".
* **Source page & shuffle** — the transport has a source-page button (popup: **Copy link** / **Open
  original page**), and shuffle now uses its own crossed-arrow icon next to repeat.
* **Downloads** — via external yt-dlp (MP3 / M4A / Opus / FLAC) with progress, completion and
  "open folder".
* **Browse & search** — search every selected platform at once; results are ranked into **Best matches /
  Producers & artists / per-platform lists**, with Show more and **Load more**, and producer cards open an
  artist page (bio / popular tracks / your history); platform filters and paste-a-link import included.
* **Play-spam protection** — clicking play repeatedly while a track is still loading is ignored (one resolve
  per track); no duplicate requests.
* **Song background (strictly relevant)** — the Now playing → Background tab only shows content truly
  about this **artist + album + song**: artist bio, album background and song background sections
  (empty when nothing relevant is found — no unrelated noise), plus interviews and source links.
* **Three themes + cover-derived accent** — **Pulse** (porcelain white, vivid blue, fluorescent
  lime; default), **Tide** (light aqua, teal, deep navy) and **Ink** (low-saturation dark slate);
  optionally let the accent follow the current cover.
* **Lua config & plugins** — `config.lua` for startup options and shortcuts, `plugins/*.lua` for
  extensions (see [Scripting](docs/SCRIPTING.md) / [Plugins](docs/PLUGINS.md)).
* **Local profiles** — multiple isolated on-device profiles with avatars (image or pastel letter
  fallback) and bios, plus **profile export/import** (portable JSON with playlists/settings); all
  stored as local JSON.
* **Chinese / English** — fully bilingual UI and docs, switchable anytime; CJK fonts are discovered
  from the system.
* **Lightweight** — native Rust + egui (OpenGL): no embedded browser, no Electron, near-zero CPU
  when idle.

## 🌐 Platform support

Legend: ✅ works · ⚠️ partial / best effort · ❌ not possible by design

| Platform | Search | Playback | Cover / metadata | Paste link | Notes |
|---|---|---|---|---|---|
| **Local files** | folder scan | ✅ native decode | ✅ embedded tags/art | — | mp3 / m4a / aac / alac / flac / wav / ogg, offline |
| **YouTube** | ✅ | ✅ full (via yt-dlp) | ✅ | ✅ | the universal audio backend; keep yt-dlp current |
| **SoundCloud** | ✅ | ⚠️ many tracks DRM/HLS-only; falls back to a YouTube match | ✅ | ✅ | per-track hint only after native sources fail |
| **Bilibili** | ⚠️ | ⚠️ audio only, often region-locked; falls back to a YouTube match | ✅ | ✅ | on 412/567 it retries via YouTube first; friendly hint only if all fail |
| **Netease** | ✅ public API | ⚠️ via yt-dlp (≈128 kbps mp3) | ✅ | ✅ | unlicensed/legally gray in some regions |
| **Apple Music** | ✅ public iTunes Search API (shown as Searchable, same as Netease) | ✅ full (yt-dlp public-audio match) | ✅ | ✅ | Apple’s own audio is FairPlay DRM — not bypassed |
| **Spotify** | ❌ no anonymous search | ✅ full (yt-dlp public-audio match; 30 s preview fallback) | ✅ (from link) | ✅ only | APIs need a token and audio is Widevine DRM — not bypassed |
| **YouTube Music** | via plain YouTube | via YouTube | ✅ | ✅ | use a normal YouTube link |

> This is the honest support envelope: EchoPort does not bypass DRM, does not store account tokens
> and does not host any music. Sources and region rules change; failures are shown as hints, not
> crashes. Full rationale and troubleshooting: [docs/PROVIDERS.md](docs/PROVIDERS.md).

## 📦 Install

### Dependencies

| Dependency | Required for | Notes |
|---|---|---|
| **Rust 1.85+** | building | install stable via [rustup](https://rustup.rs) |
| **C compiler** | building | Lua runtime (mlua) is compiled vendored; needs `cc` (no system Lua, no cmake) |
| **yt-dlp** | all online features | search / playback / downloads; keep it current (`yt-dlp -U`). Verified with 2026.10.21 |
| **ffmpeg** | some features | conversion, HLS and download transcoding. Verified with 9.0.2 |
| **CJK font** | optional | Chinese renders as boxes without one; e.g. `noto-fonts-cjk` on Linux |
| **Graphics / audio runtime** | required | Wayland or X11 + OpenGL (software Mesa works); PipeWire / PulseAudio / ALSA |

yt-dlp and ffmpeg are **external tools** and are not bundled (each has its own licence and terms).
EchoPort auto-detects them on `PATH`, `~/.local/bin`, `/usr/local/bin`, `/opt/homebrew/bin`, …; you
can also set explicit paths in **Settings → Media tools**.

### Linux: build from source

```bash
# Arch Linux example (translate package names for your distro)
sudo pacman -S --needed base-devel pkgconf rustup mesa libxkbcommon wayland libxcb noto-fonts-cjk
rustup default stable
sudo pacman -S --needed yt-dlp ffmpeg          # optional, strongly recommended

git clone <repository-url> EchoPort            # or cd into an existing checkout
cd EchoPort
cargo build --release                          # or ./scripts/build.sh
./target/release/echoport
```

Debian/Ubuntu build dependencies are roughly `build-essential pkg-config libgl1-mesa-dev
libxkbcommon-dev`; pick the runtime libraries for your desktop. Package names vary by distribution.
The C compiler is used for the vendored Lua 5.4 (mlua); no system Lua or cmake is required.

### Linux: prebuilt tarball

`dist/echoport-<version>-linux-x86_64.tar.gz` (produced by `scripts/package-linux.sh`, suitable for
release attachments) contains the `echoport` binary, `echoport.desktop`, `echoport.png`,
`README.md` and `INSTALL.txt`. Install with:

```bash
tar -xzf echoport-0.2.3-linux-x86_64.tar.gz
cd echoport-0.2.3-linux-x86_64
mkdir -p ~/.local/bin ~/.local/share/applications ~/.local/share/icons/hicolor/256x256/apps
install -m755 echoport        ~/.local/bin/echoport
install -m644 echoport.desktop ~/.local/share/applications/echoport.desktop
install -m644 echoport.png     ~/.local/share/icons/hicolor/256x256/apps/echoport.png
echoport   # or launch EchoPort from the application menu
```

* **Arch users**: `scripts/install-arch.sh` for a no-sudo `~/.local` install, or the release pacman
  package: `sudo pacman -U echoport-0.2.3-1-x86_64.pkg.tar.zst` (AUR-ready PKGBUILD in
  `packaging/arch/`; depends glibc/gcc-libs, optional yt-dlp/ffmpeg).

* **Release assets**: `echoport-0.2.3-linux-x86_64.tar.gz` (portable Linux),
  `echoport-0.2.3-1-x86_64.pkg.tar.zst` (Arch pacman), `echoport-0.2.3-windows-x86_64.zip` (portable
  Windows), `echoport-0.2.3-windows-x86_64.msi` (per-user install) and the CI-built macOS
  `EchoPort.app.zip` — each with a `.sha256` file.

> The tarball does **not** include yt-dlp / ffmpeg — install them as described above. EchoPort is at
> 0.2.3 and does not yet ship officially hosted binaries; build from source or run the packaging
> script yourself.

### Windows 10 / 11 (x86_64)

**Runtime requirements**

* Windows 10 (1903+) or Windows 11, x86_64; a working **OpenGL 3.3+ driver** (current AMD/Intel/
  NVIDIA drivers qualify; the Microsoft Basic Display Adapter / some RDP sessions only expose
  OpenGL 1.1 and the window cannot be created); an audio output device (WASAPI).
* No WebView2, .NET or installer needed — unzip and run (unsigned executable).

**Native build (recommended)**

```powershell
rustup default stable-msvc    # Rust 1.85+; install VS Build Tools "Desktop development with C++"
cd EchoPort
cargo build --release         # target\release\echoport.exe
powershell -ExecutionPolicy Bypass -File scripts\build-windows.ps1
```

Output: `dist\echoport-<version>-windows-x86_64.zip` (portable) and
`dist\echoport-<version>-windows-x86_64.msi` (per-user install, no admin; Start Menu shortcut +
uninstall; unsigned → SmartScreen). The MSI is cross-built on Linux with msitools/wixl and has not
been executed on Windows.

**Linux cross build (CI / verification only, not the official release path)**

```bash
rustup target add x86_64-pc-windows-gnu
# Arch: sudo pacman -S mingw-w64-gcc    Ubuntu: gcc-mingw-w64-x86-64-posix binutils-mingw-w64-x86-64
./scripts/package-windows-cross.sh       # dist/echoport-<version>-windows-x86_64.zip
```

The cross-built executable is an unsigned PE and has **not been run on real Windows**;
official packages should be built natively on Windows.

**Run & data**

* Double-click `echoport.exe`; SmartScreen may warn for the unsigned binary — **More info →
  Run anyway**.
* Online playback/downloads need `yt-dlp.exe` and `ffmpeg.exe` on `PATH` or next to `echoport.exe`;
  explicit paths can be set in Settings, or via `ECHOPORT_YT_DLP` / `ECHOPORT_FFMPEG`.
* Data locations are in the table below (`%APPDATA%\echoport`, `%LOCALAPPDATA%\echoport\cache`);
  downloads default to `%USERPROFILE%\Music\EchoPort`.
* DPI scaling is handled by winit/egui; the in-app UI scale (100/110/125/150 %) is in
  Settings → Appearance.
* Known caveats: unsigned / no installer / no auto-updater; OpenGL driver required; yt-dlp/ffmpeg
  not bundled; **Windows support in this round is source-audited + cross-built PE only — not
  executed on Windows**.

### macOS

* Must be built on macOS (Apple's toolchain cannot be cross-compiled from Linux); the GitHub Actions
  `macos-latest` job builds a universal `.app` and packages `EchoPort.app.zip` (CI-only, unverified
  locally, ad-hoc unsigned):

  ```bash
  cargo build --release              # produces target/release/echoport
  ```

* Universal binary (Intel + Apple Silicon):

  ```bash
  rustup target add aarch64-apple-darwin x86_64-apple-darwin
  cargo build --release --target aarch64-apple-darwin
  cargo build --release --target x86_64-apple-darwin
  lipo -create -output echoport \
    target/aarch64-apple-darwin/release/echoport \
    target/x86_64-apple-darwin/release/echoport
  ```

* No signed/notarised `.app` bundle yet: run the binary directly or build your own bundle.
* Install the tools with Homebrew: `brew install yt-dlp ffmpeg`.

## ▶️ Run

```bash
./target/release/echoport     # normal app (real network + audio)
ECHOPORT_DEMO=1 cargo run     # offline demo mode: fixtures + generated covers, no network
# demo/screenshot vars: ECHOPORT_PAGE, ECHOPORT_THEME=pulse|tide|ink
# (legacy blue_white|midnight|album still accepted), ECHOPORT_FOLLOW_COVER=1
```

The first launch creates the local profile "Me". The first online playback may take a few seconds
while yt-dlp fetches the audio; later plays hit the cache. See the
[usage guide](docs/USAGE.md).

## 📁 Data & cache locations

| Content | Linux | macOS | Windows |
|---|---|---|---|
| Profiles / playlists / favourites / settings | `$XDG_DATA_HOME/echoport/profiles/*.json` (default `~/.local/share/echoport`) | `~/Library/Application Support/echoport` | `%APPDATA%\echoport\profiles\*.json` |
| Cover cache | `$XDG_CACHE_HOME/echoport/covers` (default `~/.cache/echoport`) | `~/Library/Caches/echoport` | `%LOCALAPPDATA%\echoport\cache\covers` |
| Audio cache | `$XDG_CACHE_HOME/echoport/audio` (default `~/.cache/echoport`) | `~/Library/Caches/echoport` | `%LOCALAPPDATA%\echoport` |
| Lua config / plugins | `$XDG_CONFIG_HOME/echoport` (default `~/.config/echoport`): `config.lua`, `plugins/*.lua` | `~/Library/Application Support/echoport` | `%APPDATA%\echoport` |
| Downloaded music | download folder from Settings (default `~/Music/EchoPort`) | same | same |

Profiles are **plain JSON** — inspect, back up or move them by hand. Deleting a profile file
removes that profile (quit the app first).

### Cache policy (disk use & privacy)

* **Disk, not RAM**: streamed audio is downloaded by yt-dlp into
  `$XDG_CACHE_HOME/echoport/audio` (default `~/.cache/echoport/audio`) and decoded from the file —
  whole songs are not kept in memory.
* **Automatic pruning**: on startup the audio cache directory (recursively) is trimmed to the current
  cap — **1 GiB by default** — deleting the **oldest mtime first**. Files touched within the last
  **120 s** (current playback, prefetched next track) are protected, and `*.part`/`*.tmp`/`*.ytdl`
  temporary files are never deleted — but protected and temporary files still count towards the
  directory total, so an actively playing track may keep the cache briefly above the cap.
  `ECHOPORT_CACHE_LIMIT_MB` can override the cap for QA/CI (the GUI setting wins otherwise).
* **Cache-limit setting**: Settings → Storage → **Disk cache limit** offers **No cache / 512 MB /
  1 GB (default) / 2 GB** and applies immediately (prunes oldest files down to the new cap).
* **No-cache mode (0)**: played streamed audio is not kept — the previous track's cache file is deleted
  when switching, and old cache is purged at startup. The currently playing track is still written to
  disk while it plays (the player decodes from the file). **Music in your download folder is never
  deleted.**
* **Prefetch only the next track** — never a whole playlist.
* **Downloads are not cache**: music saved via **Download** lives in the configured download
  folder (default `~/Music/EchoPort`), is yours, and is **never auto-deleted**.
* **Cover texture cache**: at most **96** cover textures in memory (LRU), each ≤ **320 px** on its
  longest side; the cover files themselves live under `echoport/covers`.
* **Manual clearing**: Settings shows the combined cache size (audio + covers) and can clear it;
  clearing never touches your library, playlists or downloaded files. Cache directories are
  user-scoped and never uploaded — EchoPort has no account and stores no credentials.

---

## 💻 Resource usage & minimum configuration (measured on v0.1.3, re-checked on v0.1.4)

> Measured on this QA host (Arch Linux, 32-core KVM, **no GPU → Mesa llvmpipe software rendering**, no sound
> card) with `scripts/qa-resources.sh` against the **release binary**; method and raw output in
> [docs/QA_REPORT.md](docs/QA_REPORT.md) and [`docs/QA_REPORT.md` §6](docs/QA_REPORT.md).

### Current footprint

| Item | Measured |
|---|---|
| Release binary | **15,364,248 B (≈14.7 MiB, stripped, LTO fat)**, sha256 `a004315e…` |
| Release tarball | **7,063,887 B (≈6.74 MiB)**; sha256 in the release assets / `dist/*.tar.gz.sha256` |
| Startup: process → window / first frame | **163–183 ms (median 177 ms)** / **425–439 ms** (screenshot mode, frame 40 incl. PNG write) |
| Idle CPU (8 s samples ×5) | **0.000–0.125 %** (software GL host; historical repaint anomaly noted in QA_REPORT) |
| Idle memory | RSS **≈254.7 MiB** (PSS 127.9; private RssAnon 81.2; shared RssFile 167.6) |
| Idle threads / open FDs | **68 / 10** |
| Playing one streamed track (incl. cover) | RSS **≈288.8 MiB** (PSS ≈146–159; RssAnon ≈108; threads 73; FDs 39) — no sound card on this host; resolve/cache/cover completed |
| Demo animation + simulated playback | CPU **≈156 %** (software GL); RSS ≈274 MiB (PSS 147.5) |
| Cache growth (1 GiB cap, 5 tracks) | 0 → 3.3 → 8.8 → 12.7 → 16.3 → **20.6 MiB / 5 files**; download folder bytes unchanged |
| No-cache mode | old cache purged at startup; previous track removed when switching; **downloads untouched** |
| Profile JSON / `config.lua` template | **496 B / 9,296 B** |
| GL context | Mesa 26.2.5 **llvmpipe** (LLVM 23.1.3), OpenGL 4.6, `Accelerated: no` |

### Minimum configuration estimate

| Item | Recommendation |
|---|---|
| CPU | x86_64 + SSE4.2, **dual-core 1.5 GHz+** (4+ cores recommended without a GPU; llvmpipe is CPU-hungry during animation/playback) |
| RAM | **512 MB–1 GB free** (private ~80–110 MB + GL/driver shared overhead; software GL needs more) |
| GPU | **OpenGL 3.3+**; without a GPU Mesa llvmpipe works (slower, higher CPU — this host) |
| Disk | app ≈ **15 MiB** (installed footprint ~20–70 MB with docs/desktop); cache **0–2 GiB configurable**; optional yt-dlp/ffmpeg; downloads extra |
| OS | Linux x86_64. **Prebuilt tarball needs glibc ≥ 2.44** (this build baseline; older distros: rebuild from source with Rust 1.85+). Windows 10+ / macOS 11+ are build targets (not verified this round) |
| Audio | an ALSA / PipeWire / PulseAudio output device (no sound card → playback shows an error; true on this QA host) |
| Network | HTTPS for online search/playback; local playback works offline; online sources need yt-dlp/ffmpeg (optional) |

---

## 🔒 Privacy

* **No account** — EchoPort never asks you to log in and stores no credentials, cookies or tokens.
* **No telemetry** — no analytics, crash reports or usage data are sent anywhere.
* **No cloud** — profiles, playlists, favourites, history and settings are local JSON only.
* **Network only when needed** — requests happen only for actions you take (search, link import,
  play, download, background info).
* **No ads, no tracking scripts**; the interface is drawn natively by Rust.
* Covers and audio are cached locally and can be cleared at any time.

## ⚖️ Legal notice

* EchoPort is a player and tool orchestrator: it **does not host, bundle or redistribute music**.
* Online playback/downloads are performed by your own **yt-dlp / ffmpeg** installation, which means
  you agree to the terms of service of the target sites (YouTube, SoundCloud, Bilibili, Netease, …).
  **Only download content you have the right to download.**
* **No DRM circumvention**: EchoPort contains no Widevine/FairPlay or decryption code and stores no
  account credentials. DRM-protected services are limited to public metadata, 30 s previews and
  matching to public audio.
* Some sources are **region-restricted** or change over time (e.g. Bilibili, Netease); availability
  depends on your region and the date.
* Released under the GPL-3.0 licence (see [LICENSE](LICENSE)), which covers the code, not the content you
  choose to access.

## ⌨️ Keyboard shortcuts

| Key | Action |
|---|---|
| `Space` | Play / pause |
| `←` / `→` | Seek −5 s / +5 s |
| `N` | Next track |
| `B` | Previous track |
| `Esc` | Close pop-ups / overlays |
| `Enter` | Submit a search from the search field |

Playback shortcuts are ignored while a text field has focus.

## ❓ Troubleshooting

* **Online tracks won't play** — make sure yt-dlp is installed and current (`yt-dlp -U`); some
  sources are region-blocked, try another candidate.
* **"yt-dlp not found"** — install it, then press **Settings → Media tools → Detect again**, or set
  the path manually.
* **No sound** — choose an output device in **Settings → Playback → Audio device** and check both
  system and player volume.
* **Chinese shows as boxes** — install a CJK font (Linux: `noto-fonts-cjk`) or restart with
  `ECHOPORT_CJK_FONT=/path/font.ttc`.
* **Spotify search finds nothing** — by design: Spotify has no anonymous search; paste a track link.
* **The UI is too large/small** — Settings → Appearance → Interface scale: 100% / 110% / 125% / 150%
  (persisted per profile).
* **Apple Music plays a matched version** — the services’ own audio is DRM-protected; EchoPort
  matches full public audio via yt-dlp (usually full length, but possibly a live/cover version) and
  never bypasses DRM.
* **Download failed** — check that the download folder is writable and has space, and update yt-dlp;
  the error is shown on the job.
* **Blank/stuck window on a remote or GPU-less host** — try software rendering or the X11 backend:
  `LIBGL_ALWAYS_SOFTWARE=1 WAYLAND_DISPLAY= ./target/release/echoport`
  (details in [QA_REPORT.md](docs/QA_REPORT.md)).

## 🔐 v0.2.3: security/privacy hardening, slimming & installers

* **Security hardening**: the update checker only accepts `owner/name` repos and
  **https://github.com/** release links, sanitizes version strings, `OpenUrl` accepts only
  http/https, and profile/playlist import is capped at 8 MB with avatar/bio/name/playlist/track
  count limits. Personal paths were removed from source, docs and screenshots.
* **Slimming & memory**: release enables `lto="fat"` + strip + `codegen-units=1`; the Linux binary is
  **15,364,248 B** (was 16,130,920 B at v0.1.9, −4.75%). The cover-texture cache went from
  96 → 64 textures at 320 → 256 px (texture budget ~39 MB → ~17 MB). The public repo keeps only 18
  current screenshots (no historical evidence or machine logs).
* **Embedded docs**: Settings → Scripting gains **Plugin docs / Config reference** buttons; the
  bilingual HTML is embedded in the binary, written to the data dir on click and opened in the
  browser (no external HTML files required).
* **Splash**: the update prompt's Stay/Update buttons are now centred.
* **Installers**: new **Arch pacman package** (`echoport-<ver>-1-x86_64.pkg.tar.zst`; AUR-ready
  PKGBUILD in `packaging/arch/`), **Windows MSI** (per-user, no admin, Start Menu shortcut +
  uninstall) and a **macOS universal .app (CI build)**. GitHub Actions uploads the Linux tarball +
  Arch package, Windows zip + MSI, macOS zip and a `.sha256` for each.
* **Update-check repo**: default is now `https://github.com/Nilotoo/EchoPort`; the repo has not been
  pushed yet, so a real online update check is still unverified (fake/local paths + unit tests only).
* Version and packages bumped to **v0.2.3**.

---

## 🔔 v0.1.9: update monitor, startup animation, ringless icon, Arch user install

* **Built-in update check (anonymous, opt-out)**: checks GitHub Releases at startup and every **6 hours**
  (public repo, default const `nilotoo/EchoPort`, overridable via `ECHOPORT_UPDATE_REPO=owner/name`;
  falls back to the `VERSION` file on main/master when no Release exists). Version comparison only:
  **no telemetry, no token, no auto-download/install**. The About card shows an update row and
  "Check for updates" runs a manual check. `ECHOPORT_FAKE_UPDATE=9.9.9` fakes one;
  `ECHOPORT_NO_UPDATE_CHECK=1` disables the checker.
* **Startup animation (~0.9 s, up to 1.4 s while a check is in flight)**: ringless app mark + EchoPort
  wordmark + thin progress line. If an update is found during the splash it stays with
  "Update available vX" + **Stay on this version / Update** (Update downloads, verifies and installs automatically;
  no
  auto-install). `ECHOPORT_SPLASH=1` forces it; `ECHOPORT_NO_SPLASH=1` disables it.
* **New ringless app icon**: `assets/icon.png` (256), `icon-128.png`, `icon-48.png`, `icon-32.png` and the
  multi-size `echoport.ico` were redrawn; the runtime window icon loads `assets/icon.png` and the splash
  uses the same mark. The Windows package embeds the new ICO (PE `.rsrc` re-checked).
* **Arch user-local install**: `scripts/install-arch.sh` (no sudo) installs `~/.local/bin/echoport`,
  `~/.local/share/applications/echoport.desktop` and hicolor 32/48/128/256 icons; installed and verified
  with `desktop-file-validate` on this machine. Release tarballs also ship the script and those icons.
* **Release management**: `VERSION` (0.1.9); `.github/workflows/release.yml` builds the Linux tarball +
  Windows zip on `v*` tags and publishes a GitHub Release; Cargo repository/homepage point at
  `https://github.com/nilotoo/EchoPort`.
* ⚠️ **Honesty note**: the repo URL is still the `nilotoo/EchoPort` placeholder (pending the user's remote
  URL + auth confirmation), so **no live GitHub update check was performed this round**; the checker and
  version comparison were verified with fake/local paths only.

---

## 🔧 v0.1.8: on-demand profile editor + taller listening history

* **Profile editor collapsed by default**: the "Edit profile" button (top right, next to "+ New profile")
  opens the edit card on demand; the page starts with the profile cards, statistics and history only.
  The card has **Close (×) / Save / Cancel**: Save commits the bio and closes; Cancel and × close
  **without applying** the bio draft; choosing/removing an avatar still applies immediately. Switching
  profiles closes a stale editor, and draft sync is preserved.
* **Listening history card is taller**: shows the latest **12 tracks** (was 8), with the page scrolling
  instead of clipping the list in an inner viewport — more rows visible at once.
* QA hook: `ECHOPORT_PROFILE_EDIT=1` opens the editor at startup for screenshots.
* Version and packages bumped to v0.1.8 (Linux tar.gz + Windows zip).

---

## 🩹 v0.1.7: mini player / background performance mode / icon fixes

* **Mini player**: click the **EchoPort brand/logo** in the sidebar to toggle a 420×142
  **always-on-top** floating window with cover, title/artist, progress + times, previous/play-pause/
  next, "open main window" and close; clicking the cover also restores the main window. It is an
  immediate egui viewport sharing app state, so playback continues while the main window is hidden.
  `ECHOPORT_MINI=1` opens it directly for QA.
* **Background performance mode (automatic)**: when the main window is unfocused or minimized the
  repaint cadence is throttled (playing 66 ms → 1000 ms; busy 90 ms → 1000 ms; layout 50 ms → off),
  animations effectively freeze and **audio keeps playing**; the foreground cadence returns
  automatically when the window is focused again.
* **Icon fixes**: the Settings "Reload scripts" / "Detect again" reload icon now has a **connected
  arc + tangential arrowhead** (rotated ~30° as requested, no disjoint arrow); the shuffle icon is
  now **crossed arrows** derived from the icon pack's SVG path (`M7 17L17 7` + head, second copy
  rotated 90°), a clean X at 16–20 px. The source-page button stays #34 share and the search empty
  state stays #25 globe.
* Version and packages bumped to v0.1.7 (Linux tar.gz + Windows zip).

---

## 🩹 v0.1.6: profile avatars / release dates / Windows 10 & 11

* **Profile avatar & bio**: each local profile can have an image avatar (256 px PNG embedded as a data URL
  in the profile JSON) and a short bio; without an avatar a deterministic pastel letter avatar is shown.
  The profile page can **choose/remove a picture, edit + save the bio**.
* **Profile export/import**: export a portable JSON profile (avatar data URL, bio, playlists, favourites,
  settings) and import it back — handy for backups or moving to another PC.
* **Now playing**: the macaron palette dots are back under the cover, and the old "Macaron palette" text is
  replaced with **Released <date>** (Apple/Netease/Spotify/yt-dlp imports or local tags; an em dash when the
  source has no date).
* **Icons**: the source-page shortcut now uses icon-pack #34 (share); the search empty state uses #25 (globe).
* **Windows 10 / 11 support**: GUI subsystem fixed (no black console window), child tools use
  `CREATE_NO_WINDOW` (no console flashes), multi-size exe icon embedded (`.rsrc`), and tool discovery
  covers PATH, next-to-exe, winget/scoop/Python Scripts/Program Files. Ships `assets/echoport.ico`,
  `packaging/README-WINDOWS.md`, `scripts/build-windows.ps1` and `scripts/package-windows-cross.sh`.
  This round's Windows build is **cross-built, unsigned and NOT executed on Windows** (see the Windows
  section).
* **Bilingual HTML manuals**: new `docs/plugins.html` and `docs/config.html` (ZH/EN toggle), plus
  `examples/config.zh-CN.lua` / `config.en.lua` / `config.lua`; config.lua now supports `ui_scale`
  (0.8–2.0) and `cache_limit_mb` (0–10240), and `artist` as a startup page.

---

## 🩹 New search experience & artist pages in v0.1.5

* **Search upgrade (relevance-ranked sections)**: the search page shows **Best matches** (top 3 across
  platforms), **Producers & artists** cards (first 2 per platform; click opens the artist page), then
  per-platform track lists ordered by relevance. Tracks start collapsed to 5 with **Show more / Show less**;
  **Load more** appends 12 more per click (capped at 60, inline loading dots).
* **Artist / producer pages**: avatar, platform badge, source-page link, **Wikipedia bio**, **Popular tracks**
  (Netease hot-songs API / Apple `artistId` lookup / YouTube·SoundCloud·Bilibili matching) and **Your listening
  history** (locally played tracks from that artist); loading/error/no-bio states and a back-to-search button.
* **Faster search**: tracks and artists are fetched **in parallel** (~2.5 s instead of ~5 s sequential).
* **Play-spam protection**: pressing play/triangle again while a track is still loading is ignored (one resolve
  per track); pause/resume during loading is a no-op; the row triangle stays single-click play/toggle.
* **Provider support**: Netease public artist search + artist hot songs; Apple iTunes `musicArtist` search +
  `artistId` lookup; YouTube/SoundCloud/Bilibili derive producer cards from results; Spotify stays
  link-import only; Bilibili 412 during artist search/playback maps to the friendly hint and tries the
  YouTube fallback.

---

## 🩹 Fixed in v0.1.4

* **Vertically centred transport**: bottom player bar and Now-playing transport buttons (source / previous /
  play / next / repeat / shuffle) now share one vertical centre line (the left two were top-aligned before).
* **Redundant right divider removed**: only the left mini-player|transport divider remains in the bottom bar.
* **Right-hand tools shifted left**: favourites / queue / volume / info moved left (measured uniform −40 px),
  with no clipping at 980×620 or 125 % / 150 % UI scale.
* **Bilibili / SoundCloud fallback**: after their own URL/search sources fail, playback now makes a final
  **YouTube public-audio match**, so 412/anti-bot-limited results may still play; the friendly bilingual
  412/567 hint only appears when every source fails. Hard rule unchanged: yt-dlp downloads the audio —
  media URLs are never handed to the HTTP client.

---

## 🩹 Fixed & improved in v0.1.3

* **Bottom player bar alignment + dividers**: the scrubber width math was fixed so the scrubber centre,
  the play-button centre and the content-column centre agree (1200×800 measured 891.0 vs 891.25 px;
  0.0 px diff maximized); subtle low-alpha dividers now separate both sides of the transport.
* **Cleaner Now playing**: removed the macaron palette dots and "Macaron palette" label under the cover;
  the original minimal v0.1.0 layout and info block stay.
* **Apple Music shown as Searchable**: same as Netease; no more preview/match wording in the UI. Resolver
  behaviour is unchanged (full public-audio match first).
* **Double-click UX**: single click selects/highlights track rows, cards, playlists and finished downloads;
  double-click plays the song / opens the playlist; hover tooltips say "Double-click to play/open".
* **Source-page button + new shuffle icon**: the source button opens a popup with **Copy link / Open
  original page**; shuffle now has its own crossed-arrow icon next to repeat.
* **Cache-limit setting**: Settings → Storage offers **No cache / 512 MB / 1 GB (default) / 2 GB**;
  no-cache mode deletes played streamed audio immediately (downloads are untouched).
* **Offline playlist sharing**: export/import playlists as metadata JSON (track ids/title/artist/album/
  source links, no audio) with per-playlist export and a toolbar import.
* **Stricter song background**: only artist + album + song relevant content is shown (artist bio, album
  background and song background sections); unrelated results are dropped and sections stay empty.
* **Friendlier Bilibili errors**: HTTP 412/567 search/play errors map to a bilingual hint suggesting
  another source or a retry.
* **About**: "Made by nilotoo." credit added (icon MIT-0 credit kept).

---

## 🩹 Fixed in v0.1.2

* **Consistent cover corners**: real artwork, generated placeholder covers, playlist covers, sidebar mini cover,
  player bar, cards, rows and Now playing now share the same rounded geometry and clipping; placeholder
  gradients/letters are clipped to the rounded shape — no more square placeholder inside a rounded card.
* **Row play/pause icons fixed**: the icon-set play triangle is rotated +90° to point right, and every glyph is
  fitted into a centred square preserving aspect ratio (no more stretched "A"); pause stays a centred square.
  Consistent across hover / current / paused / playing / loading states, menus, player bar and Now playing
  controls at 100–150% UI scale.
* **Player bar alignment**: the transport + scrubber are centred on the **content column** (sidebar excluded) and
  match the Now playing page's order/spacing/button sizes; the play-button x position matches Now playing at
  1200×800 and maximized (measured Δ ≈0.1 px / 0.0 px).

---

## 🩹 Fixed in v0.1.1

* **Apple Music / Spotify full playback**: full-length public audio is now matched via yt-dlp first;
  the official 30 s preview is only a fallback. The services' own streams remain DRM-protected — not bypassed.
* **Consistent player state**: switching tracks stops the previous audio immediately; player bar, sidebar
  mini card and Now playing agree on the current track / loading state; the progress bar advances while playing.
* **Now playing restored to the original minimal design**: plain theme background (no cover tint,
  glow, blurred wallpaper, floating decoration or continuous animation); responsive at 2560×1600 and 980×620.
* **Interface scale**: Settings now offers 100% / 110% / 125% / 150% (persisted); layout/boundary audit done
  for HiDPI and maximized windows.
* **Covers & search bar**: deterministic generated placeholder covers redesigned (no mystery ring);
  the Search button no longer gets clipped when the field contains text / shows the clear button.
* **Profile**: listening-history rows no longer overflow or clip the rounded card.
* **Background tab**: Wikipedia fallback opens the normal `/w/index.php` search page (no more API help page),
  plus new MusicBrainz artist-search and YouTube interview-search fallbacks.
* **Caching**: startup auto-prune of the streamed-audio cache (2 GiB default, oldest-first, files touched in
  the last 120 s protected); cover texture memory cache capped at 96 entries (LRU, ≤320 px); user download
  files are never auto-deleted.
* **Lua scripting/plugins**: config + plugin system, Settings "Scripting & plugins" card, custom keybindings
  and actions (see docs/SCRIPTING.md).

---

## 🗺️ Roadmap

- [ ] Lyrics (local `.lrc` / public sources, optional)
- [ ] `.m3u` playlist import/export; richer library (sorting, album view, search)
- [ ] System media controls (MPRIS / media keys) and a tray icon
- [ ] Gapless playback / crossfade and finer volume control
- [ ] Official Windows / macOS packages (signing, `.app` bundle, installer)
- [ ] More catalogue sources (e.g. Deezer as a search fallback) and configurable sources
- [ ] Portable mode (data next to the executable) and profile import/export
- [ ] Continued accessibility and keyboard-navigation improvements

> Issues and PRs are welcome; the project keeps its "no accounts, no DRM bypass, local-first" rules.

## 🧑‍💻 Developers

```bash
cargo check                      # fast type check
cargo test                       # unit + integration tests (persistence / i18n / theme / playlists / model)
cargo build --release            # release build (LTO, strip, panic=abort)
./scripts/build.sh               # same, with environment checks and a size report
./scripts/package-linux.sh       # build dist/echoport-<version>-linux-x86_64.tar.gz
./scripts/run-demo.sh            # regenerate all screenshots (screenshots/qa-*.png) in demo mode
python3 scripts/gen-icon.py      # regenerate assets/icon.png (256) and icon-128.png
./scripts/qa-cache-prune.sh      # QA: verify the audio-cache auto-prune policy with fake files
cargo test --test live_network -- --ignored --nocapture   # QA: live Apple/search/background regression tests
```

Lua config template: `examples/config.lua`; plugin sample:
`examples/plugins/example.lua`; reference: [Scripting](docs/SCRIPTING.md) and
[Plugins](docs/PLUGINS.md).

Built-in demo / screenshot mode (no network, no audio device required):

```bash
ECHOPORT_DEMO=1 cargo run
ECHOPORT_DEMO=1 ECHOPORT_SHOT=$PWD/screenshots/demo.png ECHOPORT_PAGE=home cargo run
# ECHOPORT_PAGE: home|search|library|playlists|downloads|now_playing|profile|settings
# ECHOPORT_THEME: pulse|tide|ink（旧值 blue_white|midnight|album 仍兼容）
# ECHOPORT_FOLLOW_COVER=1 强调色跟随封面；ECHOPORT_LIVE=1 截图时保留真实网络路径
```

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for modules, threading and data flow, and
[docs/QA_REPORT.md](docs/QA_REPORT.md) for the verified release numbers. Do not hard-code
user-visible strings in `src/**`: everything must go through `i18n::t` (English + Chinese).

## 📚 Documentation

* [Usage guide](docs/USAGE.md) — first run, import, playlists, downloads, shortcuts, troubleshooting
* [Providers](docs/PROVIDERS.md) — per-platform capabilities, limits and the no-DRM rationale
* [Architecture](docs/ARCHITECTURE.md) — modules, threading, data flow, persistence schema
* [Scripting](docs/SCRIPTING.md) · [Plugins](docs/PLUGINS.md) — Lua configuration and extensions
* [QA report](docs/QA_REPORT.md) — build, tests, performance and screenshot evidence

## UI icon credits

In-app UI icons (navigation, actions, player controls) use the icon set supplied by the
project owner: author **Singular Programmer**, licence **MIT-0**. Assets and licence live in
[`assets/icons/`](assets/icons/README.md) (black/white PNG+SVG sets; the white 48 px variants
are embedded in the binary and tinted per theme). The app/launcher icon (`assets/icon.png`) is
still the original EchoPort brand icon and has not been swapped yet.

## License

GPL-3.0 © EchoPort contributors. See [LICENSE](LICENSE).

**Made by nilotoo.**
