//! Subtitles of a video: the tracks yt-dlp lists and their text, fetched from
//! YouTube as `json3` without another yt-dlp run.

use serde::Serialize;
use serde_json::{Map, Value};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackKind {
    /// Written by the author.
    Manual,
    /// Recognized speech, in the video's language.
    Auto,
    /// Recognized speech translated by YouTube.
    Translated,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleTrack {
    /// Language code as yt-dlp's `--sub-langs` takes it, e.g. `en`, `pt-BR`, `en-orig`.
    pub lang: String,
    /// Language name from YouTube, e.g. "English (Original)".
    pub name: String,
    pub kind: TrackKind,
    /// The `json3` text; see [`is_allowed`].
    pub url: String,
}

/// One phrase, in milliseconds from the start.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cue {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

/// Tracks of a full (not flat) yt-dlp video JSON: the author's first, then
/// recognized speech, then its translations; each group by name.
pub fn tracks(obj: &Map<String, Value>) -> Vec<SubtitleTrack> {
    let mut found = Vec::new();
    let manual = obj.get("subtitles").and_then(Value::as_object);
    for (lang, formats) in manual.into_iter().flatten() {
        // Chat replay of a stream, listed with the subtitles.
        if lang == "live_chat" {
            continue;
        }
        if let Some((name, url)) = json3(formats) {
            found.push((lang, name, TrackKind::Manual, url));
        }
    }
    let auto = obj.get("automatic_captions").and_then(Value::as_object);
    for (lang, formats) in auto.into_iter().flatten() {
        let Some((name, url)) = json3(formats) else {
            continue;
        };
        let kind = if url.contains("tlang=") {
            TrackKind::Translated
        } else {
            // "en-orig" is the speech as recognized; plain "en" repeats it.
            if auto.is_some_and(|a| a.contains_key(&format!("{lang}-orig"))) {
                continue;
            }
            TrackKind::Auto
        };
        found.push((lang, name, kind, url));
    }
    let order = |k: TrackKind| match k {
        TrackKind::Manual => 0,
        TrackKind::Auto => 1,
        TrackKind::Translated => 2,
    };
    found.sort_by(|a, b| order(a.2).cmp(&order(b.2)).then_with(|| a.1.cmp(&b.1)));
    found
        .into_iter()
        .map(|(lang, name, kind, url)| SubtitleTrack {
            lang: lang.clone(),
            name,
            kind,
            url,
        })
        .collect()
}

/// Name and `json3` address of one track's formats.
fn json3(formats: &Value) -> Option<(String, String)> {
    let format = formats
        .as_array()?
        .iter()
        .find(|f| f.get("ext").and_then(Value::as_str) == Some("json3"))?;
    let url = format.get("url").and_then(Value::as_str)?;
    if !is_allowed(url) {
        return None;
    }
    let name = format
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    Some((name, url.to_string()))
}

/// Only YouTube's own subtitle address is fetched for the UI.
pub fn is_allowed(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|u| {
        u.scheme() == "https"
            && u.host_str() == Some("www.youtube.com")
            && u.path() == "/api/timedtext"
    })
}

/// Phrases of a `json3` subtitle file.
pub fn parse_json3(json: &str) -> AppResult<Vec<Cue>> {
    let root: Value = serde_json::from_str(json)
        .map_err(|e| AppError::Other(format!("unreadable subtitles: {e}")))?;
    let events = root
        .get("events")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let mut cues: Vec<Cue> = Vec::new();
    for event in events {
        let Some(segs) = event.get("segs").and_then(Value::as_array) else {
            continue;
        };
        let text: String = segs
            .iter()
            .filter_map(|s| s.get("utf8").and_then(Value::as_str))
            .collect();
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let start = event.get("tStartMs").and_then(Value::as_i64).unwrap_or(0);
        let duration = event
            .get("dDurationMs")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        // Recognized speech overlaps: a phrase stays on screen under the next one.
        if let Some(last) = cues.last_mut() {
            last.end_ms = last.end_ms.min(start).max(last.start_ms + 1);
        }
        cues.push(Cue {
            start_ms: start,
            end_ms: start + duration.max(1),
            text: text.to_string(),
        });
    }
    Ok(cues)
}

/// Where to save subtitles named `name` in `dir`: a safe file name with a known
/// extension, numbered when the name is taken.
pub fn save_path(dir: &std::path::Path, name: &str) -> AppResult<std::path::PathBuf> {
    let safe: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let first = dir.join(&safe);
    check_extension(&first)?;
    let (stem, ext) = safe.rsplit_once('.').unwrap_or((&safe, ""));
    let mut path = first;
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{stem} ({n}).{ext}"));
        n += 1;
    }
    Ok(path)
}

