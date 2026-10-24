//! Android-only bridge to `PlatformPlugin.kt`: opening a file needs a content:// URI
//! from FileProvider, which tauri-plugin-opener does not create, and downloads need a
//! foreground service to keep running in the background.

use std::path::Path;

use serde::Serialize;
use tauri::plugin::{Builder, PluginHandle, TauriPlugin};
use tauri::{Manager, Runtime};

use crate::error::{AppError, AppResult};

pub struct Platform<R: Runtime>(PluginHandle<R>);

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("cliprigo-platform")
        .setup(|app, api| {
            let handle = api.register_android_plugin("com.hightemp.cliprigo", "PlatformPlugin")?;
            app.manage(Platform(handle));
            Ok(())
        })
        .build()
}

#[derive(Serialize)]
struct OpenArgs<'a> {
    path: &'a str,
}

#[derive(Serialize)]
struct KeepAliveArgs {
    text: Option<String>,
}

impl<R: Runtime> Platform<R> {
    /// Opens the file in the app the user picks for its type.
    pub fn open(&self, path: &Path) -> AppResult<()> {
        self.0
            .run_mobile_plugin::<serde_json::Value>(
                "open",
                OpenArgs {
                    path: &path.to_string_lossy(),
                },
            )
            .map(|_| ())
            .map_err(|e| AppError::Other(e.to_string()))
    }

    /// Shows the download notification with `text` and keeps the app running, or
    /// stops that with `None`.
    pub fn keep_alive(&self, text: Option<String>) -> AppResult<()> {
        self.0
            .run_mobile_plugin::<serde_json::Value>("keepAlive", KeepAliveArgs { text })
            .map(|_| ())
            .map_err(|e| AppError::Other(e.to_string()))
    }
}
