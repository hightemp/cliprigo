use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

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
pub fn settings_set(
    db: State<'_, Db>,
    proxy: State<'_, crate::net::ProxySetting>,
    settings: Settings,
) -> AppResult<Settings> {
    let saved = db.save_settings(settings)?;
    // Applies to the next request, download or probe without a restart.
    proxy.set(Some(saved.proxy.clone()).filter(|p| !p.is_empty()));
    Ok(saved)
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

/// "Check" next to the proxy field: tries YouTube with the proxy as typed, before saving.
#[tauri::command]
pub async fn proxy_check(proxy: String) -> AppResult<crate::proxy_check::ProxyCheck> {
    use crate::proxy_check::{CHECK_TIMEOUT, YOUTUBE_PROBE_URL, check};
    check(&proxy, YOUTUBE_PROBE_URL, CHECK_TIMEOUT).await
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

/// The downloaded file of a history record, if it is still on disk.
fn existing_file(record: &DownloadRecord) -> AppResult<std::path::PathBuf> {
    let path = record
        .file_path
        .as_deref()
        .map(std::path::PathBuf::from)
        .ok_or_else(|| AppError::FileMissing(format!("download {}", record.id)))?;
    if path.is_file() {
        Ok(path)
    } else {
        Err(AppError::FileMissing(path.to_string_lossy().into_owned()))
    }
}

fn history_file(db: &Db, id: i64) -> AppResult<std::path::PathBuf> {
    let record = db
        .get_download(id)?
        .ok_or_else(|| AppError::Other(format!("download {id} not found")))?;
    existing_file(&record)
}

// Opening goes through the backend with a record id, so the UI can only open files
// that Cliprigo itself downloaded instead of arbitrary paths.

#[tauri::command]
pub fn history_open(app: AppHandle, db: State<'_, Db>, id: i64) -> AppResult<()> {
    let path = history_file(&db, id)?;
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub fn history_reveal(app: AppHandle, db: State<'_, Db>, id: i64) -> AppResult<()> {
    let path = history_file(&db, id)?;
    app.opener()
        .reveal_item_in_dir(path)
        .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub fn open_logs(app: AppHandle) -> AppResult<()> {
    let dir = app
        .path()
        .app_log_dir()
        .map_err(|e| AppError::Other(e.to_string()))?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| AppError::Other(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{DownloadStatus, NewDownload};

    fn record_with(path: Option<&str>) -> DownloadRecord {
        let db = Db::open_in_memory().unwrap();
        let r = db
            .insert_download(&NewDownload {
                url: "u".into(),
                format: "video".into(),
                ..Default::default()
            })
            .unwrap();
        if let Some(p) = path {
            db.set_result(r.id, p, None).unwrap();
            db.set_status(r.id, DownloadStatus::Completed, None)
                .unwrap();
        }
        db.get_download(r.id).unwrap().unwrap()
    }

    #[test]
    fn existing_file_requires_a_file_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("v.mp4");
        std::fs::write(&file, b"x").unwrap();

        assert_eq!(
            existing_file(&record_with(Some(file.to_str().unwrap()))).unwrap(),
            file
        );
        for record in [
            record_with(None),
            record_with(Some(dir.path().join("gone.mp4").to_str().unwrap())),
            // A directory is not a downloaded file.
            record_with(Some(dir.path().to_str().unwrap())),
        ] {
            let err = existing_file(&record).unwrap_err();
            assert_eq!(err.code(), "file_missing");
        }
    }
}

/// Local copy of a video thumbnail (YouTube image hosts only).
#[tauri::command]
pub async fn thumbnail_cached(
    cache: State<'_, crate::thumbs::ThumbCache>,
    url: String,
) -> AppResult<String> {
    Ok(cache.get(&url).await?.to_string_lossy().into_owned())
}

/// Plain-text report for bug reports: versions, components, settings, recent log.
#[tauri::command]
pub fn diagnostics(
    app: AppHandle,
    db: State<'_, Db>,
    manager: State<'_, Arc<ComponentManager>>,
) -> AppResult<String> {
    const LOG_LINES: usize = 80;
    let log_tail = app
        .path()
        .app_log_dir()
        .ok()
        .and_then(|dir| std::fs::read_to_string(dir.join("cliprigo.log")).ok())
        .map(|log| crate::diagnostics::tail_lines(&log, LOG_LINES))
        .unwrap_or_else(|| "(no log)".into());
    Ok(crate::diagnostics::format_report(
        &crate::diagnostics::DiagInfo {
            app_version: env!("CARGO_PKG_VERSION").into(),
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            components: manager.status(),
            settings: db.load_settings()?,
            log_tail,
        },
    ))
}
