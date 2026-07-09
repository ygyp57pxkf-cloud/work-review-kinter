use crate::error::Result;
use rusqlite::Connection;

pub mod project_rules;
pub mod sessionize;

pub fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS work_sessions (
            id INTEGER PRIMARY KEY,
            date TEXT NOT NULL,
            started_at INTEGER NOT NULL,
            ended_at INTEGER NOT NULL,
            duration INTEGER NOT NULL,
            primary_app TEXT NOT NULL,
            activity_ids TEXT NOT NULL,
            summary TEXT,
            created_at INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_work_sessions_date
            ON work_sessions (date);

        CREATE TABLE IF NOT EXISTS project_attributions (
            id INTEGER PRIMARY KEY,
            session_id INTEGER NOT NULL,
            project_key TEXT NOT NULL,
            project_name TEXT NOT NULL,
            confidence INTEGER NOT NULL,
            evidence TEXT NOT NULL,
            needs_review INTEGER NOT NULL DEFAULT 1,
            confirmed INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            FOREIGN KEY(session_id) REFERENCES work_sessions(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_project_attributions_session
            ON project_attributions (session_id);

        CREATE TABLE IF NOT EXISTS obsidian_exports (
            id INTEGER PRIMARY KEY,
            date TEXT NOT NULL,
            target_path TEXT NOT NULL,
            content_hash TEXT NOT NULL,
            exported_at INTEGER NOT NULL,
            status TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_obsidian_exports_date
            ON obsidian_exports (date);",
    )?;

    Ok(())
}
