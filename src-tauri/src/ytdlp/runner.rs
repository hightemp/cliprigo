use std::collections::{BTreeMap, VecDeque};
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::args::{self, DownloadRequest, Tools};
use super::errors::{self, ErrorKind};
use super::probe::{self, ProbeResult};
use super::progress::{self, Snapshot, Tracker};
use crate::error::{AppError, AppResult};

const PROBE_TIMEOUT: Duration = Duration::from_secs(120);
/// Stderr lines kept for error classification.
const STDERR_TAIL: usize = 50;

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Completed { path: PathBuf, size: Option<u64> },
    Canceled,
}

/// Fetches metadata for a video or a playlist.
pub async fn probe(tools: &Tools, url: &str) -> AppResult<ProbeResult> {
    let output = tokio::time::timeout(
        PROBE_TIMEOUT,
        crate::process::output(&mut build_command(tools, args::probe_args(tools, url))),
    )
    .await
    .map_err(|_| AppError::Ytdlp {
        kind: ErrorKind::Network,
        message: "timed out while fetching video info".into(),
    })??;
    if !output.status.success() {
        return Err(errors::classify(&String::from_utf8_lossy(&output.stderr)).into());
    }
    probe::parse_probe(&String::from_utf8_lossy(&output.stdout))
}

/// Runs one download, streaming progress snapshots until it finishes or `cancel` fires.
/// On cancel the whole process tree (yt-dlp and its ffmpeg) is killed and the files
/// written by this run are removed.
pub async fn download(
    tools: &Tools,
    req: &DownloadRequest,
    cancel: CancellationToken,
    mut on_progress: impl FnMut(Snapshot) + Send,
) -> AppResult<Outcome> {
    if cancel.is_cancelled() {
        return Ok(Outcome::Canceled);
    }
    let mut child =
        crate::process::spawn(&mut build_command(tools, args::download_args(tools, req))).await?;
    let (tx, mut rx) = mpsc::unbounded_channel();
    spawn_reader(child.stdout.take(), Stream::Stdout, tx.clone());
    spawn_reader(child.stderr.take(), Stream::Stderr, tx);

    let mut tracker = Tracker::new();
    let mut stderr_tail = VecDeque::with_capacity(STDERR_TAIL);
    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => return Ok(abort(&mut child, &tracker).await),
            msg = rx.recv() => {
                // Both streams closed: the process is done writing.
                let Some((stream, line)) = msg else { break };
                match progress::parse_line(&line) {
                    Some(event) => {
                        if let Some(snapshot) = tracker.apply(&event) {
                            on_progress(snapshot);
                        }
                    }
                    None if stream == Stream::Stderr => {
                        if stderr_tail.len() == STDERR_TAIL {
                            stderr_tail.pop_front();
                        }
                        stderr_tail.push_back(line);
                    }
                    None => {}
                }
            }
        }
    }

    let status = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Ok(abort(&mut child, &tracker).await),
        status = child.wait() => status?,
    };
    if !status.success() {
        let stderr = Vec::from(stderr_tail).join("\n");
        return Err(errors::classify(&stderr).into());
    }
    let path = tracker.final_path().ok_or_else(|| AppError::Ytdlp {
        kind: ErrorKind::Other,
        message: "yt-dlp finished without reporting the output file".into(),
    })?;
    let size = tokio::fs::metadata(path).await.ok().map(|m| m.len());
    Ok(Outcome::Completed {
        path: path.to_path_buf(),
        size,
    })
}

