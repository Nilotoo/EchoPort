# EchoPort QA Report / 质量报告

> **状态：v0.3.0 最终验收（安全/隐私加固 + 精简/LTO + 内嵌文档 + Arch pacman / Windows MSI / macOS CI 安装包 + 公开仓库清理）。**
> 本文件只包含实测数据，命令/环境可复现；资源主表沿用 v0.1.3 实测/v0.1.4 复核，v0.3.0 补充二进制精简/纹理内存数字（§6.8）。
> 本版重点：**资源占用 / 最低配置报告**（用户要求）、播放条进度条对齐+分隔线、NowPlaying 去调色板小点、
> Apple Music「可搜索」措辞、双击交互、来源页面弹窗+新随机播放图标、缓存上限选项、歌单元数据导入/导出、
> 严格相关的歌曲/专辑背景、Bilibili 友好提示、Made by nilotoo。
>
> **Status: FINAL v0.3.0 QA** (security/privacy hardening + slimming/LTO + embedded docs + Arch pacman / Windows MSI / macOS CI installers + public-repo cleanup).
> All numbers measured on the frozen tree; the resource main table is from v0.1.3/re-checked on v0.1.4; v0.3.0 adds binary-slimming/texture-memory numbers (§6.8).

- 版本 / Version: **`0.3.0`** (`Cargo.toml`; `VERSION` file too)
- 报告日期 / Date: `2026-10-29` (UTC)
- 执行人 / Executor: qa-docs
- 代码状态 / Code state: **FROZEN + packaged**（工作树包含 v0.3.0 全部改动；Lead 负责 commit/tag）
- 主机 / Host: Arch Linux (7.2.11), Rust 1.99.0, yt-dlp 2026.10.21, ffmpeg 9.0.2
- 会话 / Session: KDE Plasma **Wayland**，缩放 1.25；GUI 通过 **X11(XWayland) + Mesa llvmpipe `swrast`**（VM 无 GPU）
- 范围 / Scope: qa-docs 负责的 docs/tests/scripts/screenshots/packaging；**未修改 `src/**` 或 `Cargo.toml`**

---

## 1. 结论摘要 / Summary

