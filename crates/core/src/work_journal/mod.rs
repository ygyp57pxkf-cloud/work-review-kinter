use crate::error::Result;
use rusqlite::Connection;

pub mod project_rules;
pub mod obsidian;
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

#[cfg(test)]
mod tests {
    use crate::work_journal::obsidian::{
        render_obsidian_daily_preview, ObsidianExportDay, ObsidianExportSession,
    };

    #[test]
    fn obsidian预览应包含项目链接待确认标记和隐私说明() {
        let markdown = render_obsidian_daily_preview(&ObsidianExportDay {
            date: "2026-07-09".to_string(),
            total_duration: 3_600,
            project_count: 1,
            sessions: vec![ObsidianExportSession {
                started_at: "09:00".to_string(),
                ended_at: "10:00".to_string(),
                duration: 3_600,
                project_name: "Work Journal".to_string(),
                obsidian_page: "私人/个人项目文档/Work Journal/Work Journal".to_string(),
                task_summary: "推进 Journal 审阅页".to_string(),
                evidence: vec!["window:timereview".to_string()],
                needs_review: true,
            }],
        });

        assert!(markdown.contains("# 2026-07-09 工作日志"));
        assert!(markdown.contains("[[私人/个人项目文档/Work Journal/Work Journal|Work Journal]]"));
        assert!(markdown.contains("待确认"));
        assert!(markdown.contains("隐私过滤说明"));
        assert!(markdown.contains("window:timereview"));
    }
}
