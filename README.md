# EchoPort · 回声港

**本地优先、无账号、轻量的多音源桌面音乐播放器。**  
Rust + egui 原生应用，Linux / Windows（macOS 构建暂缓），GPL-3.0。

- 本地文件播放 + 本地曲库
- Apple Music / 网易云 / YouTube / SoundCloud / Bilibili 元数据搜索
- 在线播放通过 `yt-dlp` 匹配公开音频，不绕过 DRM
- 跨平台歌单、收藏、专辑/艺人详情、下载
- 无账号、无云同步、无遥测、无广告

> 不是网易云 / Spotify 的替代品。EchoPort 不提供有版权的云曲库，也不做社交、推荐流和付费会员。
> 它适合：想用一个安静的工具管理本地音乐，同时按需搜索/下载公开网络音频的人。

---

## 功能

- **搜索**：综合 / 艺人 / 专辑；Apple Music + 网易云提供专辑元数据。
- **播放**：本地文件直接解码；在线结果经 `yt-dlp` 解析公开音频；队列、随机、循环、倍速。
- **专辑 / 艺人**：封面、年份、曲目、收藏、同艺人其它专辑；背景来自 Wikipedia / MusicBrainz。
- **歌单**：本地与在线曲目混排，自定义封面/简介，可导出/导入离线分享文件。
- **下载**：MP3 / M4A / Opus / FLAC；Windows 安装包内置 `yt-dlp.exe` + `ffmpeg.exe`。
- **本地优先**：资料、歌单、历史只存在本机；更新检查只读公开 GitHub Release。
- **轻量**：release 二进制约 15 MB，无 Electron，无后台常驻。

---

## 和其它播放器比

| | **EchoPort** | 网易云 / Spotify 类 | foobar2000 / MusicBee | Nuclear 等 Electron 多源播放器 |
|---|---|---|---|---|
| 形态 | Rust + egui 原生，约 15 MB | 大型客户端，部分 Electron | 原生，专注本地库 | Electron，运行时较重 |
| 账号 | 不需要 | 需要，会员/广告 | 不需要 | 多数不需要 |
| 曲库 | 无授权云曲库；元数据搜索 + 公开音频匹配 | 授权云曲库 + DRM | 本地文件 | 依赖第三方源 |
| 本地音乐 | 完整支持 | 支持但偏云端 | 强 | 通常较弱 |
| 在线搜索 | Apple / Netease / YouTube / SoundCloud / Bilibili | 自身曲库 | 通常没有 | 有 |
| 下载/离线 | `yt-dlp`，受平台条款/地区限制 | VIP/DRM 限制 | 本地文件 | 视实现 |
| 跨平台歌单 | 本地/在线元数据混排 | 云歌单 | 本地歌单 | 多为在线列表 |
| 扩展 | `config.lua` + Lua 插件 | 封闭 | 组件/插件生态成熟 | 封闭或有限 |
| 许可 | GPL-3.0 | 闭源 | 免费/闭源 | 多为开源 |

简单说：

- 比 **网易云 / Spotify**：更轻、无账号、不采集数据、本地优先；但没有授权曲库、社交和推荐。
- 比 **foobar2000 / MusicBee**：同样轻量、本地优先，多了多源搜索、在线播放/下载和跨平台歌单；代价是依赖 `yt-dlp` 和网络源。
- 比 **Nuclear 等 Electron 多源播放器**：没有 Electron，体积和内存更小；功能更克制，不做社交和复杂推荐。

---

## 安装

**Windows**（推荐 NSIS setup.exe，带安装目录/快捷方式/卸载器）：

- <https://github.com/Nilotoo/EchoPort/releases/latest/download/echoport-0.3.0-windows-x86_64-setup.exe>
- MSI：`...windows-x86_64.msi`（静默部署）
- 便携 zip：`...windows-x86_64.zip`

**Linux**

```bash
# tarball
tar -xzf echoport-0.3.0-linux-x86_64.tar.gz
cd echoport-0.3.0-linux-x86_64
./echoport

# Arch
sudo pacman -U echoport-0.3.0-1-x86_64.pkg.tar.zst
```

Linux 在线搜索/播放需要系统安装 `yt-dlp` 和 `ffmpeg`；Windows 安装包已内置。

---

## 构建

```bash
cargo build --release
cargo test
cargo run --release
```

Linux 依赖：`libasound2-dev`、`libxkbcommon-dev`、`libxcb-*-dev`、`libx11-dev`、`libxi-dev`、`libgl1-mesa-dev`。

---

## 安全与边界

- 不破解 DRM；Spotify / Apple Music 仅使用公开搜索/元数据，播放匹配公开音频。
- 更新检查只访问 GitHub 公开 Release，支持关闭；安装包更新会校验发布页 SHA256。
- Windows 安装器未签名，首次运行会有 SmartScreen 提示。
- 歌词、评论、社交、云同步、推荐流不在项目范围内。
- macOS 构建脚本保留，但当前 Release 暂不提供。

文档：[使用说明](docs/USAGE.md) · [架构](docs/ARCHITECTURE.md) · [QA](docs/QA_REPORT.md)

## License

GPL-3.0。第三方依赖与内置工具的许可见 `NOTICE`、`THIRD-PARTY.md`。
