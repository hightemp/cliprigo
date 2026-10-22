use std::collections::{BTreeMap, VecDeque};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
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
    probe_with_timeout(tools, url, PROBE_TIMEOUT).await
}

/// Like [`probe`]. stderr is collected while yt-dlp runs, so a hang (e.g. YouTube
/// unreachable without a proxy) ends with an error that shows where it got stuck.
async fn probe_with_timeout(tools: &Tools, url: &str, timeout: Duration) -> AppResult<ProbeResult> {
    let started = std::time::Instant::now();
    let mut child =
        crate::process::spawn(&mut build_command(tools, args::probe_args(tools, url))).await?;
    let stdout = read_all(child.stdout.take());
    let (tx, mut rx) = mpsc::unbounded_channel();
    spawn_reader(child.stderr.take(), Stream::Stderr, tx);

    let mut stderr = VecDeque::with_capacity(STDERR_TAIL);
    let deadline = tokio::time::sleep(timeout);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            msg = rx.recv() => match msg {
                Some((_, line)) => {
                    if stderr.len() == STDERR_TAIL {
                        stderr.pop_front();
                    }
                    stderr.push_back(line);
                }
                None => break,
            },
            _ = &mut deadline => {
                kill_tree(&mut child).await;
                let last = stderr.iter().rev().find(|l| !l.trim().is_empty());
                tracing::warn!(url, stderr = ?stderr, "yt-dlp timed out while probing");
                return Err(AppError::Ytdlp {
                    kind: ErrorKind::Timeout,
                    message: format!(
                        "no answer in {} s; last output: {}",
                        timeout.as_secs(),
                        last.map_or("(none)", String::as_str)
                    ),
                });
            }
        }
    }
    let status = child.wait().await?;
    let stdout = stdout.await.map_err(|e| AppError::Other(e.to_string()))??;
    tracing::info!(
        url,
        elapsed_ms = started.elapsed().as_millis() as u64,
        "probe finished"
    );
    if !status.success() {
        let stderr = Vec::from(stderr).join("\n");
        tracing::warn!(url, %stderr, "probe failed");
        return Err(errors::classify(&stderr).into());
    }
    probe::parse_probe(&String::from_utf8_lossy(&stdout))
}

/// Reads a pipe to the end in the background (probe JSON can be megabytes).
fn read_all(
    pipe: Option<impl AsyncRead + Unpin + Send + 'static>,
) -> tokio::task::JoinHandle<AppResult<Vec<u8>>> {
    tokio::spawn(async move {
        let mut buf = Vec::new();
        if let Some(mut pipe) = pipe {
            tokio::io::AsyncReadExt::read_to_end(&mut pipe, &mut buf).await?;
        }
        Ok(buf)
    })
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
        tracing::warn!(url = req.url, %stderr, "download failed");
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
        // Without it progress lines can arrive in blocks (seen on Windows), so
        // progress stutters and cancel reacts late.
        .env("PYTHONUNBUFFERED", "1")
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
    let seen = tracker.seen_files();
    // Retries sleep, so keep them off the async runtime.
    let _ = tokio::task::spawn_blocking(move || remove_partial_files(seen)).await;
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
        let taskkill =
            crate::process::output(crate::process::command(Path::new("taskkill")).args([
                "/T",
                "/F",
                "/PID",
                &pid.to_string(),
            ]))
            .await;
        match taskkill {
            Ok(out) if out.status.success() => {}
            Ok(out) => tracing::warn!(
                pid,
                stderr = %String::from_utf8_lossy(&out.stderr).trim(),
                "taskkill failed"
            ),
            Err(e) => tracing::warn!(pid, error = %e, "taskkill could not run"),
        }
    }
    let _ = child.kill().await;
}

/// Windows releases a killed process's file handles with a short delay.
const REMOVE_ATTEMPTS: u32 = 15;
const REMOVE_DELAY: Duration = Duration::from_millis(200);

