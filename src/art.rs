//! Cover art fetching and on-disk cache (owned by the providers workstream).
//!
//! Covers are downloaded to `<dir>/<sha256(url)>.<ext>` with a `.part` temp
//! file and an atomic rename, so an interrupted download can never leave a
//! half-written cache entry behind. The bytes are decoded before they are
//! accepted: HTML error pages, truncated files and 1×1 tracking pixels are
//! rejected instead of poisoning the cache.
//!
//! Blocking: `resolve` performs network IO and must run on a worker thread.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::model::Track;
use crate::net::Http;

/// Anything smaller than this cannot be a real cover.
const MIN_IMAGE_BYTES: u64 = 512;
/// Covers smaller than this on either edge are placeholders / tracking pixels.
const MIN_IMAGE_EDGE: u32 = 32;
/// Hard cap for a remote cover; abort instead of buffering something absurd.
const MAX_IMAGE_BYTES: usize = 24 * 1024 * 1024;

/// Extensions that may hold a cached cover, most likely first.
const KNOWN_EXTENSIONS: [&str; 7] = ["jpg", "png", "webp", "gif", "bmp", "tif", "img"];

#[derive(Clone, Debug)]
pub struct ArtCache {
    pub dir: PathBuf,
}

impl ArtCache {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Return a local path for the track's cover, downloading it if needed.
    pub fn resolve(&self, http: &Http, track: &Track) -> Option<PathBuf> {
        if let Some(path) = self.cached(track) {
            return Some(path);
        }
        let art_url = track.art_url.as_deref()?.trim();
        if art_url.is_empty() || art_url.starts_with("data:") {
            return None;
        }
        if self.ensure().is_err() {
            return None;
        }
        // Prefer https even when a catalogue advertises plain http.
        let mut candidates = vec![art_url.to_owned()];
        if let Some(rest) = art_url.strip_prefix("http://") {
            candidates.push(format!("https://{rest}"));
        }
        for url in candidates {
            match http.get_bytes(&url, &[]) {
                Ok(bytes) => {
                    if let Some(path) = self.store(art_url, &bytes) {
                        return Some(path);
                    }
                }
                Err(err) => log::debug!("cover download failed ({url}): {err}"),
            }
        }
        None
    }

    /// Write validated image bytes to the cache atomically.
    fn store(&self, url: &str, bytes: &[u8]) -> Option<PathBuf> {
        if bytes.len() < MIN_IMAGE_BYTES as usize || bytes.len() > MAX_IMAGE_BYTES {
            return None;
        }
        let image = image::load_from_memory(bytes).ok()?;
        if image.width() < MIN_IMAGE_EDGE || image.height() < MIN_IMAGE_EDGE {
            return None;
        }
        let extension = extension_for_bytes(bytes)?;
        let stem = cache_stem(url);
        let target = self.dir.join(format!("{stem}.{extension}"));
        let temp = self.dir.join(format!("{stem}.{extension}.part"));
        if std::fs::write(&temp, bytes).is_err() {
            let _ = std::fs::remove_file(&temp);
            return None;
        }
        if std::fs::rename(&temp, &target).is_err() {
            let _ = std::fs::remove_file(&temp);
            return None;
        }
        Some(target)
    }

    /// Extension-less path used by older callers; new code uses `resolve`.
    pub fn path_for_url(&self, url: &str) -> PathBuf {
        self.dir.join(format!("{}.img", cache_stem(url)))
    }

    pub fn clear(&self) -> std::io::Result<()> {
        if self.dir.exists() {
            std::fs::remove_dir_all(&self.dir)?;
        }
        std::fs::create_dir_all(&self.dir)?;
        Ok(())
    }

    pub fn ensure(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)
    }

    /// Return the cached file for a track's cover when one already exists.
    pub fn cached(&self, track: &Track) -> Option<PathBuf> {
        let url = track.art_url.as_deref()?.trim();
        if url.is_empty() {
            return None;
        }
        self.cached_for_url(url)
    }

    fn cached_for_url(&self, url: &str) -> Option<PathBuf> {
        let stem = cache_stem(url);
        for extension in KNOWN_EXTENSIONS {
            let path = self.dir.join(format!("{stem}.{extension}"));
            if let Ok(metadata) = std::fs::metadata(&path) {
                if metadata.is_file() && metadata.len() >= MIN_IMAGE_BYTES {
                    return Some(path);
                }
            }
        }
        None
    }

    /// Path to use when a local file has embedded art (extracted by `library`).
    pub fn embedded_path(&self, source: &Path) -> PathBuf {
        self.dir.join(format!(
            "embedded-{}.img",
            cache_stem(&source.to_string_lossy())
        ))
    }
}