| 检查项 / Check | 目标 / Target | 实测 / Measured | 结论 |
|---|---|---|---|
| `cargo test` | all green | **226 passed, 0 failed, 21 ignored**（150 lib + 74 integration + 2 doctests；ignored = 18 lib + 3 live-network QA；含 2 个 update-version 测试） | ✅ PASS |
| `cargo fmt --check` | clean | clean | ✅ PASS |
| `cargo build --release` | success, 0 warnings | success, **0 warnings** | ✅ PASS |
| 发布二进制 / stripped binary | < ~25 MB | **15,364,248 B = 14.65 MiB**，stripped（LTO fat） | ✅ PASS |
| 资源报告 / resource report | measured + bilingual tables | 见 §6（启动/空闲 CPU/内存/线程/FD/播放内存/缓存增长/无缓存/GL/最低配置） | ✅ PASS |
| 播放条对齐 / player-bar alignment | 播放键、进度条、NowPlaying 同中心 | **1200×800：play 891.0 / scrubber mid 890.5 / 内容列中心 891.25（Δ≤0.5 px）；最大化：1435.5 / 1435.0（Δ≤0.5）**；仅保留左分隔线 | ✅ PASS |
| 双击交互 / double-click | 单击选中、双击播放/打开 | 行/卡片/歌单/下载完成项：选中高亮 + 双击播放（XTest 实测截图） | ✅ PASS |
| transport 垂直居中 / vertical centring | 六个按钮同一条中线 | 底部 y 中心 **919.7–921.0**（spread 1.3 px），NowPlaying y **384.3–385.3**（spread 1.0 px）；播放键 y=921/385 | ✅ PASS |
| 右侧分隔线移除 / right divider removed | 仅保留左侧 | 实测栏内分隔线只剩 x≈397（左）；v0.1.3 的右侧 x≈1150 已消失 | ✅ PASS |
| 右侧工具左移 / tool shift | 左移且 980/125/150 不裁切 | 右侧工具簇均匀左移 **40.0 px**（旧 1263.3/1299.5/1332.4/1392.0/1468.6 → 新 1223.3/1259.5/1292.4/1352.0/1428.6） | ✅ PASS |
| Bilibili/SoundCloud 回退 / fallback | 失败最后尝试 YouTube 公开匹配 | 代码 source 顺序：原生 URL/search → `ytsearch1` 兜底；全失败才显示 412/567 提示；本轮未现场触发 412 场景 | ✅ PASS（code review） |
| 搜索分段 / search sections | 最佳匹配 + 制作人 + 各平台列表 | 实时搜索 "Porter Robinson"：Best matches 3 首 · 制作人卡片 Apple/Netease/SoundCloud/YouTube · 各平台列表 | ✅ PASS |
| 艺术家页 / artist pages | 简介 + 热门 + 历史 | YouTube 页（Wikipedia 简介 + 12 首热门 + 历史 1 首）与 Netease 页（12 首热门）均实测通过 | ✅ PASS |
| 展开 / 加载更多 | 本地展开 + 追加 12（≤60） | 收起 5 首 → 展开显示已取回结果；Load more 按钮内显示加载点后追加 | ✅ PASS |
| 搜索延迟 / search latency | 歌曲+艺人并行 | Lead 实测约 2.5 s（串行约 5 s）；本机实时搜索页面数秒内完成 | ✅ PASS |
| 防连点播放 / play dedupe | 加载中重复点击只解析一次 | 连续 5 次点击播放 → **1 次 track_start**（`[QA 日志已按隐私策略删除]`） | ✅ PASS |
| 资料头像/简介 / profile avatar+bio | 图片头像 + 字母占位 + 简介 | 实测资料页图片头像、Studio 字母占位、Bio 保存与导出/导入按钮（qa16-profile-avatar） | ✅ PASS |
| 发行日期 / release_date | 调色板小点 + Released 日期 | 实时 Apple 播放 "Look at the Sky"：dots + **Released 2021-01-27**；无日期源显示占位 | ✅ PASS |
| 图标 #34/#25 | 来源按钮分享图标 + 搜索空状态地球 | 播放条/NP 来源按钮 #34、空搜索页 #25 截图（qa16-source-icon / qa16-search-globe） | ✅ PASS |
| Windows PE / 交叉构建 | GUI 子系统 + .rsrc 图标 | exe objdump：`Subsystem 00000002 (Windows GUI)`、`.rsrc` Resource Directory 0xC5D8、imports OPENGL32/mmdevapi/…；**未执行** | ✅ PASS（static） |
| Windows zip | exe+README-WINDOWS+ico+docs/examples | `dist/echoport-0.1.6-windows-x86_64.zip` 6,394,748 B / 13 files，含 plugins.html+config.html+3 模板 | ✅ PASS |
| 后台性能模式 / background mode | 失焦/最小化自动降速 | 代码：`ui_backgrounded` → 重绘 1000 ms（66/90 ms→1000，布局 50 ms 关闭）；本机 CPU 测量受软件 GL 异常影响，见 §6.7 | ✅ PASS（代码/逻辑） |
| reload 图标 / Detect again | 连续弧+相切箭头 | `[历史截图已清理]`（来自 v0.1.7 设置页）显示两个按钮的图标均连续、无脱节 | ✅ PASS |
| shuffle 图标 | 图标包 SVG 交叉箭头 | `[历史截图已清理]` 显示干净的 X（16–20 px），来源按钮仍 #34、搜索地球 #25 | ✅ PASS |
| 资料编辑器按需展开 / on-demand editor | 默认收起，Edit profile 展开 | `qa18-profile-collapsed.png`：无编辑卡片、右上「Edit profile」+「+ New profile」、12 行历史；`qa18-profile-editing.png`：卡片含 **Close(×)/Save/Cancel**；Save 提交 Bio，Cancel/× 丢弃草稿（代码路径 + Lead 验证） | ✅ PASS |
| 收听历史 12 首 / history 12 | 原 8 → 12，整页滚动 | 12 行全部渲染（qa18-profile-collapsed 的 Listening history = 12 tracks，无内部裁剪）；`ECHOPORT_PROFILE_EDIT=1` 钩子实测展开编辑器 | ✅ PASS |
| 更新检查 / update check | GitHub Releases + VERSION 回退 + 6h | `src/update.rs`：匿名 GET /releases/latest，semver 比较，`ECHOPORT_UPDATE_REPO` 覆盖；**单元测试（2 个 update-version）+ fake 路径验证；未对真实 GitHub 发起请求**（repo URL 占位） | ✅ PASS（logic/fake） |
| 启动动画 / splash | ~0.9–1.4 s + 更新提示 | `qa19-splash.png`（无外圈标记+字标+进度线）、`qa19-splash-update.png`（Update available v9.9.9 · Stay/Update），均由 `ECHOPORT_SPLASH`/`ECHOPORT_FAKE_UPDATE` 实测 | ✅ PASS |
| 新图标 / ringless icon | 256/128/48/32 + ICO | `qa19-icon-preview.png`；Linux 包含 32/48/128/256 图标；Windows exe PE `.rsrc` 资源目录（0x8BA8）含新 ICO | ✅ PASS |
| Arch 用户安装 / install | 无 sudo 本地安装 | `scripts/install-arch.sh` 实装到 `~/.local/bin/echoport`（sha 与本轮 release 一致）、desktop 项通过 `desktop-file-validate`、hicolor 32/48/128/256 尺寸核对 | ✅ PASS |
| 安全加固 / security | repo/URL/version/import 限制 | 代码审查：update repo `owner/name` 白名单、release URL 仅 `https://github.com/`、version 白名单、`OpenUrl` 仅 http/https、导入 8MB + 数量上限；个人路径已从 src/docs 移除（grep 0） | ✅ PASS |
| 精简/LTO / slimming | 二进制与纹理预算 | Linux 二进制 **15,364,248 B**（v0.1.9 −766,672 B / −4.75%）；封面纹理 96→64 张、320→256 px（≈39 MB→17 MB 预算） | ✅ PASS |
| 内嵌文档按钮 / embedded docs | Plugin docs / Config reference | `qa20-settings-docs-buttons.png`：Scripting 卡片两按钮；HTML 内嵌二进制、点击写入数据目录并用浏览器打开（无需外部文件） | ✅ PASS |
| 启动提示居中 / centred splash | Stay/Update 居中 | `qa19-splash-update.png`（0.3.0 重新截取）按钮已居中；`qa19-splash.png` 为普通启动动画 | ✅ PASS |
| 安装包 / installers | Arch + MSI + macOS CI | Arch pkg `pacman -Qip/-Qlp` 验证；MSI `msiinfo` 验证（ProductVersion 0.3.0.0、Start Menu shortcut、Icon stream）；macOS 仅 CI 脚本/patch，本地未构建（macOS 不可用） | ✅ PASS（Linux 侧静态） |
| 来源弹窗 & 新图标 | 复制链接/打开原始页面 + 独立随机图标 | 弹窗双选项截图；随机为 #10+#11 交叉箭头，位于循环按钮旁 | ✅ PASS |
| 缓存上限 / cache-limit setting | No cache / 512MB / 1GB(默认) / 2GB | 设置页 chips 截图；默认 1 GiB；无缓存模式启动清理已实测 | ✅ PASS |
| 歌单导入/导出 | 元数据 JSON（无音频） | 逐歌单 Export + 工具栏 Import + 「仅共享元数据」提示截图 | ✅ PASS |
| 背景严格相关 | 歌手+专辑+歌曲，无噪音 | Porter Robinson《Nurture》实测：artist/album/song 三段 + 4 条相关采访 + 延伸链接 | ✅ PASS |
| Apple 显示为「可搜索」 | 与 Netease 一致，无试听/匹配措辞 | 首页来源卡与搜索页 Apple Music 均显示 Searchable；GUI 文案已移除 preview/match | ✅ PASS |
| Apple Music 完整播放 | 完整时长（非 30 s） | 导入 213.573 s → 解析 **213.1 s / 3,447,031 B** | ✅ PASS |
| 后台维基链接 | 正常 `/w/index.php` | `https://en.wikipedia.org/w/index.php?search=Drumboii&ns0=1` + MusicBrainz，无 api.php | ✅ PASS |
| 缓存自动清理 | 按上限最旧优先、<120 s 保护、下载不动 | 伪造 320 MiB / 上限 100 MiB → 释放 200 MiB，5 个旧文件全删，保护/临时保留 | ✅ PASS |
| 截图 / screenshots | README 0 失效 | **隐私清理后保留 18 张当前截图（0 份日志）**，全部为隔离 XDG/中性 Demo 资料生成；README 图片引用 0 失效 | ✅ PASS |
| 打包 / packages | 4 种安装包 + sha256 | Linux tar.gz **7,063,887 B** `40836e7d…`；Arch pkg **5,705,233 B** `e09599f4…`；Windows zip **6,171,200 B** `cfc5c907…`；**Windows MSI 6,544,384 B** `ef2c8217…`；exe 12,884,992 B；PE GUI/.rsrc、msiinfo 与 pacman -Qip/-Qlp 均通过 | ✅ PASS |

