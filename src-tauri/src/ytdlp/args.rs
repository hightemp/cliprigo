use std::ffi::OsString;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Line prefixes we ask yt-dlp to print, so our lines can't be confused with its own output.
pub const DOWNLOAD_PREFIX: &str = "CLIPRIGO_DL ";
pub const POSTPROCESS_PREFIX: &str = "CLIPRIGO_PP ";
pub const FILE_PREFIX: &str = "CLIPRIGO_FILE ";

#[derive(Debug, Clone)]
pub struct Tools {
    /// Program to start: the yt-dlp binary, or Python on Android.
    pub ytdlp: PathBuf,
    /// Arguments before yt-dlp's own: the zipapp when Python runs it.
    pub ytdlp_prefix: Vec<OsString>,
    /// Extra environment, e.g. Python's home and libraries on Android.
    pub env: Vec<(OsString, OsString)>,
    pub ffmpeg: PathBuf,
    pub js_runtime: JsRuntime,
    /// Passed as `--proxy`; `None` leaves yt-dlp to the environment.
    pub proxy: Option<String>,
}

/// JavaScript runtime yt-dlp needs for YouTube's player challenges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsRuntime {
    Deno(PathBuf),
    /// Bundled on Android, where Deno has no builds.
    QuickJs(PathBuf),
}

impl JsRuntime {
    /// `--js-runtimes` value.
    fn arg(&self) -> OsString {
        let (name, path) = match self {
            Self::Deno(path) => ("deno:", path),
            Self::QuickJs(path) => ("quickjs:", path),
        };
        let mut arg = OsString::from(name);
        arg.push(path);
        arg
    }
}

/// Preferred video codec. YouTube has H.264 only up to 1080p; above that only VP9/AV1.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Codec {
    /// Best resolution; H.264 when it is as good, so up to 1080p it plays everywhere.
    #[default]
    Auto,
    /// Plays everywhere, even if that means a lower resolution.
    H264,
    /// Smallest files; needs a recent player or an extension on Windows.
    Av1,
    Vp9,
}

