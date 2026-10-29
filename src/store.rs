//! Local-first persistence.
//!
//! Profiles are plain JSON files under the platform data directory. Nothing is
//! ever uploaded and the schema is intentionally hand-editable.

use std::path::{Path, PathBuf};

use crate::model::{Profile, Settings};

pub const DEFAULT_PROFILE: &str = "Me";

#[derive(Clone, Debug)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub profiles_dir: PathBuf,
    pub covers_dir: PathBuf,
    pub audio_cache_dir: PathBuf,
    /// User-editable Lua configuration directory (`config.lua`, `plugins/`).
    pub config_dir: PathBuf,
    pub config_file: PathBuf,
    pub plugins_dir: PathBuf,
}

impl AppPaths {
    pub fn resolve() -> Self {
        let data_root = dirs::data_dir().unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".local/share")
        });
        let cache_root = dirs::cache_dir().unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".cache")
        });
        let data_dir = data_root.join("echoport");
        let cache_dir = cache_root.join("echoport");
        let config_root = dirs::config_dir()
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(".config")
            })
            .join("echoport");
        Self {
            profiles_dir: data_dir.join("profiles"),
            covers_dir: cache_dir.join("covers"),
            audio_cache_dir: cache_dir.join("audio"),
            config_file: config_root.join("config.lua"),
            plugins_dir: config_root.join("plugins"),
            config_dir: config_root,
            data_dir,
            cache_dir,
        }
    }

    /// For tests / portable builds: place everything below one directory.
    pub fn portable(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            profiles_dir: root.join("profiles"),
            covers_dir: root.join("covers"),
            audio_cache_dir: root.join("audio"),
            config_file: root.join("config.lua"),
            plugins_dir: root.join("plugins"),
            config_dir: root.clone(),
            data_dir: root.clone(),
            cache_dir: root,
        }
    }

    pub fn ensure(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.profiles_dir)?;
        std::fs::create_dir_all(&self.covers_dir)?;
        std::fs::create_dir_all(&self.audio_cache_dir)?;
        std::fs::create_dir_all(&self.config_dir)?;
        std::fs::create_dir_all(&self.plugins_dir)?;
        Ok(())
    }

    pub fn profile_file(&self, name: &str) -> PathBuf {
        self.profiles_dir
            .join(format!("{}.json", sanitize_name(name)))
    }
}

/// Only allow portable, filesystem friendly profile names.
pub fn sanitize_name(name: &str) -> String {
    let mut out: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | ' '))
        .collect();
    if out.trim().is_empty() {
        out = DEFAULT_PROFILE.to_owned();
    }
    out.trim().to_owned()
}

#[derive(Clone, Debug)]
pub struct ProfileStore {
    pub paths: AppPaths,
}

impl ProfileStore {
    pub fn new(paths: AppPaths) -> Self {
        Self { paths }
    }

    pub fn list(&self) -> Vec<String> {
        let mut names = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.paths.profiles_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    names.push(stem.to_owned());
                }
            }
        }
        names.sort();
        if names.is_empty() {
            names.push(DEFAULT_PROFILE.to_owned());
        }
        names
    }

    pub fn load(&self, name: &str) -> Option<Profile> {
        let path = self.paths.profile_file(name);
        let text = std::fs::read_to_string(&path).ok()?;
        match serde_json::from_str::<Profile>(&text) {
            Ok(profile) => Some(profile),
            Err(err) => {
                log::warn!("failed to parse profile {path:?}: {err}");
                None
            }
        }
    }

    pub fn save(&self, profile: &Profile) -> std::io::Result<()> {
        self.paths.ensure()?;
        let path = self.paths.profile_file(&profile.name);
        let json = serde_json::to_string_pretty(profile)
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
        let tmp = path.with_extension(format!("json.tmp-{}", std::process::id()));
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    pub fn delete(&self, name: &str) -> bool {
        std::fs::remove_file(self.paths.profile_file(name)).is_ok()
    }

    /// Load the preferred profile or create the default one.
    ///
    /// * explicit name (healthy, corrupt or missing file) -> that name wins;
    ///   corrupt/missing files are recreated with defaults instead of silently
    ///   switching the user to another profile;
    /// * no preference -> default profile, else any healthy existing one.
    pub fn load_or_create(&self, preferred: Option<&str>) -> Profile {
        if let Some(name) = preferred.map(sanitize_name) {
            if let Some(profile) = self.load(&name) {
                return profile;
            }
            let profile = Profile::new(name, Settings::default());
            if let Err(err) = self.save(&profile) {
                log::warn!("could not persist profile: {err}");
            }
            return profile;
        }
        let mut candidates = vec![DEFAULT_PROFILE.to_owned()];
        candidates.extend(self.list());
        candidates.dedup();
        for name in candidates {
            if let Some(profile) = self.load(&name) {
                return profile;
            }
        }
        let profile = Profile::new(DEFAULT_PROFILE, Settings::default());
        if let Err(err) = self.save(&profile) {
            log::warn!("could not persist initial profile: {err}");
        }
        profile
    }
}

/// Human readable byte size used in the settings page.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

pub fn dir_size(path: &Path) -> u64 {
    let mut total = 0;
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            total += dir_size(&path);
        } else if let Ok(meta) = entry.metadata() {
            total += meta.len();
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Playlist, Track};

    #[test]
    fn profile_roundtrip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::portable(dir.path());
        let store = ProfileStore::new(paths);
        let mut profile = Profile::new("Tester", Settings::default());
        let id = profile.create_playlist("Road trip");
        let track = Track::new(crate::model::Platform::Spotify, "abc", "Song", "Artist");
        assert!(profile.add_to_playlist(&id, &track));
        profile.toggle_favorite(&track);
        store.save(&profile).expect("save");

        let loaded = store.load("Tester").expect("load");
        assert_eq!(loaded.playlists.len(), 1);
        assert_eq!(loaded.playlists[0].name, "Road trip");
        assert!(loaded.is_favorite(&track.id));

        assert_eq!(store.list(), vec!["Tester".to_owned()]);
        assert!(store.delete("Tester"));
        assert!(!store.paths.profile_file("Tester").exists());
    }

    #[test]
    fn name_sanitising() {
        assert_eq!(sanitize_name("../../etc/passwd"), "etcpasswd");
        assert_eq!(sanitize_name("  "), DEFAULT_PROFILE);
    }

    #[test]
    fn sizes_are_readable() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1536), "1.5 KiB");
    }

    #[test]
    fn playlists_survive() {
        let mut profile = Profile::new("x", Settings::default());
        let id = profile.create_playlist("A");
        let track = Track::new(crate::model::Platform::Local, "p", "t", "a");
        profile.add_to_playlist(&id, &track);
        let _ = Playlist::with_name("unused");
        assert_eq!(profile.playlist_tracks(&id).len(), 1);
    }
}