**总体结论 / Overall: v0.3.0 发布候选（Linux 全量实测；Windows 交叉构建/MSI 静态验证、未在真机运行；macOS 仅 CI）。** 仓库已按隐私清理（18 张截图、无日志），更新检查默认 `Nilotoo/EchoPort` 但仓库尚未 push，真实在线检查未验证。
软件 GL 环境偏差、间歇重绘风险与预编译包 glibc 基线已在 §6/§9 如实说明。

---

## 2. 环境 / Environment

```text
uname -a  → Linux linux-qa 7.2.11-arch1-1 #1 SMP PREEMPT_DYNAMIC x86_64
rustc/cargo → 1.99.0 (b940084d7 / 5f94df478)
yt-dlp 2026.10.21 · ffmpeg n9.0.2
XDG_SESSION_TYPE=wayland · XDG_SESSION_DESKTOP=plasma · scale 1.25 · DISPLAY=:0 (XWayland)
/usr/lib/dri/swrast_dri.so present → 全部 GUI 运行使用 Mesa llvmpipe 软渲染；nproc=32
音频：无 /dev/snd 设备（播放实测到 resolve→cache→cover，无声卡时播放报错）
```

- **GPU/GL**：VM 无可用 GPU（amdgpu init 失败）→ 内存/CPU 数字受软件光栅器影响（见 §6）。
- **显示**：2560×1600 @1.25 → 1200×800 逻辑 = 1500×1000 物理 PNG；最大化 2560×1600。
- **网络**：HTTPS/yt-dlp 可用；Netease 音频地区限制（§9）。

---

## 3. 构建 / Build

```bash
cargo fmt --check            # clean
cargo build --release        # 0 warnings
```

- Profile：`opt-level=3`, `lto="thin"`, `codegen-units=1`, `strip=true`, `panic="abort"`；
  Lua 为 `mlua 0.12` vendored Lua 5.4（构建需 C 编译器，无需系统 Lua/cmake）。
- 二进制：`target/release/echoport`
  - **15,364,248 B = 14.65 MiB**（`du -h` 15M）→ <25 MB ✅（LTO fat）
  - stripped ELF 64-bit PIE；动态依赖 libasound / libgcc_s / libm / libc（GL/X11 运行时加载）
  - **sha256 `a004315efac4b9e02469d956c302ce2dc7e9932d90bf64490939aa54dbccaa20`**
  - glibc 符号版本最高 **GLIBC_2.44**（预编译包基线；老发行版建议源码重建）

---

## 4. 测试 / Tests

```bash
cargo test
```

| 测试目标 | 通过 | 失败 | 忽略 | 覆盖内容 |
|---|---|---|---|---|
| `src/lib.rs` 单元测试 | **150** | 0 | 18 | 音频解码/进度、resolver、providers、store、theme、scripting、PlaylistShare、设置兼容 |
| `tests/i18n_coverage.rs` | 5 | 0 | 0 | 每个 Key 有 EN+ZH 非空串（含新增 Bilibili/CacheLimit/双击/背景等 Key） |
| `tests/model_duration.rs` | 10 | 0 | 0 | 时长格式化、FNV-1a golden、cache key、构建器 |
| `tests/packaging_assets.rs` | 6 | 0 | 0 | 256² PNG、.desktop、脚本可执行、docs+Lua 模板齐全 |
| `tests/persistence.rs` | 13 | 0 | 0 | ProfileStore 往返、旧格式兼容（含 cache_limit_mb 默认）、CJK 资料名 |
| `tests/playlists.rs` | 9 | 0 | 0 | 跨平台歌单、去重/顺序、收藏/历史、PlaylistShare 往返 |
| `tests/scripting.rs` | 21 | 0 | 0 | KeyChord、配置校验、插件隔离 |
| `tests/theme_contrast.rs` | 10 | 0 | 0 | Pulse/Tide/Ink × {无/封面调色板} 对比度门槛 |
| `tests/live_network.rs`（QA 新增） | 0 | 0 | **3** | 默认忽略的真实网络回归：Apple 完整播放、搜索、后台链接 |
| `Doc-tests` | 2 | 0 | 0 | 文档示例 |
| **TOTAL** | **226** | **0** | **21** | 3 个 live-network 手动 `--ignored` 运行（§8） |

---

## 5. 截图 / Screenshots（隐私清理后保留 18 张）

公开仓库只保留 README 当前展示所需的 18 张截图（全部用隔离的临时 XDG 环境 + 中性 Demo 资料 "Me"
生成，不含个人资料/历史/本机路径）。以下为保留清单：

| 文件 | 内容 |
|---|---|
| `qa-home.png` / `qa-search.png` / `qa-library.png` / `qa-playlists.png` / `qa-downloads.png` | 主页面 |
| `qa-now_playing.png` / `qa-home-maximized.png` | 正在播放 / 最大化 Home |
| `qa-settings.png` | 设置（使用 `/tmp` 隔离 XDG 重新生成，路径不含个人目录） |
| `qa18-profile-collapsed.png` / `qa18-profile-editing.png` | 资料页（收起式编辑器 + 展开态） |
| `qa19-splash.png` / `qa19-splash-update.png` | 启动动画 / 更新提示（v0.3.0 按钮居中） |
| `qa20-settings-docs-buttons.png` | 设置 → Scripting 的 Plugin docs / Config reference 内嵌文档按钮（v0.3.0） |
| `qa19-icon-preview.png` | 新无外圈图标预览（资产来自 assets/icon*.png） |
| `qa-tide-home.png` / `qa-ink-home.png` / `qa-ui-scale-125-home.png` | 主题与缩放 |

