mod commands;
mod components;
mod db;
mod error;
mod process;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tracing_subscriber::EnvFilter;

use components::{Component, ComponentManager, Progress};
use db::Db;

#[derive(Clone, Serialize)]
struct ComponentUpdated {
    component: Component,
    version: String,
}

/// Background yt-dlp update on startup. Failures are logged only: the app works fine
/// with the installed version, and the user can update manually from settings.
async fn auto_update(app: AppHandle) {
    let enabled = app
        .state::<Db>()
        .load_settings()
        .map(|s| s.auto_update_ytdlp)
        .unwrap_or(true);
    if !enabled {
        return;
    }
    let emit = |p: Progress| {
        let _ = app.emit("component://progress", p);
    };
    match app
        .state::<ComponentManager>()
        .auto_update_ytdlp(&emit)
        .await
    {
        Ok(Some(version)) => {
            let _ = app.emit(
                "component://updated",
                ComponentUpdated {
                    component: Component::Ytdlp,
                    version,
                },
            );
        }
        Ok(None) => {}
        Err(e) => tracing::warn!(error = %e, "yt-dlp auto-update failed"),
    }
}

fn init_logging(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let log_dir = app.path().app_log_dir()?;
    std::fs::create_dir_all(&log_dir)?;
    let file_appender = tracing_appender::rolling::never(&log_dir, "cliprigo.log");
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_writer(file_appender)
        .with_ansi(false)
        .init();
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            init_logging(app)?;
            tracing::info!(version = env!("CARGO_PKG_VERSION"), "Cliprigo started");

            let db = Db::open(&app.path().app_data_dir()?.join("cliprigo.db"))?;
            let interrupted = db.mark_interrupted()?;
            if interrupted > 0 {
                tracing::info!(interrupted, "marked unfinished downloads as interrupted");
            }
            app.manage(db);

            let data_dir = app.path().app_data_dir()?;
            app.manage(ComponentManager::new(data_dir.join("bin"))?);
            tauri::async_runtime::spawn(auto_update(app.handle().clone()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::history_list,
            commands::history_delete,
            commands::settings_get,
            commands::settings_set,
            commands::components_status,
            commands::components_install,
            commands::components_check_updates,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
