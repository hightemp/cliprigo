//! Videos saved from search to come back to later.

use serde::{Deserialize, Serialize};

use super::{Db, now_unix};
use crate::error::AppResult;

/// A saved video, shaped like a search result for the UI.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Favorite {
    /// YouTube video id.
    pub id: String,
    pub url: String,
    pub title: String,
    pub uploader: Option<String>,
    pub duration: Option<i64>,
    pub thumbnail: Option<String>,
    pub view_count: Option<i64>,
    pub published: Option<i64>,
    pub added_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewFavorite {
    pub id: String,
    pub url: String,
    pub title: String,
    pub uploader: Option<String>,
    pub duration: Option<i64>,
    pub thumbnail: Option<String>,
    pub view_count: Option<i64>,
    #[serde(default)]
    pub published: Option<i64>,
}

const COLUMNS: &str =
    "video_id, url, title, uploader, duration, thumbnail_url, view_count, added_at, published";

fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Favorite> {
    Ok(Favorite {
        id: row.get(0)?,
        url: row.get(1)?,
        title: row.get(2)?,
        uploader: row.get(3)?,
        duration: row.get(4)?,
        thumbnail: row.get(5)?,
        view_count: row.get(6)?,
        added_at: row.get(7)?,
        published: row.get(8)?,
    })
}

impl Db {
    /// Saves a video; saving it again refreshes its details but keeps its place.
    pub fn add_favorite(&self, f: &NewFavorite) -> AppResult<Favorite> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO favorites (video_id, url, title, uploader, duration, thumbnail_url, \
             view_count, added_at, published) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
             ON CONFLICT(video_id) DO UPDATE SET url = excluded.url, title = excluded.title, \
             uploader = excluded.uploader, duration = excluded.duration, \
             thumbnail_url = excluded.thumbnail_url, view_count = excluded.view_count, \
             published = coalesce(excluded.published, published)",
            rusqlite::params![
                f.id,
                f.url,
                f.title,
                f.uploader,
                f.duration,
                f.thumbnail,
                f.view_count,
                now_unix(),
                f.published,
            ],
        )?;
        Ok(conn.query_row(
            &format!("SELECT {COLUMNS} FROM favorites WHERE video_id = ?1"),
            [&f.id],
            from_row,
        )?)
    }

    /// False if it was not saved.
    pub fn remove_favorite(&self, id: &str) -> AppResult<bool> {
        Ok(self
            .conn()
            .execute("DELETE FROM favorites WHERE video_id = ?1", [id])?
            > 0)
    }

    /// Newest first; saved in the same second, by the order of saving.
    pub fn list_favorites(&self) -> AppResult<Vec<Favorite>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM favorites ORDER BY added_at DESC, rowid DESC"
        ))?;
        let rows = stmt.query_map([], from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn video(id: &str, title: &str) -> NewFavorite {
        NewFavorite {
            id: id.into(),
            url: format!("https://www.youtube.com/watch?v={id}"),
            title: title.into(),
            uploader: Some("Channel".into()),
            duration: Some(213),
            thumbnail: Some("https://i.ytimg.com/vi/x/hq.jpg".into()),
            view_count: Some(1_000),
            published: Some(1_762_300_800),
        }
    }

    #[test]
    fn saved_videos_are_listed_newest_first() {
        let db = Db::open_in_memory().unwrap();
        let first = db.add_favorite(&video("aaaaaaaaaaa", "First")).unwrap();
        assert_eq!(first.title, "First");
        assert_eq!(first.duration, Some(213));
        assert_eq!(first.published, Some(1_762_300_800));
        assert!(first.added_at > 0);
        db.add_favorite(&video("bbbbbbbbbbb", "Second")).unwrap();
        let ids: Vec<_> = db
            .list_favorites()
            .unwrap()
            .into_iter()
            .map(|f| f.id)
            .collect();
        assert_eq!(ids, ["bbbbbbbbbbb", "aaaaaaaaaaa"]);
    }

    #[test]
    fn saving_again_updates_in_place() {
        let db = Db::open_in_memory().unwrap();
        let before = db.add_favorite(&video("aaaaaaaaaaa", "Old title")).unwrap();
        db.add_favorite(&video("bbbbbbbbbbb", "Other")).unwrap();
        let after = db.add_favorite(&video("aaaaaaaaaaa", "New title")).unwrap();
        assert_eq!(after.title, "New title");
        assert_eq!(after.added_at, before.added_at);
        let list = db.list_favorites().unwrap();
        assert_eq!(list.len(), 2);
        // Still second: its place is when it was first saved.
        assert_eq!(list[1].id, "aaaaaaaaaaa");
    }

    #[test]
    fn removing() {
        let db = Db::open_in_memory().unwrap();
        db.add_favorite(&video("aaaaaaaaaaa", "A")).unwrap();
        assert!(db.remove_favorite("aaaaaaaaaaa").unwrap());
        assert!(!db.remove_favorite("aaaaaaaaaaa").unwrap());
        assert!(db.list_favorites().unwrap().is_empty());
    }
}
