use std::path::Path;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::components::{Component, ComponentManager, ComponentStatus, Progress, UpdateInfo};
use crate::db::{Db, DownloadRecord, HistoryQuery, Settings};
use crate::error::AppResult;
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
pub fn history_delete(db: State<'_, Db>, id: i64, delete_file: bool) -> AppResult<()> {
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
pub fn components_status(manager: State<'_, ComponentManager>) -> Vec<ComponentStatus> {
    manager.status()
}

/// Installs or reinstalls the latest version, streaming `component://progress` events.
#[tauri::command]
pub async fn components_install(
    app: AppHandle,
    manager: State<'_, ComponentManager>,
    component: Component,
) -> AppResult<String> {
    let emit = |p: Progress| {
        let _ = app.emit("component://progress", p);
    };
    manager.install(component, &emit).await
}

#[tauri::command]
pub async fn components_check_updates(
    manager: State<'_, ComponentManager>,
) -> AppResult<Vec<UpdateInfo>> {
    manager.check_updates().await
}

/// Metadata for a pasted link: a single video or a playlist/channel listing.
#[tauri::command]
pub async fn video_probe(
    manager: State<'_, ComponentManager>,
    url: String,
) -> AppResult<ProbeResult> {
    let tools = manager.tools()?;
    ytdlp::runner::probe(&tools, &ytdlp::url::normalize(&url)).await
}
