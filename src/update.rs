//! Built-in update checker (GitHub Releases, no account required).
//!
//! EchoPort polls the public GitHub Releases API once at startup and then every
//! six hours. Only public repositories work anonymously; a release is optional
//! (fallback: the `VERSION` file on the default branch). No data is collected.

use crate::net::Http;

/// Public repository used for update checks. Overridable at runtime for forks.
pub const DEFAULT_REPO: &str = "Nilotoo/EchoPort";
const CHECK_INTERVAL_HOURS: u64 = 6;

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
    url.starts_with("https://github.com/")
        && !url.chars().any(|c| c.is_whitespace() || c.is_control())
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

/// Query GitHub for the latest release. Returns `None` when up to date.
pub fn check(http: &Http, current_version: &str) -> Result<Option<UpdateInfo>, String> {
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
    }
}
