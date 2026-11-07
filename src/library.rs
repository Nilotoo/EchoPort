//! Local music library scanning and embedded artwork extraction
//! (owned by the audio-core workstream).
//!
//! Scans configured folders recursively with `walkdir` (hidden directories and
//! symlinks are skipped so a symlink loop cannot hang the scanner), reads tags
//! with `lofty` and falls back to the file name when a file has no title tag.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use lofty::file::{AudioFile, TaggedFileExt};
use lofty::picture::PictureType;
use lofty::tag::Accessor;

use crate::model::Track;

pub const AUDIO_EXTENSIONS: [&str; 9] = [
    "mp3", "m4a", "aac", "flac", "wav", "ogg", "opus", "wma", "aiff",
];

pub fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| AUDIO_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Recursively scan the given folders for audio files and read their tags.
///
/// * hidden files/directories are skipped
/// * symlinks are not followed (prevents loops)
/// * files are deduplicated by canonical path
/// * the result is sorted by artist, then title, then path
pub fn scan_dirs(dirs: &[PathBuf]) -> Vec<Track> {
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut tracks: Vec<Track> = Vec::new();

    for root in dirs {
        if !root.is_dir() {
            continue;
        }
        let walker = walkdir::WalkDir::new(root)
            .follow_links(false)
            .sort_by_file_name()
            .into_iter()
            .filter_entry(|entry| entry.depth() == 0 || is_visible_entry(entry));

        for entry in walker.filter_map(Result::ok) {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            if !is_audio_file(path) {
                continue;
            }
            let canonical = match std::fs::canonicalize(path) {
                Ok(canonical) => canonical,
                Err(_) => path.to_path_buf(),
            };
            if !seen.insert(canonical) {
                continue;
            }
            tracks.push(track_from_file(path));
        }
    }

    tracks.sort_by(|left, right| {
        left.artist
            .to_lowercase()
            .cmp(&right.artist.to_lowercase())
            .then_with(|| left.title.to_lowercase().cmp(&right.title.to_lowercase()))
            .then_with(|| left.local_path.cmp(&right.local_path))
    });
    tracks
}

fn is_visible_entry(entry: &walkdir::DirEntry) -> bool {
    if entry.file_type().is_symlink() {
        return false;
    }
    let name = entry.file_name().to_string_lossy();
    !name.starts_with('.')
}

/// Copy the tag date into `year` and (when a full date exists) `release_date`.
fn apply_tag_date(track: &mut Track, date: lofty::tag::items::Timestamp) {
    if (1000..=2200).contains(&date.year) {
        track.year = Some(date.year);
    }
    if let (Some(month), Some(day)) = (date.month, date.day) {
        if let Some(release_date) = crate::providers::util::date_from_parts(date.year, month, day) {
            track.release_date = Some(release_date);
        }
    }
}

/// Build a `Track` for a local file, using tags when present and the file stem
/// as fallback.
fn track_from_file(path: &Path) -> Track {
    let mut track = Track::from_local_path(path.to_path_buf());
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Unknown".to_owned());

    match lofty::read_from_path(path) {
        Ok(tagged_file) => {
            let tag = tagged_file
                .primary_tag()
                .or_else(|| tagged_file.first_tag());
            if let Some(tag) = tag {
                if let Some(title) = tag
                    .title()
                    .map(|title| title.trim().to_owned())
                    .filter(|title| !title.is_empty())
                {
                    track.title = title;
                }
                if let Some(artist) = tag
                    .artist()
                    .map(|artist| artist.trim().to_owned())
                    .filter(|artist| !artist.is_empty())
                {
                    track.artist = artist;
                }
                if let Some(album) = tag
                    .album()
                    .map(|album| album.trim().to_owned())
                    .filter(|album| !album.is_empty())
                {
                    track.album = Some(album);
                }
                // Year plus full release date when the tag carries one.
                if let Some(date) = tag.date() {
                    apply_tag_date(&mut track, date);
                }
            }
            let duration = tagged_file.properties().duration().as_secs_f64();
            if duration.is_finite() && duration > 0.0 {
                track.duration_secs = Some(duration);
            }
        }
        Err(err) => {
            log::debug!("cannot read tags from {path:?}: {err}");
        }
    }
    if track.title.trim().is_empty() {
        track.title = stem;
    }
    track
}

/// Extract embedded cover art from an audio file, returning the image bytes.
///
/// Front covers win; otherwise the first picture of any type is returned.
pub fn embedded_cover(audio_path: &Path) -> Option<Vec<u8>> {
    let tagged_file = lofty::read_from_path(audio_path).ok()?;
    let mut pictures: Vec<&lofty::picture::Picture> = Vec::new();
    for tag in tagged_file.tags() {
        if let Some(picture) = tag.get_picture_type(PictureType::CoverFront) {
            return picture_bytes(picture);
        }
        pictures.extend(tag.pictures().iter());
    }
    pictures.iter().find_map(|picture| picture_bytes(picture))
}

fn picture_bytes(picture: &lofty::picture::Picture) -> Option<Vec<u8>> {
    let data = picture.data();
    // Guard against absurdly large (or broken) embedded pictures.
    if data.is_empty() || data.len() > 24 * 1024 * 1024 {
        return None;
    }
    Some(data.to_vec())
}

