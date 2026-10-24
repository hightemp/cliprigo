use std::str::FromStr;

use rusqlite::{Row, params};
use serde::{Deserialize, Serialize};

use super::{Db, now_unix};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadStatus {
    Queued,
    Downloading,
    Processing,
    Completed,
    Failed,
    Canceled,
    Interrupted,
}

impl DownloadStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Downloading => "downloading",
            Self::Processing => "processing",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Canceled => "canceled",
            Self::Interrupted => "interrupted",
        }
    }
}

impl FromStr for DownloadStatus {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "queued" => Self::Queued,
            "downloading" => Self::Downloading,
            "processing" => Self::Processing,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "canceled" => Self::Canceled,
            "interrupted" => Self::Interrupted,
            other => return Err(AppError::Other(format!("unknown status: {other}"))),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRecord {
    pub id: i64,
    pub video_id: Option<String>,
    pub url: String,
    pub title: Option<String>,
    pub uploader: Option<String>,
    pub duration: Option<i64>,
    pub thumbnail_url: Option<String>,
    /// `video`, `audio_mp3` or `audio_m4a`.
    pub format: String,
    /// Max video height; `None` means best available.
    pub quality: Option<i64>,
    pub file_path: Option<String>,
    pub file_size: Option<i64>,
    pub status: DownloadStatus,
    pub error: Option<String>,
    pub playlist_id: Option<String>,
    pub playlist_title: Option<String>,
    pub created_at: i64,
    pub completed_at: Option<i64>,
    pub output_dir: Option<String>,
    /// Video codec preference (`auto`, `h264`, `av1`, `vp9`); `None` for audio and old rows.
    pub codec: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewDownload {
    pub video_id: Option<String>,
    pub url: String,
    pub title: Option<String>,
    pub uploader: Option<String>,
    pub duration: Option<i64>,
    pub thumbnail_url: Option<String>,
    pub format: String,
    pub quality: Option<i64>,
    pub playlist_id: Option<String>,
    pub playlist_title: Option<String>,
    pub output_dir: Option<String>,
    pub codec: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryQuery {
    /// Substring matched against title and uploader, case-insensitive (Unicode-aware).
    pub search: Option<String>,
    pub status: Option<DownloadStatus>,
    pub offset: Option<i64>,
    pub limit: Option<i64>,
}

const COLUMNS: &str = "id, video_id, url, title, uploader, duration, thumbnail_url, format, \
    quality, file_path, file_size, status, error, playlist_id, playlist_title, created_at, \
    completed_at, output_dir, codec";

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 500;

fn from_row(row: &Row) -> rusqlite::Result<DownloadRecord> {
    let status: String = row.get(11)?;
    Ok(DownloadRecord {
        id: row.get(0)?,
        video_id: row.get(1)?,
        url: row.get(2)?,
        title: row.get(3)?,
        uploader: row.get(4)?,
        duration: row.get(5)?,
        thumbnail_url: row.get(6)?,
        format: row.get(7)?,
        quality: row.get(8)?,
        file_path: row.get(9)?,
        file_size: row.get(10)?,
        status: status.parse().map_err(|e: AppError| {
            rusqlite::Error::FromSqlConversionFailure(11, rusqlite::types::Type::Text, e.into())
        })?,
        error: row.get(12)?,
        playlist_id: row.get(13)?,
        playlist_title: row.get(14)?,
        created_at: row.get(15)?,
        completed_at: row.get(16)?,
        output_dir: row.get(17)?,
        codec: row.get(18)?,
    })
}

impl Db {
    pub fn insert_download(&self, d: &NewDownload) -> AppResult<DownloadRecord> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO downloads (video_id, url, title, uploader, duration, thumbnail_url, \
             format, quality, status, playlist_id, playlist_title, created_at, output_dir, codec) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                d.video_id,
                d.url,
                d.title,
                d.uploader,
                d.duration,
                d.thumbnail_url,
                d.format,
                d.quality,
                DownloadStatus::Queued.as_str(),
                d.playlist_id,
                d.playlist_title,
                now_unix(),
                d.output_dir,
                d.codec,
            ],
        )?;
        let id = conn.last_insert_rowid();
        Ok(conn.query_row(
            &format!("SELECT {COLUMNS} FROM downloads WHERE id = ?1"),
            [id],
            from_row,
        )?)
    }

