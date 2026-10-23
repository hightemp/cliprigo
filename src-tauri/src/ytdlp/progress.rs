use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Map, Value};

use super::args::{DOWNLOAD_PREFIX, FILE_PREFIX, POSTPROCESS_PREFIX};

/// One meaningful line of yt-dlp output.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Download(DownloadProgress),
    Postprocess {
        name: String,
        finished: bool,
    },
    /// Final path after all post-processing.
    File(PathBuf),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadStatus {
    Downloading,
    Finished,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DownloadProgress {
    pub status: DownloadStatus,
    pub downloaded: u64,
    /// Exact size, or yt-dlp's estimate when the exact one is unknown.
    pub total: Option<u64>,
    /// Bytes per second.
    pub speed: Option<f64>,
    /// Seconds.
    pub eta: Option<u64>,
    pub filename: Option<PathBuf>,
    pub tmpfilename: Option<PathBuf>,
}

/// Parses one line of output; returns `None` for anything that isn't ours.
pub fn parse_line(line: &str) -> Option<Event> {
    let line = line.trim_end_matches(['\r', '\n']);
    if let Some(json) = line.strip_prefix(DOWNLOAD_PREFIX) {
        return parse_download(&json_object(json)?).map(Event::Download);
    }
    if let Some(json) = line.strip_prefix(POSTPROCESS_PREFIX) {
        let obj = json_object(json)?;
        let name = obj.get("postprocessor")?.as_str()?.to_string();
        let finished = obj.get("status")?.as_str()? == "finished";
        return Some(Event::Postprocess { name, finished });
    }
    let path = line.strip_prefix(FILE_PREFIX)?;
    (!path.trim().is_empty()).then(|| Event::File(PathBuf::from(path)))
}

fn json_object(s: &str) -> Option<Map<String, Value>> {
    match serde_json::from_str(s).ok()? {
        Value::Object(map) => Some(map),
        _ => None,
    }
}

fn parse_download(obj: &Map<String, Value>) -> Option<DownloadProgress> {
    let status = match obj.get("status")?.as_str()? {
        "downloading" => DownloadStatus::Downloading,
        "finished" => DownloadStatus::Finished,
        "error" => DownloadStatus::Error,
        _ => return None,
    };
    // yt-dlp reports some byte counts and estimates as floats.
    let num = |key: &str| {
        obj.get(key)
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite() && *v >= 0.0)
    };
    let path = |key: &str| obj.get(key).and_then(Value::as_str).map(PathBuf::from);
    Some(DownloadProgress {
        status,
        downloaded: num("downloaded_bytes").map_or(0, |v| v as u64),
        total: num("total_bytes")
            .or_else(|| num("total_bytes_estimate"))
            .map(|v| v as u64),
        speed: num("speed"),
        eta: num("eta").map(|v| v as u64),
        filename: path("filename"),
        tmpfilename: path("tmpfilename"),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    Downloading,
    Processing,
}

/// What the UI shows for a running download.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub stage: Stage,
    /// 1-based index of the file being downloaded (video and audio are separate parts).
    pub part: u32,
    /// 0–100 for the current part, `None` when the size is unknown.
    pub percent: Option<f64>,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub speed: Option<f64>,
    pub eta: Option<u64>,
    /// Post-processor currently running, e.g. `Merger`.
    pub postprocessor: Option<String>,
}

/// Folds events into UI snapshots and remembers every file yt-dlp touched,
/// so a canceled download can be cleaned up precisely.
#[derive(Debug)]
pub struct Tracker {
    snapshot: Snapshot,
    /// The previous download event finished a file, so the next one starts a new part.
    part_finished: bool,
    seen: BTreeSet<PathBuf>,
    final_path: Option<PathBuf>,
}

impl Default for Tracker {
    fn default() -> Self {
        Self::new()
    }
}

impl Tracker {
    pub fn new() -> Self {
        Self {
            snapshot: Snapshot {
                stage: Stage::Downloading,
                part: 1,
                percent: None,
                downloaded: 0,
                total: None,
                speed: None,
                eta: None,
                postprocessor: None,
            },
            part_finished: false,
            seen: BTreeSet::new(),
            final_path: None,
        }
    }

    /// Returns a snapshot when the event changes what the UI should show.
    pub fn apply(&mut self, event: &Event) -> Option<Snapshot> {
        match event {
            Event::Download(p) => {
                if p.status == DownloadStatus::Downloading {
                    if self.part_finished {
                        self.snapshot.part += 1;
                        self.part_finished = false;
                    }
                    // Only files actually written in this run are ours to clean up: for an
                    // already downloaded video yt-dlp reports just a "finished" event
                    // pointing at the user's existing file.
                    self.seen.extend(p.filename.iter().cloned());
                }
                self.seen.extend(p.tmpfilename.iter().cloned());
                if p.status == DownloadStatus::Finished {
                    self.part_finished = true;
                }
                let s = &mut self.snapshot;
                s.stage = Stage::Downloading;
                s.postprocessor = None;
                s.downloaded = p.downloaded;
                s.total = p.total;
                s.speed = p.speed;
                s.eta = p.eta;
                s.percent = match (p.status, p.total) {
                    (DownloadStatus::Finished, _) => Some(100.0),
                    (_, Some(total)) if total > 0 => {
                        Some((p.downloaded as f64 / total as f64 * 100.0).clamp(0.0, 100.0))
                    }
                    _ => None,
                };
                Some(s.clone())
            }
            Event::Postprocess { name, finished } => {
                if *finished {
                    return None;
                }
                let s = &mut self.snapshot;
                s.stage = Stage::Processing;
                s.postprocessor = Some(name.clone());
                s.speed = None;
                s.eta = None;
                Some(s.clone())
            }
            Event::File(path) => {
                self.final_path = Some(path.clone());
                None
            }
        }
    }

    pub fn final_path(&self) -> Option<&Path> {
        self.final_path.as_deref()
    }

    /// Files written in this run (format parts and `.part` temp names), excluding the result.
    pub fn seen_files(&self) -> Vec<PathBuf> {
        self.seen
            .iter()
            .filter(|p| Some(p.as_path()) != self.final_path.as_deref())
            .cloned()
            .collect()
    }
}

/// Picks leftovers of an interrupted download from a directory listing.
/// `seen` are paths reported by yt-dlp; only names derived from them are returned,
/// so unrelated files (including a finished earlier download of the same video) survive.
pub fn cleanup_candidates(dir_listing: &[String], seen: &[PathBuf]) -> Vec<String> {
    let seen_names: Vec<String> = seen
        .iter()
        .filter_map(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .collect();
    let bases: Vec<&str> = seen_names
        .iter()
        .filter_map(|n| format_part_base(n))
        .collect();

    dir_listing
        .iter()
        .filter(|name| {
            let name = name.as_str();
            // Exact files written in this run, plus their temp/fragment companions.
            seen_names.iter().any(|s| {
                name == s
                    || name
                        .strip_prefix(s.as_str())
                        .is_some_and(is_temp_suffix)
            })
            // Other format parts (finished ones too: a retry reports parts from an
            // earlier attempt only as "finished") and the merger's temp file.
            || bases.iter().any(|base| {
                name.strip_prefix(base)
                    .and_then(|rest| rest.strip_prefix('.'))
                    .is_some_and(|rest| rest.starts_with("temp.") || is_format_part(rest))
            })
        })
        .cloned()
        .collect()
}

fn is_temp_suffix(s: &str) -> bool {
    s.ends_with(".part") || s.ends_with(".ytdl") || s.contains(".part-Frag")
}

/// `f140.m4a.part` -> true: a format id (`f` + id) followed by an extension.
fn is_format_part(rest: &str) -> bool {
    rest.split_once('.').is_some_and(|(id, _)| {
        id.len() > 1
            && id.starts_with('f')
            && id[1..]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-')
    })
}

/// `Song [id].f394.mp4(.part)` -> `Song [id]`; `None` for names without a format id.
fn format_part_base(name: &str) -> Option<&str> {
    let name = name.strip_suffix(".part").unwrap_or(name);
    let (stem, _ext) = name.rsplit_once('.')?;
    let (base, id) = stem.rsplit_once('.')?;
    is_format_part(&format!("{id}.x")).then_some(base)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/progress.txt");
    const VIDEO_PART: &str = "/dl/Rick Astley - Never Gonna Give You Up (Official Video) (4K Remaster) [dQw4w9WgXcQ].f394.mp4";

    fn events() -> Vec<Event> {
        FIXTURE.lines().filter_map(parse_line).collect()
    }

    #[test]
    fn parses_every_fixture_line() {
        assert_eq!(events().len(), FIXTURE.lines().count());
    }

    #[test]
    fn parses_download_progress() {
        let Event::Download(p) = parse_line(FIXTURE.lines().next().unwrap()).unwrap() else {
            panic!("expected download event");
        };
        assert_eq!(p.status, DownloadStatus::Downloading);
        assert_eq!(p.downloaded, 1024);
        assert_eq!(p.total, Some(1_505_504));
        assert_eq!(p.eta, Some(292));
        assert!((p.speed.unwrap() - 5138.49).abs() < 0.01);
        assert_eq!(p.filename.as_deref(), Some(Path::new(VIDEO_PART)));
        assert_eq!(
            p.tmpfilename.as_deref(),
            Some(Path::new(&format!("{VIDEO_PART}.part")))
        );
    }

    #[test]
    fn parses_postprocess_and_file() {
        let ev = events();
        assert!(ev.contains(&Event::Postprocess {
            name: "Merger".into(),
            finished: false
        }));
        assert!(ev.contains(&Event::Postprocess {
            name: "Merger".into(),
            finished: true
        }));
        assert_eq!(
            ev.last(),
            Some(&Event::File(PathBuf::from(
                "/dl/Rick Astley - Never Gonna Give You Up (Official Video) (4K Remaster) [dQw4w9WgXcQ].mp4"
            )))
        );
    }

    #[test]
    fn handles_estimates_nulls_and_line_endings() {
        let line = r#"CLIPRIGO_DL {"status": "downloading", "downloaded_bytes": 10, "total_bytes": null, "total_bytes_estimate": 2000.7, "speed": null, "eta": null}"#;
        let Some(Event::Download(p)) = parse_line(&format!("{line}\r")) else {
            panic!("expected download event");
        };
        assert_eq!(p.total, Some(2000));
        assert_eq!(p.speed, None);
        assert_eq!(p.eta, None);
        assert_eq!(p.filename, None);
    }

    #[test]
    fn ignores_foreign_and_broken_lines() {
        for line in [
            "",
            "[youtube] Extracting URL: https://www.youtube.com/watch?v=x",
            "WARNING: something",
            "CLIPRIGO_DL not json",
            "CLIPRIGO_PP {\"status\": \"started\"}",
            "CLIPRIGO_FILE ",
            "xCLIPRIGO_DL {}",
        ] {
            assert_eq!(parse_line(line), None, "{line:?}");
        }
    }

    #[test]
    fn file_path_with_spaces_and_unicode() {
        assert_eq!(
            parse_line("CLIPRIGO_FILE /dl/Обзор  ноутбука [x].mp4"),
            Some(Event::File(PathBuf::from("/dl/Обзор  ноутбука [x].mp4")))
        );
    }

    #[test]
    fn tracker_counts_parts_and_stages() {
        let mut t = Tracker::new();
        let snaps: Vec<Snapshot> = events().iter().filter_map(|e| t.apply(e)).collect();

        let first = &snaps[0];
        assert_eq!((first.stage, first.part), (Stage::Downloading, 1));
        assert!((first.percent.unwrap() - 0.068).abs() < 0.001);

        // Audio is the second part; its progress starts over.
        let audio = snaps
            .iter()
            .find(|s| s.part == 2 && s.stage == Stage::Downloading)
            .expect("second part");
        assert_eq!(audio.total, Some(3_449_447));

        let merging = snaps
            .iter()
            .find(|s| s.stage == Stage::Processing)
            .expect("processing stage");
        assert_eq!(merging.postprocessor.as_deref(), Some("Merger"));
        assert_eq!(merging.part, 2);

        assert!(snaps.iter().map(|s| s.part).max() == Some(2));
        assert_eq!(
            t.final_path(),
            Some(Path::new(
                "/dl/Rick Astley - Never Gonna Give You Up (Official Video) (4K Remaster) [dQw4w9WgXcQ].mp4"
            ))
        );
    }

    #[test]
    fn tracker_remembers_seen_files() {
        let mut t = Tracker::new();
        for e in events() {
            t.apply(&e);
        }
        let seen = t.seen_files();
        assert!(seen.contains(&PathBuf::from(VIDEO_PART)));
        assert!(seen.contains(&PathBuf::from(format!("{VIDEO_PART}.part"))));
        assert!(
            seen.iter()
                .any(|p| p.to_string_lossy().ends_with(".f140.m4a"))
        );
        // The final file is not a leftover.
        assert!(
            !seen
                .iter()
                .any(|p| p.to_string_lossy().ends_with("[dQw4w9WgXcQ].mp4"))
        );
    }

    #[test]
    fn cleanup_picks_only_leftovers_of_this_download() {
        let seen = vec![
            PathBuf::from("/dl/Song [id1].f394.mp4"),
            PathBuf::from("/dl/Song [id1].f394.mp4.part"),
        ];
        let listing: Vec<String> = [
            "Song [id1].f394.mp4.part",
            "Song [id1].f394.mp4.ytdl",
            "Song [id1].f394.mp4.part-Frag3",
            "Song [id1].f140.m4a.part",
            // Finished by an earlier attempt, reported only as "finished" on retry.
            "Song [id1].f137.mp4",
            "Song [id1].temp.mp4",
            "Song [id1].mp4",  // finished earlier: keep
            "Song [id1].webp", // unrelated: keep
            "Other [id2].f394.mp4.part",
            "notes.txt",
        ]
        .map(String::from)
        .to_vec();
        let mut got = cleanup_candidates(&listing, &seen);
        got.sort();
        assert_eq!(
            got,
            vec![
                "Song [id1].f137.mp4",
                "Song [id1].f140.m4a.part",
                "Song [id1].f394.mp4.part",
                "Song [id1].f394.mp4.part-Frag3",
                "Song [id1].f394.mp4.ytdl",
                "Song [id1].temp.mp4",
            ]
        );
    }

    #[test]
    fn already_downloaded_file_is_never_a_leftover() {
        // For an existing file yt-dlp only reports "finished" with the final name.
        let line = r#"CLIPRIGO_DL {"status": "finished", "downloaded_bytes": 5, "total_bytes": 5, "filename": "/dl/Song [id1].mp4"}"#;
        let mut t = Tracker::new();
        t.apply(&parse_line(line).unwrap());
        assert!(t.seen_files().is_empty());
        let listing = vec!["Song [id1].mp4".to_string()];
        assert!(cleanup_candidates(&listing, &t.seen_files()).is_empty());
    }

    #[test]
    fn cleanup_with_nothing_seen_removes_nothing() {
        let listing = vec!["a.part".to_string(), "b.mp4".to_string()];
        assert!(cleanup_candidates(&listing, &[]).is_empty());
    }
}
