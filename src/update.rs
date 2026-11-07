//! Built-in update checker (GitHub Releases, no account required).
//!
//! EchoPort polls the public GitHub Releases API once at startup and then every
//! six hours. Only public repositories work anonymously; a release is optional
//! (fallback: the `VERSION` file on the default branch). No data is collected.

use crate::net::Http;

/// Public repository used for update checks. Overridable at runtime for forks.
pub const DEFAULT_REPO: &str = "Nilotoo/EchoPort";
/// Mainland-China mirror used for release downloads. The app never embeds a
/// Gitee token: public releases are read anonymously.
pub const DEFAULT_GITEE_REPO: &str = "nilotoo_art/echo-port";
const CHECK_INTERVAL_HOURS: u64 = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateSource {
    GitHub,
    Gitee,
}

impl UpdateSource {
    pub fn key(self) -> &'static str {
        match self {
            Self::GitHub => "github",
            Self::Gitee => "gitee",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateAsset {
    pub name: String,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub title: String,
    pub url: String,
    pub notes: String,
    pub published_at: Option<String>,
    pub assets: Vec<UpdateAsset>,
}

/// Pick the exact release asset for this platform/build. There is deliberately
/// no `contains("setup.exe")` fallback: that could silently turn a Full build
/// into Lean when release asset ordering changes.
pub fn asset_for_current_platform(info: &UpdateInfo) -> Option<&UpdateAsset> {
    #[cfg(target_os = "windows")]
    let suffix = if cfg!(feature = "scripting") {
        "windows-x86_64-setup.exe"
    } else {
        "windows-x86_64-lean-installer.exe"
    };
    #[cfg(target_os = "linux")]
    let suffix = if cfg!(feature = "scripting") {
        "linux-x86_64.tar.gz"
    } else {
        "linux-x86_64-lean.tar.gz"
    };
    #[cfg(target_os = "macos")]
    let suffix = "macos-x86_64.tar.gz";
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    let suffix = "tar.gz";
    info.assets
        .iter()
        .find(|asset| asset.name.ends_with(suffix))
}

/// Only accept actual GitHub HTTPS URLs (not `https://github.com.evil/...`).
pub fn trusted_github_url(url: &str) -> bool {
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    parsed.scheme() == "https"
        && parsed.host_str().is_some_and(|host| {
            host.eq_ignore_ascii_case("github.com")
                || host.eq_ignore_ascii_case("objects.githubusercontent.com")
        })
        && parsed.username().is_empty()
        && parsed.password().is_none()
}

/// Release assets/pages may come from GitHub or the Gitee mirror. Gitee asset
/// downloads redirect to `*.gitee.com`, so the whole official domain is trusted
/// only after an HTTPS URL check.
pub fn trusted_update_url(url: &str) -> bool {
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    parsed.scheme() == "https"
        && parsed.host_str().is_some_and(|host| {
            let host = host.to_ascii_lowercase();
            matches!(
                host.as_str(),
                "github.com" | "objects.githubusercontent.com" | "gitee.com"
            ) || host.ends_with(".gitee.com")
        })
        && parsed.username().is_empty()
        && parsed.password().is_none()
}

/// The `.sha256` sibling asset for a downloaded asset, if published.
pub fn checksum_asset<'a>(info: &'a UpdateInfo, asset: &'a UpdateAsset) -> Option<&'a UpdateAsset> {
    let wanted = format!("{}.sha256", asset.name);
    info.assets
        .iter()
        .find(|candidate| candidate.name == wanted)
}

pub fn repo() -> String {
    let candidate = std::env::var("ECHOPORT_UPDATE_REPO")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_REPO.to_owned());
    if valid_repo(&candidate) {
        candidate
    } else {
        DEFAULT_REPO.to_owned()
    }
}

pub fn gitee_repo() -> String {
    let candidate = std::env::var("ECHOPORT_GITEE_REPO")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_GITEE_REPO.to_owned());
    if valid_repo(&candidate) {
        candidate
    } else {
        DEFAULT_GITEE_REPO.to_owned()
    }
}

fn forced_source() -> Option<UpdateSource> {
    let value = std::env::var("ECHOPORT_UPDATE_SOURCE").ok()?;
    match value.trim().to_ascii_lowercase().as_str() {
        "github" | "gh" => Some(UpdateSource::GitHub),
        "gitee" | "cn" => Some(UpdateSource::Gitee),
        _ => None,
    }
}