impl Codec {
    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::H264 => "h264",
            Self::Av1 => "av1",
            Self::Vp9 => "vp9",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        [Self::Auto, Self::H264, Self::Av1, Self::Vp9]
            .into_iter()
            .find(|c| c.name() == name)
    }

    /// yt-dlp `-S` order. `proto` puts plain https before HLS (m3u8): YouTube's
    /// "Premium" HLS formats are several times larger and crawl through proxies.
    fn sort(self, max_height: Option<u32>) -> String {
        let res = match max_height {
            Some(h) => format!("res:{h}"),
            None => "res".into(),
        };
        match self {
            Self::Auto => format!("{res},proto,vcodec:h264,ext:mp4:m4a"),
            Self::H264 => format!("vcodec:h264,{res},proto,ext:mp4:m4a"),
            Self::Av1 => format!("{res},proto,vcodec:av01,ext:mp4:m4a"),
            Self::Vp9 => format!("{res},proto,vcodec:vp9,ext:mp4:m4a"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Format {
    /// Video + audio merged into mp4; `max_height: None` means best available.
    #[serde(rename_all = "camelCase")]
    Video {
        max_height: Option<u32>,
        #[serde(default)]
        codec: Codec,
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

    /// Inverse of `db_name`/`quality`/`codec`, for retrying a stored download.
    pub fn from_db(name: &str, quality: Option<i64>, codec: Option<&str>) -> Option<Self> {
        Some(match name {
            "video" => Self::Video {
                max_height: quality.and_then(|q| u32::try_from(q).ok()),
                codec: codec.and_then(Codec::from_name).unwrap_or_default(),
            },
            "audio_mp3" => Self::AudioMp3,
            "audio_m4a" => Self::AudioM4a,
            _ => return None,
        })
    }

    /// Value stored in `downloads.quality`.
    pub fn quality(self) -> Option<i64> {
        match self {
            Self::Video { max_height, .. } => max_height.map(i64::from),
            Self::AudioMp3 | Self::AudioM4a => None,
        }
    }

    /// Added to the file name, so each quality gets its own file: " 720p", " best av1".
    fn file_suffix(self) -> String {
        match self {
            Self::Video { max_height, codec } => {
                let mut suffix = match max_height {
                    Some(h) => format!(" {h}p"),
                    None => " best".into(),
                };
                if codec != Codec::Auto {
                    suffix.push(' ');
                    suffix.push_str(codec.name());
                }
                suffix
            }
            Self::AudioMp3 | Self::AudioM4a => String::new(),
        }
    }

    /// Value stored in `downloads.codec`.
    pub fn codec(self) -> Option<&'static str> {
        match self {
            Self::Video { codec, .. } => Some(codec.name()),
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
    let mut args: Vec<OsString> = vec![
        "--no-config".into(),
        "--no-color".into(),
        // The bundled yt-dlp.exe ignores PYTHONIOENCODING and prints in the ANSI
        // code page; we read stdout as UTF-8.
        "--encoding".into(),
        "utf-8".into(),
        "--ffmpeg-location".into(),
        tools.ffmpeg.clone().into(),
        "--js-runtimes".into(),
        tools.js_runtime.arg(),
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
    push(&["-o", &output_template(&req.filename_template, req.format)]);
    match req.format {
        Format::Video { max_height, codec } => {
            let sort = codec.sort(max_height);
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

/// Puts the quality suffix before the last `.%(ext)s`; templates without one are left alone.
fn output_template(template: &str, format: Format) -> String {
    const EXT: &str = ".%(ext)s";
    match template.rfind(EXT) {
        Some(i) => format!(
            "{}{}{}",
            &template[..i],
            format.file_suffix(),
            &template[i..]
        ),
        None => template.to_string(),
    }
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
        ytdlp_prefix: Vec::new(),
        env: Vec::new(),
        ffmpeg: PathBuf::from("/bin/ffmpeg"),
        js_runtime: JsRuntime::Deno(PathBuf::from("/bin/deno")),
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
        // Paths and titles are read as UTF-8; on Windows yt-dlp otherwise prints in
        // the ANSI code page and Cyrillic file names come back as mojibake.
        assert!(has_pair(&args, "--encoding", "utf-8"));
    }

    #[test]
    fn quickjs_runtime_on_android() {
        let mut tools = test_tools();
        tools.js_runtime = JsRuntime::QuickJs(PathBuf::from("/lib/libqjs.so"));
        let args = strings(common_args(&tools));
        assert!(has_pair(&args, "--js-runtimes", "quickjs:/lib/libqjs.so"));
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
            &request(Format::Video {
                max_height: None,
                codec: Codec::Auto,
            }),
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
        assert!(has_pair(&args, "-o", "%(title)s [%(id)s] best.%(ext)s"));
    }

    #[test]
    fn file_name_carries_the_requested_quality() {
        // Otherwise a second download in another quality finds the first file under
        // the same name, and yt-dlp skips it as "already downloaded".
        let video = |max_height, codec| Format::Video { max_height, codec };
        for (format, expected) in [
            (
                video(Some(720), Codec::Auto),
                "%(title)s [%(id)s] 720p.%(ext)s",
            ),
            (
                video(Some(1080), Codec::H264),
                "%(title)s [%(id)s] 1080p h264.%(ext)s",
            ),
            (
                video(None, Codec::Av1),
                "%(title)s [%(id)s] best av1.%(ext)s",
            ),
            // Audio formats already differ by extension.
            (Format::AudioMp3, "%(title)s [%(id)s].%(ext)s"),
            (Format::AudioM4a, "%(title)s [%(id)s].%(ext)s"),
        ] {
            let args = strings(download_args(&test_tools(), &request(format)));
            assert!(has_pair(&args, "-o", expected), "{format:?}");
        }
    }

    #[test]
    fn custom_template_gets_quality_before_the_extension() {
        let format = Format::Video {
            max_height: Some(480),
            codec: Codec::Auto,
        };
        for (template, expected) in [
            (
                "%(uploader)s/%(title)s.%(ext)s",
                "%(uploader)s/%(title)s 480p.%(ext)s",
            ),
            // No extension field: appending would end up after the extension.
            ("%(title)s", "%(title)s"),
        ] {
            let mut req = request(format);
            req.filename_template = template.into();
            let args = strings(download_args(&test_tools(), &req));
            assert!(has_pair(&args, "-o", expected), "{template}");
        }
    }

    #[test]
    fn fragments_are_fetched_in_parallel() {
        for format in [
            Format::Video {
                max_height: None,
                codec: Codec::Auto,
            },
            Format::AudioMp3,
        ] {
            let args = strings(download_args(&test_tools(), &request(format)));
            assert!(has_pair(&args, "-N", "4"));
        }
    }

    #[test]
    fn video_formats() {
        let best = strings(download_args(
            &test_tools(),
            &request(Format::Video {
                max_height: None,
                codec: Codec::Auto,
            }),
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
                codec: Codec::Auto,
            }),
        ));
        assert!(has_pair(&hd, "-S", "res:720,proto,vcodec:h264,ext:mp4:m4a"));
    }

    #[test]
    fn codec_sorts() {
        let sort = |max_height, codec| {
            let args = strings(download_args(
                &test_tools(),
                &request(Format::Video { max_height, codec }),
            ));
            let at = args.iter().position(|a| a == "-S").unwrap();
            args[at + 1].clone()
        };
        // Auto: best resolution first; H.264 only when it is as good.
        assert_eq!(sort(None, Codec::Auto), "res,proto,vcodec:h264,ext:mp4:m4a");
        // Compatible: H.264 first, even at a lower resolution (YouTube stops at 1080p).
        assert_eq!(sort(None, Codec::H264), "vcodec:h264,res,proto,ext:mp4:m4a");
        assert_eq!(
            sort(Some(720), Codec::H264),
            "vcodec:h264,res:720,proto,ext:mp4:m4a"
        );
        assert_eq!(sort(None, Codec::Av1), "res,proto,vcodec:av01,ext:mp4:m4a");
        assert_eq!(
            sort(Some(1080), Codec::Vp9),
            "res:1080,proto,vcodec:vp9,ext:mp4:m4a"
        );
    }

    #[test]
    fn codec_names() {
        for c in [Codec::Auto, Codec::H264, Codec::Av1, Codec::Vp9] {
            assert_eq!(Codec::from_name(c.name()), Some(c));
            let json = serde_json::to_string(&c).unwrap();
            assert_eq!(json, format!("\"{}\"", c.name()));
        }
        assert_eq!(Codec::from_name("hevc"), None);
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
            Format::Video {
                max_height: None,
                codec: Codec::Auto,
            },
            Format::Video {
                max_height: Some(720),
                codec: Codec::H264,
            },
            Format::Video {
                max_height: None,
                codec: Codec::Av1,
            },
            Format::AudioMp3,
            Format::AudioM4a,
        ] {
            assert_eq!(
                Format::from_db(f.db_name(), f.quality(), f.codec()),
                Some(f)
            );
        }
        assert_eq!(Format::from_db("webm", None, None), None);
        // Rows from before codecs (NULL) and unknown values mean Auto.
        for codec in [None, Some("hevc")] {
            assert_eq!(
                Format::from_db("video", Some(-1), codec),
                Some(Format::Video {
                    max_height: None,
                    codec: Codec::Auto
                })
            );
        }
        assert_eq!(Format::AudioMp3.codec(), None);
    }

    #[test]
    fn format_db_mapping_and_json() {
        let best = Format::Video {
            max_height: None,
            codec: Codec::Auto,
        };
        assert_eq!(best.db_name(), "video");
        assert_eq!(best.codec(), Some("auto"));
        assert_eq!(
            Format::Video {
                max_height: Some(1080),
                codec: Codec::Auto,
            }
            .quality(),
            Some(1080)
        );
        assert_eq!(Format::AudioMp3.db_name(), "audio_mp3");
        assert_eq!(Format::AudioM4a.db_name(), "audio_m4a");
        assert_eq!(Format::AudioM4a.quality(), None);

        let f: Format =
            serde_json::from_str(r#"{"kind":"video","maxHeight":720,"codec":"vp9"}"#).unwrap();
        assert_eq!(
            f,
            Format::Video {
                max_height: Some(720),
                codec: Codec::Vp9,
            }
        );
        // Without a codec (older frontend): Auto.
        let f: Format = serde_json::from_str(r#"{"kind":"video","maxHeight":null}"#).unwrap();
        assert_eq!(f, best);
        let f: Format = serde_json::from_str(r#"{"kind":"audio_mp3"}"#).unwrap();
        assert_eq!(f, Format::AudioMp3);
    }
}
