//! Small parsing / formatting helpers shared by the provider implementations.
//!
//! Everything here is pure (no network, no IO) so it can be unit tested with
//! inline fixture data.

use serde_json::Value;

/// Percent encode a query-string / path component (RFC 3986 unreserved bytes
/// stay as-is, everything else becomes `%XX`). Unlike the `application/
/// x-www-form-urlencoded` serializer this encodes spaces as `%20`, which is
/// correct for URL path segments too.
pub fn pct(input: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(input.len());
    for byte in input.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*byte as char);
            }
            other => {
                out.push('%');
                out.push(HEX[(other >> 4) as usize] as char);
                out.push(HEX[(other & 0x0f) as usize] as char);
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// JSON helpers that tolerate missing / null / wrongly typed fields
// ---------------------------------------------------------------------------

/// First non-empty trimmed string at a JSON pointer path.
pub fn get_str<'a>(value: &'a Value, pointer: &str) -> Option<&'a str> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

/// Number at a JSON pointer (accepts numbers and numeric strings).
pub fn get_f64(value: &Value, pointer: &str) -> Option<f64> {
    let raw = value.pointer(pointer)?;
    let number = raw
        .as_f64()
        .or_else(|| raw.as_i64().map(|n| n as f64))
        .or_else(|| {
            raw.as_str()
                .and_then(|text| text.trim().parse::<f64>().ok())
        })?;
    number.is_finite().then_some(number)
}

/// Signed integer at a JSON pointer (accepts numbers and numeric strings).
pub fn get_i64(value: &Value, pointer: &str) -> Option<i64> {
    let raw = value.pointer(pointer)?;
    raw.as_i64()
        .or_else(|| raw.as_u64().and_then(|n| i64::try_from(n).ok()))
        .or_else(|| {
            raw.as_str()
                .and_then(|text| text.trim().parse::<i64>().ok())
        })
}

// ---------------------------------------------------------------------------
// Generic helpers
// ---------------------------------------------------------------------------

/// Trim a string and drop it when it ends up empty.
pub fn clean(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

/// Offset used by `normalize_image_url` when only `http://` is advertised.
pub fn https_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with("https://") {
        Some(trimmed.to_owned())
    } else if let Some(rest) = trimmed.strip_prefix("http://") {
        // Cover/CDN hosts in the catalogues we use all speak TLS; the `art`
        // cache retries the plain-HTTP variant if the upgrade is rejected.
        Some(format!("https://{rest}"))
    } else {
        None
    }
}

/// Join artist names, skipping blanks and repeats while keeping order.
pub fn join_names<I, S>(names: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut out: Vec<String> = Vec::new();
    for name in names {
        let Some(name) = clean(name.as_ref()) else {
            continue;
        };
        if !out.iter().any(|kept| kept.eq_ignore_ascii_case(&name)) {
            out.push(name);
        }
    }
    out.join(", ")
}

/// Milliseconds to seconds, ignoring nonsensical values.
pub fn seconds_from_ms(ms: f64) -> Option<f64> {
    let secs = ms / 1000.0;
    (secs.is_finite() && (0.5..=4.0 * 3600.0).contains(&secs)).then_some(secs)
}

/// Extract `year` from an ISO-8601 style date (`1987-11-12T...`).
pub fn year_from_iso(date: &str) -> Option<u16> {
    let trimmed = date.trim();
    if trimmed.len() < 4 {
        return None;
    }
    let year = trimmed.get(..4)?.parse::<u16>().ok()?;
    (1000..=2200).contains(&year).then_some(year)
}

/// Extract the (UTC) year from a Unix timestamp in milliseconds.
pub fn year_from_unix_ms(ms: i64) -> Option<u16> {
    if ms <= 0 {
        return None;
    }
    // 1970-01-01 .. 2200-01-01 in milliseconds.
    let seconds = ms / 1000;
    let days = seconds.div_euclid(86_400);
    if !(-1..=84_000).contains(&days) {
        return None;
    }
    Some(civil_from_days(days).0 as u16)
}

/// Build a validated `YYYY-MM-DD` from calendar parts.
pub fn date_from_parts(year: u16, month: u8, day: u8) -> Option<String> {
    valid_calendar_date(year, month, day).then(|| format!("{year:04}-{month:02}-{day:02}"))
}

