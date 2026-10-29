# EchoPort Provider Support / 音源支持说明

> **中文摘要** — EchoPort 采用「目录与音频分离」的零账号方案：**元数据/搜索**来自各平台的公开接口，
> **音频**统一交给外部工具 yt-dlp（必要时用 ffmpeg 转码）获取公开可播放的内容。
> YouTube 是通用音频后端；SoundCloud / Bilibili / 网易云通过 yt-dlp 搜索与解析；
> Apple Music 通过公开 iTunes Search API 搜索（GUI 中与网易云一致显示「可搜索」）；其自身完整音频受 FairPlay DRM 保护，EchoPort 通过 yt-dlp 匹配完整时长的公开音频（不绕过 DRM）；
> Spotify 没有匿名搜索（官方接口返回 401/403），只支持「粘贴歌曲链接导入」；完整播放同样通过 yt-dlp 匹配公开音频，30 秒预览仅作后备（Widevine DRM 不绕过）。
> EchoPort **不提供、也不会提供任何 DRM 绕过**：不内置 Widevine/FairPlay、不保存账号令牌、不解密音频。
> 各平台都可能因地区限制、版权变化或接口调整而失败；失败时会显示错误提示而不是崩溃。
> 下载功能只应在你拥有相应权利、且符合目标网站服务条款的前提下使用。

This page explains, in end-user terms, which sources EchoPort supports, what
works, what deliberately does not, and why. It is adapted from the project's
zero-auth provider research (verified 2026-10-29 on Arch Linux with
yt-dlp 2026.10.21 and ffmpeg 9.0.2).

---

## The zero-auth model

EchoPort never asks for an account, a cookie, a token or a password. That has
two consequences:

1. **Catalogue and audio are separate.** Search results and metadata come from
   public, keyless APIs (or yt-dlp searches). Audio bytes come from yt-dlp
   resolving a *public* source (mostly YouTube), or from a public 30 s preview
   CDN. They are tied together by track metadata, not by an account.
2. **Some sources can only be imported by link.** Spotify removed anonymous
   search; Apple Music audio is FairPlay-protected. For those, EchoPort imports
   public metadata from a pasted link and matches playback to public audio. This
   is intentional and is not a bug.

EchoPort ships **no DRM circumvention**: no Widevine/FairPlay libraries, no
decryption keys, no "downloader accounts". Only content that is publicly
playable without an account can be played.

---

## Support matrix

Legend: ✅ works · ⚠️ partial / best effort · ❌ not possible (by design)

| Platform | Search | Playback | Cover / metadata | Paste link | Notes |
|---|---|---|---|---|---|
| **Local files** | folder scan | ✅ native decode | ✅ embedded tags/art | — | mp3, m4a/aac, alac, flac, wav, ogg; offline |
| **YouTube** | ✅ `ytsearch` | ✅ full (via yt-dlp) | ✅ | ✅ | universal audio backend; extractors change often — keep yt-dlp updated |
| **SoundCloud** | ✅ `scsearch` | ⚠️ many tracks DRM/HLS-only; last resort is a YouTube match | ✅ | ✅ | errors/hints only after the YouTube fallback also fails |
| **Bilibili** | ⚠️ `bilisearch`, sometimes HTTP 412 | ⚠️ audio only, often region-locked; last resort is a YouTube match | ✅ | ✅ | 412/geo may still play via the public-audio fallback |
| **Netease 网易云** | ✅ public search API | ⚠️ via yt-dlp (≈128 kbps mp3) | ✅ | ✅ | unlicensed/legally gray in some regions; opt-in by using it |
| **Apple Music** | ✅ public iTunes Search API (shown as Searchable, same as Netease) | ✅ full (yt-dlp public-audio match) | ✅ | ✅ | Apple's own audio is FairPlay DRM — not bypassed |
| **Spotify** | ❌ no anonymous search | ✅ full (yt-dlp public-audio match; 30 s preview fallback) | ✅ (from link) | ✅ only | search tokens + Widevine DRM → not used |
| **Deezer** | (not exposed) | (not exposed) | — | — | a viable fallback catalogue for future versions |
| **YouTube Music** | via plain YouTube | via YouTube | ✅ | ✅ | use a normal YouTube link |