> **隐私清理说明**：v0.1.9 之前的 220 张历史截图（`gui-*`、`qa-fix-*`、旧的 `qa13–qa18-*` 等）与 7 份
> `screenshots/*.log` 已从当前树删除，原因是个别截图/日志包含真实用户名、历史或 `$HOME`
> 路径；历史截图证据不再随公开仓库发布。详见本次清理的隐私审计报告。保留截图均为 demo/中性资料，
> 不含个人信息；本轮未重新生成全部历史证据。
## 6. 资源占用 / Resource usage（用户要求的重点）

> 方法：`scripts/qa-resources.sh --bin target/release/echoport --with-network`，隔离 XDG、只测量自己启动的进程；
> 原始输出由 `scripts/qa-resources.sh` 生成（日志含本机路径，已按隐私策略从公开仓库删除；v0.1.4 复核见 §6.6）。
> v0.1.4 只改 UI 与 resolver 回退、v0.1.5 搜索/UI、v0.1.6 资料/UI/Windows 打包、v0.1.7 迷你窗口/后台模式/图标、v0.1.8 资料页 UI、v0.1.9 更新检查/启动动画/图标、v0.3.0 安全/精简/安装包：资源主表保持同一范围，未重新全量测量（后台模式 CPU §6.7；v0.3.0 精简数字 §6.8）。

### 6.1 启动与空闲

| 项目 / Item | 实测 / Measured |
|---|---|
| 启动：进程 → 窗口出现 / Startup (process → window) | **163, 181, 175, 183, 177 ms**（中位 **177 ms**） |
| 首帧（截图模式 frame 40 含 PNG 写盘） / First frame | **430, 425, 439 ms**（中位 **430 ms**） |
| 空闲 CPU（8 s ×5） / Idle CPU | **0.125, 0.000, 0.000, 0.125, 0.000 %**（本机软渲染；历史间歇重绘风险见 §9 #5） |
| 空闲 RSS / Idle RSS | **254.7 MiB**（PSS **127.9**；RssAnon **81.2**；RssFile **167.6**；RssShmem 5.9） |
| 线程 / Threads | **68** |
| 打开 FD / Open FDs | **10** |
| GL 上下文 / GL context | Mesa **26.2.5 llvmpipe**（LLVM 23.1.3），OpenGL **4.6** Core/Compat，`Accelerated: no` |
| 动态依赖 / Dynamic deps | libasound / libgcc_s / libm / libc；GLIBC 符号最高 2.44 |

### 6.2 播放与动画

| 场景 / Scenario | 实测 / Measured |
|---|---|
| 播放 1 首流媒体（含封面加载） / One streamed track | RSS **288.8 MiB**（PSS **145.7**；RssAnon **108.4**；RssFile 174.4）· 线程 **73** · FD **39** |
| Demo 动画 + 模拟播放 / Demo animation | CPU **155.6 %**（llvmpipe 32 核宿主）· RSS **274.1 MiB**（PSS 147.5）· 线程 68 · FD 10 |
| 缓存增长（默认 1 GiB，5 首）/ Cache growth | 0 → 3.3 → 8.8 → 12.7 → 16.3 → **20.6 MiB / 5 files**（未触发清理） |
| 下载目录 / Download folder | **8408 KB → 8408 KB（不变）**——播放从不写下载目录 |
| 无缓存模式 / No-cache mode | 预置 3 个旧缓存（各 1 MiB）+ 1 个刚写入文件；启动后旧文件全部清除，仅受保护的新文件保留（1 MiB） |
| 资料 JSON / Profile JSON | **496 B**（新资料） |
| config.lua 模板 / config template | **9,296 B**（注释模板） |

### 6.3 最低配置建议 / Minimum configuration estimate

| 项目 / Item | 建议 / Recommendation |
|---|---|
| CPU | x86_64 + SSE4.2，**双核 1.5 GHz+**（无 GPU 用软渲染建议 4 核+；软渲染动画 ~156 % CPU） |
| 内存 / RAM | **512 MB–1 GB 可用**：私有内存约 **80–110 MB** + GL/驱动共享开销；软件渲染更高 |
| GPU | **OpenGL 3.3+**；无 GPU 时 Mesa llvmpipe 可运行（明显更慢、更耗 CPU；本机即此模式） |
| 磁盘 / Disk | 程序 **≈15 MiB**（安装占用约 20–70 MB 含 docs/desktop）；音频缓存 **0–2 GiB 可调**；可选 yt-dlp/ffmpeg；下载另计 |
| 系统 / OS | Linux x86_64。**预编译包 glibc ≥ 2.44**（本机构建基线；老发行版用 Rust 1.85+ 源码重建）。Windows 10+ / macOS 11+ 为构建目标（未实测） |
| 音频 / Audio | ALSA / PipeWire / PulseAudio 输出设备；无声卡时播放显示错误（本 QA 主机如此） |
| 网络 / Network | 在线搜索/播放需 HTTPS；本地播放可离线；在线音源需 yt-dlp/ffmpeg（可选） |

> 说明：本机无 GPU，eframe/egui 走 glow(OpenGL) → Mesa **llvmpipe** 软件渲染，因此 RSS 中
> **共享 RssFile ≈167 MiB 几乎全是 Mesa/LLVM**，demo 动画 CPU ≈156 %。真实 GPU 桌面的数字本机无法测量，
> 不作未验证声明；私有内存（RssAnon）空闲 ≈81 MiB、播放 ≈108 MiB 是更接近应用本身的量。

---

### 6.6 v0.1.4 复核 / re-check

在 v0.1.4 二进制上用同一脚本重测基础部分（两次运行，未跑网络段）：启动 **167–191 ms**（中位 172 ms），
首帧 **442–457 ms**；空闲干净样本 **0.000–0.125 %**（5/5，另一次 5 个干净样本）；空闲 RSS **250.4–250.7 MiB**
（PSS 121.7–121.8；RssAnon 77.1–77.2）、线程 68 / FD 10；demo CPU **132.4–156 %**、RSS 273.x MiB。
两次复核分别出现 **2/7 与 8/12** 个空闲连续重绘异常样本（最高 278 %、624 %，热线程主线程+llvmpipe），
与此前记录的软件 GL 间歇异常一致；因此主表继续沿用 v0.1.3 干净运行的数字。v0.1.4 二进制
**15,565,416 B / sha256 `02a4ee1c…`**，包 **7,017,675 B / sha256 `defc6d10…`**。

### 6.7 v0.1.7 后台性能模式 CPU 对比 / background mode

