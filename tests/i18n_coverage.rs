//! Integration tests: bilingual coverage of the i18n catalogue.
//!
//! `i18n::Key` has no `ALL` constant (the GUI workstream owns that file), so the
//! exhaustive check parses `src/i18n.rs` itself: every enum variant must have
//! exactly one `pair()` arm with a non-empty English *and* Chinese string. This
//! keeps the test strict without ever editing `src/**`, and it fails loudly when
//! a new key is added without translations.
//!
//! A small runtime sample additionally proves that `t()` dispatches each
//! language to the expected string.

use echoport::i18n::{t, Key, Language};

const I18N_SRC: &str = include_str!("../src/i18n.rs");

#[derive(Debug, Clone, PartialEq, Eq)]
struct Pair {
    name: String,
    en: String,
    zh: String,
}

fn cjk_count(text: &str) -> usize {
    text.chars()
        .filter(|c| ('\u{4e00}'..='\u{9fff}').contains(c) || ('\u{3400}'..='\u{4dbf}').contains(c))
        .count()
}

/// Extract the unit-variant names from `pub enum Key { ... }`.
fn enum_variants(src: &str) -> Vec<String> {
    let start = src
        .find("pub enum Key {")
        .expect("i18n.rs must contain `pub enum Key {`");
    let rest = &src[start..];
    let end = rest
        .find("\n}")
        .expect("could not find the end of `enum Key`");
    let mut names = Vec::new();
    for raw in rest[..end].lines().skip(1) {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("//") || line.starts_with('#') {
            continue;
        }
        let name: String = line
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        assert!(!name.is_empty(), "unexpected line in `enum Key`: {line:?}");
        let after = line[name.len()..].trim_start();
        assert!(
            after.starts_with(',') || after.starts_with('}'),
            "only unit variants are supported in `Key`; got {line:?}"
        );
        names.push(name);
    }
    names
}

fn skip_ws(chars: &[char], index: &mut usize) {
    while *index < chars.len() && chars[*index].is_whitespace() {
        *index += 1;
    }
}

fn parse_string(chars: &[char], index: &mut usize) -> String {
    assert_eq!(
        chars.get(*index),
        Some(&'"'),
        "expected a string literal at {}",
        *index
    );
    *index += 1;
    let mut out = String::new();
    while *index < chars.len() {
        let c = chars[*index];
        match c {
            '\\' => {
                let escaped = chars.get(*index + 1).copied().expect("unterminated escape");
                out.push(match escaped {
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    '0' => '\0',
                    '"' => '"',
                    '\\' => '\\',
                    other => other,
                });
                *index += 2;
            }
            '"' => {
                *index += 1;
                return out;
            }
            _ => {
                out.push(c);
                *index += 1;
            }
        }
    }
    panic!("unterminated string literal in src/i18n.rs");
}

/// Find the `{ ... }` block that starts at `open_index` (which must be `{`).
fn block_end(chars: &[char], open_index: usize) -> usize {
    assert_eq!(chars[open_index], '{');
    let mut depth = 0i32;
    let mut i = open_index;
    let mut in_string = false;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            if c == '\\' {
                i += 2;
                continue;
            }
            if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
        i += 1;
    }
    panic!("unbalanced braces in src/i18n.rs");
}

/// Parse every `Variant => ("english", "中文")` arm of `Key::pair()`.
fn pair_arms(src: &str) -> Vec<Pair> {
    let pair_pos = src
        .find("pub fn pair(")
        .expect("i18n.rs must contain `pub fn pair(`");
    let tail = &src[pair_pos..];
    let match_pos = tail
        .find("match self {")
        .expect("pair() must match on self");
    let block_start = pair_pos + match_pos + "match self ".len();
    let chars: Vec<char> = src[block_start..].chars().collect();
    let end = block_end(&chars, 0);
    let block = &chars[1..end];

    let mut arms = Vec::new();
    let mut i = 0;
    while i + 1 < block.len() {
        if block[i] != '=' || block[i + 1] != '>' {
            i += 1;
            continue;
        }
        // Variant name immediately before the arrow (whitespace tolerated).
        let mut j = i;
        while j > 0 && block[j - 1].is_whitespace() {
            j -= 1;
        }
        let name_end = j;
        while j > 0 && (block[j - 1].is_ascii_alphanumeric() || block[j - 1] == '_') {
            j -= 1;
        }
        let name: String = block[j..name_end].iter().collect();
        assert!(
            !name.is_empty(),
            "found `=>` without a variant name in pair()"
        );

        let mut k = i + 2;
        skip_ws(block, &mut k);
        let wrapped = block.get(k) == Some(&'(');
        if wrapped {
            k += 1;
            skip_ws(block, &mut k);
        }
        let en = parse_string(block, &mut k);
        skip_ws(block, &mut k);
        if block.get(k) == Some(&',') {
            k += 1;
            skip_ws(block, &mut k);
        }
        let zh = parse_string(block, &mut k);
        skip_ws(block, &mut k);
        if wrapped && block.get(k) == Some(&')') {
            k += 1;
        }
        arms.push(Pair { name, en, zh });
        i = k;
    }
    arms
}