/// True for an existing proleptic Gregorian date.
fn valid_calendar_date(year: u16, month: u8, day: u8) -> bool {
    if !(1000..=2200).contains(&year) || !(1..=12).contains(&month) || day == 0 {
        return false;
    }
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    day <= max_day
}

/// Parse a release date from the shapes platforms actually return:
/// `YYYY-MM-DDTHH:MM:SSZ`, `YYYY-MM-DD`, `YYYY-MM` or yt-dlp's `YYYYMMDD`.
/// Returns a validated `YYYY-MM-DD`; never guesses a missing day.
pub fn date_from_iso(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let bytes = trimmed.as_bytes();
    // ISO date / datetime prefix: "1987-11-12[...]".
    if bytes.len() >= 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[..4].iter().all(u8::is_ascii_digit)
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[8..10].iter().all(u8::is_ascii_digit)
    {
        let year = trimmed[..4].parse::<u16>().ok()?;
        let month = trimmed[5..7].parse::<u8>().ok()?;
        let day = trimmed[8..10].parse::<u8>().ok()?;
        return date_from_parts(year, month, day);
    }
    // yt-dlp upload_date style: "20210129".
    if bytes.len() >= 8 && bytes[..8].iter().all(u8::is_ascii_digit) {
        let year = trimmed[..4].parse::<u16>().ok()?;
        let month = trimmed[4..6].parse::<u8>().ok()?;
        let day = trimmed[6..8].parse::<u8>().ok()?;
        return date_from_parts(year, month, day);
    }
    None
}

/// Format a Unix timestamp (seconds) as `YYYY-MM-DD` in the given UTC offset.
pub fn date_from_unix(seconds: i64, utc_offset_secs: i64) -> Option<String> {
    let seconds = seconds.checked_add(utc_offset_secs)?;
    let days = seconds.div_euclid(86_400);
    if !(-1..=84_000).contains(&days) {
        return None;
    }
    let (year, month, day) = civil_from_days(days);
    date_from_parts(year as u16, month as u8, day as u8)
}

/// Format an epoch value that may be seconds *or* milliseconds (platform
/// payloads are inconsistent) as `YYYY-MM-DD`.
pub fn date_from_epoch_value(value: i64, utc_offset_secs: i64) -> Option<String> {
    if value <= 0 {
        return None;
    }
    // Values in the "year > 5138 if seconds" range must be milliseconds.
    let seconds = if value.abs() >= 100_000_000_000 {
        value.div_euclid(1000)
    } else {
        value
    };
    date_from_unix(seconds, utc_offset_secs)
}

/// Days since the Unix epoch to `(year, month, day)` in the proleptic
/// Gregorian calendar (Howard Hinnant's `civil_from_days` algorithm). Kept
/// local to avoid pulling in a date crate.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    // `year` is the year of the current March-based year; January/February
    // belong to the next calendar year.
    if month <= 2 {
        (year + 1, month, day)
    } else {
        (year, month, day)
    }
}

/// Normalise whitespace and unescape the handful of HTML entities that show up
/// in programme guide titles.
pub fn tidy_text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '&' => {
                let mut entity = String::new();
                while let Some(next) = chars.peek().copied() {
                    if next == ';' || entity.len() >= 8 {
                        break;
                    }
                    entity.push(next);
                    chars.next();
                }
                if chars.peek() == Some(&';') {
                    chars.next();
                }
                match entity.as_str() {
                    "amp" => out.push('&'),
                    "quot" => out.push('"'),
                    "apos" | "#39" => out.push('\''),
                    "lt" => out.push('<'),
                    "gt" => out.push('>'),
                    "nbsp" => out.push(' '),
                    other => {
                        out.push('&');
                        out.push_str(other);
                        out.push(';');
                    }
                }
            }
            '\u{00a0}' => out.push(' '),
            other => out.push(other),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ---------------------------------------------------------------------------
// Relevance ranking (search results / artist cards)
// ---------------------------------------------------------------------------