    pub fn get_download(&self, id: i64) -> AppResult<Option<DownloadRecord>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!("SELECT {COLUMNS} FROM downloads WHERE id = ?1"))?;
        let mut rows = stmt.query_map([id], from_row)?;
        Ok(rows.next().transpose()?)
    }

    pub fn list_downloads(&self, q: &HistoryQuery) -> AppResult<Vec<DownloadRecord>> {
        let search = q
            .search
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_lowercase);
        let status = q.status.map(DownloadStatus::as_str);
        let limit = q.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        let offset = q.offset.unwrap_or(0).max(0);

        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM downloads \
             WHERE (?1 IS NULL OR instr(lower_unicode(title), ?1) > 0 \
                    OR instr(lower_unicode(uploader), ?1) > 0) \
               AND (?2 IS NULL OR status = ?2) \
             ORDER BY created_at DESC, id DESC \
             LIMIT ?3 OFFSET ?4"
        ))?;
        let rows = stmt.query_map(params![search, status, limit, offset], from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn set_status(
        &self,
        id: i64,
        status: DownloadStatus,
        error: Option<&str>,
    ) -> AppResult<()> {
        let completed_at = (status == DownloadStatus::Completed).then(now_unix);
        self.conn().execute(
            "UPDATE downloads SET status = ?2, error = ?3, \
             completed_at = COALESCE(?4, completed_at) WHERE id = ?1",
            params![id, status.as_str(), error, completed_at],
        )?;
        Ok(())
    }

    pub fn set_result(&self, id: i64, file_path: &str, file_size: Option<i64>) -> AppResult<()> {
        self.conn().execute(
            "UPDATE downloads SET file_path = ?2, file_size = ?3 WHERE id = ?1",
            params![id, file_path, file_size],
        )?;
        Ok(())
    }

    pub fn delete_download(&self, id: i64) -> AppResult<Option<DownloadRecord>> {
        let record = self.get_download(id)?;
        self.conn()
            .execute("DELETE FROM downloads WHERE id = ?1", [id])?;
        Ok(record)
    }

    /// Completed and active records of `video_ids`, newest first, to warn before
    /// downloading a video again. The caller matches format and folder.
    pub fn find_duplicates(&self, video_ids: &[String]) -> AppResult<Vec<DownloadRecord>> {
        if video_ids.is_empty() {
            return Ok(Vec::new());
        }
        const RELEVANT: &str = "('completed', 'queued', 'downloading', 'processing')";
        let placeholders = vec!["?"; video_ids.len()].join(", ");
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM downloads \
             WHERE video_id IN ({placeholders}) AND status IN {RELEVANT} \
             ORDER BY id DESC"
        ))?;
        let rows = stmt.query_map(rusqlite::params_from_iter(video_ids), from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// True when a record other than `id` points at `path`.
    pub fn file_used_by_others(&self, path: &str, id: i64) -> AppResult<bool> {
        Ok(self.conn().query_row(
            "SELECT EXISTS (SELECT 1 FROM downloads WHERE file_path = ?1 AND id != ?2)",
            rusqlite::params![path, id],
            |row| row.get(0),
        )?)
    }

    /// Downloads that were in flight when the app last exited can't be resumed
    /// automatically; mark them so the user can retry.
    pub fn mark_interrupted(&self) -> AppResult<usize> {
        Ok(self.conn().execute(
            "UPDATE downloads SET status = 'interrupted' \
             WHERE status IN ('queued', 'downloading', 'processing')",
            [],
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new(title: &str, uploader: &str) -> NewDownload {
        NewDownload {
            video_id: Some(format!("id-{title}")),
            url: format!("https://youtu.be/{title}"),
            title: Some(title.into()),
            uploader: Some(uploader.into()),
            format: "video".into(),
            ..Default::default()
        }
    }

    #[test]
    fn insert_and_list_newest_first() {
        let db = Db::open_in_memory().unwrap();
        let a = db.insert_download(&new("first", "alice")).unwrap();
        let b = db.insert_download(&new("second", "bob")).unwrap();
        assert_eq!(a.status, DownloadStatus::Queued);

        let all = db.list_downloads(&HistoryQuery::default()).unwrap();
        assert_eq!(
            all.iter().map(|r| r.id).collect::<Vec<_>>(),
            vec![b.id, a.id]
        );
    }

    #[test]
    fn search_is_case_insensitive_unicode_and_literal() {
        let db = Db::open_in_memory().unwrap();
        db.insert_download(&new("Rust tutorial", "alice")).unwrap();
        db.insert_download(&new("100% cooking", "Bob")).unwrap();
        db.insert_download(&new("100 cats", "carol")).unwrap();
        db.insert_download(&new("Обзор ноутбука", "dave")).unwrap();

        let search = |s: &str| {
            db.list_downloads(&HistoryQuery {
                search: Some(s.into()),
                ..Default::default()
            })
            .unwrap()
            .len()
        };
        assert_eq!(search("rust"), 1);
        assert_eq!(search("bob"), 1);
        assert_eq!(search("100%"), 1);
        assert_eq!(search("обзор"), 1);
        assert_eq!(search("НОУТБУК"), 1);
        assert_eq!(search("  "), 4);
    }

    #[test]
    fn status_filter_and_completion() {
        let db = Db::open_in_memory().unwrap();
        let a = db.insert_download(&new("a", "x")).unwrap();
        db.insert_download(&new("b", "x")).unwrap();
        db.set_result(a.id, "/tmp/a.mp4", Some(42)).unwrap();
        db.set_status(a.id, DownloadStatus::Completed, None)
            .unwrap();

        let done = db
            .list_downloads(&HistoryQuery {
                status: Some(DownloadStatus::Completed),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].file_size, Some(42));
        assert!(done[0].completed_at.is_some());
    }

    #[test]
    fn mark_interrupted_only_touches_active() {
        let db = Db::open_in_memory().unwrap();
        let a = db.insert_download(&new("a", "x")).unwrap();
        let b = db.insert_download(&new("b", "x")).unwrap();
        db.set_status(b.id, DownloadStatus::Completed, None)
            .unwrap();

        assert_eq!(db.mark_interrupted().unwrap(), 1);
        let a = db.get_download(a.id).unwrap().unwrap();
        assert_eq!(a.status, DownloadStatus::Interrupted);
    }

    #[test]
    fn delete_returns_record() {
        let db = Db::open_in_memory().unwrap();
        let a = db.insert_download(&new("a", "x")).unwrap();
        assert_eq!(db.delete_download(a.id).unwrap().unwrap().id, a.id);
        assert!(db.get_download(a.id).unwrap().is_none());
        assert!(db.delete_download(a.id).unwrap().is_none());
    }

    #[test]
    fn codec_roundtrip() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.insert_download(&new("a", "x")).unwrap().codec, None);
        let mut d = new("b", "x");
        d.codec = Some("h264".into());
        let r = db.insert_download(&d).unwrap();
        assert_eq!(r.codec.as_deref(), Some("h264"));
        assert_eq!(
            db.get_download(r.id).unwrap().unwrap().codec.as_deref(),
            Some("h264")
        );
    }

    #[test]
    fn output_dir_roundtrip() {
        let db = Db::open_in_memory().unwrap();
        let mut d = new("a", "x");
        d.output_dir = Some("/home/u/Videos".into());
        let r = db.insert_download(&d).unwrap();
        assert_eq!(r.output_dir.as_deref(), Some("/home/u/Videos"));
    }

    #[test]
    fn duplicates_are_completed_or_active_only() {
        let db = Db::open_in_memory().unwrap();
        let done = db.insert_download(&new("done", "x")).unwrap();
        db.set_status(done.id, DownloadStatus::Completed, None)
            .unwrap();
        let queued = db.insert_download(&new("queued", "x")).unwrap();
        let failed = db.insert_download(&new("failed", "x")).unwrap();
        db.set_status(failed.id, DownloadStatus::Failed, Some("e"))
            .unwrap();
        // Another copy of the same video (maybe in another format): both are returned,
        // newest first, and the UI picks the one matching the chosen format.
        let again = db.insert_download(&new("done", "x")).unwrap();
        db.set_status(again.id, DownloadStatus::Completed, None)
            .unwrap();

        let ids: Vec<String> = ["id-done", "id-queued", "id-failed", "id-missing"]
            .map(String::from)
            .to_vec();
        let found: Vec<i64> = db
            .find_duplicates(&ids)
            .unwrap()
            .into_iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(found, vec![again.id, queued.id, done.id]);
        assert!(db.find_duplicates(&[]).unwrap().is_empty());
    }

    /// "Download again" in the same quality finds the file in place, so two records
    /// can point at one file; deleting one must not take the other's file.
    #[test]
    fn shared_files_are_detected() {
        let db = Db::open_in_memory().unwrap();
        let a = db.insert_download(&new("a", "x")).unwrap();
        let b = db.insert_download(&new("a", "x")).unwrap();
        db.set_result(a.id, "/dl/a.mp4", None).unwrap();
        assert!(!db.file_used_by_others("/dl/a.mp4", a.id).unwrap());
        db.set_result(b.id, "/dl/a.mp4", None).unwrap();
        assert!(db.file_used_by_others("/dl/a.mp4", a.id).unwrap());
        assert!(db.file_used_by_others("/dl/a.mp4", b.id).unwrap());
        assert!(!db.file_used_by_others("/dl/other.mp4", a.id).unwrap());
    }
}