#[test]
fn every_key_has_an_english_and_chinese_arm() {
    let variants = enum_variants(I18N_SRC);
    let arms = pair_arms(I18N_SRC);

    assert!(
        variants.len() >= 100,
        "unexpectedly small i18n catalogue: {}",
        variants.len()
    );
    assert_eq!(
        arms.len(),
        variants.len(),
        "pair() arms ({}) != Key variants ({}); did a key get added without a pair?",
        arms.len(),
        variants.len()
    );

    let mut arm_names: Vec<&str> = arms.iter().map(|arm| arm.name.as_str()).collect();
    arm_names.sort_unstable();
    let mut sorted_variants: Vec<&str> = variants.iter().map(String::as_str).collect();
    sorted_variants.sort_unstable();
    assert_eq!(
        arm_names, sorted_variants,
        "pair() arms and enum Key variants are out of sync"
    );
    let unique: std::collections::HashSet<&str> = arm_names.iter().copied().collect();
    assert_eq!(unique.len(), arm_names.len(), "duplicate arm in pair()");
}

#[test]
fn translations_are_non_empty_and_bilingual() {
    for arm in pair_arms(I18N_SRC) {
        let en = arm.en.trim();
        let zh = arm.zh.trim();
        assert!(
            !en.is_empty(),
            "empty English translation for Key::{}",
            arm.name
        );
        assert!(
            !zh.is_empty(),
            "empty Chinese translation for Key::{}",
            arm.name
        );
        for (label, text) in [("English", en), ("Chinese", zh)] {
            let upper = text.to_uppercase();
            for marker in [
                "TODO",
                "FIXME",
                "TBD",
                "???",
                "<placeholder>",
                "lorem ipsum",
            ] {
                assert!(
                    !upper.contains(marker),
                    "{label} translation for Key::{} still contains {marker:?}: {text:?}",
                    arm.name
                );
            }
        }
        assert_ne!(
            en, zh,
            "Key::{} is untranslated (en == zh == {en:?})",
            arm.name
        );
        // The English catalogue must not leak CJK, and the Chinese catalogue
        // must actually contain Chinese characters.
        assert_eq!(
            cjk_count(en),
            0,
            "English translation for Key::{} unexpectedly contains CJK: {en:?}",
            arm.name
        );
        assert!(
            cjk_count(zh) > 0,
            "Chinese translation for Key::{} contains no CJK characters: {zh:?}",
            arm.name
        );
    }
}

#[test]
fn t_dispatches_the_right_language() {
    // Stable, semantic labels: a regression here means a real UI bug.
    let samples = [
        (Key::Play, "Play", "播放"),
        (Key::Pause, "Pause", "暂停"),
        (Key::Next, "Next", "下一首"),
        (Key::Previous, "Previous", "上一首"),
        (Key::Home, "Home", "首页"),
        (Key::SearchNav, "Search", "搜索"),
        (Key::Library, "Library", "音乐库"),
        (Key::Playlists, "Playlists", "歌单"),
        (Key::Downloads, "Downloads", "下载"),
        (Key::Settings, "Settings", "设置"),
        (Key::Close, "Close", "关闭"),
        (Key::Cancel, "Cancel", "取消"),
        (Key::Save, "Save", "保存"),
        (Key::Delete, "Delete", "删除"),
        (Key::Yes, "Yes", "是"),
        (Key::No, "No", "否"),
    ];
    for (key, en, zh) in samples {
        assert_eq!(t(Language::En, key), en, "unexpected English for {key:?}");
        assert_eq!(t(Language::Zh, key), zh, "unexpected Chinese for {key:?}");
        assert_ne!(t(Language::En, key), t(Language::Zh, key));
    }
}

#[test]
fn language_metadata_roundtrips() {
    for language in [Language::En, Language::Zh] {
        assert_eq!(Language::from_key(language.key()), Some(language));
        assert!(!language.native_name().is_empty());
        assert_eq!(language.other().other(), language);
        assert_ne!(language.key(), language.other().key());
    }
    assert_eq!(Language::En.native_name(), "English");
    assert_eq!(Language::Zh.native_name(), "中文");
    assert_eq!(Language::from_key("fr"), None);
    assert_eq!(Language::from_key(""), None);
    // `detect()` must always return a supported language.
    assert!(matches!(Language::detect(), Language::En | Language::Zh));
}

#[test]
fn privacy_and_legal_copy_is_present() {
    // The product promise is user-visible copy, so it is part of i18n coverage.
    for language in [Language::En, Language::Zh] {
        let privacy = t(language, Key::PrivacyNote);
        assert!(!privacy.is_empty());
        let lower = privacy.to_lowercase();
        assert!(
            lower.contains("account") || privacy.contains("账号"),
            "privacy note missing the no-account promise: {privacy:?}"
        );
    }
}
