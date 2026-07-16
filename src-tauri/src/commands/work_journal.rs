use crate::config::{AppConfig, PrivacyConfig, WorkJournalAiConfig, WorkJournalObsidianConfig};
use crate::database::Activity;
use crate::error::AppError;
use crate::{AppState, PendingWorkJournalExport};
use chrono::{Local, NaiveDate, TimeZone};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, State};
use uuid::Uuid;
use work_review_core::work_journal::attribution::{parse_ai_attribution, AiAttributionResult};
use work_review_core::work_journal::obsidian::{
    render_obsidian_daily_preview, write_obsidian_daily_log, ObsidianExportDay,
    ObsidianExportSession,
};
use work_review_core::work_journal::privacy::{
    build_session_evidence, sanitize_activities_for_journal, SessionEvidencePacket,
};
use work_review_core::work_journal::project_rules::{
    match_project, normalize_project_rules, ProjectEvidence, ProjectMatch, ProjectRule,
};
use work_review_core::work_journal::sessionize::{sessionize_activities, WorkSession};
use work_review_core::work_journal::storage::{
    StoredObsidianExport, StoredProjectAttribution, StoredWorkSession,
};

use super::ask::{generate_text_answer_with_model, generate_vision_answer_with_model};
use super::shared::{load_filtered_activities_in_range, persist_app_config};