fn build_command(tools: &Tools, args: Vec<OsString>) -> Command {
    let mut cmd = crate::process::command(&tools.ytdlp);
    cmd.args(args)
        // Python would otherwise use the console code page on Windows and mangle titles.
        .env("PYTHONUTF8", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Own process group, so cancel can kill yt-dlp together with ffmpeg and deno.
    #[cfg(unix)]
    cmd.process_group(0);
    cmd
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stream {
    Stdout,
    Stderr,
}

/// Forwards lines from a pipe. Bytes are decoded lossily: one odd byte in a title
/// must not stop progress reporting.
fn spawn_reader(
    pipe: Option<impl AsyncRead + Unpin + Send + 'static>,
    stream: Stream,
    tx: mpsc::UnboundedSender<(Stream, String)>,
) {
    let Some(pipe) = pipe else { return };
    tokio::spawn(async move {
        let mut reader = BufReader::new(pipe);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let line = String::from_utf8_lossy(&buf).trim_end().to_string();
                    if tx.send((stream, line)).is_err() {
                        break;
                    }
                }
            }
        }
    });
}

async fn abort(child: &mut Child, tracker: &Tracker) -> Outcome {
    kill_tree(child).await;
    remove_partial_files(tracker);
    Outcome::Canceled
}

async fn kill_tree(child: &mut Child) {
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        // The child leads its own group (see `build_command`), so this reaches ffmpeg too.
        unsafe {
            libc::killpg(pid as libc::pid_t, libc::SIGKILL);
        }
    }
    #[cfg(windows)]
    if let Some(pid) = child.id() {
        let _ = crate::process::output(
            crate::process::command(std::path::Path::new("taskkill")).args([
                "/T",
                "/F",
                "/PID",
                &pid.to_string(),
            ]),
        )
        .await;
    }
    let _ = child.kill().await;
}

