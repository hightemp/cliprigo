use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use futures_util::future::BoxFuture;
use serde::{Deserialize, Serialize};
use tokio::runtime::Handle;
use tokio::sync::Notify;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::db::{Db, DownloadRecord, DownloadStatus, NewDownload};
use crate::error::{AppError, AppResult};
#[cfg(test)]
use crate::ytdlp::args::Codec;
use crate::ytdlp::args::{DownloadRequest, Format};
use crate::ytdlp::progress::{Snapshot, Stage};
use crate::ytdlp::runner::Outcome;

/// Minimum gap between progress events of one download (stage changes bypass it).
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

/// Receives progress snapshots of one running download.
pub type ProgressSink = Box<dyn FnMut(Snapshot) + Send>;

/// Performs one download. Production code runs yt-dlp; tests substitute a fake.
pub type RunFn = Arc<
    dyn Fn(
            DownloadRequest,
            CancellationToken,
            ProgressSink,
        ) -> BoxFuture<'static, AppResult<Outcome>>
        + Send
        + Sync,
>;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum QueueEvent {
    /// A record changed status (queued, downloading, processing, finished...).
    Status { record: Box<DownloadRecord> },
    /// Throttled progress of a running download.
    Progress { id: i64, snapshot: Snapshot },
}

pub type EventSink = Arc<dyn Fn(QueueEvent) + Send + Sync>;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueItemInput {
    pub url: String,
    pub video_id: Option<String>,
    pub title: Option<String>,
    pub uploader: Option<String>,
    pub duration: Option<i64>,
    pub thumbnail_url: Option<String>,
    pub playlist_id: Option<String>,
    pub playlist_title: Option<String>,
    pub format: Format,
    /// Defaults to the download folder from settings.
    pub output_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueEntry {
    #[serde(flatten)]
    pub record: DownloadRecord,
    pub progress: Option<Snapshot>,
}

struct Running {
    cancel: CancellationToken,
    snapshot: Option<Snapshot>,
}

#[derive(Default)]
struct State {
    pending: VecDeque<(i64, DownloadRequest)>,
    running: HashMap<i64, Running>,
    /// Active ids in the order they were queued.
    order: Vec<i64>,
}

struct Inner {
    runtime: Handle,
    db: Db,
    run: RunFn,
    sink: EventSink,
    state: Mutex<State>,
    /// Signalled whenever a running download stops.
    stopped: Notify,
}

/// Download queue: runs at most `settings.concurrency` downloads at once, persists every
/// status change to the database and reports progress through the event sink.
#[derive(Clone)]
pub struct Queue {
    inner: Arc<Inner>,
}

impl Queue {
    pub fn new(runtime: Handle, db: Db, run: RunFn, sink: EventSink) -> Self {
        Self {
            inner: Arc::new(Inner {
                runtime,
                db,
                run,
                sink,
                state: Mutex::new(State::default()),
                stopped: Notify::new(),
            }),
        }
    }

    pub fn add(&self, items: Vec<QueueItemInput>) -> AppResult<Vec<DownloadRecord>> {
        let settings = self.inner.db.load_settings()?;
        let mut records = Vec::with_capacity(items.len());
        for item in items {
            let output_dir = item
                .output_dir
                .filter(|d| !d.trim().is_empty())
                .unwrap_or_else(|| settings.download_dir.clone());
            let record = self.inner.db.insert_download(&NewDownload {
                video_id: item.video_id,
                url: item.url.clone(),
                title: item.title,
                uploader: item.uploader,
                duration: item.duration,
                thumbnail_url: item.thumbnail_url,
                format: item.format.db_name().into(),
                quality: item.format.quality(),
                codec: item.format.codec().map(String::from),
                playlist_id: item.playlist_id,
                playlist_title: item.playlist_title,
                output_dir: Some(output_dir.clone()),
            })?;
            let request = DownloadRequest {
                url: item.url,
                format: item.format,
                output_dir: PathBuf::from(output_dir),
                filename_template: settings.filename_template.clone(),
            };
            self.inner.enqueue(record.id, request);
            (self.inner.sink)(QueueEvent::Status {
                record: Box::new(record.clone()),
            });
            records.push(record);
        }
        self.inner.pump();
        Ok(records)
    }

    /// Cancels a queued or running download. Unknown or finished ids are ignored.
    pub fn cancel(&self, id: i64) -> AppResult<()> {
        let was_pending = {
            let mut state = self.inner.state();
            if let Some(running) = state.running.get(&id) {
                running.cancel.cancel();
                false
            } else if let Some(pos) = state.pending.iter().position(|(p, _)| *p == id) {
                state.pending.remove(pos);
                state.order.retain(|o| *o != id);
                true
            } else {
                false
            }
        };
        if was_pending {
            self.inner
                .db
                .set_status(id, DownloadStatus::Canceled, None)?;
            self.inner.emit_record(id);
        }
        Ok(())
    }

    /// Re-queues a failed, canceled or interrupted download with its stored settings.
    pub fn retry(&self, id: i64) -> AppResult<DownloadRecord> {
        let record = self
            .inner
            .db
            .get_download(id)?
            .ok_or_else(|| AppError::Other(format!("download {id} not found")))?;
        let retryable = matches!(
            record.status,
            DownloadStatus::Failed | DownloadStatus::Canceled | DownloadStatus::Interrupted
        );
        if !retryable || self.is_active(id) {
            return Err(AppError::Other(format!(
                "download {id} can't be retried while {:?}",
                record.status
            )));
        }
        let format = Format::from_db(&record.format, record.quality, record.codec.as_deref())
            .ok_or_else(|| AppError::Other(format!("unknown format {}", record.format)))?;
        let settings = self.inner.db.load_settings()?;
        let request = DownloadRequest {
            url: record.url.clone(),
            format,
            output_dir: PathBuf::from(record.output_dir.unwrap_or(settings.download_dir)),
            filename_template: settings.filename_template,
        };
        self.inner.db.set_status(id, DownloadStatus::Queued, None)?;
        self.inner.enqueue(id, request);
        self.inner.emit_record(id);
        self.inner.pump();
        self.inner
            .db
            .get_download(id)?
            .ok_or_else(|| AppError::Other(format!("download {id} not found")))
    }

    /// Queued and running downloads, in queue order, with their latest progress.
    pub fn list(&self) -> AppResult<Vec<QueueEntry>> {
        let active: Vec<(i64, Option<Snapshot>)> = {
            let state = self.inner.state();
            state
                .order
                .iter()
                .map(|id| {
                    let snapshot = state.running.get(id).and_then(|r| r.snapshot.clone());
                    (*id, snapshot)
                })
                .collect()
        };
        let mut entries = Vec::with_capacity(active.len());
        for (id, progress) in active {
            if let Some(record) = self.inner.db.get_download(id)? {
                entries.push(QueueEntry { record, progress });
            }
        }
        Ok(entries)
    }

    pub fn is_active(&self, id: i64) -> bool {
        self.inner.state().order.contains(&id)
    }

    pub fn has_active(&self) -> bool {
        !self.inner.state().order.is_empty()
    }

    /// Cancels everything and waits until all running downloads have stopped.
    pub async fn cancel_all(&self) {
        let pending: Vec<i64> = {
            let mut state = self.inner.state();
            for running in state.running.values() {
                running.cancel.cancel();
            }
            let pending: Vec<i64> = state.pending.drain(..).map(|(id, _)| id).collect();
            state.order.retain(|id| !pending.contains(id));
            pending
        };
        for id in pending {
            if let Err(e) = self.inner.db.set_status(id, DownloadStatus::Canceled, None) {
                tracing::error!(id, error = %e, "failed to mark download canceled");
            }
            self.inner.emit_record(id);
        }
        loop {
            let stopped = self.inner.stopped.notified();
            tokio::pin!(stopped);
            stopped.as_mut().enable();
            if self.inner.state().running.is_empty() {
                break;
            }
            stopped.await;
        }
    }
}

impl Inner {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn enqueue(&self, id: i64, request: DownloadRequest) {
        let mut state = self.state();
        state.pending.push_back((id, request));
        state.order.push(id);
    }

    fn emit_record(&self, id: i64) {
        match self.db.get_download(id) {
            Ok(Some(record)) => (self.sink)(QueueEvent::Status {
                record: Box::new(record),
            }),
            Ok(None) => {}
            Err(e) => tracing::error!(id, error = %e, "failed to load download"),
        }
    }

    /// Starts pending downloads while there are free slots. The limit is read on every
    /// call, so a changed setting applies to the next start.
    fn pump(self: &Arc<Self>) {
        let limit = self
            .db
            .load_settings()
            .map(|s| usize::from(s.concurrency))
            .unwrap_or(1);
        loop {
            let (id, request, cancel) = {
                let mut state = self.state();
                if state.running.len() >= limit {
                    return;
                }
                let Some((id, request)) = state.pending.pop_front() else {
                    return;
                };
                let cancel = CancellationToken::new();
                state.running.insert(
                    id,
                    Running {
                        cancel: cancel.clone(),
                        snapshot: None,
                    },
                );
                (id, request, cancel)
            };
            let inner = self.clone();
            self.runtime
                .spawn(async move { inner.run_job(id, request, cancel).await });
        }
    }

    async fn run_job(
        self: Arc<Self>,
        id: i64,
        request: DownloadRequest,
        cancel: CancellationToken,
    ) {
        self.update_status(id, DownloadStatus::Downloading, None);
        let result = (self.run)(request, cancel.clone(), self.progress_sink(id)).await;

        match result {
            Ok(Outcome::Completed { path, size }) => {
                let path = path.to_string_lossy();
                let size = size.and_then(|s| i64::try_from(s).ok());
                if let Err(e) = self.db.set_result(id, &path, size) {
                    tracing::error!(id, error = %e, "failed to store download result");
                }
                self.update_status(id, DownloadStatus::Completed, None);
            }
            Ok(Outcome::Canceled) => self.update_status(id, DownloadStatus::Canceled, None),
            // A process killed by cancel may still report an error; the user canceled it.
            Err(_) if cancel.is_cancelled() => {
                self.update_status(id, DownloadStatus::Canceled, None)
            }
            Err(e) => {
                tracing::warn!(id, error = %e, "download failed");
                let error = serde_json::to_string(&e).unwrap_or_else(|_| e.to_string());
                self.update_status(id, DownloadStatus::Failed, Some(&error));
            }
        }

        {
            let mut state = self.state();
            state.running.remove(&id);
            state.order.retain(|o| *o != id);
        }
        self.stopped.notify_waiters();
        self.pump();
    }

    fn update_status(&self, id: i64, status: DownloadStatus, error: Option<&str>) {
        if let Err(e) = self.db.set_status(id, status, error) {
            tracing::error!(id, ?status, error = %e, "failed to update download status");
        }
        self.emit_record(id);
    }

    /// Stores every snapshot for `list`, but emits at most one per interval unless the
    /// stage or part changed, so the UI isn't flooded by yt-dlp's frequent updates.
    fn progress_sink(self: &Arc<Self>, id: i64) -> ProgressSink {
        let inner = self.clone();
        let mut last: Option<(Instant, Stage, u32)> = None;
        let mut processing = false;
        Box::new(move |snapshot: Snapshot| {
            if let Some(running) = inner.state().running.get_mut(&id) {
                running.snapshot = Some(snapshot.clone());
            }
            if snapshot.stage == Stage::Processing && !processing {
                processing = true;
                inner.update_status(id, DownloadStatus::Processing, None);
            }
            let now = Instant::now();
            let due = match last {
                None => true,
                Some((at, stage, part)) => {
                    stage != snapshot.stage
                        || part != snapshot.part
                        || now.duration_since(at) >= PROGRESS_INTERVAL
                }
            };
            if due {
                last = Some((now, snapshot.stage, snapshot.part));
                (inner.sink)(QueueEvent::Progress { id, snapshot });
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use futures_util::FutureExt;

    use super::*;
    use crate::db::{DownloadStatus, NewDownload, Settings};
    use crate::error::AppError;
    use crate::ytdlp::errors::ErrorKind;
    use crate::ytdlp::progress::Stage;

    #[derive(Default)]
    struct Fake {
        current: AtomicUsize,
        max: AtomicUsize,
        calls: Mutex<Vec<DownloadRequest>>,
        attempts: Mutex<HashMap<String, u32>>,
    }

    fn snapshot(stage: Stage, percent: f64) -> Snapshot {
        Snapshot {
            stage,
            part: 1,
            percent: Some(percent),
            downloaded: percent as u64,
            total: Some(100),
            speed: None,
            eta: None,
            postprocessor: (stage == Stage::Processing).then(|| "Merger".into()),
        }
    }

    /// Behavior is chosen by the URL: `fake://<kind>/<name>`.
    fn fake_runner(fake: Arc<Fake>) -> RunFn {
        Arc::new(
            move |req: DownloadRequest, cancel: CancellationToken, mut sink: ProgressSink| {
                let fake = fake.clone();
                async move {
                    let now = fake.current.fetch_add(1, Ordering::SeqCst) + 1;
                    fake.max.fetch_max(now, Ordering::SeqCst);
                    fake.calls.lock().unwrap().push(req.clone());
                    let attempt = {
                        let mut a = fake.attempts.lock().unwrap();
                        let n = a.entry(req.url.clone()).or_default();
                        *n += 1;
                        *n
                    };
                    let kind = req
                        .url
                        .trim_start_matches("fake://")
                        .split('/')
                        .next()
                        .unwrap()
                        .to_string();
                    let complete = |req: &DownloadRequest| {
                        let name = req.url.rsplit('/').next().unwrap();
                        let path = req.output_dir.join(format!("{name}.mp4"));
                        std::fs::write(&path, b"data").unwrap();
                        Ok(Outcome::Completed {
                            path,
                            size: Some(4),
                        })
                    };
                    let result = match kind.as_str() {
                        "ok" => {
                            sink(snapshot(Stage::Downloading, 0.0));
                            tokio::time::sleep(Duration::from_millis(100)).await;
                            sink(snapshot(Stage::Processing, 100.0));
                            complete(&req)
                        }
                        "burst" => {
                            for i in 0..100 {
                                sink(snapshot(Stage::Downloading, i as f64));
                                tokio::time::sleep(Duration::from_millis(10)).await;
                            }
                            sink(snapshot(Stage::Processing, 100.0));
                            complete(&req)
                        }
                        "fail" => Err(AppError::Ytdlp {
                            kind: ErrorKind::BotCheck,
                            message: "Sign in to confirm you're not a bot".into(),
                        }),
                        "flaky" if attempt == 1 => Err(AppError::Ytdlp {
                            kind: ErrorKind::Network,
                            message: "timed out".into(),
                        }),
                        "flaky" => complete(&req),
                        "block" => {
                            cancel.cancelled().await;
                            Ok(Outcome::Canceled)
                        }
                        other => panic!("unknown fake kind {other}"),
                    };
                    fake.current.fetch_sub(1, Ordering::SeqCst);
                    result
                }
                .boxed()
            },
        )
    }

    struct Harness {
        queue: Queue,
        db: Db,
        fake: Arc<Fake>,
        events: Arc<Mutex<Vec<QueueEvent>>>,
        dir: tempfile::TempDir,
    }

    fn harness(concurrency: u8) -> Harness {
        let db = Db::open_in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        db.save_settings(Settings {
            concurrency,
            download_dir: dir.path().to_string_lossy().into_owned(),
            ..Default::default()
        })
        .unwrap();
        let fake = Arc::new(Fake::default());
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink_events = events.clone();
        let queue = Queue::new(
            Handle::current(),
            db.clone(),
            fake_runner(fake.clone()),
            Arc::new(move |e| sink_events.lock().unwrap().push(e)),
        );
        Harness {
            queue,
            db,
            fake,
            events,
            dir,
        }
    }

    fn item(url: &str) -> QueueItemInput {
        QueueItemInput {
            url: url.into(),
            video_id: Some(url.rsplit('/').next().unwrap().into()),
            title: Some(url.into()),
            uploader: None,
            duration: None,
            thumbnail_url: None,
            playlist_id: None,
            playlist_title: None,
            format: Format::Video {
                max_height: None,
                codec: Codec::Auto,
            },
            output_dir: None,
        }
    }

    impl Harness {
        fn status(&self, id: i64) -> DownloadStatus {
            self.db.get_download(id).unwrap().unwrap().status
        }

        fn statuses_of(&self, id: i64) -> Vec<DownloadStatus> {
            self.events
                .lock()
                .unwrap()
                .iter()
                .filter_map(|e| match e {
                    QueueEvent::Status { record } if record.id == id => Some(record.status),
                    _ => None,
                })
                .collect()
        }

        fn progress_events(&self, id: i64) -> Vec<Snapshot> {
            self.events
                .lock()
                .unwrap()
                .iter()
                .filter_map(|e| match e {
                    QueueEvent::Progress { id: i, snapshot } if *i == id => Some(snapshot.clone()),
                    _ => None,
                })
                .collect()
        }

        async fn until(&self, what: &str, f: impl Fn(&Self) -> bool) {
            for _ in 0..2000 {
                if f(self) {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            panic!("timed out waiting for: {what}");
        }

        async fn until_idle(&self) {
            self.until("queue idle", |h| !h.queue.has_active()).await;
        }
    }

    #[tokio::test(start_paused = true)]
    async fn completes_and_records_result() {
        let h = harness(2);
        let records = h.queue.add(vec![item("fake://ok/a")]).unwrap();
        assert_eq!(records.len(), 1);
        let id = records[0].id;
        assert_eq!(records[0].status, DownloadStatus::Queued);
        assert_eq!(
            records[0].output_dir.as_deref(),
            Some(h.dir.path().to_str().unwrap())
        );
        assert!(h.queue.is_active(id));

        h.until_idle().await;
        let r = h.db.get_download(id).unwrap().unwrap();
        assert_eq!(r.status, DownloadStatus::Completed);
        assert_eq!(
            r.file_path,
            Some(h.dir.path().join("a.mp4").to_string_lossy().into_owned())
        );
        assert_eq!(r.file_size, Some(4));
        assert!(r.completed_at.is_some());
        assert_eq!(
            h.statuses_of(id),
            vec![
                DownloadStatus::Queued,
                DownloadStatus::Downloading,
                DownloadStatus::Processing,
                DownloadStatus::Completed
            ]
        );
        // The runner got the settings' template and folder.
        let call = h.fake.calls.lock().unwrap()[0].clone();
        assert_eq!(call.output_dir, PathBuf::from(h.dir.path()));
        assert_eq!(
            call.filename_template,
            Settings::default().filename_template
        );
    }

    #[tokio::test(start_paused = true)]
    async fn respects_concurrency_limit() {
        let h = harness(2);
        let items = (0..5).map(|i| item(&format!("fake://ok/v{i}"))).collect();
        let records = h.queue.add(items).unwrap();
        h.until_idle().await;
        assert_eq!(h.fake.max.load(Ordering::SeqCst), 2);
        for r in records {
            assert_eq!(h.status(r.id), DownloadStatus::Completed);
        }
        // Started in the order they were added.
        let urls: Vec<String> = h
            .fake
            .calls
            .lock()
            .unwrap()
            .iter()
            .map(|c| c.url.clone())
            .collect();
        assert_eq!(
            urls,
            (0..5)
                .map(|i| format!("fake://ok/v{i}"))
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn concurrency_setting_is_read_live() {
        let h = harness(3);
        let mut s = h.db.load_settings().unwrap();
        s.concurrency = 1;
        h.db.save_settings(s).unwrap();
        h.queue
            .add((0..3).map(|i| item(&format!("fake://ok/v{i}"))).collect())
            .unwrap();
        h.until_idle().await;
        assert_eq!(h.fake.max.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn failure_is_stored_with_error_code() {
        let h = harness(2);
        let id = h.queue.add(vec![item("fake://fail/x")]).unwrap()[0].id;
        h.until_idle().await;
        let r = h.db.get_download(id).unwrap().unwrap();
        assert_eq!(r.status, DownloadStatus::Failed);
        let err: serde_json::Value = serde_json::from_str(r.error.as_deref().unwrap()).unwrap();
        assert_eq!(err["code"], "ytdlp_bot_check");
        assert!(err["message"].as_str().unwrap().contains("not a bot"));
    }

    #[tokio::test(start_paused = true)]
    async fn cancel_running_starts_next() {
        let h = harness(1);
        let ids: Vec<i64> = h
            .queue
            .add(vec![item("fake://block/a"), item("fake://ok/b")])
            .unwrap()
            .iter()
            .map(|r| r.id)
            .collect();
        h.until("first running", |h| {
            h.status(ids[0]) == DownloadStatus::Downloading
        })
        .await;
        assert_eq!(h.status(ids[1]), DownloadStatus::Queued);

        h.queue.cancel(ids[0]).unwrap();
        h.until_idle().await;
        assert_eq!(h.status(ids[0]), DownloadStatus::Canceled);
        assert_eq!(h.status(ids[1]), DownloadStatus::Completed);
        assert_eq!(
            h.statuses_of(ids[0]).last(),
            Some(&DownloadStatus::Canceled)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn cancel_pending_never_runs_it() {
        let h = harness(1);
        let ids: Vec<i64> = h
            .queue
            .add(vec![item("fake://block/a"), item("fake://ok/b")])
            .unwrap()
            .iter()
            .map(|r| r.id)
            .collect();
        h.queue.cancel(ids[1]).unwrap();
        assert_eq!(h.status(ids[1]), DownloadStatus::Canceled);
        assert!(!h.queue.is_active(ids[1]));
        h.queue.cancel(ids[0]).unwrap();
        h.until_idle().await;
        assert_eq!(h.fake.calls.lock().unwrap().len(), 1);
        // Unknown ids are ignored.
        h.queue.cancel(9999).unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn retry_failed_download() {
        let h = harness(2);
        let id = h.queue.add(vec![item("fake://flaky/x")]).unwrap()[0].id;
        h.until_idle().await;
        assert_eq!(h.status(id), DownloadStatus::Failed);

        let r = h.queue.retry(id).unwrap();
        assert_eq!(r.id, id);
        assert_eq!(r.status, DownloadStatus::Queued);
        assert_eq!(r.error, None);
        h.until_idle().await;
        assert_eq!(h.status(id), DownloadStatus::Completed);
    }

    #[tokio::test(start_paused = true)]
    async fn retry_interrupted_record_uses_stored_settings() {
        let h = harness(2);
        let other_dir = tempfile::tempdir().unwrap();
        // A download interrupted by an app restart: only the database knows about it.
        let r =
            h.db.insert_download(&NewDownload {
                url: "fake://ok/old".into(),
                format: "audio_m4a".into(),
                output_dir: Some(other_dir.path().to_string_lossy().into_owned()),
                ..Default::default()
            })
            .unwrap();
        h.db.mark_interrupted().unwrap();

        h.queue.retry(r.id).unwrap();
        h.until_idle().await;
        let call = h.fake.calls.lock().unwrap()[0].clone();
        assert_eq!(call.format, Format::AudioM4a);
        assert_eq!(call.output_dir, other_dir.path());
        assert_eq!(h.status(r.id), DownloadStatus::Completed);
    }

    #[tokio::test(start_paused = true)]
    async fn retry_rejects_completed_and_active() {
        let h = harness(1);
        let ids: Vec<i64> = h
            .queue
            .add(vec![item("fake://ok/a"), item("fake://block/b")])
            .unwrap()
            .iter()
            .map(|r| r.id)
            .collect();
        h.until("first done", |h| {
            h.status(ids[0]) == DownloadStatus::Completed
        })
        .await;
        assert!(h.queue.retry(ids[0]).is_err());
        assert!(h.queue.retry(ids[1]).is_err());
        assert!(h.queue.retry(9999).is_err());
        h.queue.cancel_all().await;
    }

    #[tokio::test(start_paused = true)]
    async fn progress_is_throttled_but_stage_changes_are_not() {
        let h = harness(1);
        let id = h.queue.add(vec![item("fake://burst/a")]).unwrap()[0].id;
        h.until_idle().await;
        let progress = h.progress_events(id);
        // 100 updates over ~1 s at 10 ms intervals; ~4 per second get through.
        assert!(progress.len() <= 8, "{} progress events", progress.len());
        assert!(progress.len() >= 3, "{} progress events", progress.len());
        assert_eq!(progress[0].percent, Some(0.0));
        // The switch to processing is always delivered.
        assert_eq!(progress.last().unwrap().stage, Stage::Processing);
    }

    #[tokio::test(start_paused = true)]
    async fn list_shows_active_with_progress() {
        let h = harness(1);
        let ids: Vec<i64> = h
            .queue
            .add(vec![item("fake://block/a"), item("fake://ok/b")])
            .unwrap()
            .iter()
            .map(|r| r.id)
            .collect();
        h.until("first running", |h| {
            h.status(ids[0]) == DownloadStatus::Downloading
        })
        .await;
        let list = h.queue.list().unwrap();
        assert_eq!(list.iter().map(|e| e.record.id).collect::<Vec<_>>(), ids);
        assert_eq!(list[0].record.status, DownloadStatus::Downloading);
        assert_eq!(list[1].record.status, DownloadStatus::Queued);
        assert!(list[1].progress.is_none());
        h.queue.cancel_all().await;
        assert!(h.queue.list().unwrap().is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn cancel_all_stops_everything() {
        let h = harness(2);
        let ids: Vec<i64> = h
            .queue
            .add(
                (0..4)
                    .map(|i| item(&format!("fake://block/v{i}")))
                    .collect(),
            )
            .unwrap()
            .iter()
            .map(|r| r.id)
            .collect();
        h.until("two running", |h| {
            h.fake.current.load(Ordering::SeqCst) == 2
        })
        .await;
        h.queue.cancel_all().await;
        assert!(!h.queue.has_active());
        assert_eq!(h.fake.current.load(Ordering::SeqCst), 0);
        for id in ids {
            assert_eq!(h.status(id), DownloadStatus::Canceled);
        }
        // Nothing new was started by the cancellations.
        assert_eq!(h.fake.calls.lock().unwrap().len(), 2);
    }
}

/// The real runner behind the queue: `cargo test live_queue -- --ignored`.
#[cfg(test)]
mod live_tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use futures_util::FutureExt;

    use super::*;
    use crate::components::ComponentManager;
    use crate::db::Settings;
    use crate::ytdlp::errors::ErrorKind;

    #[tokio::test]
    #[ignore = "network"]
    async fn live_queue_runs_real_downloads() {
        let bin = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-bin");
        let components = Arc::new(ComponentManager::new(bin, crate::net::test_proxy()).unwrap());
        for status in components.status() {
            if !status.installed {
                components.install(status.component, &|_| {}).await.unwrap();
            }
        }

        let db = Db::open_in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        db.save_settings(Settings {
            concurrency: 2,
            download_dir: dir.path().to_string_lossy().into_owned(),
            ..Default::default()
        })
        .unwrap();
        let run: RunFn = Arc::new(move |request, cancel, sink| {
            let components = components.clone();
            async move {
                let tools = components.tools()?;
                crate::ytdlp::runner::download(&tools, &request, cancel, sink).await
            }
            .boxed()
        });
        let progress = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = progress.clone();
        let queue = Queue::new(
            Handle::current(),
            db.clone(),
            run,
            Arc::new(move |e| {
                if matches!(e, QueueEvent::Progress { .. }) {
                    counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
            }),
        );

        let item = |format| QueueItemInput {
            url: "https://www.youtube.com/watch?v=dQw4w9WgXcQ".into(),
            video_id: Some("dQw4w9WgXcQ".into()),
            title: None,
            uploader: None,
            duration: None,
            thumbnail_url: None,
            playlist_id: None,
            playlist_title: None,
            format,
            output_dir: None,
        };
        let ids: Vec<i64> = queue
            .add(vec![
                item(Format::Video {
                    max_height: Some(144),
                    codec: Codec::Auto,
                }),
                item(Format::AudioM4a),
            ])
            .unwrap()
            .iter()
            .map(|r| r.id)
            .collect();

        for _attempt in 0..3 {
            while queue.has_active() {
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            // YouTube sometimes demands a bot check from datacenter IPs; retry only that.
            let mut retried = false;
            for id in &ids {
                let r = db.get_download(*id).unwrap().unwrap();
                if r.status == DownloadStatus::Failed
                    && r.error
                        .as_deref()
                        .unwrap_or("")
                        .contains(ErrorKind::BotCheck.code())
                {
                    queue.retry(*id).unwrap();
                    retried = true;
                }
            }
            if !retried {
                break;
            }
        }

        for id in ids {
            let r = db.get_download(id).unwrap().unwrap();
            assert_eq!(r.status, DownloadStatus::Completed, "{r:?}");
            assert!(PathBuf::from(r.file_path.unwrap()).is_file());
            assert!(r.file_size.unwrap() > 0);
        }
        assert!(progress.load(std::sync::atomic::Ordering::Relaxed) > 0);
    }
}
