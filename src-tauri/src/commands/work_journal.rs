use crate::config::{AppConfig, PrivacyConfig};
use crate::database::Activity;
use crate::error::AppError;
use crate::AppState;
use chrono::{Local, NaiveDate, TimeZone};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, State};
use work_review_core::work_journal::obsidian::{
    render_obsidian_daily_preview, ObsidianExportDay, ObsidianExportSession,
};
use work_review_core::work_journal::project_rules::{
    match_project, normalize_project_rules, ProjectEvidence, ProjectMatch, ProjectRule,
};
use work_review_core::work_journal::sessionize::{sessionize_activities, WorkSession};

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
    pub id: String,
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
) -> Result<String, AppError> {
    let state = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
    let day = build_work_journal_day(&date, &state)?;
    Ok(render_obsidian_daily_preview(&obsidian_export_day(&day)))
}

fn build_work_journal_day(date: &str, state: &AppState) -> Result<WorkJournalDay, AppError> {
    let (date_from, date_to) = journal_query_bounds(date)?;
    let activities =
        load_filtered_activities_in_range(state, Some(&date_from), Some(&date_to), 10_000)?;
    let sessions = sessionize_activities(&activities);
    let journal_sessions = build_journal_sessions(
        date,
        &sessions,
        &activities,
        &state.config.work_journal_project_rules,
    );
    let project_summaries = summarize_projects(&journal_sessions);
    let total_duration = journal_sessions
        .iter()
        .map(|session| session.duration)
        .sum();
    let needs_review_count = journal_sessions
        .iter()
        .filter(|session| session.needs_review)
        .count();
    let interruption_count = journal_sessions
        .iter()
        .filter(|session| is_communication_app(&session.primary_app))
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
) -> Vec<WorkJournalSession> {
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
                project_match_to_fields(project_match, &session_activities);

            WorkJournalSession {
                id: format!("{date}-{}", index + 1),
                started_at: session.started_at,
                ended_at: session.ended_at,
                duration: session.duration,
                primary_app: session.primary_app.clone(),
                activity_ids: session.activity_ids.clone(),
                project_key,
                project_name,
                obsidian_page,
                confidence,
                task_summary: task_summary(session, &session_activities),
                evidence,
                needs_review,
            }
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
) -> (String, String, String, i32, Vec<String>) {
    if let Some(project_match) = project_match {
        let confidence = project_match.score.clamp(0, 100);
        let mut evidence = project_match.evidence;
        evidence.extend(activity_evidence(activities));
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
        activity_evidence(activities),
    )
}

fn activity_evidence(activities: &[Activity]) -> Vec<String> {
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
            evidence.push(format!(
                "window:{}",
                trim_evidence(activity.window_title.trim(), 80)
            ));
        }
    }
    evidence.sort();
    evidence.dedup();
    evidence.truncate(8);
    evidence
}

fn task_summary(session: &WorkSession, activities: &[Activity]) -> String {
    activities
        .iter()
        .find(|activity| activity.app_name == session.primary_app && !activity.window_title.is_empty())
        .or_else(|| activities.iter().find(|activity| !activity.window_title.is_empty()))
        .map(|activity| trim_evidence(&activity.window_title, 60))
        .unwrap_or_else(|| session.primary_app.clone())
}

fn summarize_projects(sessions: &[WorkJournalSession]) -> Vec<WorkJournalProjectSummary> {
    let mut summaries: HashMap<String, WorkJournalProjectSummary> = HashMap::new();
    for session in sessions {
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
    ["wecom", "wechat", "微信", "企业微信", "teams", "slack", "dingtalk"]
        .iter()
        .any(|keyword| app_name.contains(keyword))
}

#[cfg(test)]
mod tests {
    use super::{activity_evidence, journal_query_bounds};
    use crate::database::Activity;

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

    #[test]
    fn journal_query_bounds_should_stay_on_requested_date() {
        let (date_from, date_to) = journal_query_bounds("2026-07-10").expect("日期应有效");

        assert_eq!(date_from, "2026-07-10");
        assert_eq!(date_to, "2026-07-10");
    }

    #[test]
    fn journal_evidence_should_not_expose_url_path_or_query() {
        let evidence = activity_evidence(&[activity_with_url(
            "https://example.com/private/path?token=secret-value",
        )]);
        let combined = evidence.join("\n");

        assert!(combined.contains("domain:example.com"));
        assert!(!combined.contains("private/path"));
        assert!(!combined.contains("secret-value"));
    }
}
