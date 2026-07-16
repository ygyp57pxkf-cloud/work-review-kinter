use crate::error::Result;
use rusqlite::Connection;

pub mod attribution;
pub mod obsidian;
pub mod privacy;
pub mod project_rules;
pub mod sessionize;
pub mod storage;

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
            obsidian_page TEXT NOT NULL DEFAULT '',
            confidence INTEGER NOT NULL,
            evidence TEXT NOT NULL,
            needs_review INTEGER NOT NULL DEFAULT 1,
            confirmed INTEGER NOT NULL DEFAULT 0,
            review_state TEXT NOT NULL DEFAULT 'included',
            source TEXT NOT NULL DEFAULT 'rule',
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
            ON obsidian_exports (date);

        CREATE TABLE IF NOT EXISTS work_journal_imports (
            id TEXT PRIMARY KEY,
            source_path TEXT NOT NULL,
            source_db_hash TEXT NOT NULL,
            backup_path TEXT NOT NULL,
            imported_count INTEGER NOT NULL,
            skipped_duplicate_count INTEGER NOT NULL,
            copied_screenshot_count INTEGER NOT NULL,
            skipped_screenshot_count INTEGER NOT NULL,
            completed_at INTEGER NOT NULL,
            status TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS work_journal_imported_activities (
            fingerprint TEXT PRIMARY KEY,
            import_id TEXT NOT NULL,
            source_activity_id INTEGER,
            destination_activity_id INTEGER NOT NULL,
            imported_at INTEGER NOT NULL,
            FOREIGN KEY(import_id) REFERENCES work_journal_imports(id)
        );

        CREATE INDEX IF NOT EXISTS idx_work_journal_imported_activities_import
            ON work_journal_imported_activities (import_id);",
    )?;

    let _ = conn.execute(
        "ALTER TABLE project_attributions ADD COLUMN obsidian_page TEXT NOT NULL DEFAULT ''",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE project_attributions ADD COLUMN review_state TEXT NOT NULL DEFAULT 'included'",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE project_attributions ADD COLUMN source TEXT NOT NULL DEFAULT 'rule'",
        [],
    );
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS project_attributions_dedup_backup (
            backup_id INTEGER PRIMARY KEY AUTOINCREMENT,
            original_id INTEGER NOT NULL,
            session_id INTEGER NOT NULL,
            project_key TEXT NOT NULL,
            project_name TEXT NOT NULL,
            obsidian_page TEXT NOT NULL,
            confidence INTEGER NOT NULL,
            evidence TEXT NOT NULL,
            needs_review INTEGER NOT NULL,
            confirmed INTEGER NOT NULL,
            review_state TEXT NOT NULL,
            source TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            backup_reason TEXT NOT NULL,
            backed_up_at INTEGER NOT NULL
        );",
    )?;
    let backup_schema: String = conn.query_row(
        "SELECT sql FROM sqlite_master
         WHERE type = 'table' AND name = 'project_attributions_dedup_backup'",
        [],
        |row| row.get(0),
    )?;
    if backup_schema
        .to_ascii_lowercase()
        .contains("original_id integer not null unique")
    {
        let transaction = conn.unchecked_transaction()?;
        transaction.execute_batch(
            "ALTER TABLE project_attributions_dedup_backup
                RENAME TO project_attributions_dedup_backup_legacy;
             CREATE TABLE project_attributions_dedup_backup (
                backup_id INTEGER PRIMARY KEY AUTOINCREMENT,
                original_id INTEGER NOT NULL,
                session_id INTEGER NOT NULL,
                project_key TEXT NOT NULL,
                project_name TEXT NOT NULL,
                obsidian_page TEXT NOT NULL,
                confidence INTEGER NOT NULL,
                evidence TEXT NOT NULL,
                needs_review INTEGER NOT NULL,
                confirmed INTEGER NOT NULL,
                review_state TEXT NOT NULL,
                source TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                backup_reason TEXT NOT NULL,
                backed_up_at INTEGER NOT NULL
             );
             INSERT INTO project_attributions_dedup_backup (
                backup_id, original_id, session_id, project_key, project_name, obsidian_page,
                confidence, evidence, needs_review, confirmed, review_state, source,
                created_at, backup_reason, backed_up_at
             )
             SELECT backup_id, original_id, session_id, project_key, project_name, obsidian_page,
                    confidence, evidence, needs_review, confirmed, review_state, source,
                    created_at, backup_reason, backed_up_at
             FROM project_attributions_dedup_backup_legacy;
             DROP TABLE project_attributions_dedup_backup_legacy;",
        )?;
        transaction.commit()?;
    }
    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(
        "DROP TABLE IF EXISTS temp.work_journal_current_duplicate_attributions;
         CREATE TEMP TABLE work_journal_current_duplicate_attributions (
            original_id INTEGER PRIMARY KEY
         );
         INSERT INTO work_journal_current_duplicate_attributions (original_id)
         SELECT id
         FROM (
            SELECT id, ROW_NUMBER() OVER (
                PARTITION BY session_id
                ORDER BY confirmed DESC,
                    CASE source
                        WHEN 'manual' THEN 4
                        WHEN 'ai_vision' THEN 3
                        WHEN 'ai_text' THEN 2
                        ELSE 1
                    END DESC,
                    created_at DESC,
                    id DESC
            ) AS attribution_rank
            FROM project_attributions
         ) ranked
         WHERE attribution_rank > 1;

        INSERT INTO project_attributions_dedup_backup (
            original_id, session_id, project_key, project_name, obsidian_page, confidence,
            evidence, needs_review, confirmed, review_state, source, created_at,
            backup_reason, backed_up_at
        )
        SELECT attribution.id, attribution.session_id, attribution.project_key,
               attribution.project_name, attribution.obsidian_page, attribution.confidence,
               attribution.evidence, attribution.needs_review, attribution.confirmed,
               attribution.review_state, attribution.source, attribution.created_at,
               'duplicate_session_attribution', CAST(strftime('%s', 'now') AS INTEGER)
        FROM project_attributions attribution
        INNER JOIN work_journal_current_duplicate_attributions duplicate
            ON duplicate.original_id = attribution.id;

        DELETE FROM project_attributions
        WHERE id IN (
            SELECT original_id FROM work_journal_current_duplicate_attributions
        );
        DROP TABLE work_journal_current_duplicate_attributions;",
    )?;
    transaction.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_project_attributions_session_unique
         ON project_attributions (session_id)",
        [],
    )?;
    transaction.commit()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::config::{AppPrivacyRule, PrivacyConfig, PrivacyLevel};
    use crate::database::Activity;
    use crate::work_journal::attribution::parse_ai_attribution;
    use crate::work_journal::obsidian::{
        render_obsidian_daily_preview, ObsidianExportDay, ObsidianExportSession,
    };
    use crate::work_journal::privacy::{
        build_session_evidence, sanitize_activities_for_journal, REDACTED_WINDOW_TITLE,
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

    #[test]
    fn ai证据包不应携带脱敏截图或_url_参数() {
        let activities = vec![Activity {
            id: Some(1),
            timestamp: 1_700_000_000,
            app_name: "Google Chrome".to_string(),
            window_title: "[内容已脱敏]".to_string(),
            screenshot_path: "screenshots/private.jpg".to_string(),
            ocr_text: Some("token=secret-value".to_string()),
            category: "browser".to_string(),
            duration: 600,
            browser_url: Some("https://example.com/private?token=secret-value".to_string()),
            executable_path: None,
            semantic_category: None,
            semantic_confidence: None,
            screenshot_url: None,
        }];

        let packet = build_session_evidence(&activities, &PrivacyConfig::default(), true);

        assert!(packet.screenshot_paths.is_empty());
        assert!(packet.domains.is_empty());
        assert!(!packet.to_prompt_text().contains("secret-value"));
    }

    #[test]
    fn ai结构化结果应兼容_markdown_json_代码块() {
        let parsed = parse_ai_attribution(
            r#"```json
            {
              "project_key": "work-review-fork",
              "project_name": "Work Journal",
              "task_summary": "完成工作日志审阅",
              "activity_type": "编码开发",
              "confidence": 88,
              "evidence": ["window:timereview"],
              "risk_flags": [],
              "needs_review": false
            }
            ```"#,
        )
        .expect("应解析 AI JSON");

        assert_eq!(parsed.project_key, "work-review-fork");
        assert_eq!(parsed.confidence, 88);
        assert!(!parsed.needs_review);
    }

    #[test]
    fn ai证据包应限制条目数量和单项长度() {
        let activities = (0..20)
            .map(|index| Activity {
                id: Some(index),
                timestamp: 1_700_000_000 + index,
                app_name: format!("App {index}"),
                window_title: format!("Window {index} {}", "x".repeat(300)),
                screenshot_path: format!("screenshots/{index}.jpg"),
                ocr_text: Some(format!("OCR {index} {}", "y".repeat(1_000))),
                category: "work".to_string(),
                duration: 60,
                browser_url: Some(format!("https://example{index}.com/path?secret=value")),
                executable_path: None,
                semantic_category: None,
                semantic_confidence: None,
                screenshot_url: None,
            })
            .collect::<Vec<_>>();

        let packet = build_session_evidence(&activities, &PrivacyConfig::default(), true);

        assert!(packet.apps.len() <= 8);
        assert!(packet.window_titles.len() <= 8);
        assert!(packet.ocr_texts.len() <= 6);
        assert!(packet.domains.len() <= 8);
        assert!(packet.screenshot_paths.len() <= 3);
        assert!(packet
            .window_titles
            .iter()
            .all(|value| value.chars().count() <= 160));
        assert!(packet
            .ocr_texts
            .iter()
            .all(|value| value.chars().count() <= 500));
    }

    #[test]
    fn journal隐私应使用当前应用规则并脱敏普通标题() {
        let activities = vec![
            Activity {
                id: Some(1),
                timestamp: 1_700_000_000,
                app_name: "Secret App".to_string(),
                window_title: "user@example.com token=secret-value".to_string(),
                screenshot_path: "screenshots/secret.jpg".to_string(),
                ocr_text: Some("Bearer abcdefghijklmnop".to_string()),
                category: "work".to_string(),
                duration: 60,
                browser_url: Some("https://private.example.com/path".to_string()),
                executable_path: Some("/private/app".to_string()),
                semantic_category: None,
                semantic_confidence: None,
                screenshot_url: None,
            },
            Activity {
                id: Some(2),
                timestamp: 1_700_000_060,
                app_name: "Ignored App".to_string(),
                window_title: "ignored".to_string(),
                screenshot_path: "screenshots/ignored.jpg".to_string(),
                ocr_text: None,
                category: "work".to_string(),
                duration: 60,
                browser_url: None,
                executable_path: None,
                semantic_category: None,
                semantic_confidence: None,
                screenshot_url: None,
            },
            Activity {
                id: Some(3),
                timestamp: 1_700_000_120,
                app_name: "Code".to_string(),
                window_title: "user@example.com API_KEY=abcdefghijk".to_string(),
                screenshot_path: "screenshots/code.jpg".to_string(),
                ocr_text: Some("github_pat_abcdefghijk12345".to_string()),
                category: "work".to_string(),
                duration: 60,
                browser_url: Some("https://github.com/private/repo".to_string()),
                executable_path: None,
                semantic_category: None,
                semantic_confidence: None,
                screenshot_url: None,
            },
        ];
        let mut config = PrivacyConfig::default();
        config.app_rules.push(AppPrivacyRule {
            app_name: "Secret App".to_string(),
            level: PrivacyLevel::Anonymized,
        });
        config.app_rules.push(AppPrivacyRule {
            app_name: "Ignored App".to_string(),
            level: PrivacyLevel::Ignored,
        });

        let sanitized = sanitize_activities_for_journal(&activities, &config);
        assert_eq!(sanitized.len(), 2);
        assert_eq!(sanitized[0].window_title, REDACTED_WINDOW_TITLE);
        assert!(sanitized[0].browser_url.is_none());
        assert!(sanitized[0].ocr_text.is_none());
        assert!(sanitized[0].screenshot_path.is_empty());
        assert!(sanitized[1].window_title.contains("[已过滤]"));
        assert!(!sanitized[1].window_title.contains("example.com"));
        assert!(!sanitized[1]
            .ocr_text
            .as_deref()
            .unwrap_or_default()
            .contains("github_pat_"));

        let packet = build_session_evidence(&activities, &config, true);
        assert_eq!(packet.apps, vec!["Secret App", "Code"]);
        assert!(!packet.to_prompt_text().contains("secret-value"));
        assert!(!packet.to_prompt_text().contains("abcdefghijk"));
        assert!(!packet.to_prompt_text().contains("private.example.com"));
    }
}
