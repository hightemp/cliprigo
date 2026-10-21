use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::components::{Component, ComponentManager, ComponentStatus, Progress, UpdateInfo};
use crate::db::{Db, DownloadRecord, HistoryQuery, Settings};
use crate::error::{AppError, AppResult};
use crate::queue::{Queue, QueueEntry, QueueItemInput};
use crate::ytdlp;
use crate::ytdlp::probe::ProbeResult;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryItem {
    #[serde(flatten)]
    pub record: DownloadRecord,
    /// `false` when the file was moved or deleted outside the app.
    pub file_exists: bool,
}

#[tauri::command]
pub fn history_list(db: State<'_, Db>, query: HistoryQuery) -> AppResult<Vec<HistoryItem>> {
    Ok(db
        .list_downloads(&query)?
        .into_iter()
        .map(|record| {
            let file_exists = record
                .file_path
                .as_deref()
                .is_some_and(|p| Path::new(p).is_file());
            HistoryItem {
                record,
                file_exists,
            }
        })
        .collect())
}

#[tauri::command]
pub fn history_delete(
    db: State<'_, Db>,
    queue: State<'_, Queue>,
    id: i64,
    delete_file: bool,
) -> AppResult<()> {
    if queue.is_active(id) {
        return Err(AppError::Other(format!(
            "download {id} is in progress, cancel it first"
        )));
    }
    let Some(record) = db.delete_download(id)? else {
        return Ok(());
    };
    if delete_file && let Some(path) = record.file_path {
        match std::fs::remove_file(&path) {
            Ok(()) => tracing::info!(path, "deleted downloaded file"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

#[tauri::command]
pub fn settings_get(db: State<'_, Db>) -> AppResult<Settings> {
    db.load_settings()
}

#[tauri::command]
pub fn settings_set(db: State<'_, Db>, settings: Settings) -> AppResult<Settings> {
    db.save_settings(settings)
}

#[tauri::command]
pub fn components_status(manager: State<'_, Arc<ComponentManager>>) -> Vec<ComponentStatus> {
    manager.status()
}

/// Installs or reinstalls the latest version, streaming `component://progress` events.
#[tauri::command]
pub async fn components_install(
    app: AppHandle,
    manager: State<'_, Arc<ComponentManager>>,
    component: Component,
) -> AppResult<String> {
    let emit = |p: Progress| {
        let _ = app.emit("component://progress", p);
    };
    manager.install(component, &emit).await
}

#[tauri::command]
pub async fn components_check_updates(
    manager: State<'_, Arc<ComponentManager>>,
) -> AppResult<Vec<UpdateInfo>> {
    manager.check_updates().await
}

/// Metadata for a pasted link: a single video or a playlist/channel listing.
#[tauri::command]
pub async fn video_probe(
    manager: State<'_, Arc<ComponentManager>>,
    url: String,
) -> AppResult<ProbeResult> {
    let tools = manager.tools()?;
    ytdlp::runner::probe(&tools, &ytdlp::url::normalize(&url)).await
}

// Queue commands are async so they run inside the Tauri runtime the queue spawns onto.

#[tauri::command]
pub async fn queue_add(
    queue: State<'_, Queue>,
    items: Vec<QueueItemInput>,
) -> AppResult<Vec<DownloadRecord>> {
    queue.add(items)
}

#[tauri::command]
pub async fn queue_cancel(queue: State<'_, Queue>, id: i64) -> AppResult<()> {
    queue.cancel(id)
}

#[tauri::command]
pub async fn queue_retry(queue: State<'_, Queue>, id: i64) -> AppResult<DownloadRecord> {
    queue.retry(id)
}

#[tauri::command]
pub async fn queue_list(queue: State<'_, Queue>) -> AppResult<Vec<QueueEntry>> {
    queue.list()
}

/// Newest completed or active download for each video id, to warn about duplicates.
#[tauri::command]
pub fn find_duplicates(
    db: State<'_, Db>,
    video_ids: Vec<String>,
) -> AppResult<Vec<DownloadRecord>> {
    db.find_duplicates(&video_ids)
}

/// Exit after the user confirmed closing with active downloads: stop them first so no
/// yt-dlp or ffmpeg process outlives the app.
#[tauri::command]
pub async fn app_quit(app: AppHandle, queue: State<'_, Queue>) -> AppResult<()> {
    if tokio::time::timeout(Duration::from_secs(10), queue.cancel_all())
        .await
        .is_err()
    {
        tracing::warn!("downloads did not stop in time, exiting anyway");
    }
    app.exit(0);
    Ok(())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UrlInfo {
    /// The link as it will be probed (trimmed, channel roots point at their videos tab).
    pub normalized: String,
    /// `false` triggers a warning: other sites may work, but aren't the target.
    pub is_youtube: bool,
    /// Treated as one video even if the link carries `list=`.
    pub single_video: bool,
}

#[tauri::command]
pub fn url_inspect(url: String) -> UrlInfo {
    UrlInfo {
        normalized: ytdlp::url::normalize(&url),
        is_youtube: ytdlp::url::is_youtube(&url),
        single_video: ytdlp::url::is_single_video(&url),
    }
}