#[derive(Debug, Clone, Serialize)]
pub struct WorkJournalProjectSummary {
    pub project_key: String,
    pub project_name: String,
    pub obsidian_page: String,
    pub duration: i64,
    pub session_count: usize,
    pub needs_review_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkJournalSession {
    pub id: i64,
    pub started_at: i64,
    pub ended_at: i64,
    pub duration: i64,
    pub primary_app: String,
    pub activity_ids: Vec<i64>,
    pub project_key: String,
    pub project_name: String,
    pub obsidian_page: String,
    pub confidence: i32,
    pub task_summary: String,
    pub evidence: Vec<String>,
    pub needs_review: bool,
    pub confirmed: bool,
    pub review_state: String,
    pub source: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkJournalReviewInput {
    pub session_id: i64,
    pub date: String,
    pub project_key: String,
    pub project_name: String,
    pub obsidian_page: String,
    pub task_summary: String,
    pub review_state: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkJournalExportResult {
    pub target_path: String,
    pub backup_path: Option<String>,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkJournalExportPreview {
    pub date: String,
    pub markdown: String,
    pub content_hash: String,
    pub confirmation_token: String,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkJournalExportInput {
    pub date: String,
    pub content_hash: String,
    pub confirmation_token: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkJournalAiRunResult {
    pub analyzed_sessions: usize,
    pub updated_sessions: usize,
    pub skipped_due_to_manual_review: usize,
    pub vision_sessions: usize,
    pub errors: Vec<String>,
    pub day: WorkJournalDay,
}

#[derive(Debug, Clone)]
struct WorkJournalAiCandidate {
    session: WorkJournalSession,
    activities: Vec<Activity>,
    evidence: SessionEvidencePacket,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkJournalDay {
    pub date: String,
    pub total_duration: i64,
    pub project_count: usize,
    pub needs_review_count: usize,
    pub interruption_count: usize,
    pub project_summaries: Vec<WorkJournalProjectSummary>,
    pub sessions: Vec<WorkJournalSession>,
}

#[tauri::command]
pub async fn get_work_journal_project_rules(
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<ProjectRule>, AppError> {
    let state = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
    Ok(state.config.work_journal_project_rules.clone())
}

#[tauri::command]
pub async fn save_work_journal_project_rules(
    rules: Vec<ProjectRule>,
    app: AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<ProjectRule>, AppError> {
    let mut normalized_rules = rules;
    normalize_project_rules(&mut normalized_rules);

    let mut next_config: AppConfig = {
        let state = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
        state.config.clone()
    };
    next_config.work_journal_project_rules = normalized_rules.clone();
    persist_app_config(next_config, app, state.inner())?;

    Ok(normalized_rules)
}

#[tauri::command]
pub async fn save_work_journal_obsidian_settings(
    settings: WorkJournalObsidianConfig,
    app: AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<WorkJournalObsidianConfig, AppError> {
    let mut next_config: AppConfig = {
        let state = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
        state.config.clone()
    };
    next_config.work_journal_obsidian = settings;
    persist_app_config(next_config, app, state.inner())?;
    let state = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
    Ok(state.config.work_journal_obsidian.clone())
}

#[tauri::command]
pub async fn save_work_journal_ai_settings(
    settings: WorkJournalAiConfig,
    app: AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<WorkJournalAiConfig, AppError> {
    let mut next_config: AppConfig = {
        let state = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
        state.config.clone()
    };
    next_config.work_journal_ai = settings;
    persist_app_config(next_config, app, state.inner())?;
    let state = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
    Ok(state.config.work_journal_ai.clone())
}

#[tauri::command]
pub async fn match_work_journal_project(
    evidence: ProjectEvidence,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<Option<ProjectMatch>, AppError> {
    let state = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
    Ok(match_project(
        &evidence,
        &state.config.work_journal_project_rules,
    ))
}

#[tauri::command]
pub async fn get_work_journal_day(
    date: String,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<WorkJournalDay, AppError> {
    let state = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
    build_work_journal_day(&date, &state)
}

#[tauri::command]
pub async fn preview_work_journal_obsidian_export(
    date: String,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<WorkJournalExportPreview, AppError> {
    let mut state = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
    let day = build_work_journal_day(&date, &state)?;
    let markdown = render_obsidian_daily_preview(&obsidian_export_day(&day));
    let content_hash = hash_markdown(&markdown);
    let confirmation_token = Uuid::new_v4().simple().to_string();
    let expires_at = Local::now().timestamp() + 600;
    prune_pending_exports(
        &mut state.pending_work_journal_exports,
        Local::now().timestamp(),
    );
    if state.pending_work_journal_exports.len() >= 10 {
        if let Some(oldest_token) = state
            .pending_work_journal_exports
            .iter()
            .min_by_key(|(_, pending)| pending.expires_at)
            .map(|(token, _)| token.clone())
        {
            state.pending_work_journal_exports.remove(&oldest_token);
        }
    }
    let obsidian_settings = state.config.work_journal_obsidian.clone();
    state.pending_work_journal_exports.insert(
        confirmation_token.clone(),
        PendingWorkJournalExport {
            date: date.clone(),
            content_hash: content_hash.clone(),
            vault_path: obsidian_settings.vault_path,
            daily_folder: obsidian_settings.daily_folder,
            conflict_behavior: obsidian_settings.conflict_behavior,
            expires_at,
        },
    );
    Ok(WorkJournalExportPreview {
        date,
        markdown,
        content_hash,
        confirmation_token,
        expires_at,
    })
}

#[tauri::command]
pub async fn export_work_journal_obsidian(
    input: WorkJournalExportInput,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<WorkJournalExportResult, AppError> {
    let mut state = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
    let pending = take_pending_export(
        &mut state.pending_work_journal_exports,
        &input.confirmation_token,
        Local::now().timestamp(),
    )?;
    let settings = &state.config.work_journal_obsidian;
    if settings.export_mode != "daily_log" {
        return Err(AppError::Unknown(
            "Obsidian 导出尚未启用，请先在工作日志设置中开启".to_string(),
        ));
    }
    if settings.vault_path.trim().is_empty() {
        return Err(AppError::Unknown(
            "请先配置 Obsidian vault 路径".to_string(),
        ));
    }
    validate_pending_export(&pending, &input, settings)?;

    let day = build_work_journal_day(&input.date, &state)?;
    let markdown = render_obsidian_daily_preview(&obsidian_export_day(&day));
    let content_hash = hash_markdown(&markdown);
    if content_hash != input.content_hash {
        return Err(AppError::Privacy(
            "工作日志内容已变化，请重新生成预览后再导出".to_string(),
        ));
    }
    let result = write_obsidian_daily_log(
        std::path::Path::new(&settings.vault_path),
        &settings.daily_folder,
        &input.date,
        &markdown,
        &settings.conflict_behavior,
    )?;
    let target_path = result.target_path.to_string_lossy().to_string();
    state
        .database
        .record_work_journal_obsidian_export(&StoredObsidianExport {
            date: input.date,
            target_path: target_path.clone(),
            content_hash: content_hash.clone(),
            exported_at: Local::now().timestamp(),
            status: "success".to_string(),
        })?;

    Ok(WorkJournalExportResult {
        target_path,
        backup_path: result
            .backup_path
            .map(|path| path.to_string_lossy().to_string()),
        content_hash,
    })
}

fn hash_markdown(markdown: &str) -> String {
    format!("{:x}", Sha256::digest(markdown.as_bytes()))
}

fn prune_pending_exports(
    pending_exports: &mut HashMap<String, PendingWorkJournalExport>,
    now: i64,
) {
    pending_exports.retain(|_, pending| pending.expires_at > now);
}

fn take_pending_export(
    pending_exports: &mut HashMap<String, PendingWorkJournalExport>,
    token: &str,
    now: i64,
) -> Result<PendingWorkJournalExport, AppError> {
    let pending = pending_exports
        .remove(token)
        .ok_or_else(|| AppError::Privacy("导出确认已失效，请重新生成预览".to_string()))?;
    if pending.expires_at <= now {
        return Err(AppError::Privacy(
            "导出确认已过期，请重新生成预览".to_string(),
        ));
    }
    Ok(pending)
}

fn validate_pending_export(
    pending: &PendingWorkJournalExport,
    input: &WorkJournalExportInput,
    settings: &WorkJournalObsidianConfig,
) -> Result<(), AppError> {
    if pending.date != input.date || pending.content_hash != input.content_hash {
        return Err(AppError::Privacy("导出内容与已确认预览不一致".to_string()));
    }
    if pending.vault_path != settings.vault_path
        || pending.daily_folder != settings.daily_folder
        || pending.conflict_behavior != settings.conflict_behavior
    {
        return Err(AppError::Privacy(
            "Obsidian 导出设置已变化，请重新生成预览".to_string(),
        ));
    }
    Ok(())
}

#[tauri::command]
pub async fn review_work_journal_session(
    input: WorkJournalReviewInput,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<WorkJournalDay, AppError> {
    let state = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
    journal_query_bounds(&input.date)?;
    let review_state = normalize_review_state(&input.review_state)?;
    state
        .database
        .get_work_journal_session(input.session_id)?
        .ok_or_else(|| AppError::Unknown("工作块不存在或尚未生成".to_string()))?;
    let existing = state
        .database
        .get_work_journal_project_attribution(input.session_id)?;

    let (project_key, project_name, obsidian_page, evidence) = match review_state.as_str() {
        "private" => (
            "private".to_string(),
            "私密工作".to_string(),
            String::new(),
            Vec::new(),
        ),
        "excluded" => (
            "excluded".to_string(),
            "已排除".to_string(),
            String::new(),
            Vec::new(),
        ),
        _ => {
            let project_key = input.project_key.trim().to_string();
            let project_name = input.project_name.trim().to_string();
            if project_key.is_empty() || project_name.is_empty() {
                return Err(AppError::Unknown("请选择有效项目".to_string()));
            }
            (
                project_key,
                project_name,
                input.obsidian_page.trim().to_string(),
                existing
                    .as_ref()
                    .map(|attribution| attribution.evidence.clone())
                    .unwrap_or_default(),
            )
        }
    };

    let privacy_filter = crate::privacy::PrivacyFilter::from_config(&state.config.privacy);
    let task_summary = privacy_filter.filter_text(input.task_summary.trim());
    if !task_summary.is_empty() {
        state
            .database
            .update_work_journal_session_summary(input.session_id, &task_summary)?;
    }
    state
        .database
        .save_reviewed_project_attribution(&StoredProjectAttribution {
            session_id: input.session_id,
            project_key,
            project_name,
            obsidian_page,
            confidence: 100,
            evidence,
            needs_review: false,
            confirmed: true,
            review_state,
            source: "manual".to_string(),
            created_at: Local::now().timestamp(),
        })?;

    build_work_journal_day(&input.date, &state)
}

#[tauri::command]
pub async fn analyze_work_journal_with_ai(
    date: String,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<WorkJournalAiRunResult, AppError> {
    let (ai_config, text_model, vision_model, privacy_config, rules, database, candidates) = {
        let state_guard = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
        journal_query_bounds(&date)?;
        if !state_guard.config.work_journal_ai.enabled {
            return Err(AppError::Analysis(
                "Work Journal AI 未启用，请先在设置中显式开启".to_string(),
            ));
        }
        if state_guard.config.text_model.model.trim().is_empty() {
            return Err(AppError::Analysis(
                "文本模型未配置，无法执行工作日志归因".to_string(),
            ));
        }

        let day = build_work_journal_day(&date, &state_guard)?;
        let (date_from, date_to) = journal_query_bounds(&date)?;
        let activities = load_filtered_activities_in_range(
            &state_guard,
            Some(&date_from),
            Some(&date_to),
            10_000,
        )?;
        let max_sessions = state_guard.config.work_journal_ai.max_sessions_per_run;
        let candidates = day
            .sessions
            .into_iter()
            .filter(|session| {
                session.review_state == "included" && session.needs_review && !session.confirmed
            })
            .take(max_sessions)
            .map(|session| {
                let session_activities = activities
                    .iter()
                    .filter(|activity| {
                        activity
                            .id
                            .is_some_and(|id| session.activity_ids.contains(&id))
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                let evidence =
                    build_session_evidence(&session_activities, &state_guard.config.privacy, false);
                WorkJournalAiCandidate {
                    session,
                    activities: session_activities,
                    evidence,
                }
            })
            .collect::<Vec<_>>();

        (
            state_guard.config.work_journal_ai.clone(),
            state_guard.config.text_model.clone(),
            state_guard.config.vision_model.clone(),
            state_guard.config.privacy.clone(),
            state_guard.config.work_journal_project_rules.clone(),
            state_guard.database.clone(),
            candidates,
        )
    };

    let analyzed_sessions = candidates.len();
    let mut updated_sessions = 0;
    let mut skipped_due_to_manual_review = 0;
    let mut vision_sessions = 0;
    let mut errors = Vec::new();
    let system_prompt = work_journal_ai_system_prompt();

    for candidate in candidates {
        let prompt = work_journal_ai_prompt(&candidate, &rules, None)?;
        let text_response =
            match generate_text_answer_with_model(&text_model, system_prompt, &prompt).await {
                Ok(response) => response,
                Err(error) => {
                    errors.push(format!("session {}: {error}", candidate.session.id));
                    continue;
                }
            };
        let mut attribution = match parse_ai_attribution(&text_response) {
            Ok(attribution) => attribution,
            Err(error) => {
                errors.push(format!(
                    "session {}: AI 返回格式无效 ({error})",
                    candidate.session.id
                ));
                continue;
            }
        };
        let mut source = "ai_text";

        if should_use_vision(
            ai_config.vision_enabled,
            attribution.confidence,
            attribution.needs_review,
            i32::from(ai_config.confidence_threshold),
        ) {
            let screenshot_path =
                build_session_evidence(&candidate.activities, &privacy_config, true)
                    .screenshot_paths
                    .into_iter()
                    .next();
            if let Some(screenshot_path) = screenshot_path {
                let thumbnail = {
                    let state_guard = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
                    resolve_journal_screenshot_path(&state_guard.data_dir, &screenshot_path)
                        .and_then(|path| {
                            state_guard
                                .screenshot_service
                                .generate_thumbnail_base64(&path, 960)
                        })
                };
                match thumbnail {
                    Ok(image_base64) => {
                        let vision_prompt =
                            work_journal_ai_prompt(&candidate, &rules, Some(&attribution))?;
                        match generate_vision_answer_with_model(
                            &vision_model,
                            system_prompt,
                            &vision_prompt,
                            &image_base64,
                        )
                        .await
                        {
                            Ok(response) => match parse_ai_attribution(&response) {
                                Ok(vision_attribution) => {
                                    attribution = vision_attribution;
                                    source = "ai_vision";
                                    vision_sessions += 1;
                                }
                                Err(error) => errors.push(format!(
                                    "session {}: 视觉模型返回格式无效 ({error})",
                                    candidate.session.id
                                )),
                            },
                            Err(error) => errors.push(format!(
                                "session {}: 视觉归因失败 ({error})",
                                candidate.session.id
                            )),
                        }
                    }
                    Err(error) => errors.push(format!(
                        "session {}: 无法读取代表截图 ({error})",
                        candidate.session.id
                    )),
                }
            }
        }

        if persist_ai_attribution(
            &database,
            &candidate,
            &attribution,
            &rules,
            &privacy_config,
            i32::from(ai_config.confidence_threshold),
            source,
        )? {
            updated_sessions += 1;
        } else {
            skipped_due_to_manual_review += 1;
        }
    }

    let day = {
        let state_guard = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
        build_work_journal_day(&date, &state_guard)?
    };

    Ok(WorkJournalAiRunResult {
        analyzed_sessions,
        updated_sessions,
        skipped_due_to_manual_review,
        vision_sessions,
        errors,
        day,
    })
}

fn work_journal_ai_system_prompt() -> &'static str {
    "你是本地个人工作日志的项目归因助手。只能依据提供的脱敏证据，从允许的项目中选择；证据不足时选择 unassigned。必须只输出一个 JSON 对象，不要输出解释、Markdown 或额外字段。"
}

fn work_journal_ai_prompt(
    candidate: &WorkJournalAiCandidate,
    rules: &[ProjectRule],
    previous: Option<&AiAttributionResult>,
) -> Result<String, AppError> {
    let allowed_projects = rules
        .iter()
        .map(|rule| {
            serde_json::json!({
                "project_key": rule.project_key,
                "project_name": rule.project_name,
            })
        })
        .chain(std::iter::once(serde_json::json!({
            "project_key": "unassigned",
            "project_name": "待确认",
        })))
        .collect::<Vec<_>>();
    let previous = previous
        .map(serde_json::to_string)
        .transpose()?
        .unwrap_or_else(|| "无".to_string());

    Ok(format!(
        "允许的项目:\n{}\n\n工作块时长: {} 秒\n脱敏证据:\n{}\n\n上一轮文本判断: {}\n\n请输出以下 JSON 字段：project_key、project_name、task_summary、activity_type、confidence（0-100）、evidence（字符串数组）、risk_flags（字符串数组）、needs_review（布尔值）。task_summary 应具体但不得补造事实；看不清或证据冲突时降低 confidence 并将 needs_review 设为 true。",
        serde_json::to_string(&allowed_projects)?,
        candidate.session.duration,
        candidate.evidence.to_prompt_text(),
        previous,
    ))
}

fn persist_ai_attribution(
    database: &crate::database::Database,
    candidate: &WorkJournalAiCandidate,
    attribution: &AiAttributionResult,
    rules: &[ProjectRule],
    privacy_config: &PrivacyConfig,
    confidence_threshold: i32,
    source: &str,
) -> Result<bool, AppError> {
    let canonical_rule = rules
        .iter()
        .find(|rule| rule.project_key == attribution.project_key);
    let recognized_project = canonical_rule.is_some();
    let (project_key, project_name, obsidian_page) = match canonical_rule {
        Some(rule) => (
            rule.project_key.clone(),
            rule.project_name.clone(),
            rule.obsidian_page.clone(),
        ),
        None => (
            "unassigned".to_string(),
            "待确认".to_string(),
            String::new(),
        ),
    };
    let confidence = attribution.confidence.clamp(0, 100);
    let needs_review =
        attribution.needs_review || confidence < confidence_threshold || !recognized_project;
    let privacy_filter = crate::privacy::PrivacyFilter::from_config(privacy_config);
    let task_summary = privacy_filter.filter_text(attribution.task_summary.trim());
    database.save_ai_project_attribution_if_unreviewed(
        &StoredProjectAttribution {
            session_id: candidate.session.id,
            project_key,
            project_name,
            obsidian_page,
            confidence,
            evidence: safe_ai_evidence(&candidate.evidence, attribution, &privacy_filter),
            needs_review,
            confirmed: false,
            review_state: "included".to_string(),
            source: source.to_string(),
            created_at: Local::now().timestamp(),
        },
        &trim_evidence(&task_summary, 160),
    )
}

fn safe_ai_evidence(
    packet: &SessionEvidencePacket,
    attribution: &AiAttributionResult,
    privacy_filter: &crate::privacy::PrivacyFilter,
) -> Vec<String> {
    let mut evidence = Vec::new();
    evidence.extend(packet.apps.iter().map(|value| format!("app:{value}")));
    evidence.extend(packet.domains.iter().map(|value| format!("domain:{value}")));
    evidence.extend(packet.window_titles.iter().take(2).map(|value| {
        format!(
            "window:{}",
            trim_evidence(&privacy_filter.filter_text(value), 80)
        )
    }));
    if !attribution.activity_type.trim().is_empty() {
        evidence.push(format!(
            "type:{}",
            trim_evidence(
                &privacy_filter.filter_text(attribution.activity_type.trim()),
                40,
            )
        ));
    }
    evidence.extend(attribution.risk_flags.iter().take(2).map(|value| {
        format!(
            "risk:{}",
            trim_evidence(&privacy_filter.filter_text(value.trim()), 60)
        )
    }));
    evidence.retain(|value| !value.trim().is_empty());
    evidence.sort();
    evidence.dedup();
    evidence.truncate(8);
    evidence
}

fn resolve_journal_screenshot_path(
    data_dir: &Path,
    relative_path: &str,
) -> Result<PathBuf, AppError> {
    let relative = Path::new(relative_path);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::RootDir))
    {
        return Err(AppError::Privacy("截图路径越过数据目录".to_string()));
    }
    let data_root = data_dir.canonicalize()?;
    let screenshot_path = data_dir.join(relative).canonicalize()?;
    if !screenshot_path.starts_with(&data_root) {
        return Err(AppError::Privacy("截图路径越过数据目录".to_string()));
    }
    Ok(screenshot_path)
}

fn build_work_journal_day(date: &str, state: &AppState) -> Result<WorkJournalDay, AppError> {
    let (date_from, date_to) = journal_query_bounds(date)?;
    let activities =
        load_filtered_activities_in_range(state, Some(&date_from), Some(&date_to), 10_000)?;
    let activities = sanitize_activities_for_journal(&activities, &state.config.privacy);
    let sessions = sessionize_activities(&activities);
    let journal_sessions = build_journal_sessions(
        date,
        &sessions,
        &activities,
        &state.config.work_journal_project_rules,
        &state.config.privacy,
        &state.database,
    )?;
    let project_summaries = summarize_projects(&journal_sessions);
    let total_duration = journal_sessions
        .iter()
        .filter(|session| session.review_state != "excluded")
        .map(|session| session.duration)
        .sum();
    let needs_review_count = journal_sessions
        .iter()
        .filter(|session| session.review_state != "excluded" && session.needs_review)
        .count();
    let interruption_count = journal_sessions
        .iter()
        .filter(|session| {
            session.review_state != "excluded" && is_communication_app(&session.primary_app)
        })
        .count();
    let project_count = project_summaries
        .iter()
        .filter(|project| project.project_key != "unassigned")
        .count();

    Ok(WorkJournalDay {
        date: date.to_string(),
        total_duration,
        project_count,
        needs_review_count,
        interruption_count,
        project_summaries,
        sessions: journal_sessions,
    })
}

fn build_journal_sessions(
    date: &str,
    sessions: &[WorkSession],
    activities: &[Activity],
    rules: &[ProjectRule],
    privacy_config: &PrivacyConfig,
    database: &crate::database::Database,
) -> Result<Vec<WorkJournalSession>, AppError> {
    sessions
        .iter()
        .enumerate()
        .map(|(index, session)| {
            let session_activities = activities_for_session(session, activities);
            let project_match = best_project_match(&session_activities, rules);
            let needs_review = project_match
                .as_ref()
                .map(|matched| matched.score < 80)
                .unwrap_or(true);
            let (project_key, project_name, obsidian_page, confidence, evidence) =
                project_match_to_fields(project_match, &session_activities, privacy_config);
            let session_id = session
                .activity_ids
                .first()
                .copied()
                .unwrap_or(-((index as i64) + 1));
            let generated_summary = task_summary(session, &session_activities, privacy_config);
            database.upsert_work_journal_session(&StoredWorkSession {
                id: session_id,
                date: date.to_string(),
                started_at: session.started_at,
                ended_at: session.ended_at,
                duration: session.duration,
                primary_app: session.primary_app.clone(),
                activity_ids: session.activity_ids.clone(),
                summary: generated_summary,
                created_at: Local::now().timestamp(),
            })?;
            database.save_generated_project_attribution(&StoredProjectAttribution {
                session_id,
                project_key,
                project_name,
                obsidian_page,
                confidence,
                evidence,
                needs_review,
                confirmed: false,
                review_state: "included".to_string(),
                source: "rule".to_string(),
                created_at: Local::now().timestamp(),
            })?;

            let stored_session = database
                .get_work_journal_session(session_id)?
                .ok_or_else(|| AppError::Unknown("保存工作块后无法读取".to_string()))?;
            let attribution = database
                .get_work_journal_project_attribution(session_id)?
                .ok_or_else(|| AppError::Unknown("保存项目归因后无法读取".to_string()))?;
            let (task_summary, evidence) = match attribution.review_state.as_str() {
                "private" => ("私密工作".to_string(), Vec::new()),
                "excluded" => ("已排除".to_string(), Vec::new()),
                _ => (stored_session.summary, attribution.evidence.clone()),
            };

            Ok(WorkJournalSession {
                id: session_id,
                started_at: session.started_at,
                ended_at: session.ended_at,
                duration: session.duration,
                primary_app: session.primary_app.clone(),
                activity_ids: session.activity_ids.clone(),
                project_key: attribution.project_key,
                project_name: attribution.project_name,
                obsidian_page: attribution.obsidian_page,
                confidence: attribution.confidence,
                task_summary,
                evidence,
                needs_review: attribution.needs_review,
                confirmed: attribution.confirmed,
                review_state: attribution.review_state,
                source: attribution.source,
            })
        })
        .collect()
}

fn activities_for_session(session: &WorkSession, activities: &[Activity]) -> Vec<Activity> {
    let ids: HashSet<i64> = session.activity_ids.iter().copied().collect();
    activities
        .iter()
        .filter(|activity| activity.id.is_some_and(|id| ids.contains(&id)))
        .cloned()
        .collect()
}

fn best_project_match(activities: &[Activity], rules: &[ProjectRule]) -> Option<ProjectMatch> {
    activities
        .iter()
        .filter_map(|activity| match_project(&project_evidence(activity), rules))
        .max_by(|left, right| {
            left.score
                .cmp(&right.score)
                .then_with(|| left.project_key.cmp(&right.project_key))
        })
}

fn project_evidence(activity: &Activity) -> ProjectEvidence {
    ProjectEvidence {
        app_name: activity.app_name.clone(),
        window_title: activity.window_title.clone(),
        browser_url: activity.browser_url.clone(),
        executable_path: activity.executable_path.clone(),
        ocr_text: activity.ocr_text.clone(),
        category: Some(activity.category.clone()),
        semantic_category: activity.semantic_category.clone(),
    }
}

fn project_match_to_fields(
    project_match: Option<ProjectMatch>,
    activities: &[Activity],
    privacy_config: &PrivacyConfig,
) -> (String, String, String, i32, Vec<String>) {
    if let Some(project_match) = project_match {
        let confidence = project_match.score.clamp(0, 100);
        let mut evidence = project_match.evidence;
        evidence.extend(activity_evidence(activities, privacy_config));
        evidence.sort();
        evidence.dedup();
        evidence.truncate(8);
        return (
            project_match.project_key,
            project_match.project_name,
            project_match.obsidian_page,
            confidence,
            evidence,
        );
    }

    (
        "unassigned".to_string(),
        "待确认".to_string(),
        String::new(),
        0,
        activity_evidence(activities, privacy_config),
    )
}

fn activity_evidence(activities: &[Activity], privacy_config: &PrivacyConfig) -> Vec<String> {
    let privacy_filter = crate::privacy::PrivacyFilter::from_config(privacy_config);
    let mut evidence = Vec::new();
    for activity in activities {
        if !activity.app_name.trim().is_empty() {
            evidence.push(format!("app:{}", activity.app_name.trim()));
        }
        if let Some(url) = activity.browser_url.as_deref() {
            let domain = PrivacyConfig::extract_domain(url);
            if !domain.is_empty() {
                evidence.push(format!("domain:{domain}"));
            }
        }
        if !activity.window_title.trim().is_empty() {
            let title = privacy_filter.filter_text(activity.window_title.trim());
            evidence.push(format!("window:{}", trim_evidence(&title, 80)));
        }
    }
    evidence.sort();
    evidence.dedup();
    evidence.truncate(8);
    evidence
}

fn task_summary(
    session: &WorkSession,
    activities: &[Activity],
    privacy_config: &PrivacyConfig,
) -> String {
    let privacy_filter = crate::privacy::PrivacyFilter::from_config(privacy_config);
    activities
        .iter()
        .find(|activity| {
            activity.app_name == session.primary_app && !activity.window_title.is_empty()
        })
        .or_else(|| {
            activities
                .iter()
                .find(|activity| !activity.window_title.is_empty())
        })
        .map(|activity| trim_evidence(&privacy_filter.filter_text(&activity.window_title), 60))
        .unwrap_or_else(|| session.primary_app.clone())
}

fn summarize_projects(sessions: &[WorkJournalSession]) -> Vec<WorkJournalProjectSummary> {
    let mut summaries: HashMap<String, WorkJournalProjectSummary> = HashMap::new();
    for session in sessions
        .iter()
        .filter(|session| session.review_state != "excluded")
    {
        let entry = summaries
            .entry(session.project_key.clone())
            .or_insert_with(|| WorkJournalProjectSummary {
                project_key: session.project_key.clone(),
                project_name: session.project_name.clone(),
                obsidian_page: session.obsidian_page.clone(),
                duration: 0,
                session_count: 0,
                needs_review_count: 0,
            });
        entry.duration += session.duration;
        entry.session_count += 1;
        if session.needs_review {
            entry.needs_review_count += 1;
        }
    }

    let mut summaries: Vec<_> = summaries.into_values().collect();
    summaries.sort_by(|left, right| {
        right
            .duration
            .cmp(&left.duration)
            .then_with(|| left.project_name.cmp(&right.project_name))
    });
    summaries
}

fn obsidian_export_day(day: &WorkJournalDay) -> ObsidianExportDay {
    ObsidianExportDay {
        date: day.date.clone(),
        total_duration: day.total_duration,
        project_count: day.project_count,
        sessions: day
            .sessions
            .iter()
            .filter(|session| session.review_state != "excluded")
            .map(|session| ObsidianExportSession {
                started_at: format_time(session.started_at),
                ended_at: format_time(session.ended_at),
                duration: session.duration,
                project_name: session.project_name.clone(),
                obsidian_page: session.obsidian_page.clone(),
                task_summary: session.task_summary.clone(),
                evidence: session.evidence.clone(),
                needs_review: session.needs_review,
            })
            .collect(),
    }
}

fn journal_query_bounds(date: &str) -> Result<(String, String), AppError> {
    let date = NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|_| AppError::Unknown(format!("无效日期: {date}")))?;
    let date = date.format("%Y-%m-%d").to_string();
    Ok((date.clone(), date))
}

fn normalize_review_state(review_state: &str) -> Result<String, AppError> {
    match review_state.trim().to_lowercase().as_str() {
        "included" => Ok("included".to_string()),
        "private" => Ok("private".to_string()),
        "excluded" => Ok("excluded".to_string()),
        _ => Err(AppError::Unknown("无效的工作块审阅状态".to_string())),
    }
}

fn format_time(timestamp: i64) -> String {
    Local
        .timestamp_opt(timestamp, 0)
        .single()
        .map(|datetime| datetime.format("%H:%M").to_string())
        .unwrap_or_else(|| "--:--".to_string())
}

fn trim_evidence(value: &str, limit: usize) -> String {
    let value = value.trim();
    let mut chars = value.chars();
    let trimmed: String = chars.by_ref().take(limit).collect();
    if chars.next().is_some() {
        format!("{trimmed}...")
    } else {
        trimmed
    }
}

fn is_communication_app(app_name: &str) -> bool {
    let app_name = app_name.to_lowercase();
    [
        "wecom",
        "wechat",
        "微信",
        "企业微信",
        "teams",
        "slack",
        "dingtalk",
    ]
    .iter()
    .any(|keyword| app_name.contains(keyword))
}

fn should_use_vision(
    vision_enabled: bool,
    confidence: i32,
    needs_review: bool,
    confidence_threshold: i32,
) -> bool {
    vision_enabled && (needs_review || confidence < confidence_threshold)
}

#[cfg(test)]
mod tests {
    use super::{
        activity_evidence, journal_query_bounds, should_use_vision, take_pending_export,
        validate_pending_export, work_journal_ai_prompt, WorkJournalAiCandidate,
        WorkJournalExportInput, WorkJournalSession,
    };
    use crate::commands::ask::{
        generate_text_answer_with_model, generate_vision_answer_with_model,
    };
    use crate::config::{AiProvider, ModelConfig, WorkJournalObsidianConfig};
    use crate::database::Activity;
    use crate::PendingWorkJournalExport;
    use std::collections::HashMap;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use work_review_core::config::PrivacyConfig;
    use work_review_core::work_journal::privacy::build_session_evidence;

    fn activity_with_url(url: &str) -> Activity {
        Activity {
            id: Some(1),
            timestamp: 1_700_000_600,
            app_name: "Google Chrome".to_string(),
            window_title: "Work Journal dashboard".to_string(),
            screenshot_path: String::new(),
            ocr_text: None,
            category: "browser".to_string(),
            duration: 600,
            browser_url: Some(url.to_string()),
            executable_path: None,
            semantic_category: None,
            semantic_confidence: None,
            screenshot_url: None,
        }
    }

    async fn openai_mock_server() -> (String, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("应启动本地模拟端点");
        let address = listener.local_addr().expect("应读取模拟端点地址");
        let handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("应收到模型请求");
            let mut request = Vec::new();
            let (body_start, content_length) = loop {
                let mut chunk = [0_u8; 4096];
                let read = stream.read(&mut chunk).await.expect("应读取模型请求");
                assert!(read > 0, "模型请求不应在请求体前结束");
                request.extend_from_slice(&chunk[..read]);

                if let Some(header_end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                    let header_text = String::from_utf8_lossy(&request[..header_end]);
                    let content_length = header_text
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().ok())
                                .flatten()
                        })
                        .expect("请求应包含 Content-Length");
                    break (header_end + 4, content_length);
                }
            };

            while request.len() < body_start + content_length {
                let mut chunk = [0_u8; 4096];
                let read = stream.read(&mut chunk).await.expect("应读取完整请求体");
                assert!(read > 0, "模型请求体不应提前结束");
                request.extend_from_slice(&chunk[..read]);
            }

            let response_body = r#"{"choices":[{"message":{"content":"ok"}}]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            stream
                .write_all(response.as_bytes())
                .await
                .expect("应返回模拟模型响应");

            String::from_utf8(request[body_start..body_start + content_length].to_vec())
                .expect("模型请求体应为 UTF-8 JSON")
        });

        (format!("http://{address}"), handle)
    }

    fn sensitive_ai_candidate() -> WorkJournalAiCandidate {
        let activity = Activity {
            id: Some(7),
            timestamp: 1_700_000_600,
            app_name: "Google Chrome".to_string(),
            window_title: "alice@example.com token: super-secret-token Bearer raw-bearer-value"
                .to_string(),
            screenshot_path: "screenshots/private-capture.jpg".to_string(),
            ocr_text: Some("api_key=raw-api-key sk-abcdefghijk12345 13912345678".to_string()),
            category: "browser".to_string(),
            duration: 600,
            browser_url: Some("https://example.com/private/path?token=raw-query-token".to_string()),
            executable_path: None,
            semantic_category: None,
            semantic_confidence: None,
            screenshot_url: None,
        };
        let activities = vec![activity];
        let evidence = build_session_evidence(&activities, &PrivacyConfig::default(), false);

        WorkJournalAiCandidate {
            session: WorkJournalSession {
                id: 7,
                started_at: 1_700_000_600,
                ended_at: 1_700_001_200,
                duration: 600,
                primary_app: "Google Chrome".to_string(),
                activity_ids: vec![7],
                project_key: "unassigned".to_string(),
                project_name: "待确认".to_string(),
                obsidian_page: String::new(),
                confidence: 0,
                task_summary: String::new(),
                evidence: Vec::new(),
                needs_review: true,
                confirmed: false,
                review_state: "included".to_string(),
                source: "rule".to_string(),
            },
            activities,
            evidence,
        }
    }

    #[test]
    fn journal_query_bounds_should_stay_on_requested_date() {
        let (date_from, date_to) = journal_query_bounds("2026-07-10").expect("日期应有效");

        assert_eq!(date_from, "2026-07-10");
        assert_eq!(date_to, "2026-07-10");
    }

    #[test]
    fn journal_evidence_should_not_expose_url_path_or_query() {
        let evidence = activity_evidence(
            &[activity_with_url(
                "https://example.com/private/path?token=secret-value",
            )],
            &PrivacyConfig::default(),
        );
        let combined = evidence.join("\n");

        assert!(combined.contains("domain:example.com"));
        assert!(!combined.contains("private/path"));
        assert!(!combined.contains("secret-value"));
    }

    #[test]
    fn vision_should_only_run_for_low_confidence_when_explicitly_enabled() {
        assert!(should_use_vision(true, 79, true, 80));
        assert!(should_use_vision(true, 90, true, 80));
        assert!(!should_use_vision(false, 79, true, 80));
        assert!(should_use_vision(true, 79, false, 80));
        assert!(!should_use_vision(true, 80, false, 80));
    }

    #[tokio::test]
    async fn ai_requests_should_only_contain_sanitized_evidence() {
        let candidate = sensitive_ai_candidate();
        let prompt = work_journal_ai_prompt(&candidate, &[], None).expect("应生成归因提示词");
        let forbidden = [
            "alice@example.com",
            "super-secret-token",
            "raw-bearer-value",
            "raw-api-key",
            "sk-abcdefghijk12345",
            "13912345678",
            "private/path",
            "raw-query-token",
            "screenshots/private-capture.jpg",
        ];
        for value in forbidden {
            assert!(!prompt.contains(value), "提示词泄露敏感值: {value}");
        }
        assert!(prompt.contains("[已过滤]"));
        assert!(prompt.contains("example.com"));

        let (text_endpoint, text_request) = openai_mock_server().await;
        let text_model = ModelConfig {
            provider: AiProvider::OpenAI,
            endpoint: text_endpoint,
            api_key: None,
            model: "mock-text".to_string(),
        };
        let text_answer = generate_text_answer_with_model(
            &text_model,
            super::work_journal_ai_system_prompt(),
            &prompt,
        )
        .await
        .expect("文本模型请求应成功");
        assert_eq!(text_answer, "ok");
        let text_body = text_request.await.expect("应捕获文本模型请求");

        let test_image = "dGVzdC1pbWFnZS1vbmx5";
        let (vision_endpoint, vision_request) = openai_mock_server().await;
        let vision_model = ModelConfig {
            provider: AiProvider::OpenAI,
            endpoint: vision_endpoint,
            api_key: None,
            model: "mock-vision".to_string(),
        };
        let vision_answer = generate_vision_answer_with_model(
            &vision_model,
            super::work_journal_ai_system_prompt(),
            &prompt,
            test_image,
        )
        .await
        .expect("视觉模型请求应成功");
        assert_eq!(vision_answer, "ok");
        let vision_body = vision_request.await.expect("应捕获视觉模型请求");

        for body in [&text_body, &vision_body] {
            for value in forbidden {
                assert!(!body.contains(value), "模型请求泄露敏感值: {value}");
            }
            assert!(body.contains("[已过滤]"));
        }
        assert!(!text_body.contains(test_image));
        assert!(vision_body.contains(test_image));
    }

    fn pending_export(expires_at: i64) -> PendingWorkJournalExport {
        PendingWorkJournalExport {
            date: "2026-07-10".to_string(),
            content_hash: "abc123".to_string(),
            vault_path: "/tmp/vault".to_string(),
            daily_folder: "Daily".to_string(),
            conflict_behavior: "append_under_marker".to_string(),
            expires_at,
        }
    }

    fn export_input() -> WorkJournalExportInput {
        WorkJournalExportInput {
            date: "2026-07-10".to_string(),
            content_hash: "abc123".to_string(),
            confirmation_token: "one-time-token".to_string(),
        }
    }

    #[test]
    fn obsidian_export_token_should_be_one_time_and_expire() {
        let mut pending = HashMap::from([("one-time-token".to_string(), pending_export(2_000))]);

        let taken =
            take_pending_export(&mut pending, "one-time-token", 1_000).expect("首次确认应成功");
        assert_eq!(taken.content_hash, "abc123");
        assert!(take_pending_export(&mut pending, "one-time-token", 1_000).is_err());

        pending.insert("expired".to_string(), pending_export(900));
        assert!(take_pending_export(&mut pending, "expired", 1_000).is_err());
        assert!(!pending.contains_key("expired"));
    }

    #[test]
    fn obsidian_export_token_should_bind_hash_date_and_settings() {
        let pending = pending_export(2_000);
        let settings = WorkJournalObsidianConfig {
            vault_path: "/tmp/vault".to_string(),
            daily_folder: "Daily".to_string(),
            export_mode: "daily_log".to_string(),
            conflict_behavior: "append_under_marker".to_string(),
        };
        assert!(validate_pending_export(&pending, &export_input(), &settings).is_ok());

        let mut wrong_hash = export_input();
        wrong_hash.content_hash = "changed".to_string();
        assert!(validate_pending_export(&pending, &wrong_hash, &settings).is_err());

        let mut changed_settings = settings.clone();
        changed_settings.daily_folder = "Changed".to_string();
        assert!(validate_pending_export(&pending, &export_input(), &changed_settings).is_err());
    }
}
