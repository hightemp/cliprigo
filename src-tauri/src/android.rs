//! Android-only bridge to `FilesPlugin.kt`: opening a file needs a content:// URI from
//! FileProvider, which tauri-plugin-opener does not create.

use std::path::Path;

use serde::Serialize;
use tauri::plugin::{Builder, PluginHandle, TauriPlugin};
use tauri::{Manager, Runtime};

use crate::error::{AppError, AppResult};

pub struct Files<R: Runtime>(PluginHandle<R>);

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("cliprigo-files")
        .setup(|app, api| {
            let handle = api.register_android_plugin("com.hightemp.cliprigo", "FilesPlugin")?;
            app.manage(Files(handle));
            Ok(())
        })
        .build()
}

#[derive(Serialize)]
struct OpenArgs<'a> {
    path: &'a str,
}

impl<R: Runtime> Files<R> {
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
}