/// Subtitles are saved only as text formats.
pub fn check_extension(path: &std::path::Path) -> AppResult<()> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    match ext.as_deref() {
        Some("txt" | "srt" | "vtt") => Ok(()),
        _ => Err(AppError::Other(format!(
            "subtitles are saved as .txt, .srt or .vtt: {}",
            path.display()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANUAL: &str = include_str!("../../tests/fixtures/subs_manual.json3");
    const AUTO: &str = include_str!("../../tests/fixtures/subs_auto.json3");

    fn track_json(lang: &str, name: &str, translated: bool) -> Value {
        let tlang = if translated { "&tlang=xx" } else { "" };
        serde_json::json!([
            {"ext": "vtt", "url": format!("https://www.youtube.com/api/timedtext?v=x&lang={lang}&fmt=vtt{tlang}"), "name": name},
            {"ext": "json3", "url": format!("https://www.youtube.com/api/timedtext?v=x&lang={lang}&fmt=json3{tlang}"), "name": name},
        ])
    }

    #[test]
    fn tracks_are_grouped_and_sorted() {
        let video = serde_json::json!({
            "subtitles": {
                "en": track_json("en", "English", false),
                "de-DE": track_json("de-DE", "German (Germany)", false),
                // Chat replay of a stream: not subtitles.
                "live_chat": [{"ext": "json", "url": "https://www.youtube.com/live_chat_replay"}],
            },
            "automatic_captions": {
                "en-orig": track_json("en", "English (Original)", false),
                // The same speech again: hidden when there is an "-orig" one.
                "en": track_json("en", "English", false),
                "ru": track_json("ru", "Russian", true),
                "ab": track_json("ab", "Abkhazian", true),
                // No json3: can't be shown.
                "fr": [{"ext": "vtt", "url": "https://www.youtube.com/api/timedtext?v=x&lang=fr&tlang=fr", "name": "French"}],
            },
        });
        let found = tracks(video.as_object().unwrap());
        let summary: Vec<_> = found
            .iter()
            .map(|t| (t.lang.as_str(), t.name.as_str(), t.kind))
            .collect();
        assert_eq!(
            summary,
            [
                ("en", "English", TrackKind::Manual),
                ("de-DE", "German (Germany)", TrackKind::Manual),
                ("en-orig", "English (Original)", TrackKind::Auto),
                ("ab", "Abkhazian", TrackKind::Translated),
                ("ru", "Russian", TrackKind::Translated),
            ]
        );
        assert!(found[0].url.contains("fmt=json3"));
        assert!(found.iter().all(|t| is_allowed(&t.url)));
    }

    #[test]
    fn no_subtitles() {
        assert!(tracks(&Map::new()).is_empty());
    }

    #[test]
    fn only_youtube_subtitle_urls_are_fetched() {
        assert!(is_allowed(
            "https://www.youtube.com/api/timedtext?v=x&lang=en&fmt=json3"
        ));
        for url in [
            "http://www.youtube.com/api/timedtext?v=x",
            "https://www.youtube.com/watch?v=x",
            "https://evil.example/api/timedtext?v=x",
            "https://www.youtube.com.evil.example/api/timedtext",
            "not a url",
        ] {
            assert!(!is_allowed(url), "{url}");
        }
    }

    #[test]
    fn manual_subtitles_text() {
        let cues = parse_json3(MANUAL).unwrap();
        assert_eq!(cues.len(), 6);
        assert_eq!(
            cues[1],
            Cue {
                start_ms: 18640,
                end_ms: 21880,
                text: "Line one of the sample".into()
            }
        );
        // Line breaks inside a phrase stay.
        assert_eq!(cues[2].text, "Line two\nwith a break");
    }

    #[test]
    fn recognized_speech_is_joined_into_phrases() {
        let cues = parse_json3(AUTO).unwrap();
        // The window setup and line-break events carry no text.
        assert!(cues.iter().all(|c| !c.text.trim().is_empty()), "{cues:?}");
        assert_eq!(cues[0].text, "[Music]");
        // Word segments make one phrase.
        assert_eq!(cues[1].text, "Alpha bravo charlie delta");
        // Phrases overlap in json3; each one ends when the next starts.
        for pair in cues.windows(2) {
            assert!(pair[0].end_ms <= pair[1].start_ms, "{pair:?}");
            assert!(pair[0].start_ms < pair[0].end_ms, "{pair:?}");
        }
    }

    #[test]
    fn saved_files_get_safe_free_names() {
        let dir = tempfile::tempdir().unwrap();
        let path = save_path(dir.path(), "AC/DC: Live? <1991>.en.srt").unwrap();
        assert_eq!(path, dir.path().join("AC_DC_ Live_ _1991_.en.srt"));
        std::fs::write(&path, "x").unwrap();
        let next = save_path(dir.path(), "AC/DC: Live? <1991>.en.srt").unwrap();
        assert_eq!(next, dir.path().join("AC_DC_ Live_ _1991_.en (2).srt"));
        // Not a way to leave the folder.
        assert_eq!(
            save_path(dir.path(), "../../etc/x.txt").unwrap(),
            dir.path().join(".._.._etc_x.txt")
        );
    }

    #[test]
    fn only_text_formats_are_saved() {
        for ok in ["a.srt", "a.VTT", "dir/a.txt"] {
            assert!(check_extension(std::path::Path::new(ok)).is_ok(), "{ok}");
        }
        for bad in ["a.exe", "a", "a.srt.sh"] {
            assert!(check_extension(std::path::Path::new(bad)).is_err(), "{bad}");
        }
        assert!(save_path(std::path::Path::new("/tmp"), "a.exe").is_err());
    }

    #[test]
    fn broken_json3() {
        assert!(parse_json3("<html>").is_err());
        assert!(parse_json3("{}").unwrap().is_empty());
    }
}
