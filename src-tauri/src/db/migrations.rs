use rusqlite::Connection;

use crate::error::AppResult;

/// Each entry upgrades the schema by one version. Never edit an entry once released;
/// append a new one instead. The current version is kept in `PRAGMA user_version`.
const MIGRATIONS: &[&str] = &[
    // 1: initial schema
    r#"
    CREATE TABLE downloads (
        id              INTEGER PRIMARY KEY AUTOINCREMENT,
        video_id        TEXT,
        url             TEXT NOT NULL,
        title           TEXT,
        uploader        TEXT,
        duration        INTEGER,
        thumbnail_url   TEXT,
        format          TEXT NOT NULL,
        quality         INTEGER,
        file_path       TEXT,
        file_size       INTEGER,
        status          TEXT NOT NULL,
        error           TEXT,
        playlist_id     TEXT,
        playlist_title  TEXT,
        created_at      INTEGER NOT NULL,
        completed_at    INTEGER
    );
    CREATE INDEX idx_downloads_video_id ON downloads(video_id);
    CREATE INDEX idx_downloads_created_at ON downloads(created_at);
    CREATE INDEX idx_downloads_status ON downloads(status);

    CREATE TABLE settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
    "#,
    // 2: remember the target folder so failed downloads can be retried
    "ALTER TABLE downloads ADD COLUMN output_dir TEXT;",
    // 3: video codec preference, so retries use the same one
    "ALTER TABLE downloads ADD COLUMN codec TEXT;",
    // 4: videos saved from search
    r#"
    CREATE TABLE favorites (
        video_id      TEXT PRIMARY KEY,
        url           TEXT NOT NULL,
        title         TEXT NOT NULL,
        uploader      TEXT,
        duration      INTEGER,
        thumbnail_url TEXT,
        view_count    INTEGER,
        added_at      INTEGER NOT NULL
    );
    "#,
    // 5: search history with the results of the last search for each query
    r#"
    CREATE TABLE searches (
        id           INTEGER PRIMARY KEY,
        query        TEXT NOT NULL,
        -- The query as compared: trimmed, single spaces, lower case.
        query_key    TEXT NOT NULL UNIQUE,
        -- SearchFilters as JSON.
        filters      TEXT NOT NULL,
        searched_at  INTEGER NOT NULL
    );
    CREATE TABLE search_results (
        search_id     INTEGER NOT NULL REFERENCES searches(id) ON DELETE CASCADE,
        position      INTEGER NOT NULL,
        video_id      TEXT NOT NULL,
        url           TEXT NOT NULL,
        title         TEXT NOT NULL,
        uploader      TEXT,
        duration      INTEGER,
        thumbnail_url TEXT,
        view_count    INTEGER,
        live          INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (search_id, video_id)
    );
    "#,
    // 6: when videos were published (Unix seconds, approximate in lists)
    r#"
    ALTER TABLE favorites ADD COLUMN published INTEGER;
    ALTER TABLE search_results ADD COLUMN published INTEGER;
    "#,
    // 7: subtitle languages saved with a video
    r#"
    ALTER TABLE downloads ADD COLUMN subtitles TEXT;
    "#,
];

pub fn run(conn: &mut Connection) -> AppResult<()> {
    let current: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    for (version, sql) in (1_i64..).zip(MIGRATIONS).skip(current.max(0) as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", version)?;
        tx.commit()?;
        tracing::info!(version, "applied database migration");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn).unwrap();
        run(&mut conn).unwrap();
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, MIGRATIONS.len() as i64);
    }

    #[test]
    fn upgrade_from_v1_keeps_rows() {
        let mut conn = Connection::open_in_memory().unwrap();
        let tx = conn.transaction().unwrap();
        tx.execute_batch(MIGRATIONS[0]).unwrap();
        tx.pragma_update(None, "user_version", 1).unwrap();
        tx.commit().unwrap();
        conn.execute(
            "INSERT INTO downloads (url, format, status, created_at) VALUES ('u', 'video', 'completed', 1)",
            [],
        )
        .unwrap();

        run(&mut conn).unwrap();
        let (count, dir): (i64, Option<String>) = conn
            .query_row("SELECT count(*), max(output_dir) FROM downloads", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((count, dir), (1, None));
        let codec: Option<String> = conn
            .query_row("SELECT codec FROM downloads", [], |r| r.get(0))
            .unwrap();
        assert_eq!(codec, None);
    }
}
