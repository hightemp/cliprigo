mod commands;
mod components;
mod db;
mod diagnostics;
mod error;
mod net;
mod net_env;
mod panic_log;
mod process;
mod proxy_check;
mod queue;
mod thumbs;
mod ytdlp;

use std::sync::Arc;

use futures_util::FutureExt;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tracing_subscriber::EnvFilter;

use components::{Component, ComponentManager, Progress};
use db::Db;
use queue::{Queue, QueueEvent};

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
        .state::<Arc<ComponentManager>>()
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

/// Production queue: runs yt-dlp with the installed tools and forwards events to the UI.
fn create_queue(app: &AppHandle, db: Db, components: Arc<ComponentManager>) -> Queue {
    let run: queue::RunFn = Arc::new(move |request, cancel, sink| {
        let components = components.clone();
        async move {
            let tools = components.tools()?;
            ytdlp::runner::download(&tools, &request, cancel, sink).await
        }
        .boxed()
    });
    let app = app.clone();
    let sink: queue::EventSink = Arc::new(move |event| {
        let result = match &event {
            QueueEvent::Status { .. } => app.emit("download://status", event),
            QueueEvent::Progress { .. } => app.emit("download://progress", event),
        };
        if let Err(e) = result {
            tracing::warn!(error = %e, "failed to emit queue event");
        }
    });
    Queue::new(
        tauri::async_runtime::handle().inner().clone(),
        db,
        run,
        sink,
    )
}

fn init_logging(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let log_dir = app.path().app_log_dir()?;
    std::fs::create_dir_all(&log_dir)?;
    panic_log::install(log_dir.join("cliprigo.log"));
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
    net_env::bypass_proxy_for_localhost();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            init_logging(app)?;
            tracing::info!(version = env!("CARGO_PKG_VERSION"), "Cliprigo started");

            let db = Db::open(&app.path().app_data_dir()?.join("cliprigo.db"))?;
            let interrupted = db.mark_interrupted()?;
            if interrupted > 0 {
                tracing::info!(interrupted, "marked unfinished downloads as interrupted");
            }
            let proxy = net::ProxySetting::default();
            proxy.set(
                net::validate_proxy(&db.load_settings()?.proxy)
                    .ok()
                    .flatten(),
            );
            app.manage(proxy.clone());
            app.manage(db.clone());

            let data_dir = app.path().app_data_dir()?;
            let components = Arc::new(ComponentManager::new(data_dir.join("bin"), proxy.clone())?);
            app.manage(components.clone());
            app.manage(create_queue(app.handle(), db, components));
            app.manage(thumbs::ThumbCache::new(
                app.path().app_cache_dir()?.join("thumbs"),
                proxy,
            )?);
            tauri::async_runtime::spawn(auto_update(app.handle().clone()));
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing would kill running downloads: let the UI ask first, then call app_quit.
            if let WindowEvent::CloseRequested { api, .. } = event
                && window.state::<Queue>().has_active()
            {
                api.prevent_close();
                let _ = window.emit("app://close-requested", ());
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::history_list,
            commands::history_delete,
            commands::history_open,
            commands::history_reveal,
            commands::open_logs,
            commands::settings_get,
            commands::settings_set,
            commands::proxy_check,
            commands::components_status,
            commands::components_install,
            commands::components_check_updates,
            commands::video_probe,
            commands::url_inspect,
            commands::thumbnail_cached,
            commands::diagnostics,
            commands::queue_add,
            commands::queue_cancel,
            commands::queue_retry,
            commands::queue_list,
            commands::find_duplicates,
            commands::app_quit,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