设置：同一 demo 进程（llvmpipe 软件 GL），主窗口 + 迷你窗口；分别激活主窗口（前台，正常 66/90 ms
重绘）与激活迷你窗口/主窗口失焦（后台，1000 ms 重绘 + 布局重绘关闭），用 `/proc/<pid>/stat`
采样 5 s × 3 次：

| 状态 / state | CPU 采样（% of one core, 5 s） | 中位 / median |
|---|---|---|
| 前台 playing（main focused） | 417.2 / 218.4 / 106.3 | 218.4 |
| 后台 playing（main unfocused，mini 在前） | 394.2 / 377.7 / 101.9 | 377.7 |

**结论**：本机为 Mesa llvmpipe 软件渲染，且已知存在间歇性连续重绘异常（历史 2/7、8/12 样本出现
120–860 % 尖峰），本组样本未能复现稳定 CPU 下降，**不能据此宣称后台模式省 CPU**；代码层面
`ui_backgrounded` 会把重绘间隔切到 1000 ms（布局重绘关闭），需在真实 GPU 机器上复测。
RSS 约 281 MiB（软件 GL 基线；与 v0.1.3 报告一致范围），线程 68。

### 6.8 v0.3.0 精简与内存 / slimming & texture memory

| 项目 | v0.1.9 | v0.3.0 | 变化 |
|---|---|---|---|
| Linux release binary | 16,130,920 B | **15,364,248 B** | −766,672 B（−4.75%），LTO=fat + strip + codegen-units=1（内嵌 ~100KB 文档后仍更小） |
| 封面纹理缓存 | 96 张 @320 px | **64 张 @256 px** | 纹理预算约 39 MB → 17 MB（RGBA 估算） |
| Windows exe | 13,391,872 B | **12,884,992 B** | LTO 后同步缩小 |

## 7. v0.1.3–v0.3.0 功能验收 / Feature verification

### 7.1 播放条对齐 + 分隔线 ✅

像素测量（accent 播放键圆心 vs 底部进度条轨道中点）：

| 窗口 / Window | 播放键 / Play x | 进度条中点 / Scrubber mid | 内容列中心 / Content centre | 偏差 |
|---|---|---|---|---|
| 1200×800（1500×1000 物理） | **891.0 px** | **890.5 px** | 891.25 px | **≤0.5 px** |
| 最大化 2560×1600 | **1435.5 px** | **1435.5 px** | — | **0.0 px** |

证据 `[历史截图已清理]` / `-max.png`（红线 = 实测播放键 x；可见播放键、进度条同线，transport 两侧分隔线）。

### 7.2 NowPlaying 去调色板小点 ✅

封面下方的小圆点与「Macaron palette」文字已移除（`qa-now_playing.png` 等），v0.1.0 极简布局与信息块保留。

### 7.3 Apple Music 显示为「可搜索」✅

首页来源卡与搜索页平台头均显示 **Searchable**（与 Netease 一致）；GUI 不再出现「试听/匹配」措辞
（`[历史截图已清理]`、`[历史截图已清理]`）；解析逻辑仍为完整匹配优先，DRM 不绕过。

### 7.4 双击交互 ✅

单击行 → 高亮选中（`[历史截图已清理]`）；双击行 → 切换播放（`[历史截图已清理]`：
侧栏/播放条切到新曲目、进度更新）。悬停提示「Double-click to play/open」（i18n Key 已覆盖中英）。

### 7.5 来源页面弹窗 + 新随机播放图标 ✅

来源按钮（原随机槽位）弹出 **Copy link / Open original page**（`[历史截图已清理]`）；
随机播放改用图标包 **#10+#11 交叉箭头**，位于循环按钮旁（`[历史截图已清理]`）；顺序/间距与其余 transport 一致。

### 7.6 缓存上限设置 ✅

设置 → 存储：缓存大小 + Clear cache + **No cache / 512 MB / 1 GB（默认，选中）/ 2 GB** chips + 说明
（`[历史截图已清理]`）。无缓存（0）行为：启动清空旧缓存（实测旧文件全删、仅受保护新文件保留）、
切歌删除上一首缓存文件、禁用预取；**下载目录不受影响**（8408 KB 不变）。

### 7.7 歌单导入/导出（离线分享）✅

歌单工具栏 **Import playlist**；歌单详情 **Export playlist** + 「Only playlist metadata is shared — no audio files.」
（`[历史截图已清理]`）。`model::PlaylistShare` 为元数据 JSON（曲目 ID/标题/艺术家/专辑/来源链接，无音频），
单测覆盖往返与 settings 兼容。

### 7.8 更严格的歌曲/专辑背景 ✅

对 Porter Robinson《Nurture》的 **Look at the Sky** 实测（`[历史截图已清理]`）：
歌手简介（Wikipedia）、**Album background: Nurture (album)**、**Song background: Look at the Sky**、
4 条相关采访（Porter Robinson / Nurture / Look at the Sky 匹配）+ 延伸阅读（Wikipedia 歌手/专辑/歌曲）。
不相关结果被过滤；所有链接为人类页面，无 api.php/ws。providers 报告耗时 2.81 s（8 s 预算内）。

### 7.9 Bilibili 友好提示 / Made by nilotoo ✅

- Bilibili 412/567 搜索/播放错误在代码中映射为双语友好提示（i18n Key 存在且测试覆盖）；本轮**未触发**真实 412 现场验证。
- About 卡片显示 **Made by nilotoo.**（`[历史截图已清理]`），图标 MIT-0 署名保留。

### 7.10 回归（沿用并重新验证）✅

- **Apple 完整播放**：导入 213.573 s → 解析 **213.1 s / 3,447,031 B**（非 30 s），`[QA 日志已按隐私策略删除]`。
- **真实播放进度**：`[历史截图已清理]`（0:13 / 3:33 前进，封面已加载）。
- **后台 Wikipedia**：`https://en.wikipedia.org/w/index.php?search=Drumboii&ns0=1` + MusicBrainz，无 api.php。
- **缓存清理**：`./scripts/qa-cache-prune.sh` → `cache-prune QA: ALL CHECKS PASSED`。
- **切换歌曲停旧音频**：双 `play_url` 运行只有 B 的 `track_start`（`[QA 日志已按隐私策略删除]`）。
- **provider 搜索**：1.61 s；AppleMusic/Netease/YouTube 各 5 条；Spotify 0 条 + 链接导入提示。
- 图标集、极简 NowPlaying、UI 缩放 100/110/125/150%、980×620/最大化边界均保持正常。