// ---------------------------------------------------------------------------
// Tests (temp files only, no network / audio hardware)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn write_file(path: &Path) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        std::fs::write(path, b"not a real audio file").expect("write file");
    }

    #[test]
    fn detects_audio_extensions_case_insensitively() {
        assert!(is_audio_file(Path::new("/music/song.mp3")));
        assert!(is_audio_file(Path::new("/music/song.FLAC")));
        assert!(is_audio_file(Path::new("/music/song.M4A")));
        assert!(is_audio_file(Path::new("/music/song.ogg")));
        assert!(is_audio_file(Path::new("/music/song.wav")));
        assert!(!is_audio_file(Path::new("/music/cover.jpg")));
        assert!(!is_audio_file(Path::new("/music/readme")));
    }

    #[test]
    fn scans_recursively_and_skips_hidden_and_symlinks() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        write_file(&root.join("alpha.mp3"));
        write_file(&root.join("sub").join("beta.flac"));
        write_file(&root.join("sub").join("notes.txt"));
        write_file(&root.join(".hidden").join("secret.mp3"));
        write_file(&root.join("sub").join(".cover.mp3"));

        // A symlink loop must not be followed.
        #[cfg(unix)]
        {
            let link = root.join("loop");
            let _ = std::os::unix::fs::symlink(root, &link);
        }

        let tracks = scan_dirs(&[root.to_path_buf()]);
        let names: Vec<&str> = tracks.iter().map(|track| track.title.as_str()).collect();
        assert_eq!(names, vec!["alpha", "beta"]);
        assert!(tracks.iter().all(|track| track.artist.is_empty()));
        assert!(tracks.iter().all(|track| track.local_path.is_some()));
        assert!(tracks
            .iter()
            .all(|track| track.platform == crate::model::Platform::Local));
    }

    #[test]
    fn deduplicates_overlapping_roots() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("music");
        write_file(&root.join("one.mp3"));
        let tracks = scan_dirs(&[root.clone(), root.clone()]);
        assert_eq!(tracks.len(), 1);
    }

    #[test]
    fn sorts_by_artist_then_title() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        write_file(&root.join("Zeta.mp3"));
        write_file(&root.join("Alpha.mp3"));
        write_file(&root.join("beta.mp3"));
        let titles: Vec<String> = scan_dirs(&[root.to_path_buf()])
            .into_iter()
            .map(|track| track.title.to_lowercase())
            .collect();
        assert_eq!(titles, vec!["alpha", "beta", "zeta"]);
    }

    #[test]
    fn missing_folders_scan_to_nothing() {
        assert!(scan_dirs(&[PathBuf::from("/definitely/not/here")]).is_empty());
        assert!(scan_dirs(&[]).is_empty());
    }

    /// Minimal MP3: ID3v2 tag bytes followed by a few silent MPEG frames so
    /// lofty can detect the file type and read the tag back.
    fn write_tagged_mp3(path: &Path, tag: &lofty::tag::Tag) {
        use lofty::config::WriteOptions;
        use lofty::tag::TagExt;

        let mut bytes = Vec::new();
        tag.dump_to(&mut bytes, WriteOptions::default())
            .expect("dump tag bytes");
        let mut frame = vec![0xFF, 0xFB, 0x90, 0x00];
        frame.resize(417, 0);
        for _ in 0..8 {
            bytes.extend_from_slice(&frame);
        }
        std::fs::write(path, bytes).expect("write tagged mp3");
    }

    #[test]
    fn local_tags_populate_year_and_release_date() {
        use lofty::tag::{items::Timestamp, Tag, TagType};

        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("tagged.mp3");
        let mut tag = Tag::new(TagType::Id3v2);
        tag.set_title("Tagged Song".to_owned());
        tag.set_artist("Tagged Artist".to_owned());
        tag.set_album("Tagged Album".to_owned());
        tag.set_date(Timestamp {
            year: 2021,
            month: Some(2),
            day: Some(10),
            hour: None,
            minute: None,
            second: None,
        });
        write_tagged_mp3(&path, &tag);

        let track = track_from_file(&path);
        assert_eq!(track.title, "Tagged Song");
        assert_eq!(track.artist, "Tagged Artist");
        assert_eq!(track.album.as_deref(), Some("Tagged Album"));
        assert_eq!(track.year, Some(2021));
        assert_eq!(track.release_date.as_deref(), Some("2021-02-10"));
    }

    #[test]
    fn year_only_tags_do_not_guess_a_release_date() {
        use lofty::tag::{items::Timestamp, Tag, TagType};

        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("year-only.mp3");
        let mut tag = Tag::new(TagType::Id3v2);
        tag.set_title("Year Only".to_owned());
        tag.set_date(Timestamp {
            year: 1999,
            month: None,
            day: None,
            hour: None,
            minute: None,
            second: None,
        });
        write_tagged_mp3(&path, &tag);

        let track = track_from_file(&path);
        assert_eq!(track.year, Some(1999));
        assert!(
            track.release_date.is_none(),
            "no day/month -> no guessed date"
        );
    }

    #[test]
    fn embedded_cover_returns_none_for_untagged_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        let file = temp.path().join("plain.mp3");
        write_file(&file);
        assert!(embedded_cover(&file).is_none());
        assert!(embedded_cover(Path::new("/definitely/not/here.mp3")).is_none());
    }
}
