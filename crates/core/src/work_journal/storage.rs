use serde::{Deserialize, Serialize};

use crate::database::Activity;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredWorkSession {
    pub id: i64,
    pub date: String,
    pub started_at: i64,
    pub ended_at: i64,
    pub duration: i64,
    pub primary_app: String,
    pub activity_ids: Vec<i64>,
    pub summary: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredProjectAttribution {
    pub session_id: i64,
    pub project_key: String,
    pub project_name: String,
    pub obsidian_page: String,
    pub confidence: i32,
    pub evidence: Vec<String>,
    pub needs_review: bool,
    pub confirmed: bool,
    pub review_state: String,
    pub source: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredObsidianExport {
    pub date: String,
    pub target_path: String,
    pub content_hash: String,
    pub exported_at: i64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyActivityImportItem {
    pub source_activity_id: Option<i64>,
    pub fingerprint: String,
    pub activity: Activity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyActivityImportResult {
    pub imported_count: usize,
    pub skipped_duplicate_count: usize,
}