/// Stable sha256 of a cover URL, used as the cache file name.
fn cache_stem(url: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(url.trim().as_bytes());
    let digest = hasher.finalize();
    let bytes: &[u8] = digest.as_ref();
    let mut out = String::with_capacity(bytes.len() * 2);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

/// Sniff the real image format instead of trusting the URL suffix.
fn extension_for_bytes(bytes: &[u8]) -> Option<&'static str> {
    match image::guess_format(bytes).ok()? {
        image::ImageFormat::Jpeg => Some("jpg"),
        image::ImageFormat::Png => Some("png"),
        image::ImageFormat::WebP => Some("webp"),
        image::ImageFormat::Gif => Some("gif"),
        image::ImageFormat::Bmp => Some("bmp"),
        image::ImageFormat::Tiff => Some("tif"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Platform;

    const JPEG_HEADER: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46];
    const PNG_HEADER: &[u8] = &[
        0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, b'I', b'H', b'D',
        b'R', 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00,
    ];

    fn track_with_art(url: &str) -> Track {
        Track::new(Platform::AppleMusic, "id", "Song", "Artist").with_art(url)
    }

    #[test]
    fn cache_stem_is_stable_sha256() {
        let stem = cache_stem("https://example.com/cover.jpg");
        assert_eq!(stem.len(), 64);
        assert!(stem.chars().all(|ch| ch.is_ascii_hexdigit()));
        assert_eq!(stem, cache_stem("  https://example.com/cover.jpg  "));
        assert_ne!(stem, cache_stem("https://example.com/other.jpg"));
        // Known SHA-256 of the empty string (sanity check the hasher wiring).
        assert_eq!(
            cache_stem(""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn image_formats_are_sniffed_from_bytes() {
        assert_eq!(extension_for_bytes(JPEG_HEADER), Some("jpg"));
        assert_eq!(extension_for_bytes(PNG_HEADER), Some("png"));
        assert_eq!(extension_for_bytes(b"<html>not an image</html>"), None);
        assert_eq!(extension_for_bytes(&[]), None);
    }

    #[test]
    fn cached_finds_existing_files_with_any_known_extension() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cache = ArtCache::new(dir.path().to_path_buf());
        cache.ensure().expect("ensure");
        let track = track_with_art("https://example.com/cover.png");
        assert!(cache.cached(&track).is_none());

        let stem = cache_stem("https://example.com/cover.png");
        let path = dir.path().join(format!("{stem}.png"));
        std::fs::write(&path, vec![0u8; MIN_IMAGE_BYTES as usize + 1]).expect("write");
        assert_eq!(cache.cached(&track), Some(path));

        // Too small to be a real cover: ignored.
        let tiny_dir = tempfile::tempdir().expect("tempdir");
        let tiny = ArtCache::new(tiny_dir.path().to_path_buf());
        tiny.ensure().expect("ensure");
        let tiny_path = tiny_dir.path().join(format!("{stem}.jpg"));
        std::fs::write(&tiny_path, b"tiny").expect("write");
        assert!(tiny.cached(&track).is_none());
    }

    #[test]
    fn embedded_paths_are_stable_and_distinct() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cache = ArtCache::new(dir.path().to_path_buf());
        let first = cache.embedded_path(Path::new("/music/a.mp3"));
        let second = cache.embedded_path(Path::new("/music/b.flac"));
        assert_ne!(first, second);
        assert_eq!(first, cache.embedded_path(Path::new("/music/a.mp3")));
        assert_eq!(first.parent(), Some(dir.path()));
        assert_eq!(
            cache
                .path_for_url("https://example.com/x.jpg")
                .extension()
                .unwrap(),
            "img"
        );
    }

    #[test]
    fn clear_recreates_the_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cache = ArtCache::new(dir.path().join("covers"));
        cache.ensure().expect("ensure");
        std::fs::write(cache.dir.join("file"), b"x").expect("write");
        cache.clear().expect("clear");
        assert!(cache.dir.is_dir());
        assert!(std::fs::read_dir(&cache.dir)
            .expect("read")
            .next()
            .is_none());
    }

    #[test]
    #[ignore = "network smoke test"]
    fn net_resolve_cover() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cache = ArtCache::new(dir.path().to_path_buf());
        let http = Http::new();
        let track = track_with_art(
            "https://is1-ssl.mzstatic.com/image/thumb/Music124/v4/ce/6d/5b/\
             ce6d5b48-8c36-b990-3b9c-81862fadb459/0859381157694.jpg/600x600bb.jpg",
        );
        let path = cache.resolve(&http, &track).expect("cover downloaded");
        assert!(path.is_file());
        assert_eq!(cache.cached(&track), Some(path.clone()));
        let image = image::open(&path).expect("cover decodes");
        assert!(image.width() >= 100);
        assert_eq!(cache.resolve(&http, &track), Some(path));
    }
}