/// Case-fold and turn punctuation into single spaces, padded with spaces so
/// `" token "` matching never hits inside another word.
pub fn normalize_match(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push(' ');
    let mut in_gap = true;
    for ch in text.chars().flat_map(char::to_lowercase) {
        if ch.is_alphanumeric() {
            out.push(ch);
            in_gap = false;
        } else if !in_gap {
            out.push(' ');
            in_gap = true;
        }
    }
    if !in_gap {
        out.push(' ');
    }
    out
}

/// Distinct normalized words of `text` (kept in first-seen order).
pub fn match_tokens(text: &str) -> Vec<String> {
    let mut tokens: Vec<String> = Vec::new();
    for token in normalize_match(text).split_whitespace() {
        if !tokens.iter().any(|kept| kept == token) {
            tokens.push(token.to_owned());
        }
    }
    tokens
}

/// Relevance score of a candidate artist name for a query.
///
/// Ordering (highest first): exact normalized match, prefix, contains, full
/// token coverage in any order, partial token overlap. Ties favour candidates
/// with fewer extra tokens (shorter names).
pub fn name_relevance(query: &str, candidate: &str) -> i64 {
    let query_norm = normalize_match(query);
    let candidate_norm = normalize_match(candidate);
    let query_phrase = query_norm.trim();
    let candidate_phrase = candidate_norm.trim();
    if query_phrase.is_empty() || candidate_phrase.is_empty() {
        return 0;
    }
    let query_tokens = match_tokens(query);
    let candidate_tokens = match_tokens(candidate);
    let extra = candidate_tokens.len().saturating_sub(query_tokens.len()) as i64;

    if candidate_phrase == query_phrase {
        return 10_000;
    }
    if candidate_phrase.starts_with(query_phrase) {
        return 8_000 - extra * 10;
    }
    if candidate_phrase.contains(query_phrase) {
        return 6_000 - extra * 10;
    }
    let hits = query_tokens
        .iter()
        .filter(|token| candidate_tokens.iter().any(|cand| cand == *token))
        .count();
    if !query_tokens.is_empty() && hits == query_tokens.len() {
        return 4_000 - extra * 10;
    }
    if hits > 0 {
        return 1_000 * hits as i64 / query_tokens.len() as i64;
    }
    0
}

/// Relevance of a track row for the search query: exact title/artist match
/// first, then title prefix, then query-token coverage over title + artist.
pub fn track_relevance(query: &str, title: &str, artist: &str) -> i64 {
    let query_norm = normalize_match(query);
    let query_phrase = query_norm.trim();
    if query_phrase.is_empty() {
        return 0;
    }
    let title_norm = normalize_match(title);
    let title_phrase = title_norm.trim();
    let artist_norm = normalize_match(artist);
    let artist_phrase = artist_norm.trim();
    let combined = format!("{title_phrase} {artist_phrase}");
    let query_tokens = match_tokens(query);
    let combined_tokens = match_tokens(&combined);

    let mut score = 0i64;
    if title_phrase == query_phrase || artist_phrase == query_phrase {
        score += 10_000;
    } else if (!title_phrase.is_empty() && title_phrase.starts_with(query_phrase))
        || (!title_phrase.is_empty() && query_phrase.starts_with(title_phrase))
    {
        score += 7_000;
    } else if combined.contains(&format!(" {query_phrase} ")) || combined.starts_with(query_phrase)
    {
        score += 5_000;
    }
    if !query_tokens.is_empty() {
        let hits = query_tokens
            .iter()
            .filter(|token| combined_tokens.iter().any(|cand| cand == *token))
            .count();
        score += 3_000 * hits as i64 / query_tokens.len() as i64;
    }
    score
}

