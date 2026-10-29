//! Integration tests: release packaging assets.
//!
//! These files are produced/inspected by `scripts/package-linux.sh`; the tests
//! guard the invariants a Linux desktop needs (a valid PNG icon and a valid
//! XDG `.desktop` entry) so a broken asset cannot slip into a release tarball.

use std::path::{Path, PathBuf};

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Minimal PNG header check: signature + IHDR (width, height, bit depth, colour type).
fn png_size(path: &Path) -> Option<(u32, u32, u8, u8)> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() < 33 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    if &bytes[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    Some((width, height, bytes[24], bytes[25]))
}

#[test]
fn application_icon_is_a_valid_256_png() {
    let icon = manifest_dir().join("assets/icon.png");
    let (width, height, bit_depth, colour_type) = png_size(&icon)
        .unwrap_or_else(|| panic!("assets/icon.png is missing or not a valid PNG: {icon:?}"));
    assert_eq!(
        (width, height),
        (256, 256),
        "desktop icon should be 256x256"
    );
    assert_eq!(bit_depth, 8, "8 bit per channel expected");
    assert_eq!(colour_type, 6, "RGBA PNG expected (colour type 6)");

    let small = manifest_dir().join("assets/icon-128.png");
    if small.exists() {
        let (w, h, _, _) = png_size(&small).expect("icon-128.png is a valid PNG");
        assert_eq!((w, h), (128, 128));
    }
}

#[test]
fn desktop_entry_has_the_fields_desktops_need() {
    let path = manifest_dir().join("packaging/echoport.desktop");
    let text = std::fs::read_to_string(&path).expect("packaging/echoport.desktop exists");
    assert!(
        text.starts_with("[Desktop Entry]"),
        "desktop file must start with the group header"
    );

    let required = [
        "Type=Application",
        "Name=EchoPort",
        "Exec=echoport",
        "Icon=echoport",
        "Terminal=false",
    ];
    for line in required {
        assert!(
            text.lines().any(|l| l.trim() == line),
            "packaging/echoport.desktop is missing `{line}`"
        );
    }

    let categories = text
        .lines()
        .find(|line| line.starts_with("Categories="))
        .expect("Categories= is required");
    for category in ["AudioVideo", "Audio", "Player"] {
        assert!(
            categories.contains(category),
            "Categories must include {category}: {categories}"
        );
    }

    // Bilingual comment: English plus a Chinese localized value.
    let has_comment = text
        .lines()
        .any(|line| line.trim_start().starts_with("Comment=") && line.len() > "Comment=".len());
    let has_chinese_comment = text
        .lines()
        .any(|line| line.trim_start().starts_with("Comment[zh_CN]=") && line.contains("轻量"));
    assert!(
        has_comment && has_chinese_comment,
        "desktop Comment must be bilingual"
    );

    // No CRLF / trailing-whitespace surprises for the launcher parser.
    assert!(
        !text.contains('\r'),
        "desktop file must not contain CRLF line endings"
    );
    assert!(
        text.ends_with('\n'),
        "desktop file should end with a newline"
    );
}

#[test]
fn packaging_scripts_exist_and_are_non_empty() {
    for script in [
        "scripts/build.sh",
        "scripts/run-demo.sh",
        "scripts/package-linux.sh",
        "scripts/build-windows.ps1",
        "scripts/gen-icon.py",
    ] {
        let path = manifest_dir().join(script);
        let meta =
            std::fs::metadata(&path).unwrap_or_else(|_| panic!("missing release script: {script}"));
        assert!(meta.is_file(), "{script} must be a file");
        assert!(meta.len() > 200, "{script} looks empty");
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for script in [
            "scripts/build.sh",
            "scripts/run-demo.sh",
            "scripts/package-linux.sh",
            "scripts/gen-icon.py",
        ] {
            let path = manifest_dir().join(script);
            let mode = std::fs::metadata(&path)
                .expect("script metadata")
                .permissions()
                .mode();
            assert!(
                mode & 0o111 != 0,
                "{script} should be executable (chmod +x)"
            );
        }
    }
}