fn remove_partial_files(tracker: &Tracker) {
    let mut by_dir: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();
    for path in tracker.seen_files() {
        if let Some(dir) = path.parent() {
            by_dir.entry(dir.to_path_buf()).or_default().push(path);
        }
    }
    for (dir, seen) in by_dir {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let listing: Vec<String> = entries
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        for name in progress::cleanup_candidates(&listing, &seen) {
            let path = dir.join(&name);
            match std::fs::remove_file(&path) {
                Ok(()) => tracing::info!(?path, "removed partial file"),
                Err(e) => tracing::warn!(?path, error = %e, "could not remove partial file"),
            }
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::time::Duration;

    use super::*;
    use crate::error::AppError;
    use crate::ytdlp::args::Format;
    use crate::ytdlp::errors::ErrorKind;
    use crate::ytdlp::probe::ProbeResult;
    use crate::ytdlp::progress::Stage;

    const PROBE_VIDEO: &str = include_str!("../../tests/fixtures/probe_video.json");
    const PROGRESS: &str = include_str!("../../tests/fixtures/progress.txt");

    /// A fake yt-dlp: a shell script with the given body. `$ARGS_FILE` receives the
    /// arguments it was called with, one per line.
    fn fake_tools(dir: &Path, body: &str) -> Tools {
        let script = dir.join("yt-dlp");
        let args_file = dir.join("args.txt");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\"; done > '{}'\necho \"PYTHONUTF8=$PYTHONUTF8\" >> '{}'\n{body}\n",
                args_file.display(),
                args_file.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        Tools {
            ytdlp: script,
            ffmpeg: dir.join("ffmpeg"),
            deno: dir.join("deno"),
        }
    }

    fn recorded_args(dir: &Path) -> Vec<String> {
        std::fs::read_to_string(dir.join("args.txt"))
            .unwrap()
            .lines()
            .map(String::from)
            .collect()
    }

    fn request(out: &Path) -> DownloadRequest {
        DownloadRequest {
            url: "https://www.youtube.com/watch?v=dQw4w9WgXcQ".into(),
            format: Format::Video { max_height: None },
            output_dir: out.to_path_buf(),
            filename_template: "%(title)s [%(id)s].%(ext)s".into(),
        }
    }

    fn ytdlp_kind(err: AppError) -> ErrorKind {
        match err {
            AppError::Ytdlp { kind, .. } => kind,
            other => panic!("expected yt-dlp error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn probe_parses_output_and_passes_url_after_double_dash() {
        let dir = tempfile::tempdir().unwrap();
        let fixture = dir.path().join("probe.json");
        std::fs::write(&fixture, PROBE_VIDEO).unwrap();
        let tools = fake_tools(dir.path(), &format!("cat '{}'", fixture.display()));

        let result = probe(&tools, "https://www.youtube.com/watch?v=dQw4w9WgXcQ")
            .await
            .unwrap();
        assert!(matches!(result, ProbeResult::Video(v) if v.id == "dQw4w9WgXcQ"));

        let args = recorded_args(dir.path());
        assert!(args.contains(&"-J".to_string()));
        let dash = args.iter().position(|a| a == "--").unwrap();
        assert_eq!(
            args[dash + 1],
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        );
        // Forces UTF-8 output, which matters on Windows consoles.
        assert!(args.contains(&"PYTHONUTF8=1".to_string()));
    }

    #[tokio::test]
    async fn probe_failure_is_classified() {
        let dir = tempfile::tempdir().unwrap();
        let tools = fake_tools(
            dir.path(),
            "echo 'ERROR: [youtube] aaaaaaaaaaa: This video is unavailable' >&2; exit 1",
        );
        let err = probe(&tools, "https://youtu.be/aaaaaaaaaaa")
            .await
            .unwrap_err();
        assert_eq!(ytdlp_kind(err), ErrorKind::Unavailable);
    }

    #[tokio::test]
    async fn download_reports_progress_from_both_streams() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out");
        std::fs::create_dir(&out).unwrap();
        let final_file = out.join("Video [dQw4w9WgXcQ].mp4");

        // Download progress goes to stdout, post-processing to stderr, like real yt-dlp.
        let fixture = dir.path().join("progress.txt");
        std::fs::write(&fixture, PROGRESS).unwrap();
        let body = format!(
            "grep '^CLIPRIGO_DL' '{fx}'\n\
             grep '^CLIPRIGO_PP' '{fx}' >&2\n\
             printf 'hello' > '{file}'\n\
             echo 'CLIPRIGO_FILE {file}'",
            fx = fixture.display(),
            file = final_file.display()
        );
        let tools = fake_tools(dir.path(), &body);

        let mut snaps = Vec::new();
        let outcome = download(&tools, &request(&out), CancellationToken::new(), |s| {
            snaps.push(s)
        })
        .await
        .unwrap();

        assert_eq!(
            outcome,
            Outcome::Completed {
                path: final_file,
                size: Some(5)
            }
        );
        assert!(snaps.iter().any(|s| s.part == 2));
        assert!(
            snaps
                .iter()
                .any(|s| s.stage == Stage::Processing
                    && s.postprocessor.as_deref() == Some("Merger"))
        );
    }

    #[tokio::test]
    async fn download_failure_is_classified() {
        let dir = tempfile::tempdir().unwrap();
        let tools = fake_tools(
            dir.path(),
            "echo 'ERROR: [youtube] x: Sign in to confirm you\u{2019}re not a bot.' >&2; exit 1",
        );
        let err = download(
            &tools,
            &request(dir.path()),
            CancellationToken::new(),
            |_| {},
        )
        .await
        .unwrap_err();
        assert_eq!(ytdlp_kind(err), ErrorKind::BotCheck);
    }

    #[tokio::test]
    async fn success_without_final_path_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let tools = fake_tools(dir.path(), "exit 0");
        let err = download(
            &tools,
            &request(dir.path()),
            CancellationToken::new(),
            |_| {},
        )
        .await
        .unwrap_err();
        assert!(
            matches!(
                err,
                AppError::Ytdlp {
                    kind: ErrorKind::Other,
                    ..
                }
            ),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn invalid_utf8_output_is_tolerated() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("Ok [x].mp4");
        std::fs::write(&file, b"x").unwrap();
        let tools = fake_tools(
            dir.path(),
            &format!(
                "printf 'garbage \\377\\376\\n'\necho 'CLIPRIGO_FILE {}'",
                file.display()
            ),
        );
        let outcome = download(
            &tools,
            &request(dir.path()),
            CancellationToken::new(),
            |_| {},
        )
        .await
        .unwrap();
        assert!(matches!(outcome, Outcome::Completed { path, .. } if path == file));
    }

    fn process_alive(pid: i32) -> bool {
        // Signal 0 only checks existence; a zombie counts as gone for our purposes
        // because the reaper is the shell we killed.
        let exists = unsafe { libc::kill(pid, 0) == 0 };
        let zombie = std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .map(|s| s.contains(") Z "))
            .unwrap_or(true);
        exists && !zombie
    }

    #[tokio::test]
    async fn cancel_kills_process_tree_and_removes_partial_files() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out");
        std::fs::create_dir(&out).unwrap();
        let part = out.join("Song [id1].f394.mp4.part");
        let finished_earlier = out.join("Song [id1].mp4");
        std::fs::write(&finished_earlier, b"keep me").unwrap();
        let child_pid_file = dir.path().join("child.pid");

        // Writes a .part file, starts a long-running child (stands in for ffmpeg), then waits.
        let body = format!(
            "printf 'data' > '{part}'\n\
             sleep 30 &\n\
             echo $! > '{pid}'\n\
             echo 'CLIPRIGO_DL {{\"status\": \"downloading\", \"downloaded_bytes\": 4, \"total_bytes\": 100, \"filename\": \"{file}\", \"tmpfilename\": \"{part}\"}}'\n\
             wait",
            part = part.display(),
            pid = child_pid_file.display(),
            file = out.join("Song [id1].f394.mp4").display(),
        );
        let tools = fake_tools(dir.path(), &body);

        let cancel = CancellationToken::new();
        let trigger = cancel.clone();
        let outcome = tokio::time::timeout(
            Duration::from_secs(10),
            download(&tools, &request(&out), cancel, move |_| trigger.cancel()),
        )
        .await
        .expect("cancel must not hang")
        .unwrap_or_else(|e| panic!("download failed: {e:?}"));

        assert_eq!(outcome, Outcome::Canceled);
        assert!(!part.exists(), "partial file must be removed");
        assert!(finished_earlier.exists(), "unrelated file must survive");

        let child: i32 = std::fs::read_to_string(&child_pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        // Give the kernel a moment to deliver the signal.
        for _ in 0..50 {
            if !process_alive(child) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(
            !process_alive(child),
            "child process {child} must be killed"
        );
    }

    #[tokio::test]
    async fn cancel_before_start_returns_canceled() {
        let dir = tempfile::tempdir().unwrap();
        let tools = fake_tools(dir.path(), "sleep 30");
        let cancel = CancellationToken::new();
        cancel.cancel();
        let outcome = tokio::time::timeout(
            Duration::from_secs(5),
            download(&tools, &request(dir.path()), cancel, |_| {}),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(outcome, Outcome::Canceled);
    }
}

/// Real yt-dlp against YouTube: `cargo test live -- --ignored --test-threads=1`.
/// Components are cached in `target/test-bin` between runs.
#[cfg(test)]
mod live_tests {
    use std::path::PathBuf;

    use super::*;
    use crate::components::ComponentManager;
    use crate::ytdlp::args::Format;
    use crate::ytdlp::progress::Stage;

    const VIDEO: &str = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";
    const PLAYLIST: &str = "https://www.youtube.com/playlist?list=PLBCF2DAC6FFB574DE";

    async fn tools() -> Tools {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-bin");
        let manager = ComponentManager::new(dir).unwrap();
        for status in manager.status() {
            if !status.installed {
                manager.install(status.component, &|_| {}).await.unwrap();
            }
        }
        manager.tools().unwrap()
    }

    /// YouTube sometimes demands a bot check from datacenter IPs; retry only that.
    async fn retry_bot_check<T, F, Fut>(mut f: F) -> AppResult<T>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = AppResult<T>>,
    {
        for attempt in 1..=3 {
            match f().await {
                Err(AppError::Ytdlp {
                    kind: ErrorKind::BotCheck,
                    ..
                }) if attempt < 3 => {
                    eprintln!("bot check, retrying ({attempt})");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
                result => return result,
            }
        }
        unreachable!()
    }

    fn request(url: &str, format: Format, dir: &std::path::Path) -> DownloadRequest {
        DownloadRequest {
            url: url.into(),
            format,
            output_dir: dir.to_path_buf(),
            filename_template: "%(title)s [%(id)s].%(ext)s".into(),
        }
    }

    #[tokio::test]
    #[ignore = "network"]
    async fn live_probe_video_and_playlist() {
        let tools = tools().await;
        let ProbeResult::Video(v) = retry_bot_check(|| probe(&tools, VIDEO)).await.unwrap() else {
            panic!("expected video");
        };
        assert_eq!(v.id, "dQw4w9WgXcQ");
        assert!(v.heights.contains(&1080), "{:?}", v.heights);

        // A watch link with list= is treated as the single video.
        let with_list = format!("{VIDEO}&list=PLBCF2DAC6FFB574DE");
        let r = retry_bot_check(|| probe(&tools, &with_list)).await.unwrap();
        assert!(matches!(r, ProbeResult::Video(_)));

        let ProbeResult::Playlist(p) = retry_bot_check(|| probe(&tools, PLAYLIST)).await.unwrap()
        else {
            panic!("expected playlist");
        };
        assert!(p.entries.len() >= 10, "{}", p.entries.len());
    }

    #[tokio::test]
    #[ignore = "network"]
    async fn live_download_video_and_audio() {
        let tools = tools().await;
        let dir = tempfile::tempdir().unwrap();

        let video_req = request(
            VIDEO,
            Format::Video {
                max_height: Some(144),
            },
            dir.path(),
        );
        let stages = std::sync::Mutex::new(Vec::new());
        let outcome = retry_bot_check(|| {
            stages.lock().unwrap().clear();
            download(&tools, &video_req, CancellationToken::new(), |s| {
                stages.lock().unwrap().push((s.stage, s.part))
            })
        })
        .await
        .unwrap();
        let Outcome::Completed { path, size } = outcome else {
            panic!("expected completion");
        };
        assert_eq!(path.extension().unwrap(), "mp4");
        assert!(size.unwrap() > 100_000, "{size:?}");
        let stages = stages.into_inner().unwrap();
        assert!(stages.contains(&(Stage::Processing, 2)), "{stages:?}");

        let audio_req = request(VIDEO, Format::AudioM4a, dir.path());
        let outcome =
            retry_bot_check(|| download(&tools, &audio_req, CancellationToken::new(), |_| {}))
                .await
                .unwrap();
        let Outcome::Completed { path, .. } = outcome else {
            panic!("expected completion");
        };
        assert_eq!(path.extension().unwrap(), "m4a");

        // Only the two results remain, no temp files.
        let mut names: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names.len(), 2, "{names:?}");
    }

    #[tokio::test]
    #[ignore = "network"]
    async fn live_cancel_leaves_no_files() {
        let tools = tools().await;
        let dir = tempfile::tempdir().unwrap();
        let req = request(VIDEO, Format::Video { max_height: None }, dir.path());
        let outcome = retry_bot_check(|| {
            let cancel = CancellationToken::new();
            let trigger = cancel.clone();
            download(&tools, &req, cancel, move |s| {
                if s.downloaded > 0 {
                    trigger.cancel();
                }
            })
        })
        .await
        .unwrap();
        assert_eq!(outcome, Outcome::Canceled);
        let left: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
        assert!(left.is_empty(), "{left:?}");
    }
}
