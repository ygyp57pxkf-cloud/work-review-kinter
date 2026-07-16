use crate::error::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AiAttributionResult {
    pub project_key: String,
    pub project_name: String,
    pub task_summary: String,
    pub activity_type: String,
    pub confidence: i32,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub risk_flags: Vec<String>,
    #[serde(default = "default_needs_review")]
    pub needs_review: bool,
}

fn default_needs_review() -> bool {
    true
}

pub fn parse_ai_attribution(response: &str) -> Result<AiAttributionResult> {
    let trimmed = response.trim();
    let json = if trimmed.starts_with("```") {
        let body = trimmed
            .strip_prefix("```json")
            .or_else(|| trimmed.strip_prefix("```JSON"))
            .or_else(|| trimmed.strip_prefix("```"))
            .unwrap_or(trimmed);
        body.strip_suffix("```").unwrap_or(body).trim()
    } else {
        trimmed
    };

    Ok(serde_json::from_str(json)?)
}
