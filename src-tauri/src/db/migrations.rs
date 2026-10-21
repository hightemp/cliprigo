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
}
