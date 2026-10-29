//! Live-network QA checks (ignored by default).
//!
//! These tests hit public, keyless endpoints and yt-dlp, so they are **not**
//! part of normal `cargo test` (which must stay offline). Run them explicitly
//! for release/regression QA:
//!
//! ```bash
//! cargo test --test live_network -- --ignored --nocapture --test-threads=1
//! ```
//!
//! They verify two user-visible bugfixes:
//! * Background tab Wikipedia links must be human pages (`/w/index.php?search=`
//!   or `Special:Search`), never the MediaWiki `api.php` endpoint;
//! * Apple Music imported tracks must resolve for playback through yt-dlp to a
//!   *full-length* public audio match, not the 30 s preview.

use lofty::file::AudioFile;

use echoport::model::{Platform, Track};
use echoport::net::Http;
use echoport::resolver::{self, ResolveConfig};

fn config() -> ResolveConfig {
    ResolveConfig {
        ytdlp: resolver::detect_ytdlp(),
        ffmpeg: resolver::detect_ffmpeg(),
        cache_dir: std::env::temp_dir().join(format!("echoport-live-qa-{}", std::process::id())),
    }
}

/// Read the real duration of a local audio file with lofty (already a runtime dep).
fn audio_duration_secs(path: &std::path::Path) -> f64 {
    // lofty 0.25: TaggedFile::properties() comes from the AudioFile trait and
    // returns FileProperties whose duration() is a std::time::Duration.
    let tagged = lofty::read_from_path(path).expect("audio file should be readable by lofty");
    tagged.properties().duration().as_secs_f64()
}

#[test]
#[ignore = "live network: background links for a niche artist must be human-readable pages"]
fn background_links_are_human_pages_not_api_endpoints() {
    let http = Http::new();
    let config = config();
    // "Drumboii" is the user-reported niche artist with no Wikipedia biography.
    let mut track = Track::new(Platform::YouTube, "qa-drumboii", "Some Song", "Drumboii");
    track.page_url = Some("https://www.youtube.com/watch?v=qa".to_owned());

    let background = echoport::providers::background(&http, &config, &track);
    println!("\nbackground links for {:?}:", track.title);
    for link in background.links.iter().chain(background.videos.iter()) {
        println!("  [{:?}] {} — {}", link.kind, link.title, link.url);
    }
    if let Some(bio) = &background.bio {
        println!("  bio: {} — {}", bio.title, bio.url);
        assert!(
            !bio.url.contains("/api.php"),
            "artist biography link must not be an api.php URL: {}",
            bio.url
        );
    }

    assert!(
        !background.links.is_empty(),
        "expected fallback links for a niche artist"
    );
    for link in &background.links {
        assert!(
            !link.url.contains("/api.php"),
            "background link must never point at the MediaWiki API: {}",
            link.url
        );
        assert!(
            link.url.starts_with("https://") || link.url.starts_with("http://"),
            "background link must be clickable: {}",
            link.url
        );
    }

    let wikipedia_links: Vec<&str> = background
        .links
        .iter()
        .filter(|link| link.url.contains("wikipedia.org"))
        .map(|link| link.url.as_str())
        .collect();
    assert!(
        !wikipedia_links.is_empty(),
        "expected at least one Wikipedia fallback link for a niche artist"
    );
    assert!(
        wikipedia_links
            .iter()
            .any(|url| { url.contains("/w/index.php?search=") || url.contains("Special:Search") }),
        "Wikipedia fallback must use the human search page, got {wikipedia_links:?}"
    );
    if let Some(url) = wikipedia_links.first() {
        assert!(
            url.contains("Drumboii") || url.contains("drumboii"),
            "Wikipedia search URL should be percent-encoded with the artist name: {url}"
        );
    }
    // MusicBrainz artist search makes the fallback useful for niche artists.
    assert!(
        background
            .links
            .iter()
            .any(|link| link.url.contains("musicbrainz.org")),
        "expected a MusicBrainz artist-search fallback link, links: {:?}",
        background.links.iter().map(|l| &l.url).collect::<Vec<_>>()
    );
}

#[test]
#[ignore = "live network + yt-dlp: Apple Music import must resolve to full-length public audio"]
fn apple_music_track_resolves_full_length_not_preview() {
    let http = Http::new();
    let config = config();
    std::fs::create_dir_all(&config.cache_dir).expect("create cache dir");

    let apple_url =
        "https://music.apple.com/us/album/never-gonna-give-you-up/1559885420?i=1559885421";
    let track = echoport::providers::track_from_url(&http, &config, apple_url)
        .expect("Apple Music link import should work (iTunes public metadata)");
    println!(
        "\nimported Apple Music track: {} — {} ({:?} s)",
        track.title, track.artist, track.duration_secs
    );
    assert_eq!(track.platform, Platform::AppleMusic);

    let resolved = resolver::resolve(&config, &track)
        .expect("Apple Music track should resolve via yt-dlp match");
    let bytes = std::fs::metadata(&resolved.path)
        .map(|meta| meta.len())
        .unwrap_or(0);
    let secs = audio_duration_secs(&resolved.path);
    println!(
        "resolved: {:?} ({} bytes, {:.1} s)",
        resolved.path, bytes, secs
    );

    assert!(
        bytes > 256 * 1024,
        "resolved audio is suspiciously small: {bytes} bytes"
    );
    assert!(
        secs > 120.0,
        "Apple Music track must resolve to FULL audio (>120 s), got {secs:.1} s — \
         this would be the 30 s preview bug"
    );
    // The track is ~3:33; a YouTube match within ±20 s is a sane full-length result.
    assert!(
        (secs - 213.0).abs() < 20.0,
        "expected a ~213 s match, got {secs:.1} s"
    );
    // Clean up only our own cache file.
    let _ = std::fs::remove_file(&resolved.path);
    let _ = std::fs::remove_dir_all(&config.cache_dir);
}

#[test]
#[ignore = "live network: provider search must return results from public catalogues"]
fn search_returns_results_from_public_catalogues() {
    let http = Http::new();
    let config = config();
    let platforms = [
        echoport::model::Platform::AppleMusic,
        echoport::model::Platform::Netease,
        echoport::model::Platform::YouTube,
        echoport::model::Platform::Spotify,
    ];
    let started = std::time::Instant::now();
    let groups = echoport::providers::search(
        &http,
        &config,
        "Rick Astley Never Gonna Give You Up",
        &platforms,
        5,
    );
    let elapsed = started.elapsed();
    let mut total = 0usize;
    println!("\nsearch took {elapsed:?}");
    for group in &groups {
        total += group.tracks.len();
        println!(
            "  {:?}: {} tracks, error={:?}",
            group.platform,
            group.tracks.len(),
            group.error
        );
        if let Some(first) = group.tracks.first() {
            println!("    first: {} — {}", first.title, first.artist);
        }
    }
    assert!(
        groups.len() >= 3,
        "expected a result group per requested platform"
    );
    // Spotify has no anonymous search: it must return the hint, not fake results.
    let spotify = groups
        .iter()
        .find(|group| group.platform == echoport::model::Platform::Spotify)
        .expect("Spotify group present");
    assert!(
        spotify.tracks.is_empty(),
        "Spotify must not fabricate anonymous search results"
    );
    assert!(spotify.error.is_some(), "Spotify should show a search hint");
    // At least one public catalogue must answer; the network is known to work on this host.
    assert!(
        groups
            .iter()
            .any(|group| group.platform != echoport::model::Platform::Spotify
                && !group.tracks.is_empty()),
        "expected at least one public catalogue to return tracks, got {total}"
    );
}
