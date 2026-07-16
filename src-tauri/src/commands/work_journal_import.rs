use crate::database::Activity;
use crate::error::AppError;
use crate::{AppState, PendingWorkReviewImport};
use chrono::{Local, TimeZone};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{ErrorKind, Read};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::State;
use uuid::Uuid;
use work_review_core::work_journal::storage::LegacyActivityImportItem;

const IMPORT_CONFIRMATION_TTL_SECONDS: i64 = 600;
const SOURCE_DATABASE_NAMES: &[&str] = &["workreview.db", "work_review.db"];

#[derive(Debug, Clone, Serialize)]
pub struct WorkReviewImportPreview {
    pub source_dir: String,
    pub source_db_path: String,
    pub activity_count: usize,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub screenshot_count: usize,
    pub screenshot_bytes: u64,
    pub skipped_screenshot_count: usize,
    pub confirmation_token: String,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkReviewImportInput {
    pub confirmation_token: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkReviewImportResult {
    pub import_id: String,
    pub imported_count: usize,
    pub skipped_duplicate_count: usize,
    pub copied_screenshot_count: usize,
    pub skipped_screenshot_count: usize,
    pub backup_path: String,
}

struct SourceInspection {
    source_dir: PathBuf,
    source_db_path: PathBuf,
    source_db_hash: String,
    activities: Vec<Activity>,
    date_from: Option<String>,
    date_to: Option<String>,
    screenshot_count: usize,
    screenshot_bytes: u64,
    skipped_screenshot_count: usize,
}

#[tauri::command]
pub async fn preview_work_review_import(
    source_dir: Option<String>,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<WorkReviewImportPreview, AppError> {
    let (current_data_dir, requested_source_dir) = {
        let state = state
            .lock()
            .map_err(|error| AppError::Unknown(error.to_string()))?;
        let source_dir = source_dir
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(default_work_review_data_dir);
        (state.data_dir.clone(), source_dir)
    };
    let inspection = inspect_source(&requested_source_dir, &current_data_dir)?;
    let now = Local::now().timestamp();
    let expires_at = now + IMPORT_CONFIRMATION_TTL_SECONDS;
    let confirmation_token = Uuid::new_v4().simple().to_string();

    let mut state = state
        .lock()
        .map_err(|error| AppError::Unknown(error.to_string()))?;
    state
        .pending_work_review_imports
        .retain(|_, pending| pending.expires_at > now);
    if state.pending_work_review_imports.len() >= 10 {
        if let Some(oldest_token) = state
            .pending_work_review_imports
            .iter()
            .min_by_key(|(_, pending)| pending.expires_at)
            .map(|(token, _)| token.clone())
        {
            state.pending_work_review_imports.remove(&oldest_token);
        }
    }
    state.pending_work_review_imports.insert(
        confirmation_token.clone(),
        PendingWorkReviewImport {
            source_dir: inspection.source_dir.clone(),
            source_db_path: inspection.source_db_path.clone(),
            source_db_hash: inspection.source_db_hash.clone(),
            expires_at,
        },
    );

    Ok(WorkReviewImportPreview {
        source_dir: inspection.source_dir.to_string_lossy().to_string(),
        source_db_path: inspection.source_db_path.to_string_lossy().to_string(),
        activity_count: inspection.activities.len(),
        date_from: inspection.date_from,
        date_to: inspection.date_to,
        screenshot_count: inspection.screenshot_count,
        screenshot_bytes: inspection.screenshot_bytes,
        skipped_screenshot_count: inspection.skipped_screenshot_count,
        confirmation_token,
        expires_at,
    })
}

#[tauri::command]
pub async fn import_work_review_data(
    input: WorkReviewImportInput,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<WorkReviewImportResult, AppError> {
    let (pending, data_dir, database) = {
        let mut state = state
            .lock()
            .map_err(|error| AppError::Unknown(error.to_string()))?;
        let pending = state
            .pending_work_review_imports
            .remove(&input.confirmation_token)
            .ok_or_else(|| AppError::Privacy("旧数据导入确认已失效，请重新预检".to_string()))?;
        if pending.expires_at <= Local::now().timestamp() {
            return Err(AppError::Privacy(
                "旧数据导入确认已过期，请重新预检".to_string(),
            ));
        }
        (pending, state.data_dir.clone(), state.database.clone())
    };

    let current_hash = hash_sqlite_source(&pending.source_db_path)?;
    if current_hash != pending.source_db_hash {
        return Err(AppError::Privacy(
            "旧 Work Review 数据在预检后发生变化，请重新预检".to_string(),
        ));
    }
    let inspection = inspect_source(&pending.source_dir, &data_dir)?;
    if inspection.source_db_path != pending.source_db_path
        || inspection.source_db_hash != pending.source_db_hash
    {
        return Err(AppError::Privacy(
            "旧数据来源与已确认预检不一致".to_string(),
        ));
    }

    let import_id = Uuid::new_v4().simple().to_string();
    let backup_dir = data_dir.join("import-backups");
    fs::create_dir_all(&backup_dir)?;
    let backup_path = backup_dir.join(format!(
        "workreview-before-import-{}-{}.db",
        Local::now().format("%Y%m%d-%H%M%S"),
        &import_id[..8]
    ));
    database.backup_to(&backup_path)?;

    let imported_root = data_dir.join("screenshots").join("imported");
    fs::create_dir_all(&imported_root)?;
    let temporary_dir = imported_root.join(format!(".{import_id}.tmp"));
    let final_dir = imported_root.join(&import_id);
    fs::create_dir_all(&temporary_dir)?;

    let mut items = Vec::with_capacity(inspection.activities.len());
    let mut fingerprints_seen = HashSet::new();
    let mut copied_screenshot_count = 0usize;
    let mut skipped_screenshot_count = 0usize;
    for mut activity in inspection.activities {
        let source_activity_id = activity.id;
        let fingerprint = activity_fingerprint(&activity)?;
        if !fingerprints_seen.insert(fingerprint.clone())
            || database.has_imported_activity_fingerprint(&fingerprint)?
        {
            items.push(LegacyActivityImportItem {
                source_activity_id,
                fingerprint,
                activity,
            });
            continue;
        }

        if activity.screenshot_path.trim().is_empty() {
            activity.screenshot_path.clear();
        } else {
            match resolve_source_screenshot(&inspection.source_dir, &activity.screenshot_path) {
                Ok(source_screenshot) => {
                    let extension = safe_extension(&source_screenshot);
                    let file_name = match extension {
                        Some(extension) => format!("{fingerprint}.{extension}"),
                        None => fingerprint.clone(),
                    };
                    let temporary_target = temporary_dir.join(&file_name);
                    fs::copy(&source_screenshot, &temporary_target)?;
                    activity.screenshot_path = Path::new("screenshots")
                        .join("imported")
                        .join(&import_id)
                        .join(file_name)
                        .to_string_lossy()
                        .to_string();
                    copied_screenshot_count += 1;
                }
                Err(_) => {
                    activity.screenshot_path.clear();
                    skipped_screenshot_count += 1;
                }
            }
        }
        activity.id = None;
        activity.screenshot_url = None;
        items.push(LegacyActivityImportItem {
            source_activity_id,
            fingerprint,
            activity,
        });
    }

    if let Err(error) = fs::rename(&temporary_dir, &final_dir) {
        let _ = fs::remove_dir_all(&temporary_dir);
        return Err(error.into());
    }
    let imported_at = Local::now().timestamp();
    let database_result = database.import_work_review_activities(
        &import_id,
        &inspection.source_dir.to_string_lossy(),
        &inspection.source_db_hash,
        &backup_path.to_string_lossy(),
        copied_screenshot_count,
        skipped_screenshot_count,
        imported_at,
        &items,
    );
    let database_result = match database_result {
        Ok(result) => result,
        Err(error) => {
            let _ = fs::remove_dir_all(&final_dir);
            return Err(error);
        }
    };

    Ok(WorkReviewImportResult {
        import_id,
        imported_count: database_result.imported_count,
        skipped_duplicate_count: database_result.skipped_duplicate_count,
        copied_screenshot_count,
        skipped_screenshot_count,
        backup_path: backup_path.to_string_lossy().to_string(),
    })
}

fn default_work_review_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("work-review")
}

fn inspect_source(
    source_dir: &Path,
    current_data_dir: &Path,
) -> Result<SourceInspection, AppError> {
    let canonical_source_dir = source_dir
        .canonicalize()
        .map_err(|_| AppError::Config("旧 Work Review 数据目录不存在或无法读取".to_string()))?;
    let canonical_current_data_dir = current_data_dir.canonicalize()?;
    if canonical_source_dir == canonical_current_data_dir {
        return Err(AppError::Privacy(
            "旧 Work Review 来源不能是当前 Work Journal 数据目录".to_string(),
        ));
    }
    let source_db_path = find_source_database(&canonical_source_dir)?;
    let source_hash_before = hash_sqlite_source(&source_db_path)?;
    let activities = read_source_activities(&source_db_path)?;
    let source_db_hash = hash_sqlite_source(&source_db_path)?;
    if source_hash_before != source_db_hash {
        return Err(AppError::Config(
            "旧 Work Review 数据正在变化，请暂停旧应用后重试预检".to_string(),
        ));
    }

    let mut screenshot_count = 0usize;
    let mut screenshot_bytes = 0u64;
    let mut skipped_screenshot_count = 0usize;
    for activity in &activities {
        if activity.screenshot_path.trim().is_empty() {
            continue;
        }
        match resolve_source_screenshot(&canonical_source_dir, &activity.screenshot_path) {
            Ok(path) => match fs::metadata(path) {
                Ok(metadata) => {
                    screenshot_count += 1;
                    screenshot_bytes = screenshot_bytes.saturating_add(metadata.len());
                }
                Err(_) => skipped_screenshot_count += 1,
            },
            Err(_) => skipped_screenshot_count += 1,
        }
    }

    let date_from = activities
        .iter()
        .map(|activity| activity.timestamp)
        .min()
        .and_then(timestamp_date);
    let date_to = activities
        .iter()
        .map(|activity| activity.timestamp)
        .max()
        .and_then(timestamp_date);

    Ok(SourceInspection {
        source_dir: canonical_source_dir,
        source_db_path,
        source_db_hash,
        activities,
        date_from,
        date_to,
        screenshot_count,
        screenshot_bytes,
        skipped_screenshot_count,
    })
}

fn find_source_database(source_dir: &Path) -> Result<PathBuf, AppError> {
    for name in SOURCE_DATABASE_NAMES {
        let candidate = source_dir.join(name);
        let metadata = match fs::symlink_metadata(&candidate) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(AppError::Privacy(
                "旧 Work Review 数据库必须是来源目录内的普通文件".to_string(),
            ));
        }
        let canonical = candidate.canonicalize()?;
        if !canonical.starts_with(source_dir) {
            return Err(AppError::Privacy(
                "旧 Work Review 数据库越过来源目录".to_string(),
            ));
        }
        return Ok(canonical);
    }
    Err(AppError::Config(
        "所选目录中未找到 workreview.db 或 work_review.db".to_string(),
    ))
}

fn read_source_activities(source_db_path: &Path) -> Result<Vec<Activity>, AppError> {
    let connection = Connection::open_with_flags(
        source_db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let columns = table_columns(&connection, "activities")?;
    for required in [
        "id",
        "timestamp",
        "app_name",
        "window_title",
        "screenshot_path",
        "category",
        "duration",
    ] {
        if !columns.contains(required) {
            return Err(AppError::Config(format!(
                "旧 Work Review 数据库缺少 activities.{required}"
            )));
        }
    }

    let optional = |name: &str| {
        if columns.contains(name) {
            name.to_string()
        } else {
            "NULL".to_string()
        }
    };
    let sql = format!(
        "SELECT id, timestamp, app_name, window_title, screenshot_path, {}, category, duration,
                {}, {}, {}, {}, {}
         FROM activities ORDER BY id ASC",
        optional("ocr_text"),
        optional("browser_url"),
        optional("executable_path"),
        optional("semantic_category"),
        optional("semantic_confidence"),
        optional("screenshot_url"),
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map([], |row| {
        Ok(Activity {
            id: row.get(0)?,
            timestamp: row.get(1)?,
            app_name: row.get(2)?,
            window_title: row.get(3)?,
            screenshot_path: row.get(4)?,
            ocr_text: row.get(5)?,
            category: row.get(6)?,
            duration: row.get(7)?,
            browser_url: row.get(8)?,
            executable_path: row.get(9)?,
            semantic_category: row.get(10)?,
            semantic_confidence: row.get(11)?,
            screenshot_url: row.get(12)?,
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(AppError::from)
}

fn table_columns(connection: &Connection, table_name: &str) -> Result<HashSet<String>, AppError> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table_name})"))?;
    let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
    Ok(columns.collect::<std::result::Result<HashSet<_>, _>>()?)
}

fn hash_sqlite_source(source_db_path: &Path) -> Result<String, AppError> {
    let mut hasher = Sha256::new();
    hash_file_into(source_db_path, &mut hasher)?;
    let wal_path = PathBuf::from(format!("{}-wal", source_db_path.to_string_lossy()));
    match fs::symlink_metadata(&wal_path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(AppError::Privacy(
                    "旧 Work Review WAL 必须是普通文件".to_string(),
                ));
            }
            let canonical_wal = wal_path.canonicalize()?;
            let source_parent = source_db_path
                .parent()
                .ok_or_else(|| AppError::Config("旧 Work Review 数据库缺少父目录".to_string()))?;
            if !canonical_wal.starts_with(source_parent) {
                return Err(AppError::Privacy(
                    "旧 Work Review WAL 越过来源目录".to_string(),
                ));
            }
            hash_file_into(&canonical_wal, &mut hasher)?;
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn hash_file_into(path: &Path, hasher: &mut Sha256) -> Result<(), AppError> {
    let mut file = File::open(path)?;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(())
}

fn resolve_source_screenshot(
    source_dir: &Path,
    screenshot_path: &str,
) -> Result<PathBuf, AppError> {
    let path = Path::new(screenshot_path.trim());
    if path.as_os_str().is_empty()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        }) && !path.is_absolute()
    {
        return Err(AppError::Privacy("旧截图路径无效".to_string()));
    }
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        source_dir.join(path)
    };
    let metadata = fs::symlink_metadata(&candidate)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AppError::Privacy(
            "旧截图必须是来源目录内的普通文件".to_string(),
        ));
    }
    let canonical = candidate.canonicalize()?;
    if !canonical.starts_with(source_dir) {
        return Err(AppError::Privacy("旧截图路径越过来源目录".to_string()));
    }
    Ok(canonical)
}

fn activity_fingerprint(activity: &Activity) -> Result<String, AppError> {
    let payload = serde_json::json!({
        "timestamp": activity.timestamp,
        "app_name": activity.app_name,
        "window_title": activity.window_title,
        "screenshot_path": activity.screenshot_path,
        "ocr_text": activity.ocr_text,
        "category": activity.category,
        "duration": activity.duration,
        "browser_url": activity.browser_url,
        "executable_path": activity.executable_path,
        "semantic_category": activity.semantic_category,
        "semantic_confidence": activity.semantic_confidence,
    });
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&payload)?)
    ))
}