/// Stable sort: highest score first; equal scores keep their original order.
pub fn rank_by_score<T, F>(items: Vec<T>, mut score: F) -> Vec<T>
where
    F: FnMut(&T) -> i64,
{
    let mut scored: Vec<(i64, usize, T)> = items
        .into_iter()
        .enumerate()
        .map(|(index, item)| (score(&item), index, item))
        .collect();
    scored.sort_by(|left, right| right.0.cmp(&left.0).then(left.1.cmp(&right.1)));
    scored.into_iter().map(|(_, _, item)| item).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn percent_encodes_query_terms() {
        assert_eq!(pct("never gonna"), "never%20gonna");
        assert_eq!(pct("周杰伦"), "%E5%91%A8%E6%9D%B0%E4%BC%A6");
        assert_eq!(pct("a&b=c"), "a%26b%3Dc");
    }

    #[test]
    fn json_helpers_tolerate_missing_fields() {
        let value = json!({
            "name": "  Song  ",
            "duration": 1234.5,
            "big": "9007199254740993",
            "nested": { "pic": null, "num": "42" }
        });
        assert_eq!(get_str(&value, "/name"), Some("Song"));
        assert_eq!(get_str(&value, "/missing"), None);
        assert_eq!(get_str(&value, "/nested/pic"), None);
        assert_eq!(get_f64(&value, "/duration"), Some(1234.5));
        assert_eq!(get_f64(&value, "/nested/num"), Some(42.0));
        assert_eq!(get_i64(&value, "/nested/num"), Some(42));
        assert_eq!(get_f64(&value, "/name"), None);
    }

    #[test]
    fn image_urls_are_upgraded_to_https() {
        assert_eq!(
            https_url("http://p1.music.126.net/x.jpg"),
            Some("https://p1.music.126.net/x.jpg".to_owned())
        );
        assert_eq!(
            https_url("https://is1-ssl.mzstatic.com/x.jpg"),
            Some("https://is1-ssl.mzstatic.com/x.jpg".to_owned())
        );
        assert_eq!(https_url("  "), None);
        assert_eq!(https_url("data:image/png;base64,xx"), None);
    }

    #[test]
    fn artist_names_are_joined_without_blanks() {
        assert_eq!(
            join_names(["Beyond", "", "Beyond", "黄家驹"]),
            "Beyond, 黄家驹"
        );
        assert_eq!(join_names(Vec::<&str>::new()), "");
    }

    #[test]
    fn durations_and_years_are_sane() {
        assert_eq!(seconds_from_ms(213_573.0), Some(213.573));
        assert_eq!(seconds_from_ms(0.0), None);
        assert_eq!(seconds_from_ms(-5.0), None);
        assert_eq!(seconds_from_ms(40.0 * 3600.0 * 1000.0), None);
        assert_eq!(year_from_iso("1987-11-12T00:00:00Z"), Some(1987));
        assert_eq!(year_from_iso("not-a-date"), None);
        // 1994-12-31T16:00:00Z (local midnight in UTC+8, see netease.rs).
        assert_eq!(year_from_unix_ms(788_889_600_004), Some(1994));
        assert_eq!(year_from_unix_ms(1_700_000_000_000), Some(2023));
        assert_eq!(year_from_unix_ms(-1), None);
    }

    #[test]
    fn tidy_text_collapses_whitespace_and_entities() {
        assert_eq!(tidy_text("  A &amp; B\u{00a0} "), "A & B");
        assert_eq!(tidy_text("Tom &amp; Jerry"), "Tom & Jerry");
        assert_eq!(tidy_text("plain"), "plain");
    }
    #[test]
    fn relevance_normalization_is_case_and_punctuation_insensitive() {
        assert_eq!(
            normalize_match("Never Gonna Give You Up (Official Video)"),
            " never gonna give you up official video "
        );
        assert_eq!(normalize_match("  "), " ");
        assert_eq!(
            match_tokens("Porter Robinson, Madeon"),
            ["porter", "robinson", "madeon"]
        );
    }

    #[test]
    fn artist_name_relevance_orders_exact_prefix_contains_tokens() {
        let query = "Porter Robinson";
        let exact = name_relevance(query, "Porter Robinson");
        let prefix = name_relevance(query, "Porter Robinson Tribute Band");
        let contains = name_relevance(query, "The Porter Robinson Experience");
        let tokens = name_relevance(query, "Robinson Porter");
        let unrelated = name_relevance(query, "Daft Punk");
        assert_eq!(exact, 10_000);
        assert!(exact > prefix, "{exact} vs {prefix}");
        assert!(prefix > contains, "{prefix} vs {contains}");
        assert!(contains > tokens, "{contains} vs {tokens}");
        assert!(tokens > unrelated, "{tokens} vs {unrelated}");
        assert_eq!(unrelated, 0);
        // Fewer extra tokens wins a prefix tie.
        assert!(
            name_relevance(query, "Porter Robinson Live")
                > name_relevance(query, "Porter Robinson Tribute Band")
        );
        // Case/punctuation do not matter.
        assert_eq!(name_relevance("porter robinson", "PORTER ROBINSON"), 10_000);
    }

    #[test]
    fn track_relevance_prefers_exact_then_prefix_then_tokens() {
        let query = "Never Gonna Give You Up Rick Astley";
        let exact = track_relevance(query, "Never Gonna Give You Up Rick Astley", "");
        let coverage = track_relevance(query, "Never Gonna Give You Up", "Rick Astley");
        let partial = track_relevance(query, "Never Gonna Give You Up", "Some Cover Band");
        let unrelated = track_relevance(query, "Bohemian Rhapsody", "Queen");
        assert!(exact >= coverage, "{exact} vs {coverage}");
        assert!(coverage > partial, "{coverage} vs {partial}");
        assert!(partial > unrelated, "{partial} vs {unrelated}");
        assert_eq!(track_relevance("", "Song", "Artist"), 0);
        // Exact title match ranks above a contains-only artist match.
        assert!(
            track_relevance("Nurture", "Nurture", "")
                > track_relevance("Nurture", "X", "Nurture Tribute")
        );
    }

    #[test]
    fn rank_by_score_is_stable_for_ties() {
        #[derive(Debug, PartialEq)]
        struct Item(i32, &'static str);
        let items = vec![Item(1, "b"), Item(2, "a"), Item(1, "c"), Item(3, "d")];
        let ranked = rank_by_score(items, |item| i64::from(item.0));
        assert_eq!(ranked[0].1, "d");
        assert_eq!(ranked[1], Item(2, "a"));
        assert_eq!(ranked[2], Item(1, "b"), "ties keep original order");
        assert_eq!(ranked[3], Item(1, "c"));
    }
    #[test]
    fn release_dates_are_parsed_from_platform_shapes() {
        // Apple / Spotify ISO timestamps and plain dates.
        assert_eq!(
            date_from_iso("1987-11-12T00:00:00Z").as_deref(),
            Some("1987-11-12")
        );
        assert_eq!(
            date_from_iso("1987-07-27T07:00:00Z").as_deref(),
            Some("1987-07-27")
        );
        assert_eq!(date_from_iso("2021-02-10").as_deref(), Some("2021-02-10"));
        // yt-dlp upload_date compact form.
        assert_eq!(date_from_iso("20210129").as_deref(), Some("2021-01-29"));
        // Invalid dates are rejected instead of guessed.
        assert_eq!(date_from_iso("2021-02-30"), None);
        assert_eq!(date_from_iso("2021-13-01"), None);
        assert_eq!(date_from_iso("2021-02"), None);
        assert_eq!(date_from_iso("not a date"), None);
        assert_eq!(date_from_iso(""), None);
    }

    #[test]
    fn epoch_values_accept_seconds_and_milliseconds() {
        // 1995-01-01T00:00:00+08:00 (Netease local midnight).
        assert_eq!(
            date_from_epoch_value(788_889_600_004, 8 * 3_600).as_deref(),
            Some("1995-01-01")
        );
        // Same instant as plain seconds.
        assert_eq!(
            date_from_epoch_value(788_889_600, 8 * 3_600).as_deref(),
            Some("1995-01-01")
        );
        // UTC vs +08:00 boundary crossing.
        assert_eq!(
            date_from_epoch_value(1_700_000_000, 0).as_deref(),
            Some("2023-11-14")
        );
        assert_eq!(
            date_from_epoch_value(1_700_000_000, 8 * 3_600).as_deref(),
            Some("2023-11-15")
        );
        assert_eq!(date_from_epoch_value(0, 0), None);
        assert_eq!(date_from_epoch_value(-5, 0), None);
    }

    #[test]
    fn calendar_parts_validate_real_dates() {
        assert_eq!(date_from_parts(2021, 2, 10).as_deref(), Some("2021-02-10"));
        assert_eq!(date_from_parts(2020, 2, 29).as_deref(), Some("2020-02-29"));
        assert_eq!(date_from_parts(2021, 2, 29), None, "not a leap year");
        assert_eq!(date_from_parts(2021, 4, 31), None);
        assert_eq!(date_from_parts(999, 1, 1), None);
    }
}