Searches are per-platform: one platform failing shows an inline error and never
breaks the others. Results are capped by **Settings → Results per platform**
(default 12, clamp 4–40).

---

## Platform details

### Local files

* Recursive folder scan (multiple folders), skips hidden directories and symlink
  loops. Tags via `lofty`; missing tags fall back to the file name. Embedded
  cover art is extracted and cached.
* Decoded natively by symphonia (mp3, aac/m4a, alac, flac, wav, ogg/vorbis).
  Anything else (e.g. opus-in-webm from a web cache) is transcoded to m4a by
  ffmpeg before playback.
* No network required.

### YouTube (primary audio backend)

* Search: `yt-dlp "ytsearchN:<query>"` (flat playlist, ~1.2 s for 5 results).
* Playback: yt-dlp downloads/extracts the best audio stream into EchoPort's
  cache (audio-only). A first-time resolve is typically a few seconds; cached
  tracks start instantly.
* Why yt-dlp: resolved media URLs are bound to the yt-dlp session/IP and return
  HTTP 403 to normal HTTP clients, and YouTube frequently changes its stream
  protection. Letting yt-dlp download the file is the only reliable approach —
  EchoPort never hands a resolved URL to its HTTP client.
* Caveat: YouTube's terms restrict downloading. Keep yt-dlp current; some videos
  (age/region/login gated) will fail and show an error.

### SoundCloud

* Search: `yt-dlp "scsearchN:<query>"`. Playback/import by URL.
* Many tracks are DRM-protected or preview-only, and some are HLS (`m3u8`) that
  requires ffmpeg. Expect a noticeable failure rate; EchoPort reports failures
  per track instead of aborting the search.

### Bilibili

* Search: `yt-dlp "bilisearchN:<query>"`; playback extracts audio only.
* Bilibili can answer search with HTTP 412 (rate limiting) and much content is
  geo-restricted. When blocked, the platform group shows an error; other
  platforms still work.

### Netease 网易云音乐

* Search uses the public endpoint `music.163.com/api/search/get` (no account).
* Playback/import resolves `music.163.com/song?id=<id>` through yt-dlp, falling
  back to a YouTube match when needed.
* Legal note: these are unlicensed/public endpoints whose availability and
  legality vary by region. Use it only where you have the right to listen to /
  download the content, and respect Netease's terms.

### Apple Music

* Search works through the public **iTunes Search API** (song search, artwork,
  preview URL, duration). Search currently targets the US iTunes store; prices
  and even availability can differ by country.
* Full playback: Apple's own stream is **FairPlay DRM** and yt-dlp does not support
  `music.apple.com` links, so EchoPort never streams or decrypts it. Instead it:
  * imports metadata from `music.apple.com/.../…?i=<trackId>` links,
  * resolves **full-length playback through yt-dlp** by matching the track to public
    audio (title/artist/duration sanity checks), usually a complete song,
  * falls back to the official **30 s preview** (a public CDN file) only when no
    public match is available (e.g. yt-dlp missing or no result).
  Matches are validated by title/artist/duration and can occasionally pick a live or
  alternate version. A previously cached 30 s preview is never allowed to shadow the
  full match once yt-dlp is available.

### Spotify

* **No anonymous search.** Spotify's token endpoint returns 403 and its APIs
  return 401 from a normal client; the web search page is client-rendered.
  EchoPort therefore shows a clear hint instead of fake results.
* What works: paste an `open.spotify.com/track/<id>` link. EchoPort reads the
  public embed page metadata (title, artists, duration, cover, release date,
  30 s preview), then resolves **full-length playback through yt-dlp** against
  public audio; the 30 s preview is only the fallback. Spotify's own Widevine
  stream is never decrypted.