/// Removes `path`, retrying while it is still locked. A missing file counts as removed.
fn remove_with_retry(
    path: &Path,
    attempts: u32,
    delay: Duration,
    mut remove: impl FnMut(&Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
    let mut last = None;
    for attempt in 1..=attempts {
        match remove(path) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => {
                last = Some(e);
                if attempt < attempts {
                    std::thread::sleep(delay);
                }
            }
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::other("no attempts")))
}

fn remove_partial_files(seen: Vec<PathBuf>) {
    let mut by_dir: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();
    for path in seen {
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
            match remove_with_retry(&path, REMOVE_ATTEMPTS, REMOVE_DELAY, |p| {
                std::fs::remove_file(p)
            }) {
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
                "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\"; done > '{}'\necho \"PYTHONUTF8=$PYTHONUTF8\" >> '{}'\necho \"PYTHONUNBUFFERED=$PYTHONUNBUFFERED\" >> '{}'\n{body}\n",
                args_file.display(),
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
            proxy: None,
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
        // Progress must arrive line by line, not in blocks (seen on Windows).
        assert!(args.contains(&"PYTHONUNBUFFERED=1".to_string()));
    }

    #[tokio::test]
    async fn probe_timeout_reports_last_output_and_kills_tree() {
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("child.pid");
        // Stands in for yt-dlp stuck while Deno solves YouTube's JS challenge.
        let body = format!(
            "echo '[youtube] x: Downloading webpage' >&2\n\
             echo '[jsc:deno] Solving JS challenges using deno' >&2\n\
             sleep 30 &\n\
             echo $! > '{}'\n\
             wait",
            pid_file.display()
        );
        let tools = fake_tools(dir.path(), &body);

        let started = std::time::Instant::now();
        let err = probe_with_timeout(&tools, "https://youtu.be/x", Duration::from_secs(1))
            .await
            .unwrap_err();
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
        match err {
            AppError::Ytdlp {
                kind: ErrorKind::Timeout,
                message,
            } => assert!(
                message.contains("Solving JS challenges using deno"),
                "{message}"
            ),
            other => panic!("expected timeout, got {other:?}"),
        }

        let child: i32 = std::fs::read_to_string(&pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        for _ in 0..50 {
            if !process_alive(child) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(!process_alive(child), "child {child} must be killed");
    }

    #[test]
    fn removal_is_retried_while_the_file_is_locked() {
        // Windows keeps a file locked for a moment after the process holding it died.
        let mut calls = 0;
        let ok = remove_with_retry(Path::new("x"), 5, Duration::ZERO, |_| {
            calls += 1;
            if calls < 3 {
                Err(std::io::Error::other("used by another process"))
            } else {
                Ok(())
            }
        });
        assert!(ok.is_ok());
        assert_eq!(calls, 3);

        let mut calls = 0;
        let err = remove_with_retry(Path::new("x"), 4, Duration::ZERO, |_| {
            calls += 1;
            Err(std::io::Error::other("locked"))
        });
        assert!(err.is_err());
        assert_eq!(calls, 4);

        // A file that is already gone is fine.
        let mut calls = 0;
        assert!(
            remove_with_retry(Path::new("x"), 4, Duration::ZERO, |_| {
                calls += 1;
                Err(std::io::ErrorKind::NotFound.into())
            })
            .is_ok()
        );
        assert_eq!(calls, 1);
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

/// Real yt-dlp against YouTube: `cargo test live_ -- --ignored --test-threads=1`.
/// Runs in CI on Windows, macOS and Linux. Components are cached in `target/test-bin`.
#[cfg(test)]
mod live_tests {
    use std::future::Future;
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::components::ComponentManager;
    use crate::ytdlp::args::Format;

    /// A well-known reference video and one reported to hang on Windows.
    const VIDEOS: &[&str] = &[
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
        "https://www.youtube.com/watch?v=lN4HTYNJNjI",
    ];
    /// Has a "Premium" HLS format that the old sort order picked (seen on Windows).
    const PREMIUM_HLS_VIDEO: &str = "https://www.youtube.com/watch?v=CliFaMi-gpY";
    const PLAYLIST: &str = "https://www.youtube.com/playlist?list=PLBCF2DAC6FFB574DE";
    /// A hang must fail the test instead of blocking CI.
    const STEP_TIMEOUT: Duration = Duration::from_secs(180);

    async fn tools() -> Tools {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-bin");
        let manager = ComponentManager::new(dir, crate::net::test_proxy()).unwrap();
        for status in manager.status() {
            if !status.installed {
                tokio::time::timeout(STEP_TIMEOUT, manager.install(status.component, &|_| {}))
                    .await
                    .unwrap_or_else(|_| panic!("installing {:?} hung", status.component))
                    .unwrap_or_else(|e| panic!("installing {:?}: {e:?}", status.component));
            }
        }
        manager.tools().unwrap()
    }

    /// Runs one step against YouTube with a timeout. A bot check (YouTube distrusting
    /// the runner's network) is retried, then skipped with a warning: it is about the
    /// network, not the app. Every other error or a hang fails the test.
    async fn live<T, F, Fut>(what: &str, mut step: F) -> Option<T>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = AppResult<T>>,
    {
        for attempt in 1..=3 {
            let started = std::time::Instant::now();
            let result = tokio::time::timeout(STEP_TIMEOUT, step())
                .await
                .unwrap_or_else(|_| panic!("{what}: no result in {STEP_TIMEOUT:?} (hang)"));
            eprintln!("{what}: attempt {attempt} took {:?}", started.elapsed());
            match result {
                Ok(value) => return Some(value),
                Err(AppError::Ytdlp {
                    kind: ErrorKind::BotCheck,
                    ..
                }) if attempt < 3 => tokio::time::sleep(Duration::from_secs(10)).await,
                Err(AppError::Ytdlp {
                    kind: ErrorKind::BotCheck,
                    message,
                }) => {
                    // Shown as an annotation in GitHub Actions.
                    println!("::warning::{what}: skipped, YouTube bot check: {message}");
                    return None;
                }
                Err(e) => panic!("{what}: {e:?}"),
            }
        }
        unreachable!()
    }

    fn request(url: &str, format: Format, dir: &Path) -> DownloadRequest {
        DownloadRequest {
            url: url.into(),
            format,
            output_dir: dir.to_path_buf(),
            filename_template: "%(title)s [%(id)s].%(ext)s".into(),
        }
    }

    fn files_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    fn video_id(url: &str) -> &str {
        url.rsplit("v=").next().unwrap()
    }

    #[tokio::test]
    #[ignore = "network"]
    async fn live_probe_videos_and_playlist() {
        let tools = tools().await;
        for url in VIDEOS {
            let Some(result) = live(&format!("probe {url}"), || probe(&tools, url)).await else {
                continue;
            };
            let ProbeResult::Video(v) = result else {
                panic!("{url}: expected a video");
            };
            assert_eq!(v.id, video_id(url));
            assert!(!v.title.is_empty());
            assert!(!v.heights.is_empty(), "{url}: no video formats");
        }

        // A watch link with list= is treated as the single video.
        let with_list = format!("{}&list=PLBCF2DAC6FFB574DE", VIDEOS[0]);
        if let Some(r) = live("probe watch+list", || probe(&tools, &with_list)).await {
            assert!(matches!(r, ProbeResult::Video(_)));
        }
        if let Some(r) = live("probe playlist", || probe(&tools, PLAYLIST)).await {
            let ProbeResult::Playlist(p) = r else {
                panic!("expected playlist");
            };
            assert!(p.entries.len() >= 10, "{}", p.entries.len());
        }
    }

    #[tokio::test]
    #[ignore = "network"]
    async fn live_download_videos_and_audio() {
        let tools = tools().await;
        for url in VIDEOS {
            let dir = tempfile::tempdir().unwrap();
            let req = request(
                url,
                Format::Video {
                    max_height: Some(144),
                },
                dir.path(),
            );
            let stages = std::sync::Mutex::new(Vec::new());
            let outcome = live(&format!("download 144p {url}"), || {
                stages.lock().unwrap().clear();
                download(&tools, &req, CancellationToken::new(), |s| {
                    stages.lock().unwrap().push((s.stage, s.part))
                })
            })
            .await;
            let Some(outcome) = outcome else { continue };
            let Outcome::Completed { path, size } = outcome else {
                panic!("{url}: expected completion");
            };
            assert_eq!(path.extension().unwrap(), "mp4");
            assert!(size.unwrap() > 10_000, "{url}: {size:?}");
            assert!(
                !stages.lock().unwrap().is_empty(),
                "{url}: no progress reported"
            );
            // Only the result remains, no temp files.
            assert_eq!(files_in(dir.path()).len(), 1, "{:?}", files_in(dir.path()));
        }

        let dir = tempfile::tempdir().unwrap();
        let req = request(VIDEOS[1], Format::AudioM4a, dir.path());
        if let Some(outcome) = live("download m4a", || {
            download(&tools, &req, CancellationToken::new(), |_| {})
        })
        .await
        {
            let Outcome::Completed { path, .. } = outcome else {
                panic!("expected completion");
            };
            assert_eq!(path.extension().unwrap(), "m4a");
            assert_eq!(files_in(dir.path()).len(), 1, "{:?}", files_in(dir.path()));
        }
    }

    /// The protocols yt-dlp would pick for a download, without downloading.
    async fn chosen_protocols(tools: &Tools, req: &DownloadRequest) -> AppResult<String> {
        let mut args = crate::ytdlp::args::download_args(tools, req);
        let url_at = args.len() - 2;
        // Explicit: the after_move --print in download_args turns implicit simulate off.
        let extra = ["--simulate", "--print", "%(protocol)s"];
        args.splice(url_at..url_at, extra.map(OsString::from));
        let output = build_command(tools, args).output().await?;
        if !output.status.success() {
            return Err(errors::classify(&String::from_utf8_lossy(&output.stderr)).into());
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// YouTube's "Premium" HLS formats were picked over https ones and downloaded
    /// ~90x slower through a proxy (23 KiB/s vs 2 MiB/s).
    #[tokio::test]
    #[ignore = "network"]
    async fn live_best_video_avoids_hls() {
        let tools = tools().await;
        let dir = tempfile::tempdir().unwrap();
        for url in VIDEOS.iter().chain([&PREMIUM_HLS_VIDEO]) {
            let req = request(url, Format::Video { max_height: None }, dir.path());
            let Some(protocols) =
                live(&format!("formats {url}"), || chosen_protocols(&tools, &req)).await
            else {
                continue;
            };
            eprintln!("{url}: {protocols}");
            assert!(!protocols.contains("m3u8"), "{url}: {protocols}");
        }
    }

    #[tokio::test]
    #[ignore = "network"]
    async fn live_cancel_leaves_no_files() {
        let tools = tools().await;
        let dir = tempfile::tempdir().unwrap();
        let req = request(VIDEOS[0], Format::Video { max_height: None }, dir.path());
        let canceled_at = std::sync::Mutex::new(None);
        let outcome = live("cancel download", || {
            let cancel = CancellationToken::new();
            let trigger = cancel.clone();
            let canceled_at = &canceled_at;
            download(&tools, &req, cancel, move |s| {
                if s.downloaded > 0 && !trigger.is_cancelled() {
                    *canceled_at.lock().unwrap() = Some(std::time::Instant::now());
                    trigger.cancel();
                }
            })
        })
        .await;
        if let Some(at) = *canceled_at.lock().unwrap() {
            let took = at.elapsed();
            eprintln!("cancel took {took:?} from request to return");
            assert!(took < Duration::from_secs(10), "cancel took {took:?}");
        }
        if let Some(outcome) = outcome {
            assert_eq!(outcome, Outcome::Canceled);
            assert!(
                files_in(dir.path()).is_empty(),
                "{:?}",
                files_in(dir.path())
            );
        }
    }
}
