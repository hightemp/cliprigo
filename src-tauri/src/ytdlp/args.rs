use std::ffi::OsString;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Line prefixes we ask yt-dlp to print, so our lines can't be confused with its own output.
pub const DOWNLOAD_PREFIX: &str = "CLIPRIGO_DL ";
pub const POSTPROCESS_PREFIX: &str = "CLIPRIGO_PP ";
pub const FILE_PREFIX: &str = "CLIPRIGO_FILE ";

#[derive(Debug, Clone)]
pub struct Tools {
    pub ytdlp: PathBuf,
    pub ffmpeg: PathBuf,
    pub deno: PathBuf,
    /// Passed as `--proxy`; `None` leaves yt-dlp to the environment.
    pub proxy: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Format {
    /// Video + audio merged into mp4; `max_height: None` means best available.
    #[serde(rename_all = "camelCase")]
    Video {
        max_height: Option<u32>,
    },
    AudioMp3,
    AudioM4a,
}

impl Format {
    /// Value stored in `downloads.format`.
    pub fn db_name(self) -> &'static str {
        match self {
            Self::Video { .. } => "video",
            Self::AudioMp3 => "audio_mp3",
            Self::AudioM4a => "audio_m4a",
        }
    }

    /// Inverse of `db_name`/`quality`, for retrying a stored download.
    pub fn from_db(name: &str, quality: Option<i64>) -> Option<Self> {
        Some(match name {
            "video" => Self::Video {
                max_height: quality.and_then(|q| u32::try_from(q).ok()),
            },
            "audio_mp3" => Self::AudioMp3,
            "audio_m4a" => Self::AudioM4a,
            _ => return None,
        })
    }