* Full Spotify audio is Widevine/DRM protected; EchoPort does **not** implement
  that. Any tool claiming otherwise relies on account credentials and
  decryption — out of scope for this project.

### Artist search & artist pages (v0.1.5)

Search now also returns ranked artists; single-click a card opens an artist page.

* **Netease**: public artist search (`/api/search/get?s=<q>&type=100`) → artist id/name/pic; hot tracks via
  the public artist endpoint (`/api/artist/{id}`) with title/album/duration/page URLs.
* **Apple Music**: iTunes Search API `entity=musicArtist` → `artistId`/`artistName`/`artistLinkUrl`; hot tracks
  via `itunes.apple.com/lookup?id=<artistId>&entity=song&limit=12`, filtered to the same artist.
* **YouTube / SoundCloud / Bilibili**: derived from the platform track search — unique channel/uploader names
  matching the query tokens become artist cards (page URL = platform search URL for the name); hot tracks are
  `yt-dlp` platform searches filtered to the artist name.
* **Spotify**: no anonymous search → no artists; the link-import hint stays.
* **Bilibili 412**: during artist search/playback 412/567 responses map to the bilingual friendly hint;
  playback still tries the last-resort YouTube public-audio match.
* Per-platform track lists are re-ranked by relevance so the collapsed/expanded sections are meaningful.

Latency: tracks + artists are fetched in parallel (~2.5 s for six platforms, previously ~5 s sequential).

## Release dates (v0.1.6)

`Track.release_date` is filled where the public metadata provides it: Apple iTunes Search
(`releaseDate`), Netease search/artist songs (album publish time), Spotify embed page (`releaseDate`),
full yt-dlp imports (`release_date`/`upload_date`) and local file tags. Flat YouTube/SoundCloud/Bilibili
search results may not expose a date, so the NowPlaying page shows `—` for those until a richer import
fills it. Dates are kept with remembered tracks in the profile JSON.

## Background information (keyless)

The **Now playing → Background** tab uses only public, keyless sources:

* Wikipedia REST summary for the artist (biography + source article link);
* MusicBrainz for disambiguation/verification (low request rate, descriptive
  User-Agent);
* Genius **search** links (EchoPort opens Genius; it does not scrape lyrics);
* YouTube search links for interviews / song explanations (`<artist> <song>
  interview`), ranked by relevance/views.

Nothing here requires an API key. Links open in your default browser.

---

## Troubleshooting providers

| Symptom | Likely cause / fix |
|---|---|
| "yt-dlp was not found" | Install yt-dlp (`pacman -S yt-dlp`, `apt install yt-dlp`, or download the release binary) and press **Settings → Detect again**. |
| YouTube playback suddenly fails after months of working | Extractors changed: update yt-dlp (`yt-dlp -U` or your package manager). |
| Search returns errors for one platform only | That platform is rate-limited/region-blocked; the other groups still work. |
| SoundCloud track fails with a DRM error | The app first tries a YouTube public-audio match; if that also fails, try another result or the source page. |
| Bilibili returns HTTP 412 | The app automatically retries with a YouTube public-audio match; if every source fails, retry later or use another source. |
| Spotify search shows a hint instead of results | By design: paste a Spotify link (see above). |
| Apple Music plays a matched version | Apple's own audio is DRM-protected. EchoPort matches full public audio via yt-dlp (usually full length; can be a live/cover version). The GUI shows Apple as Searchable like Netease. |
| Netease track does not resolve | The song may be region-restricted or removed; try a YouTube match. |

---

## Legal & ethical notes

* Only download music you have the right to download, and respect the terms of
  service of each site (YouTube, SoundCloud, Bilibili, Netease, …).
* EchoPort does not host, bundle or redistribute any music. It orchestrates
  your own installed yt-dlp/ffmpeg against public endpoints.
* EchoPort deliberately does not circumvent DRM and does not store credentials.
* Providers and regions can change at any time; the support matrix above
  reflects what was verified on 2026-10-29 and may drift as services change.
* The GPL-3.0 license covers EchoPort's code, not the content you choose to access.