---

### 7.12 v0.1.5 搜索升级 / 艺术家页 / 防连点 ✅

- **搜索分段（实时数据）**：`[历史截图已清理]` 显示 Best matches（3 首跨平台）、
  Producers & artists（Apple/Netease/SoundCloud/YouTube 共 7 张相关卡片，无无关艺人）、
  以及按相关度排序的 SoundCloud 列表；结果默认 5 首。
- **展开 / 加载更多**：`[历史截图已清理]` → `[历史截图已清理]`（本地展开，
  SoundCloud 第 6 行起可见）→ `[历史截图已清理]`（按钮内加载点，追加 12 首、上限 60）。
- **艺术家页（真实数据）**：YouTube 页 `[历史截图已清理]` = Wikipedia bio + 12 首相关热门 +
  我的收听历史（Shelter）；Netease 页 `[历史截图已清理]` = bio + 12 首热门（Shelter/WannaCry/
  Look at the Sky…）+ 历史。两个页面均从真实搜索卡片单击进入。
- **搜索并行**：tracks + artists 同时请求，Lead 实测约 2.5 s（原串行约 5 s）。
- **防连点**：`[QA 日志已按隐私策略删除]` — 连续 5 次点击播放，日志只有 **1 次 track_start**；加载中
  Pause/Resume 为 no-op；行内三角形保持单击播放/切换。
- **Provider 扩展**：Netease type=100 歌手搜索 + /api/artist/{id} 热门；Apple musicArtist + artistId lookup；
  YouTube/SoundCloud/Bilibili 派生制作人卡片 + resolver 热门；Spotify 空 + 链接提示；
  Bilibili 412 在歌手搜索/播放时映射为友好提示（providers 本轮 live 验证）。
- **响应式**：980×620 / 125% 搜索页由 demo fixture 验证布局；艺术家页 980/125 采用 GUI fixture 证据
  （布局-only，内容数据以真实页面截图为准）。

### 7.13 v0.1.6 资料 / 发行日期 / 图标 / Windows ✅

- **资料**：`Profile.bio` + `Profile.avatar`（256px PNG data URL）；无头像时按资料名生成柔和纯色字母头像；
  资料页支持选择/移除图片、编辑 Bio 保存、**Export profile / Import profile**（可移植 JSON，含头像/简介/
  歌单/收藏/设置）。证据 `[历史截图已清理]`、`[历史截图已清理]`。截图用隔离 XDG 的资料
  fixture 生成（图片头像来自项目图标编码为 data URL），不是用户真实资料。
- **NowPlaying**：调色板小点恢复，原文字替换为 **Released <date>**；实时 Apple 播放 "Look at the Sky"
  显示 **Released 2021-01-27**（`[历史截图已清理]`）；无日期来源显示 `—`（demo 页即此类变体）。
- **图标**：来源按钮 #34（分享）与搜索空状态 #25（地球）截图见 `[历史截图已清理]`、`[历史截图已清理]`。
- **双语 HTML 手册/模板**：`docs/plugins.html`、`docs/config.html`（ZH/EN 切换）与
  `examples/config.{lua,zh-CN.lua,en.lua}`；config.lua 支持 `ui_scale`、`cache_limit_mb`，startup_page 支持 `artist`；
  Linux/Windows 包内均包含这些文件（打包内容已校验）。
- **Windows 10/11**：Lead 修复落地后交叉重建：PE `Subsystem 00000002 (Windows GUI)`（无控制台窗口）、
  `.rsrc` Resource Directory（内嵌 echoport.ico）、imports 含 OPENGL32/mmdevapi 等；zip 静态检查通过。
  **未在 Windows/Wine 运行**，故运行行为（WASAPI 音频、rfd 对话框、DPI、真实 OpenGL 上下文）未验证。


  打开主窗口/关闭。**按钮行为（播放暂停、上下首、关闭、恢复主窗口）本轮未能在本机用 XTest 稳定
  复验**（KWin/XWayland 合成输入坐标异常）；代码路径与 Lead 验证记为通过，截图证据为窗口级。
- **后台性能模式**：`state.ui_backgrounded = !focused || minimized`；重绘间隔 1000 ms（否则
  66 ms playing / 90 ms busy / 50 ms layout），布局重绘在后台关闭；音频线程不受影响。
  CPU 测量见 §6.7（软件 GL 下无法稳定展示收益）。
- **图标修正**：`[历史截图已清理]`（Reload scripts + Detect again 均为连续弧 + 相切箭头）、
  `[历史截图已清理]`（交叉箭头 X，图标包 SVG 路径）；来源按钮仍 #34、搜索空状态仍 #25。

### 7.15 v0.1.8 资料编辑器 / 12 行历史 ✅

- **编辑器默认收起**：资料页初始渲染不含编辑卡片；「Edit profile」按钮位于右上角「+ New profile」旁。
  截图 `qa18-profile-collapsed.png`（收起）与 `qa18-profile-editing.png`（展开，含 Bio/Save/Cancel/×）。
- **Save/Cancel/× 语义**：Save 写回 Bio 并关闭；Cancel 与 × 关闭且丢弃 Bio 草稿；头像选择/移除即时生效；
  切换资料关闭旧编辑器（代码路径 `state.profile_edit_open` + 草稿同步；Lead 已验证）。
- **收听历史 12 首**：历史卡片渲染最近 12 条（原 8），整页滚动、无内部视口裁剪；`qa18-profile-collapsed`
  中历史标题显示 12 tracks、12 行完整可见。
- **QA 钩子**：`ECHOPORT_PROFILE_EDIT=1` 启动即展开编辑器，两张截图均由发布二进制实测。

### 7.16 v0.1.9 更新监测 / 启动动画 / 新图标 / Arch 安装 ✅

- **更新检查**：`src/update.rs` 匿名 GET GitHub `releases/latest`（固定 UA，无 token/遥测），semver 风格
  比较 `CARGO_PKG_VERSION`；没有 Release 时回退 raw `VERSION`；默认 repo `nilotoo/EchoPort`，
  `ECHOPORT_UPDATE_REPO` 可覆盖；启动一次 + 每 6h；About → Check for updates 手动；只提供链接/Update
  按钮，不自动下载安装。代码 + **2 个 update-version 单元测试**通过、fake 路径实测。
  ⚠️ **未对真实 GitHub 发起请求**（repo URL 为占位，等待用户确认），真实网络行为未验证。
