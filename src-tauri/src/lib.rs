mod commands;
mod db;
mod error;

use tauri::Manager;
use tracing_subscriber::EnvFilter;

use db::Db;

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
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::history_list,
            commands::history_delete,
            commands::settings_get,
            commands::settings_set,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
