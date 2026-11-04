//! Past searches, each with the results of its last run (all the pages loaded),
//! to look through again later.

use serde::Serialize;

use super::{Db, now_unix};
use crate::error::AppResult;
use crate::ytdlp::probe::PlaylistEntry;
use crate::ytdlp::search::SearchFilters;

/// Searches kept; older ones are dropped with their results.
pub const SEARCH_HISTORY_LIMIT: usize = 50;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRecord {
    pub id: i64,
    /// As typed the last time.
    pub query: String,
    /// The filters of the last run, which the saved results come from.
    pub filters: SearchFilters,
    pub searched_at: i64,
    pub result_count: i64,
}

/// How queries are compared: "Cats", " cats " and "CATS" are one search.
pub fn query_key(query: &str) -> String {
    query
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

const RESULT_COLUMNS: &str =
    "video_id, url, title, uploader, duration, thumbnail_url, view_count, live";

fn result_from_row(row: &rusqlite::Row) -> rusqlite::Result<PlaylistEntry> {
    Ok(PlaylistEntry {
        id: row.get(0)?,
        url: row.get(1)?,
        title: row.get(2)?,
        uploader: row.get(3)?,
        duration: row.get(4)?,
        thumbnail: row.get(5)?,
        available: true,
        view_count: row.get::<_, Option<i64>>(6)?.map(|v| v.max(0) as u64),
        live: row.get(7)?,
    })
}

impl Db {
    /// Saves one page (from 1) of results. The first page starts the search over:
    /// it replaces the results saved before and moves the query to the top.
    pub fn record_search(
        &self,
        query: &str,
        filters: &SearchFilters,
        page: u32,
        entries: &[PlaylistEntry],
    ) -> AppResult<()> {
        let key = query_key(query);
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let id: i64 = if page <= 1 {
            // A new row (and id) puts it on top; its old results go with the old row.
            tx.execute("DELETE FROM searches WHERE query_key = ?1", [&key])?;
            tx.execute(
                "INSERT INTO searches (query, query_key, filters, searched_at) \
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![
                    query.trim(),
                    key,
                    serde_json::to_string(filters)?,
                    now_unix()
                ],
            )?;
            let id = tx.last_insert_rowid();
            tx.execute(
                &format!(
                    "DELETE FROM searches WHERE id NOT IN \
                     (SELECT id FROM searches ORDER BY id DESC LIMIT {SEARCH_HISTORY_LIMIT})"
                ),
                [],
            )?;
            id
        } else {
            match tx.query_row(
                "SELECT id FROM searches WHERE query_key = ?1",
                [&key],
                |r| r.get(0),
            ) {
                Ok(id) => id,
                // The first page was not saved: nothing to add to.
                Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(()),
                Err(e) => return Err(e.into()),
            }
        };
        let next: i64 = tx.query_row(
            "SELECT coalesce(max(position) + 1, 0) FROM search_results WHERE search_id = ?1",
            [id],
            |r| r.get(0),
        )?;
        {
            let mut insert = tx.prepare(&format!(
                "INSERT OR IGNORE INTO search_results (search_id, position, {RESULT_COLUMNS}) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"
            ))?;
            for (i, e) in entries.iter().enumerate() {
                insert.execute(rusqlite::params![
                    id,
                    next + i as i64,
                    e.id,
                    e.url,
                    e.title,
                    e.uploader,
                    e.duration,
                    e.thumbnail,
                    e.view_count.map(|v| v.min(i64::MAX as u64) as i64),
                    e.live,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Newest first.
    pub fn list_searches(&self) -> AppResult<Vec<SearchRecord>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT s.id, s.query, s.filters, s.searched_at, \
             (SELECT count(*) FROM search_results r WHERE r.search_id = s.id) \
             FROM searches s ORDER BY s.id DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            let filters: String = row.get(2)?;
            Ok(SearchRecord {
                id: row.get(0)?,
                query: row.get(1)?,
                filters: serde_json::from_str(&filters).unwrap_or_default(),
                searched_at: row.get(3)?,
                result_count: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// In the order they were found.
    pub fn search_results(&self, id: i64) -> AppResult<Vec<PlaylistEntry>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT {RESULT_COLUMNS} FROM search_results WHERE search_id = ?1 ORDER BY position"
        ))?;
        let rows = stmt.query_map([id], result_from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// False if there was no such search.
    pub fn remove_search(&self, id: i64) -> AppResult<bool> {
        Ok(self
            .conn()
            .execute("DELETE FROM searches WHERE id = ?1", [id])?
            > 0)
    }

    pub fn clear_searches(&self) -> AppResult<()> {
        self.conn().execute("DELETE FROM searches", [])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ytdlp::search::{Feature, Sort};

    fn video(n: usize) -> PlaylistEntry {
        let id = format!("video{n:06}");
        PlaylistEntry {
            url: format!("https://www.youtube.com/watch?v={id}"),
            id,
            title: format!("Video {n}"),
            uploader: Some("Channel".into()),
            duration: Some(60),
            thumbnail: Some("https://i.ytimg.com/vi/x/hq.jpg".into()),
            available: true,
            view_count: Some(1_000),
            live: n == 2,
        }
    }

    fn videos(range: std::ops::Range<usize>) -> Vec<PlaylistEntry> {
        range.map(video).collect()
    }

    fn ids(entries: &[PlaylistEntry]) -> Vec<String> {
        entries.iter().map(|e| e.id.clone()).collect()
    }

    fn queries(db: &Db) -> Vec<String> {
        db.list_searches()
            .unwrap()
            .into_iter()
            .map(|s| s.query)
            .collect()
    }

    #[test]
    fn keys_ignore_case_and_spacing() {
        assert_eq!(query_key("  Котики   и\tсобаки "), "котики и собаки");
        assert_eq!(query_key("CATS"), query_key("cats"));
    }

    #[test]
    fn a_search_is_saved_with_its_results() {
        let db = Db::open_in_memory().unwrap();
        let filters = SearchFilters {
            features: vec![Feature::Uhd],
            sort: Sort::Views,
            ..Default::default()
        };
        db.record_search("cats", &filters, 1, &videos(0..3))
            .unwrap();

        let list = db.list_searches().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].query, "cats");
        assert_eq!(list[0].filters, filters);
        assert_eq!(list[0].result_count, 3);
        assert!(list[0].searched_at > 0);
        let results = db.search_results(list[0].id).unwrap();
        assert_eq!(results, videos(0..3));
    }

    #[test]
    fn next_pages_are_added_after_without_repeats() {
        let db = Db::open_in_memory().unwrap();
        let filters = SearchFilters::default();
        db.record_search("cats", &filters, 1, &videos(0..3))
            .unwrap();
        db.record_search("Cats", &filters, 2, &videos(2..5))
            .unwrap();

        let list = db.list_searches().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].result_count, 5);
        assert_eq!(
            ids(&db.search_results(list[0].id).unwrap()),
            ids(&videos(0..5))
        );
    }

    #[test]
    fn a_next_page_of_an_unsaved_search_is_ignored() {
        let db = Db::open_in_memory().unwrap();
        db.record_search("cats", &SearchFilters::default(), 2, &videos(0..3))
            .unwrap();
        assert!(db.list_searches().unwrap().is_empty());
    }

    #[test]
    fn searching_again_replaces_results_and_moves_up() {
        let db = Db::open_in_memory().unwrap();
        let plain = SearchFilters::default();
        db.record_search("cats", &plain, 1, &videos(0..3)).unwrap();
        db.record_search("dogs", &plain, 1, &videos(10..12))
            .unwrap();
        assert_eq!(queries(&db), ["dogs", "cats"]);

        let views = SearchFilters {
            sort: Sort::Views,
            ..Default::default()
        };
        db.record_search(" CATS ", &views, 1, &videos(20..21))
            .unwrap();
        let list = db.list_searches().unwrap();
        assert_eq!(queries(&db), ["CATS", "dogs"]);
        assert_eq!(list[0].filters, views);
        assert_eq!(list[0].result_count, 1);
        assert_eq!(
            ids(&db.search_results(list[0].id).unwrap()),
            ids(&videos(20..21))
        );
    }

    #[test]
    fn a_search_with_nothing_found_is_kept() {
        let db = Db::open_in_memory().unwrap();
        db.record_search("zzzz", &SearchFilters::default(), 1, &[])
            .unwrap();
        let list = db.list_searches().unwrap();
        assert_eq!(list[0].result_count, 0);
        assert!(db.search_results(list[0].id).unwrap().is_empty());
    }

    #[test]
    fn only_the_latest_searches_are_kept() {
        let db = Db::open_in_memory().unwrap();
        let plain = SearchFilters::default();
        db.record_search("first", &plain, 1, &videos(0..2)).unwrap();
        let first = db.list_searches().unwrap()[0].id;
        for n in 1..=SEARCH_HISTORY_LIMIT {
            db.record_search(&format!("query {n}"), &plain, 1, &videos(0..1))
                .unwrap();
        }
        let list = queries(&db);
        assert_eq!(list.len(), SEARCH_HISTORY_LIMIT);
        assert_eq!(list[0], format!("query {SEARCH_HISTORY_LIMIT}"));
        assert!(!list.contains(&"first".to_string()));
        assert!(db.search_results(first).unwrap().is_empty());
    }

    #[test]
    fn removing_and_clearing() {
        let db = Db::open_in_memory().unwrap();
        let plain = SearchFilters::default();
        db.record_search("cats", &plain, 1, &videos(0..2)).unwrap();
        db.record_search("dogs", &plain, 1, &videos(0..2)).unwrap();
        let cats = db.list_searches().unwrap()[1].id;

        assert!(db.remove_search(cats).unwrap());
        assert!(!db.remove_search(cats).unwrap());
        assert_eq!(queries(&db), ["dogs"]);
        assert!(db.search_results(cats).unwrap().is_empty());

        db.clear_searches().unwrap();
        assert!(db.list_searches().unwrap().is_empty());
        let left: i64 = db
            .conn()
            .query_row("SELECT count(*) FROM search_results", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 0);
    }
}