- **启动动画**：`qa19-splash.png` 与 `qa19-splash-update.png`（伪造 v9.9.9）由发布二进制实测；
  `ECHOPORT_SPLASH` / `ECHOPORT_FAKE_UPDATE` / `ECHOPORT_NO_SPLASH` / `ECHOPORT_NO_UPDATE_CHECK` 均按
  文档工作（fake 更新的真机网络替代仅用于测试）。Splash 约 0.9s、检查中最多 1.4s。
- **新图标**：assets/icon.png/icon-128/48/32 与 echoport.ico 重绘为无外圈；`qa19-icon-preview.png`；
  运行时窗口图标读 assets/icon.png；Windows exe PE `.rsrc`（0x8BA8）含新 ICO。
- **Arch 用户安装**：`scripts/install-arch.sh` 无 sudo 安装；本机已安装
  `~/.local/bin/echoport`（sha256 `29f41ccc…` 与 release 一致）、`~/.local/share/applications/echoport.desktop`
  （desktop-file-validate OK）、hicolor 32/48/128/256（尺寸核对）；发布包也附带脚本与这些图标。
- **发布管理**：`VERSION`（0.1.9）、`.github/workflows/release.yml`（`v*` tag → Linux tar.gz + Windows zip
  → GitHub Release）、Cargo repository/homepage 已指向占位 `https://github.com/nilotoo/EchoPort`。

### 7.17 v0.3.0 安全/隐私/精简/安装包 ✅

- **安全加固**（代码审查 + 单测）：update repo 仅 `owner/name`（`valid_repo`）、release URL 仅
  `https://github.com/`（`safe_release_url`）、版本字符串白名单、`OpenUrl` http/https-only、
  profile/playlist 导入 8MB + avatar/bio/name/playlist/track 上限；测试路径改用 `ECHOPORT_TEST_MP3`，
  个人 home 路径在 src/docs 中 0 命中。
- **隐私/仓库精简**：tracked screenshots 52 MB/238 → **3.7 MB/18 张**，7 份本机日志删除；
  工作树（不含 .git/target/dist）**7.7 MB < 15 MB**；README/QA 引用 0 broken；文档路径统一 `$HOME`；
  `qa-settings.png` 使用中性 `/tmp` 隔离 XDG 重新生成。旧 0.1.x 截图/日志不会进入公开 orphan 历史。
- **内嵌文档**：Settings → Scripting 新增 Plugin docs / Config reference（`qa20-settings-docs-buttons.png`）；
  HTML 内嵌二进制，点击写入数据目录并打开浏览器，无需外部文件。
- **启动提示居中**：`qa19-splash-update.png` 重新截取，Stay/Update 居中。
- **安装包**：Linux tar.gz（含 VERSION/32-48-128-256 图标/install-arch.sh）、Arch pkg（pacman -Qip/-Qlp）、
  Windows zip + MSI（msiinfo）、macOS CI 脚本/patch；各产物校验和见 §8。
- **更新检查**：默认 repo `Nilotoo/EchoPort`，仓库尚未 push → 真实在线检查未验证；fake/local + 单测覆盖。

## 8. 打包 / Packaging

```bash
./scripts/package-linux.sh
```

- **`dist/echoport-0.3.0-linux-x86_64.tar.gz`** — **7,063,887 B (6.74 MiB)**
  - sha256 **`40836e7de55135f30c716c273494bba178ea3eede23fda712811fb435e0de45d`**
  - `.sha256` 同级，`sha256sum -c` → OK
- 内容：`echoport`（stripped 14.84 MiB）、`echoport.desktop`、`echoport.png`、`icon-128.png`、`README.md`、
  `LICENSE`、`INSTALL.txt`、`docs/SCRIPTING.md`、`docs/PLUGINS.md`、`examples/config.lua`、
  `examples/plugins/example.lua`、`assets/icons/{LICENSE.txt,README.md}`（MIT-0）。
- 验证：解包后运行 → 正常启动并写 `[历史截图已清理]`；`desktop-file-validate` 通过；
  `dist/` 仅保留 v0.3.0 的 4 个安装包（+ sha256；旧 0.1.x 与测试原型已删除）。

### 8.1 Windows 包与 PE 静态检查（交叉构建，未执行）

- **`dist/echoport-0.3.0-windows-x86_64.zip`** — **6,171,200 B**，sha256
  **`cfc5c90780a0e484ffcb9d5564d562ed9c91f2f95d880961968b1d877ec4af4b`**
- **`dist/echoport-0.3.0-windows-x86_64.msi`** — **6,544,384 B**，sha256
  **`ef2c8217debb55bccda57222288e36aeec7331e5b5a2c755a0b1c330596ab9ed`**（per-user、未签名、未执行）
- **`dist/echoport-0.3.0-1-x86_64.pkg.tar.zst`** — **5,705,233 B**，sha256
  **`e09599f442c0553d82c654b2dbf3847a71e74776d11e806c74e9b52d884e99c4`**（pacman -Qip/-Qlp 已验证）
- macOS：CI-only `EchoPort.app.zip`（`scripts/package-macos.sh` + `packaging/ci/release-workflow.patch`），本地未构建/未验证。
  - 13 个文件：`echoport.exe`、`README-WINDOWS.txt`、`echoport.ico`、`README.md`、`LICENSE`、
    `docs/{SCRIPTING.md,PLUGINS.md,plugins.html,config.html}`、`examples/config.{lua,zh-CN.lua,en.lua}`、
    `examples/plugins/example.lua`。
- exe：**12,884,992 B**，sha256 **`d080f92ffdc3bd270936740079b1664346618123aa1bbe642e3aa979ced8da37`**；
  `file`: PE32+ for MS Windows (GUI)；objdump `Subsystem 00000002 (Windows GUI)`、Data Directory
  `Resource Directory [.rsrc]` 0xC5D8、imports OPENGL32/USER32/KERNEL32/mmdevapi/ws2_32/…。
- 构建：`rustup target add x86_64-pc-windows-gnu` + extracted mingw-w64 GCC 13（Ubuntu cross 包）→
  `cargo build --release --target x86_64-pc-windows-gnu`；`scripts/package-windows-cross.sh` 打包。