fn timezone_is_china(value: &str) -> bool {
    let value = value.trim().to_ascii_lowercase();
    value.contains("asia/shanghai")
        || value.contains("asia/chongqing")
        || value.contains("asia/harbin")
        || value.contains("asia/urumqi")
        || value.contains("asia/kashgar")
        || value.contains("china standard time")
        || value.contains("prc")
        || value.contains("cst-8")
}

fn timezone_hint() -> Option<String> {
    if let Ok(tz) = std::env::var("TZ") {
        if !tz.trim().is_empty() {
            return Some(tz);
        }
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        if let Ok(tz) = std::fs::read_to_string("/etc/timezone") {
            if !tz.trim().is_empty() {
                return Some(tz);
            }
        }
        if let Ok(path) = std::fs::read_link("/etc/localtime") {
            return Some(path.to_string_lossy().into_owned());
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(output) = std::process::Command::new("reg")
            .args([
                "query",
                r"HKLM\SYSTEM\CurrentControlSet\Control\TimeZoneInformation",
                "/v",
                "TimeZoneKeyName",
            ])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            if !text.trim().is_empty() {
                return Some(text.into_owned());
            }
        }
    }
    None
}

/// Approximate Mainland-China detection without an external geo-IP call.
/// Timezone/locale are local-only; `ECHOPORT_UPDATE_SOURCE` is a manual
/// override for edge cases (VPNs, custom images, Windows locale mismatches).
pub fn mainland_china() -> bool {
    if let Some(tz) = timezone_hint() {
        return timezone_is_china(&tz);
    }
    let locale = std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_default()
        .to_ascii_lowercase();
    locale.starts_with("zh_cn") || locale.starts_with("zh-cn")
}

pub fn preferred_source() -> UpdateSource {
    forced_source().unwrap_or_else(|| {
        if mainland_china() {
            UpdateSource::Gitee
        } else {
            UpdateSource::GitHub
        }
    })
}

