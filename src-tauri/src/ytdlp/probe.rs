use serde::Serialize;
use serde_json::{Map, Value};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ProbeResult {
    Video(VideoInfo),
    Playlist(PlaylistInfo),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoInfo {
    pub id: String,
    pub url: String,
    pub title: String,
    pub uploader: Option<String>,
    pub duration: Option<i64>,
    pub thumbnail: Option<String>,
    /// Distinct video heights, highest first, for the quality picker.
    pub heights: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistInfo {
    pub id: String,
    pub url: String,
    pub title: String,
    pub uploader: Option<String>,
    pub entries: Vec<PlaylistEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistEntry {
    pub id: String,
    pub url: String,
    pub title: String,
    pub uploader: Option<String>,
    pub duration: Option<i64>,
    pub thumbnail: Option<String>,
    /// `false` for private/deleted entries that can't be downloaded.
    pub available: bool,
    pub view_count: Option<u64>,
    /// A stream on air or scheduled: there is no finished file to download yet.
    pub live: bool,
}

/// Parses `yt-dlp -J --flat-playlist` output.
pub fn parse_probe(json: &str) -> AppResult<ProbeResult> {
    let Value::Object(obj) = serde_json::from_str(json)? else {
        return Err(bad("expected a JSON object"));
    };
    let id = str_field(&obj, "id").ok_or_else(|| bad("missing id"))?;
    let title = str_field(&obj, "title").unwrap_or_else(|| id.clone());
    let url = str_field(&obj, "webpage_url").unwrap_or_default();
    let uploader = uploader(&obj);

    if obj.get("_type").and_then(Value::as_str) == Some("playlist") {
        let entries = obj
            .get("entries")
            .and_then(Value::as_array)
            .map(|e| {
                let mut seen = std::collections::HashSet::new();
                e.iter()
                    .filter_map(Value::as_object)
                    .filter_map(entry)
                    .filter(|e| seen.insert(e.id.clone()))
                    .collect()
            })
            .unwrap_or_default();
        return Ok(ProbeResult::Playlist(PlaylistInfo {
            id,
            url,
            title,
            uploader,
            entries,
        }));
    }

    Ok(ProbeResult::Video(VideoInfo {
        heights: heights(&obj),
        duration: duration(&obj),
        thumbnail: str_field(&obj, "thumbnail").or_else(|| last_thumbnail(&obj)),
        id,
        url,
        title,
        uploader,
    }))
}

/// One page of search results.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchPage {
    pub entries: Vec<PlaylistEntry>,
    pub has_more: bool,
}

/// Parses `yt-dlp -J --flat-playlist ytsearchN:…` output for a page of `page_size`:
/// the videos found. Channels found too are left out, so a full page may list fewer.
pub fn parse_search(json: &str, page_size: usize) -> AppResult<SearchPage> {
    let found = serde_json::from_str::<Value>(json)?
        .get("entries")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    match parse_probe(json)? {
        ProbeResult::Playlist(p) => Ok(SearchPage {
            entries: p.entries,
            has_more: found >= page_size,
        }),
        ProbeResult::Video(_) => Err(bad("expected search results")),
    }
}

/// Titles yt-dlp uses for hidden items in flat playlists.
const UNAVAILABLE_TITLES: &[&str] = &["[Private video]", "[Deleted video]"];
const UNAVAILABLE_AVAILABILITY: &[&str] =
    &["private", "needs_auth", "subscriber_only", "premium_only"];

fn entry(obj: &Map<String, Value>) -> Option<PlaylistEntry> {
    // Nested playlists (e.g. channel tabs) and links to channels or playlists (in
    // search results) are not videos.
    if obj.get("_type").and_then(Value::as_str) == Some("playlist")
        || obj.get("ie_key").and_then(Value::as_str) == Some("YoutubeTab")
    {
        return None;
    }
    let id = str_field(obj, "id")?;
    let title = str_field(obj, "title").unwrap_or_else(|| id.clone());
    let available = !UNAVAILABLE_TITLES.contains(&title.as_str())
        && !obj
            .get("availability")
            .and_then(Value::as_str)
            .is_some_and(|a| UNAVAILABLE_AVAILABILITY.contains(&a));
    Some(PlaylistEntry {
        url: str_field(obj, "url")
            .unwrap_or_else(|| format!("https://www.youtube.com/watch?v={id}")),
        uploader: uploader(obj),
        duration: duration(obj),
        thumbnail: last_thumbnail(obj),
        available,
        view_count: obj.get("view_count").and_then(Value::as_u64),
        live: matches!(
            obj.get("live_status").and_then(Value::as_str),
            Some("is_live" | "is_upcoming")
        ),
        id,
        title,
    })
}

fn str_field(obj: &Map<String, Value>, key: &str) -> Option<String> {
    obj.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
}

fn uploader(obj: &Map<String, Value>) -> Option<String> {
    str_field(obj, "uploader").or_else(|| str_field(obj, "channel"))
}

fn duration(obj: &Map<String, Value>) -> Option<i64> {
    obj.get("duration")
        .and_then(Value::as_f64)
        .filter(|d| d.is_finite() && *d >= 0.0)
        .map(|d| d.round() as i64)
}

/// yt-dlp sorts thumbnails by preference, best last.
fn last_thumbnail(obj: &Map<String, Value>) -> Option<String> {
    obj.get("thumbnails")?
        .as_array()?
        .iter()
        .rev()
        .find_map(|t| t.get("url")?.as_str().map(String::from))
}

fn heights(obj: &Map<String, Value>) -> Vec<u32> {
    let mut heights: Vec<u32> = obj
        .get("formats")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|f| {
            // Storyboards and audio-only formats have vcodec "none".
            f.get("vcodec")
                .and_then(Value::as_str)
                .is_some_and(|v| v != "none")
        })
        .filter_map(|f| f.get("height")?.as_u64())
        .filter_map(|h| u32::try_from(h).ok())
        .collect();
    heights.sort_unstable_by(|a, b| b.cmp(a));
    heights.dedup();
    heights
}

fn bad(msg: &str) -> AppError {
    AppError::Other(format!("unexpected yt-dlp output: {msg}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIDEO: &str = include_str!("../../tests/fixtures/probe_video.json");
    const PLAYLIST: &str = include_str!("../../tests/fixtures/probe_playlist.json");
    const CHANNEL_ROOT: &str = include_str!("../../tests/fixtures/probe_channel_root.json");
    const SEARCH: &str = include_str!("../../tests/fixtures/search.json");

    #[test]
    fn video() {
        let ProbeResult::Video(v) = parse_probe(VIDEO).unwrap() else {
            panic!("expected video");
        };
        assert_eq!(v.id, "dQw4w9WgXcQ");
        assert_eq!(v.url, "https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        assert!(v.title.starts_with("Rick Astley"));
        assert_eq!(v.uploader.as_deref(), Some("Rick Astley"));
        assert_eq!(v.duration, Some(213));
        assert!(v.thumbnail.unwrap().starts_with("https://i.ytimg.com/"));
        // Storyboards (`sb*`, vcodec none) report heights 27/45/90/180 and must be ignored.
        assert_eq!(v.heights, vec![2160, 1440, 1080, 720, 480, 360, 240, 144]);
    }

    #[test]
    fn playlist() {
        let ProbeResult::Playlist(p) = parse_probe(PLAYLIST).unwrap() else {
            panic!("expected playlist");
        };
        assert_eq!(p.id, "PLBCF2DAC6FFB574DE");
        assert_eq!(p.title, "Google Search Stories");
        assert_eq!(p.entries.len(), 4);

        let e = &p.entries[0];
        assert_eq!(e.id, "GvgqDSnpRQM");
        assert_eq!(e.url, "https://www.youtube.com/watch?v=GvgqDSnpRQM");
        assert_eq!(e.duration, Some(90));
        assert!(e.available);
        assert!(e.thumbnail.as_deref().unwrap().contains("hqdefault"));

        let private = p.entries.last().unwrap();
        assert!(!private.available);
        assert_eq!(private.thumbnail, None);
    }

    /// User playlists may hold one video twice; the UI keys rows by id and crashed.
    #[test]
    fn playlist_entries_are_unique() {
        let json = r#"{"_type":"playlist","id":"PL1","title":"t","entries":[
            {"id":"a","url":"https://www.youtube.com/watch?v=a"},
            {"id":"b","url":"https://www.youtube.com/watch?v=b"},
            {"id":"a","url":"https://www.youtube.com/watch?v=a"}]}"#;
        let ProbeResult::Playlist(p) = parse_probe(json).unwrap() else {
            panic!("expected playlist");
        };
        let ids: Vec<_> = p.entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["a", "b"]);
    }

    #[test]
    fn nested_tabs_are_not_videos() {
        // A channel root lists tabs, not videos; they must not show up as entries.
        let ProbeResult::Playlist(p) = parse_probe(CHANNEL_ROOT).unwrap() else {
            panic!("expected playlist");
        };
        assert!(p.entries.is_empty());
    }

    #[test]
    fn minimal_and_odd_values() {
        let ProbeResult::Video(v) = parse_probe(
            r#"{"id":"x","title":"t","channel":"c","duration":12.6,"webpage_url":"u"}"#,
        )
        .unwrap() else {
            panic!("expected video");
        };
        assert_eq!(v.uploader.as_deref(), Some("c"));
        assert_eq!(v.duration, Some(13));
        assert!(v.heights.is_empty());

        // Entries without a URL get a watch link built from the id.
        let ProbeResult::Playlist(p) = parse_probe(
            r#"{"_type":"playlist","id":"p","title":"t","entries":[{"_type":"url","id":"abc","title":"v"}, null]}"#,
        )
        .unwrap() else {
            panic!("expected playlist");
        };
        assert_eq!(p.entries[0].url, "https://www.youtube.com/watch?v=abc");
        assert_eq!(p.url, "");
    }

    #[test]
    fn search_results() {
        let found = parse_search(SEARCH, 20).unwrap();
        assert_eq!(found.entries.len(), 3);
        // Fewer results than asked for: that was the last page.
        assert!(!found.has_more);
        assert!(parse_search(SEARCH, 3).unwrap().has_more);

        let first = &found.entries[0];
        assert_eq!(first.id, "FwNqnodc9eg");
        assert_eq!(first.uploader.as_deref(), Some("4K Planet Earth"));
        assert_eq!(first.view_count, Some(338921));
        assert_eq!(first.url, "https://www.youtube.com/watch?v=FwNqnodc9eg");
        assert!(
            first
                .thumbnail
                .as_deref()
                .unwrap()
                .starts_with("https://i.ytimg.com/")
        );
        // A finished stream is a normal video.
        assert!(!first.live);
        assert!(first.available);
    }

    #[test]
    fn live_streams_are_marked() {
        let found = parse_search(
            r#"{"_type":"playlist","id":"q","entries":[
                {"id":"a","title":"On air","live_status":"is_live"},
                {"id":"b","title":"Soon","live_status":"is_upcoming"},
                {"id":"c","title":"Video"}]}"#,
            20,
        )
        .unwrap();
        let live: Vec<_> = found.entries.iter().map(|e| e.live).collect();
        assert_eq!(live, [true, true, false]);
        assert_eq!(found.entries[2].view_count, None);
    }

    /// Searches also find channels and playlists; they are not videos to download.
    #[test]
    fn channels_are_not_videos() {
        let found = parse_search(
            r#"{"_type":"playlist","id":"q","entries":[
                {"_type":"url","ie_key":"YoutubeTab","id":"UCuAXFkgsw1L7xaCfnd5JJOw",
                 "url":"https://www.youtube.com/channel/UCuAXFkgsw1L7xaCfnd5JJOw","title":"Rick Astley"},
                {"_type":"url","ie_key":"Youtube","id":"dQw4w9WgXcQ","title":"Video"}]}"#,
            2,
        )
        .unwrap();
        let ids: Vec<_> = found.entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["dQw4w9WgXcQ"]);
        // The page was full before the channel was dropped.
        assert!(found.has_more);
    }

    #[test]
    fn invalid_json_is_an_error() {
        assert!(parse_probe("").is_err());
        assert!(parse_probe("[]").is_err());
        assert!(parse_probe(r#"{"title":"no id"}"#).is_err());
    }
}
