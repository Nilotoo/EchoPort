//! Release notes shown in Settings. Kept as compiled data so the changelog is
//! available offline (and in Lean builds without docs).

use crate::i18n::Language;

pub struct ReleaseNote {
    pub version: &'static str,
    pub date: &'static str,
    pub title_en: &'static str,
    pub title_zh: &'static str,
    pub items_en: &'static [&'static str],
    pub items_zh: &'static [&'static str],
}

impl ReleaseNote {
    pub fn title(&self, lang: Language) -> &'static str {
        match lang {
            Language::En => self.title_en,
            Language::Zh => self.title_zh,
        }
    }

    pub fn items(&self, lang: Language) -> &'static [&'static str] {
        match lang {
            Language::En => self.items_en,
            Language::Zh => self.items_zh,
        }
    }
}

/// Newest first. Add a new entry for every public release.
pub const RELEASES: &[ReleaseNote] = &[
    ReleaseNote {
        version: "0.4.0",
        date: "2026-11-07",
        title_en: "Theme images, photo theme and polish",
        title_zh: "主题图片、照片主题与细节完善",
        items_en: &[
            "Theme image: pick a sidebar-shaped crop with drag, zoom and arrow-key fine tuning; the cropped palette now colors the entire UI.",
            "Photo is now a fourth theme preset beside Pulse/Tide/Ink, with automatic light/dark selection and contrast-checked text, icons and accents.",
            "Large photos are prepared on a worker thread and bounded to 4096 px, so selecting camera originals no longer blocks the UI.",
            "Settings controls stay on the title row with wrapping hints; the offline changelog is available in Settings.",
            "Sidebar image fills both edges, empty-player artwork uses the new note asset, and photo-theme previews are properly rounded.",
        ],
        items_zh: &[
            "主题图片：按左侧栏比例裁剪，支持拖动、缩放与方向键微调；裁剪区域的颜色会生成整套界面主题。",
            "照片成为脉冲蓝/潮汐/墨蓝之外的第四个主题预设，自动选择亮/暗，并对文字、图标和强调色做对比度校正。",
            "大尺寸照片在后台线程预处理并限制到 4096px，选相机原图不再阻塞界面。",
            "设置项控件固定在标题行、提示文字自动换行；设置内提供离线更新日志。",
            "侧栏图片铺满左右边缘，播放器空态使用新音符素材，照片主题预览卡片改为正确圆角。",
        ],
    },
    ReleaseNote {
        version: "0.3.2",
        date: "2026-11-02",
        title_en: "Security and reliability",
        title_zh: "安全与可靠性",
        items_en: &[
            "Update installer now requires the published SHA256 and rolls back if the new binary cannot start.",
            "Full/Lean assets selected exactly; imported yt-dlp URLs are host-whitelisted and argv-safe.",
            "Profiles sync to disk with a .bak recovery copy; the last active profile is remembered.",
            "HTTP/image size limits, a bounded cover cache and a CI quality gate (fmt, clippy, tests, audit).",
        ],
        items_zh: &[
            "更新器强制校验发布 SHA256，新二进制无法启动时自动回滚。",
            "Full/Lean 资产精确选择；导入的 yt-dlp URL 增加域名白名单与 argv 防护。",
            "资料保存增加 fsync 与 .bak 恢复，并记住上次使用的 profile。",
            "HTTP/图片大小上限、封面缓存上限，以及 fmt/clippy/test/audit 的 CI 门禁。",
        ],
    },
    ReleaseNote {
        version: "0.3.1",
        date: "2026-10-29",
        title_en: "Lyrics and release polish",
        title_zh: "歌词与发布完善",
        items_en: &[
            "Five-line scrolling lyrics in Now Playing, with local .lrc first and LRCLIB fallback.",
            "Player bar restored to its original layout, separator removed and transport nudged left.",
            "Full and Lean release assets for Windows and Linux; Internet Archive added as a legal source.",
            "Larger default window and new note artwork.",
        ],
        items_zh: &[
            "正在播放页加入五行滚动歌词，优先本地 .lrc，LRCLIB 兜底。",
            "播放栏恢复原布局，去掉分隔线并把播放控制整体左移。",
            "Windows/Linux 提供 Full 与 Lean 发布包；新增 Internet Archive 合法公开音频源。",
            "默认窗口扩大，并更换新的音符图标。",
        ],
    },
    ReleaseNote {
        version: "0.3.0",
        date: "2026-10-28",
        title_en: "Albums, favorites and imports",
        title_zh: "专辑、收藏与导入",
        items_en: &[
            "Album search/detail pages with cover, track order and same-artist releases.",
            "Favorite albums, playlist export/import, drag-and-drop local audio.",
            "Security/privacy audit fixes and slimmer release packaging.",
        ],
        items_zh: &[
            "新增专辑搜索/详情页，显示封面、曲序与同艺人专辑。",
            "支持收藏专辑、播放列表导入导出、本地音频拖放。",
            "安全/隐私审计修复与更精简的发布打包。",
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn releases_are_documented_in_both_languages() {
        assert!(!RELEASES.is_empty());
        for release in RELEASES {
            assert!(!release.version.is_empty());
            assert!(!release.date.is_empty());
            assert!(!release.title_en.is_empty());
            assert!(!release.title_zh.is_empty());
            assert_eq!(release.items_en.len(), release.items_zh.len());
            assert!(!release.items_en.is_empty());
            assert!(!release.items(Language::En).is_empty());
            assert!(!release.items(Language::Zh).is_empty());
        }
        assert!(RELEASES.iter().any(|release| release.version == "0.4.0"));
    }
}