/// Only `owner/name` with GitHub-safe characters is accepted; this keeps an
/// environment override from injecting arbitrary URL path segments.
fn valid_repo(repo: &str) -> bool {
    let mut parts = repo.split('/');
    let owner = parts.next().unwrap_or_default();
    let name = parts.next().unwrap_or_default();
    parts.next().is_none()
        && !owner.is_empty()
        && !name.is_empty()
        && owner.len() <= 80
        && name.len() <= 100
        && owner
            .chars()
            .chain(name.chars())
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn safe_version(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 40
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'))
}

fn safe_release_url(url: &str) -> bool {
    trusted_update_url(url)
}

/// Only expose versions we can display and compare safely.
fn sanitized_latest(tag: &str) -> Option<String> {
    let latest = tag.trim().trim_start_matches(['v', 'V']);
    safe_version(latest).then(|| latest.to_owned())
}

pub fn check_interval() -> std::time::Duration {
    std::time::Duration::from_secs(CHECK_INTERVAL_HOURS * 60 * 60)
}

/// Compare dotted numeric versions (`v0.1.9`, `1.2`, `1.2.3-beta`).
pub fn is_newer(latest: &str, current: &str) -> bool {
    let parse = |value: &str| -> Vec<u64> {
        value
            .trim()
            .trim_start_matches(['v', 'V'])
            .split('.')
            .map(|part| {
                part.chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse::<u64>()
                    .unwrap_or(0)
            })
            .collect()
    };
    let mut latest = parse(latest);
    let mut current = parse(current);
    let len = latest.len().max(current.len());
    latest.resize(len, 0);
    current.resize(len, 0);
    latest > current
}

/// Query the preferred release source. Mainland-China installs use the Gitee
/// mirror; other regions use GitHub. An explicit `ECHOPORT_UPDATE_SOURCE`
/// disables automatic choice/fallback.
pub fn check(http: &Http, current_version: &str) -> Result<Option<UpdateInfo>, String> {
    let primary = preferred_source();
    let forced = forced_source().is_some();
    match primary {
        UpdateSource::GitHub => {
            log::info!("update source: github");
            let result = check_github(http, current_version);
            // GitHub is often unreachable from Mainland China, including on
            // machines whose timezone was configured to another region. Fail
            // over to the anonymous Gitee mirror instead of losing updates.
            if !forced && result.is_err() {
                log::info!("GitHub update check failed; trying Gitee mirror");
                return check_gitee(http, current_version);
            }
            result
        }
        UpdateSource::Gitee => {
            log::info!("update source: gitee");
            let result = check_gitee(http, current_version);
            if !forced && result.is_err() {
                log::info!("Gitee update check failed; trying GitHub");
                return check_github(http, current_version);
            }
            result
        }
    }
}

/// Query GitHub for the latest release. Returns `None` when up to date.
fn check_github(http: &Http, current_version: &str) -> Result<Option<UpdateInfo>, String> {
    let repo = repo();
    let release_url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let headers = [
        ("Accept", "application/vnd.github+json"),
        ("X-GitHub-Api-Version", "2022-11-28"),
    ];
    match http.get_text(&release_url, &headers) {
        Ok(text) => {
            let value: serde_json::Value =
                serde_json::from_str(&text).map_err(|err| err.to_string())?;
            let tag = value
                .get("tag_name")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            if tag.is_empty() || !is_newer(tag, current_version) {
                return Ok(None);
            }
            let Some(latest) = sanitized_latest(tag) else {
                return Ok(None);
            };
            let url = value
                .get("html_url")
                .and_then(|value| value.as_str())
                .filter(|url| safe_release_url(url))
                .map(str::to_owned)
                .unwrap_or_else(|| format!("https://github.com/{repo}/releases"));
            let title = value
                .get("name")
                .and_then(|value| value.as_str())
                .filter(|name| !name.trim().is_empty())
                .unwrap_or(tag)
                .to_owned();
            let notes = value
                .get("body")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_owned();
            let published_at = value
                .get("published_at")
                .and_then(|value| value.as_str())
                .map(str::to_owned);
            let assets = value
                .get("assets")
                .and_then(|value| value.as_array())
                .map(|assets| {
                    assets
                        .iter()
                        .filter_map(|asset| {
                            Some(UpdateAsset {
                                name: asset.get("name")?.as_str()?.to_owned(),
                                url: asset.get("browser_download_url")?.as_str()?.to_owned(),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            Ok(Some(UpdateInfo {
                current: current_version.to_owned(),
                latest,
                title,
                url,
                notes,
                published_at,
                assets,
            }))
        }
        // A fresh repository often has no Release yet: fall back to a VERSION
        // file on `main`, keeping the built-in checker useful from day one.
        Err(err) if err.is_status(404) || err.is_status(403) => {
            check_version_file(http, &repo, current_version)
        }
        Err(err) => Err(err.to_string()),
    }
}

fn check_gitee(http: &Http, current_version: &str) -> Result<Option<UpdateInfo>, String> {
    let repo = gitee_repo();
    let release_url = format!("https://gitee.com/api/v5/repos/{repo}/releases/latest");
    match http.get_text(&release_url, &[("Accept", "application/json")]) {
        Ok(text) => {
            let value: serde_json::Value =
                serde_json::from_str(&text).map_err(|err| err.to_string())?;
            let tag = value
                .get("tag_name")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            if tag.is_empty() || !is_newer(tag, current_version) {
                return Ok(None);
            }
            let Some(latest) = sanitized_latest(tag) else {
                return Ok(None);
            };
            let url = value
                .get("html_url")
                .and_then(|value| value.as_str())
                .filter(|url| safe_release_url(url))
                .map(str::to_owned)
                .unwrap_or_else(|| format!("https://gitee.com/{repo}/releases/tag/{tag}"));
            let title = value
                .get("name")
                .and_then(|value| value.as_str())
                .filter(|name| !name.trim().is_empty())
                .unwrap_or(tag)
                .to_owned();
            let notes = value
                .get("body")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_owned();
            let published_at = value
                .get("created_at")
                .or_else(|| value.get("published_at"))
                .and_then(|value| value.as_str())
                .map(str::to_owned);
            let assets = value
                .get("assets")
                .and_then(|value| value.as_array())
                .map(|assets| {
                    assets
                        .iter()
                        .filter_map(|asset| {
                            Some(UpdateAsset {
                                name: asset.get("name")?.as_str()?.to_owned(),
                                url: asset.get("browser_download_url")?.as_str()?.to_owned(),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            Ok(Some(UpdateInfo {
                current: current_version.to_owned(),
                latest,
                title,
                url,
                notes,
                published_at,
                assets,
            }))
        }
        Err(err) if err.is_status(404) || err.is_status(403) => {
            check_gitee_version_file(http, &repo, current_version)
        }
        Err(err) => Err(err.to_string()),
    }
}

fn check_gitee_version_file(
    http: &Http,
    repo: &str,
    current_version: &str,
) -> Result<Option<UpdateInfo>, String> {
    for branch in ["main", "master"] {
        let url = format!("https://gitee.com/{repo}/raw/{branch}/VERSION");
        if let Ok(text) = http.get_text(&url, &[]) {
            let Some(latest) = sanitized_latest(text.trim()) else {
                return Ok(None);
            };
            if is_newer(&latest, current_version) {
                return Ok(Some(UpdateInfo {
                    current: current_version.to_owned(),
                    latest: latest.clone(),
                    title: format!("EchoPort {latest}"),
                    url: format!("https://gitee.com/{repo}/releases"),
                    notes: String::new(),
                    published_at: None,
                    assets: Vec::new(),
                }));
            }
            return Ok(None);
        }
    }
    Err("Gitee mirror has no release or VERSION file yet".to_owned())
}

fn check_version_file(
    http: &Http,
    repo: &str,
    current_version: &str,
) -> Result<Option<UpdateInfo>, String> {
    for branch in ["main", "master"] {
        let url = format!("https://raw.githubusercontent.com/{repo}/{branch}/VERSION");
        if let Ok(text) = http.get_text(&url, &[]) {
            let Some(latest) = sanitized_latest(text.trim()) else {
                return Ok(None);
            };
            if is_newer(&latest, current_version) {
                return Ok(Some(UpdateInfo {
                    current: current_version.to_owned(),
                    latest: latest.clone(),
                    title: format!("EchoPort {latest}"),
                    url: format!("https://github.com/{repo}/releases"),
                    notes: String::new(),
                    published_at: None,
                    assets: Vec::new(),
                }));
            }
            return Ok(None);
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comparison_works() {
        assert!(is_newer("v0.1.10", "0.1.9"));
        assert!(is_newer("1.0", "0.9.9"));
        assert!(!is_newer("0.1.9", "0.1.9"));
        assert!(!is_newer("0.1.8", "0.1.9"));
        assert!(!is_newer("v0.1.9-beta", "0.1.9"));
    }

    #[test]
    fn repo_env_override() {
        // Default is always available.
        assert!(!repo().is_empty());
        assert!(valid_repo("Nilotoo/EchoPort"));
        assert!(!valid_repo("evil/../path"));
        assert!(!valid_repo("https://evil.example/x"));
        assert!(safe_release_url(
            "https://github.com/Nilotoo/EchoPort/releases"
        ));
        assert!(!safe_release_url("javascript:alert(1)"));
        assert_eq!(sanitized_latest("v0.2.0"), Some("0.2.0".to_owned()));
        assert!(sanitized_latest("bad version").is_none());
        assert!(trusted_github_url(
            "https://github.com/Nilotoo/EchoPort/releases/download/v1/a.tar.gz"
        ));
        assert!(!trusted_github_url("https://github.com.evil/a.tar.gz"));
        assert!(!trusted_github_url("http://github.com/a.tar.gz"));
        assert!(trusted_update_url(
            "https://gitee.com/nilotoo_art/echo-port/releases/download/v1/a.tar.gz"
        ));
        assert!(trusted_update_url(
            "https://foruda.gitee.com/attach_file/1/a.tar.gz?token=x"
        ));
        assert!(!trusted_update_url("https://gitee.com.evil/a.tar.gz"));
        assert!(timezone_is_china("Asia/Shanghai"));
        assert!(timezone_is_china("Asia/Urumqi"));
        assert!(timezone_is_china("China Standard Time"));
        assert!(!timezone_is_china("Europe/London"));
    }

    #[test]
    fn update_picks_exact_variant_and_checksum() {
        let info = UpdateInfo {
            current: "0.3.1".to_owned(),
            latest: "0.3.2".to_owned(),
            title: "t".to_owned(),
            url: "https://github.com/Nilotoo/EchoPort/releases".to_owned(),
            notes: String::new(),
            published_at: None,
            assets: vec![
                UpdateAsset {
                    name: "echoport-0.3.2-linux-x86_64-lean.tar.gz".to_owned(),
                    url: String::new(),
                },
                UpdateAsset {
                    name: "echoport-0.3.2-linux-x86_64.tar.gz".to_owned(),
                    url: String::new(),
                },
                UpdateAsset {
                    name: "echoport-0.3.2-linux-x86_64.tar.gz.sha256".to_owned(),
                    url: String::new(),
                },
            ],
        };
        #[cfg(target_os = "linux")]
        {
            let picked = asset_for_current_platform(&info).expect("asset");
            if cfg!(feature = "scripting") {
                assert_eq!(picked.name, "echoport-0.3.2-linux-x86_64.tar.gz");
            } else {
                assert_eq!(picked.name, "echoport-0.3.2-linux-x86_64-lean.tar.gz");
            }
            assert!(checksum_asset(&info, picked).is_some());
        }
    }
}