    /// Value stored in `downloads.quality`.
    pub fn quality(self) -> Option<i64> {
        match self {
            Self::Video { max_height } => max_height.map(i64::from),
            Self::AudioMp3 | Self::AudioM4a => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DownloadRequest {
    pub url: String,
    pub format: Format,
    pub output_dir: PathBuf,
    pub filename_template: String,
}

/// Options shared by every invocation: ignore user config files, use our bundled tools.
pub fn common_args(tools: &Tools) -> Vec<OsString> {
    let mut deno = OsString::from("deno:");
    deno.push(&tools.deno);
    let mut args: Vec<OsString> = vec![
        "--no-config".into(),
        "--no-color".into(),
        "--ffmpeg-location".into(),
        tools.ffmpeg.clone().into(),
        "--js-runtimes".into(),
        deno,
    ];
    if let Some(proxy) = &tools.proxy {
        args.extend(["--proxy".into(), proxy.into()]);
    }
    args
}

pub fn probe_args(tools: &Tools, url: &str) -> Vec<OsString> {
    let mut args = common_args(tools);
    args.extend([
        "-J".into(),
        "--flat-playlist".into(),
        "--no-warnings".into(),
    ]);
    if super::url::is_single_video(url) {
        args.push("--no-playlist".into());
    }
    push_url(&mut args, url);
    args
}

pub fn download_args(tools: &Tools, req: &DownloadRequest) -> Vec<OsString> {
    let mut args = common_args(tools);
    let mut push = |items: &[&str]| args.extend(items.iter().map(OsString::from));
    // Playlists are expanded into separate queue items, so each run fetches one video.
    push(&[
        "--no-playlist",
        "--newline",
        "--progress",
        "--embed-metadata",
    ]);
    // Fragmented formats (HLS/DASH) download one tiny piece at a time otherwise.
    push(&["-N", "4"]);
    push(&[
        "--progress-template",
        &format!("download:{DOWNLOAD_PREFIX}%(progress)j"),
    ]);
    push(&[
        "--progress-template",
        &format!("postprocess:{POSTPROCESS_PREFIX}%(progress)j"),
    ]);
    push(&["--print", &format!("after_move:{FILE_PREFIX}%(filepath)s")]);
    push(&["-o", &req.filename_template]);
    match req.format {
        Format::Video { max_height } => {
            let sort = match max_height {
                // `proto` prefers plain https over HLS (m3u8): YouTube's "Premium"
                // HLS formats are several times larger and crawl through proxies.
                // H.264 plays everywhere; AV1 needs an extra codec on Windows 10.
                Some(h) => format!("res:{h},proto,vcodec:h264,ext:mp4:m4a"),
                None => "res,proto,vcodec:h264,ext:mp4:m4a".to_string(),
            };
            push(&[
                "-f",
                "bv*+ba/b",
                "-S",
                &sort,
                "--merge-output-format",
                "mp4",
            ]);
        }
        Format::AudioMp3 => {
            push(&[
                "-f",
                "ba/b",
                "-x",
                "--audio-format",
                "mp3",
                "--audio-quality",
                "0",
            ]);
        }
        Format::AudioM4a => push(&["-f", "ba[ext=m4a]/ba/b", "-x", "--audio-format", "m4a"]),
    }
    args.push("-P".into());
    args.push(req.output_dir.clone().into());
    push_url(&mut args, &req.url);
    args
}

/// `--` stops option parsing, so a pasted "URL" can never inject yt-dlp options.
fn push_url(args: &mut Vec<OsString>, url: &str) {
    args.push("--".into());
    args.push(url.into());
}

#[cfg(test)]
pub(crate) fn test_tools() -> Tools {
    Tools {
        ytdlp: PathBuf::from("/bin/yt-dlp"),
        ffmpeg: PathBuf::from("/bin/ffmpeg"),
        deno: PathBuf::from("/bin/deno"),
        proxy: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: Vec<OsString>) -> Vec<String> {
        args.into_iter().map(|a| a.into_string().unwrap()).collect()
    }

    /// True when `pair` appears as adjacent arguments.
    fn has_pair(args: &[String], flag: &str, value: &str) -> bool {
        args.windows(2).any(|w| w[0] == flag && w[1] == value)
    }

    fn request(format: Format) -> DownloadRequest {
        DownloadRequest {
            url: "https://www.youtube.com/watch?v=dQw4w9WgXcQ".into(),
            format,
            output_dir: PathBuf::from("/home/u/Downloads"),
            filename_template: "%(title)s [%(id)s].%(ext)s".into(),
        }
    }

    #[test]
    fn common_args_use_bundled_tools_and_ignore_config() {
        let args = strings(common_args(&test_tools()));
        assert!(args.contains(&"--no-config".to_string()));
        assert!(has_pair(&args, "--ffmpeg-location", "/bin/ffmpeg"));
        assert!(has_pair(&args, "--js-runtimes", "deno:/bin/deno"));
    }

    #[test]
    fn proxy_is_passed_when_set() {
        assert!(!strings(common_args(&test_tools())).contains(&"--proxy".to_string()));
        let mut tools = test_tools();
        tools.proxy = Some("http://127.0.0.1:8888".into());
        let args = strings(probe_args(&tools, "https://youtu.be/x"));
        assert!(has_pair(&args, "--proxy", "http://127.0.0.1:8888"));
        let args = strings(download_args(&tools, &request(Format::AudioMp3)));
        assert!(has_pair(&args, "--proxy", "http://127.0.0.1:8888"));
    }

    #[test]
    fn probe_args_for_playlist_and_single_video() {
        let args = strings(probe_args(
            &test_tools(),
            "https://www.youtube.com/playlist?list=PLBCF2DAC6FFB574DE",
        ));
        assert!(args.contains(&"-J".to_string()));
        assert!(args.contains(&"--flat-playlist".to_string()));
        assert!(!args.contains(&"--no-playlist".to_string()));
        assert_eq!(
            &args[args.len() - 2..],
            [
                "--",
                "https://www.youtube.com/playlist?list=PLBCF2DAC6FFB574DE"
            ]
        );

        let args = strings(probe_args(
            &test_tools(),
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ&list=PLBCF2DAC6FFB574DE",
        ));
        assert!(args.contains(&"--no-playlist".to_string()));
    }

    #[test]
    fn url_always_follows_double_dash() {
        // A "URL" that looks like an option must never be parsed as one.
        let mut req = request(Format::AudioMp3);
        req.url = "--exec=rm -rf ~".into();
        let args = strings(download_args(&test_tools(), &req));
        assert_eq!(&args[args.len() - 2..], ["--", "--exec=rm -rf ~"]);
        let args = strings(probe_args(&test_tools(), "--exec=x"));
        assert_eq!(&args[args.len() - 2..], ["--", "--exec=x"]);
    }

    #[test]
    fn download_args_report_progress_and_final_path() {
        let args = strings(download_args(
            &test_tools(),
            &request(Format::Video { max_height: None }),
        ));
        for flag in [
            "--no-playlist",
            "--newline",
            "--progress",
            "--embed-metadata",
        ] {
            assert!(args.contains(&flag.to_string()), "{flag}");
        }
        assert!(has_pair(
            &args,
            "--progress-template",
            "download:CLIPRIGO_DL %(progress)j"
        ));
        assert!(has_pair(
            &args,
            "--progress-template",
            "postprocess:CLIPRIGO_PP %(progress)j"
        ));
        assert!(has_pair(
            &args,
            "--print",
            "after_move:CLIPRIGO_FILE %(filepath)s"
        ));
        assert!(has_pair(&args, "-P", "/home/u/Downloads"));
        assert!(has_pair(&args, "-o", "%(title)s [%(id)s].%(ext)s"));
    }

    #[test]
    fn fragments_are_fetched_in_parallel() {
        for format in [Format::Video { max_height: None }, Format::AudioMp3] {
            let args = strings(download_args(&test_tools(), &request(format)));
            assert!(has_pair(&args, "-N", "4"));
        }
    }

    #[test]
    fn video_formats() {
        let best = strings(download_args(
            &test_tools(),
            &request(Format::Video { max_height: None }),
        ));
        assert!(has_pair(&best, "-f", "bv*+ba/b"));
        // proto: progressive https beats HLS (m3u8), which is fetched in tiny
        // fragments and was ~90x slower through a proxy.
        // vcodec:h264: plays everywhere; AV1 needs an extension on Windows 10.
        assert!(has_pair(&best, "-S", "res,proto,vcodec:h264,ext:mp4:m4a"));
        assert!(has_pair(&best, "--merge-output-format", "mp4"));
        assert!(!best.contains(&"-x".to_string()));

        let hd = strings(download_args(
            &test_tools(),
            &request(Format::Video {
                max_height: Some(720),
            }),
        ));
        assert!(has_pair(&hd, "-S", "res:720,proto,vcodec:h264,ext:mp4:m4a"));
    }

    #[test]
    fn audio_formats() {
        let mp3 = strings(download_args(&test_tools(), &request(Format::AudioMp3)));
        assert!(has_pair(&mp3, "-f", "ba/b"));
        assert!(mp3.contains(&"-x".to_string()));
        assert!(has_pair(&mp3, "--audio-format", "mp3"));
        assert!(has_pair(&mp3, "--audio-quality", "0"));
        assert!(!mp3.contains(&"--merge-output-format".to_string()));

        let m4a = strings(download_args(&test_tools(), &request(Format::AudioM4a)));
        assert!(has_pair(&m4a, "-f", "ba[ext=m4a]/ba/b"));
        assert!(has_pair(&m4a, "--audio-format", "m4a"));
    }

    #[test]
    fn format_from_db_roundtrip() {
        for f in [
            Format::Video { max_height: None },
            Format::Video {
                max_height: Some(720),
            },
            Format::AudioMp3,
            Format::AudioM4a,
        ] {
            assert_eq!(Format::from_db(f.db_name(), f.quality()), Some(f));
        }
        assert_eq!(Format::from_db("webm", None), None);
        assert_eq!(
            Format::from_db("video", Some(-1)),
            Some(Format::Video { max_height: None })
        );
    }

    #[test]
    fn format_db_mapping_and_json() {
        assert_eq!(Format::Video { max_height: None }.db_name(), "video");
        assert_eq!(
            Format::Video {
                max_height: Some(1080)
            }
            .quality(),
            Some(1080)
        );
        assert_eq!(Format::AudioMp3.db_name(), "audio_mp3");
        assert_eq!(Format::AudioM4a.db_name(), "audio_m4a");
        assert_eq!(Format::AudioM4a.quality(), None);

        let f: Format = serde_json::from_str(r#"{"kind":"video","maxHeight":720}"#).unwrap();
        assert_eq!(
            f,
            Format::Video {
                max_height: Some(720)
            }
        );
        let f: Format = serde_json::from_str(r#"{"kind":"audio_mp3"}"#).unwrap();
        assert_eq!(f, Format::AudioMp3);
    }
}
