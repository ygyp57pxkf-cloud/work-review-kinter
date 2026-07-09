use crate::config::AppConfig;
use crate::error::AppError;
use crate::AppState;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, State};
use work_review_core::work_journal::project_rules::{
    match_project, normalize_project_rules, ProjectEvidence, ProjectMatch, ProjectRule,
};

use super::shared::persist_app_config;

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