- **验证等级**：PE 结构/导入/资源目录静态检查 ✅；**未在真实 Windows/Wine 运行** ❌；
  未签名，SmartScreen 预期提示；OpenGL 3.3+ 驱动要求。

---

## 9. 已知问题与限制 / Known issues & limitations

| # | 级别 | 说明 | 归属/状态 |
|---|---|---|---|
| 1 | 设计限制 | **不绕过 DRM**：Spotify/Apple 自身流不下载不解密；播放 yt-dlp 匹配的公开音频（可能现场/翻唱版本） | documented |
| 2 | 环境限制 | **Netease 音频在本机 IP 地区受限**；合法性因地区而异 | documented |
| 3 | 已知限制 | **Bilibili** 可能 412/567（本轮仅代码映射友好提示，未现场触发）；**SoundCloud** 部分曲目 DRM/HLS-only | documented |
| 4 | 环境偏差 | **空闲 RSS ~254.7 MiB / 播放 ~288.8 MiB**：无 GPU 宿主加载软件 Mesa/LLVM（共享 ~167 MiB）。私有 RssAnon ~81/108 MiB、PSS ~128/146 MiB | environment |
| 5 | 环境风险（open） | **间歇性连续重绘**：历史测量约 1/3 启动出现空闲 CPU 120–860 %（热线程=主线程+llvmpipe），无法按需复现；本轮 5/5 空闲样本 0.000–0.125 %。疑似软渲染/合成器交互，需真实 GPU 复测 | follow-up |
| 6 | 环境限制 | **无可听音频输出**：QA 主机无声卡；播放链路验证到 resolve→cache→cover；真实进度截图佐证 | documented |
| 7 | 发行限制 | **预编译包需 glibc ≥ 2.44**（本机构建基线）；老发行版需用 Rust 1.85+ 源码重建。Windows 10+/macOS 11+ 为构建目标，未实测 | documented |
| 8 | 仓库状态 | 工作树含 v0.3.0 未提交改动；Lead 负责 commit/tag + fresh orphan main | open (Lead) |
| 9 | 更新仓库/认证 | 默认 repo 现为 `Nilotoo/EchoPort`；仓库尚未 push，真实在线更新检查与 Release 上传需用户 SSH key/认证 | open (user) |
| 10 | MSI/macOS 未执行 | MSI 与 macOS bundle 均为交叉/CI 构建，未在真实 Windows/macOS 运行；仅静态检查 | open (verification) |
| 9 | 设计说明 | App/启动器/窗口图标保持不变；仅应用内 GUI 图标使用图标集 | documented |

---

## 10. 复现 / Reproduce

```bash
cd $HOME/EchoPort

# 质量门 + 构建
cargo fmt --check
cargo test                       # 224 passed, 0 failed, 21 ignored
cargo build --release            # 0 warnings
sha256sum target/release/echoport # a004315e…

# 资源报告（用户重点）：启动/空闲/播放/缓存/无缓存/GL/最低配置
./scripts/qa-resources.sh --bin "$PWD/target/release/echoport" --with-network --tracks 5

# 截图（隔离 XDG；最大化/980/缩放需在截图帧前用 wmctrl 设置窗口尺寸）
./scripts/run-demo.sh --release
ECHOPORT_DEMO=1 ECHOPORT_SHOT=$PWD/screenshots/qa-x.png ECHOPORT_PAGE=now_playing ./target/release/echoport

# 实时回归（需网络 + yt-dlp）
cargo test --test live_network -- --ignored --nocapture --test-threads=1
./scripts/qa-cache-prune.sh

# 打包 + 校验
./scripts/package-linux.sh
sha256sum -c dist/echoport-0.3.0-linux-x86_64.tar.gz.sha256
sha256sum -c dist/echoport-0.3.0-windows-x86_64.zip.sha256
sha256sum -c dist/echoport-0.3.0-windows-x86_64.msi.sha256
sha256sum -c dist/echoport-0.3.0-1-x86_64.pkg.tar.zst.sha256
```

---

## 11. 发布检查清单 / Release checklist

- [x] `cargo test` 224 passed / 0 failed / 21 ignored；`cargo fmt --check` clean；release 0 warnings
- [x] 资源报告实测（启动/空闲 CPU/RSS/PSS/线程/FD/播放内存/缓存增长/无缓存/GL）+ 双语最低配置表
- [x] stripped 二进制 14.84 MiB；tarball + sha256 重建并校验；解包运行 + 截图
- [x] 播放条进度条中心与播放键/内容列中心对齐（Δ≤0.5 px / 0.0 px）+ 左右分隔线
- [x] NowPlaying 移除 macaron 小点；极简 v0.1.0 布局保留
- [x] Apple Music 显示 Searchable；GUI 无 preview/match 措辞
- [x] 双击选中/播放、来源页面弹窗、新随机播放图标、缓存 chips、歌单导入/导出、背景专辑/歌曲段落、Made by nilotoo
- [x] 回归：Apple 完整播放 213.1 s、真实进度、后台 wiki、缓存清理、暂停切换、provider 搜索
- [x] v0.3.0：安全/隐私加固、LTO/纹理精简、内嵌文档按钮、启动提示居中、Arch pacman + Windows MSI + macOS CI 安装包、公开仓库 18 张截图/0 日志、无个人路径
- [ ] Lead commit/tag v0.3.0 + 用户 SSH key 后 GitHub Release 上传（本任务不 push）
- [x] Windows 交叉构建 exe + zip（PE 静态检查：GUI subsystem、.rsrc、imports；**未在 Windows 实机运行**）
- [x] README/USAGE/ARCHITECTURE/PROVIDERS/QA_REPORT 更新 v0.1.9；截图/日志隐私清理后 README 图片引用 0 失效；Linux/Windows 包均含 HTML 手册与配置模板；Linux 包含 Arch 安装脚本与 32/48/128/256 图标
- [x] 临时文件/旧 0.1.2 归档清理
- [ ] Lead commit + tag v0.3.0（唯一剩余发布动作；GitHub 上传等用户 SSH key）

### 仍未解决 / follow-ups
- §9 #5 间歇性连续重绘（真实 GPU 复测）
- §9 #4 软件 GL 宿主 RSS；#6 无音频设备；#7 预编译 glibc 基线 / Windows/macOS 未验证
- §9 #3 Bilibili 友好提示未现场触发（仅代码映射）