fn safe_extension(path: &Path) -> Option<String> {
    let extension = path.extension()?.to_str()?.to_lowercase();
    if extension.len() <= 10
        && extension
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        Some(extension)
    } else {
        None
    }
}

fn timestamp_date(timestamp: i64) -> Option<String> {
    Local
        .timestamp_opt(timestamp, 0)
        .single()
        .map(|datetime| datetime.format("%Y-%m-%d").to_string())
}

#[cfg(test)]
mod tests {
    use super::{activity_fingerprint, inspect_source, resolve_source_screenshot};
    use crate::database::Activity;
    use rusqlite::Connection;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("work-journal-import-{name}-{unique}"));
        fs::create_dir_all(&path).expect("创建临时目录失败");
        path
    }

    fn create_source(source_dir: &Path) {
        let screenshots = source_dir.join("screenshots");
        fs::create_dir_all(&screenshots).expect("创建截图目录失败");
        fs::write(screenshots.join("one.jpg"), b"jpeg-data").expect("创建截图失败");
        let connection =
            Connection::open(source_dir.join("workreview.db")).expect("创建来源数据库失败");
        connection
            .execute_batch(
                "CREATE TABLE activities (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    timestamp INTEGER NOT NULL,
                    app_name TEXT NOT NULL,
                    window_title TEXT NOT NULL,
                    screenshot_path TEXT NOT NULL,
                    ocr_text TEXT,
                    category TEXT NOT NULL,
                    duration INTEGER NOT NULL
                );
                INSERT INTO activities (
                    timestamp, app_name, window_title, screenshot_path, ocr_text, category, duration
                ) VALUES (1704067200, 'Code', 'Work Journal', 'screenshots/one.jpg', 'OCR', 'work', 60);",
            )
            .expect("写入来源活动失败");
    }

    #[test]
    fn legacy_import_preview_should_read_old_schema_and_screenshot_stats() {
        let source = temp_dir("source");
        let current = temp_dir("current");
        create_source(&source);

        let inspection = inspect_source(&source, &current).expect("预检旧数据失败");

        assert_eq!(inspection.activities.len(), 1);
        assert_eq!(inspection.screenshot_count, 1);
        assert_eq!(inspection.screenshot_bytes, 9);
        assert_eq!(inspection.skipped_screenshot_count, 0);
        assert_eq!(inspection.date_from.as_deref(), Some("2024-01-01"));
        assert_eq!(inspection.date_to.as_deref(), Some("2024-01-01"));

        let _ = fs::remove_dir_all(source);
        let _ = fs::remove_dir_all(current);
    }

    #[cfg(unix)]
    #[test]
    fn legacy_import_should_reject_symlinked_or_escaping_screenshot() {
        use std::os::unix::fs::symlink;

        let source = temp_dir("source-symlink");
        let outside = temp_dir("outside-symlink").join("outside.jpg");
        fs::write(&outside, b"outside").expect("创建外部截图失败");
        symlink(&outside, source.join("linked.jpg")).expect("创建截图链接失败");

        assert!(resolve_source_screenshot(&source, "linked.jpg").is_err());
        assert!(resolve_source_screenshot(&source, "../outside.jpg").is_err());

        let outside_root = outside.parent().expect("外部目录存在").to_path_buf();
        let _ = fs::remove_dir_all(source);
        let _ = fs::remove_dir_all(outside_root);
    }

    #[test]
    fn legacy_import_fingerprint_should_ignore_source_id() {
        let mut first = Activity {
            id: Some(1),
            timestamp: 1_704_067_200,
            app_name: "Code".to_string(),
            window_title: "Work Journal".to_string(),
            screenshot_path: "screenshots/one.jpg".to_string(),
            ocr_text: Some("OCR".to_string()),
            category: "work".to_string(),
            duration: 60,
            browser_url: None,
            executable_path: None,
            semantic_category: None,
            semantic_confidence: None,
            screenshot_url: None,
        };
        let first_hash = activity_fingerprint(&first).expect("生成首个指纹失败");
        first.id = Some(999);
        let second_hash = activity_fingerprint(&first).expect("生成第二个指纹失败");

        assert_eq!(first_hash, second_hash);
    }
}