#[test]
fn docs_and_readme_are_present() {
    for file in [
        "README.md",
        "docs/ARCHITECTURE.md",
        "docs/USAGE.md",
        "docs/PROVIDERS.md",
        "docs/QA_REPORT.md",
        "docs/SCRIPTING.md",
        "docs/PLUGINS.md",
    ] {
        let path = manifest_dir().join(file);
        let meta =
            std::fs::metadata(&path).unwrap_or_else(|_| panic!("missing documentation: {file}"));
        assert!(meta.len() > 500, "{file} looks like a stub");
    }
    let readme = std::fs::read_to_string(manifest_dir().join("README.md")).expect("README.md");
    assert!(
        readme.contains("EchoPort"),
        "README must introduce EchoPort"
    );
    assert!(
        readme.lines().count() < 220,
        "README is intentionally a concise geek intro"
    );
    assert!(
        readme.contains("docs/USAGE.md"),
        "README must link the usage guide"
    );
}

#[test]
fn scripting_templates_ship_with_the_repo() {
    for file in ["examples/config.lua", "examples/plugins/example.lua"] {
        let path = manifest_dir().join(file);
        let meta =
            std::fs::metadata(&path).unwrap_or_else(|_| panic!("missing Lua template: {file}"));
        assert!(meta.len() > 200, "{file} looks empty");
    }
    let config =
        std::fs::read_to_string(manifest_dir().join("examples/config.lua")).expect("config.lua");
    assert!(
        config.contains("echoport.config"),
        "config.lua must define the echoport.config table"
    );
    // Bilingual template comments are part of the project style.
    assert!(config.contains("配置") && config.contains("configuration"));
}

#[test]
fn windows_assets_and_build_helpers_exist() {
    // Multi-size ICO used by the Windows packaging script / shortcuts.
    let ico = manifest_dir().join("assets/echoport.ico");
    let bytes = std::fs::read(&ico).expect("assets/echoport.ico exists");
    assert!(bytes.len() > 16, "ICO must not be empty");
    assert_eq!(&bytes[..4], &[0x00, 0x00, 0x01, 0x00], "ICO magic");
    let count = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;
    assert!(count >= 4, "ICO should contain several sizes, got {count}");

    let script = manifest_dir().join("scripts/build-windows.ps1");
    let text = std::fs::read_to_string(&script).expect("scripts/build-windows.ps1 exists");
    assert!(
        text.contains("echoport.ico"),
        "PowerShell build packages the icon"
    );
    assert!(
        text.contains("README-WINDOWS"),
        "PowerShell build packages README-WINDOWS"
    );

    let cross = manifest_dir().join("scripts/package-windows-cross.sh");
    let cross_text =
        std::fs::read_to_string(&cross).expect("scripts/package-windows-cross.sh exists");
    assert!(cross_text.contains("x86_64-pc-windows-gnu"));
    assert!(cross_text.contains("echoport.ico"));

    // Bilingual HTML manuals + config templates ship with release packages.
    for doc in ["docs/plugins.html", "docs/config.html"] {
        let path = manifest_dir().join(doc);
        let text = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{doc} exists"));
        assert!(text.contains("<html"), "{doc} should be an HTML document");
    }
    for cfg in [
        "examples/config.lua",
        "examples/config.zh-CN.lua",
        "examples/config.en.lua",
    ] {
        assert!(manifest_dir().join(cfg).exists(), "{cfg} exists");
    }

    let readme = manifest_dir().join("packaging/README-WINDOWS.md");
    let readme_text = std::fs::read_to_string(&readme).expect("packaging/README-WINDOWS.md exists");
    for needle in [
        "SmartScreen",
        "OpenGL",
        "%APPDATA%",
        "yt-dlp.exe",
        "not been",
    ] {
        assert!(
            readme_text.contains(needle),
            "Windows readme should mention {needle:?}"
        );
    }
}
