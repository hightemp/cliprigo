use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::Db;
use crate::error::AppResult;

pub const MIN_CONCURRENCY: u8 = 1;
pub const MAX_CONCURRENCY: u8 = 5;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub download_dir: String,
    /// `video`, `audio_mp3` or `audio_m4a`.
    pub default_format: String,
    /// Max video height; `None` means best available.
    pub default_quality: Option<i64>,
    pub concurrency: u8,
    pub filename_template: String,
    /// `system`, `ru` or `en`.
    pub language: String,
    pub auto_update_ytdlp: bool,
}

impl Default for Settings {
    fn default() -> Self {
        let download_dir = dirs::download_dir()
            .or_else(dirs::home_dir)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self {
            download_dir,
            default_format: "video".into(),
            default_quality: None,
            concurrency: 2,
            filename_template: "%(title)s [%(id)s].%(ext)s".into(),
            language: "system".into(),
            auto_update_ytdlp: true,
        }
    }
}

impl Settings {
    fn normalized(mut self) -> Self {
        let defaults = Settings::default();
        self.concurrency = self.concurrency.clamp(MIN_CONCURRENCY, MAX_CONCURRENCY);
        if self.filename_template.trim().is_empty() {
            self.filename_template = defaults.filename_template;
        }
        if self.download_dir.trim().is_empty() {
            self.download_dir = defaults.download_dir;
        }
        if !matches!(
            self.default_format.as_str(),
            "video" | "audio_mp3" | "audio_m4a"
        ) {
            self.default_format = defaults.default_format;
        }
        if !matches!(self.language.as_str(), "system" | "ru" | "en") {
            self.language = defaults.language;
        }
        self
    }
}

impl Db {
    /// Each field is stored as its own row (key = field name, value = JSON), so adding
    /// a field later needs no migration: missing keys fall back to defaults.
    pub fn load_settings(&self) -> AppResult<Settings> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
        let mut map = Map::new();
        for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (key, raw) = row?;
            match serde_json::from_str::<Value>(&raw) {
                Ok(v) => {
                    map.insert(key, v);
                }
                Err(e) => tracing::warn!(key, error = %e, "ignoring malformed setting"),
            }
        }
        let settings = serde_json::from_value(Value::Object(map)).unwrap_or_else(|e| {
            tracing::warn!(error = %e, "settings unreadable, using defaults");
            Settings::default()
        });
        Ok(settings.normalized())
    }

    pub fn save_settings(&self, settings: Settings) -> AppResult<Settings> {
        let settings = settings.normalized();
        let Value::Object(map) = serde_json::to_value(&settings)? else {
            unreachable!("Settings serializes to an object");
        };
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        for (key, value) in map {
            tx.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2) \
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value.to_string()],
            )?;
        }
        tx.commit()?;
        Ok(settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_when_empty() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.load_settings().unwrap(), Settings::default());
    }

    #[test]
    fn roundtrip_and_normalization() {
        let db = Db::open_in_memory().unwrap();
        let saved = db
            .save_settings(Settings {
                concurrency: 99,
                default_quality: Some(720),
                language: "ru".into(),
                filename_template: " ".into(),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(saved.concurrency, MAX_CONCURRENCY);
        assert_eq!(
            saved.filename_template,
            Settings::default().filename_template
        );
        assert_eq!(db.load_settings().unwrap(), saved);
    }

    #[test]
    fn unknown_and_malformed_rows_are_ignored() {
        let db = Db::open_in_memory().unwrap();
        db.conn()
            .execute_batch(
                "INSERT INTO settings VALUES ('removedField', '1');
                 INSERT INTO settings VALUES ('language', 'not json');
                 INSERT INTO settings VALUES ('concurrency', '3');",
            )
            .unwrap();
        let s = db.load_settings().unwrap();
        assert_eq!(s.concurrency, 3);
        assert_eq!(s.language, "system");
    }
}
