use crate::config::PrivacyConfig;
use crate::database::Activity;
use crate::privacy::{PrivacyAction, PrivacyFilter};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const REDACTED_WINDOW_TITLE: &str = "[内容已脱敏]";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SessionEvidencePacket {
    pub apps: Vec<String>,
    pub window_titles: Vec<String>,
    pub ocr_texts: Vec<String>,
    pub domains: Vec<String>,
    pub screenshot_paths: Vec<String>,
}

impl SessionEvidencePacket {
    pub fn to_prompt_text(&self) -> String {
        let mut sections = Vec::new();
        if !self.apps.is_empty() {
            sections.push(format!("Apps: {}", self.apps.join(", ")));
        }
        if !self.window_titles.is_empty() {
            sections.push(format!("Window titles: {}", self.window_titles.join(" | ")));
        }
        if !self.domains.is_empty() {
            sections.push(format!("Domains: {}", self.domains.join(", ")));
        }
        if !self.ocr_texts.is_empty() {
            sections.push(format!("OCR: {}", self.ocr_texts.join(" | ")));
        }
        sections.join("\n")
    }
}

pub fn build_session_evidence(
    activities: &[Activity],
    privacy_config: &PrivacyConfig,
    include_screenshots: bool,
) -> SessionEvidencePacket {
    let filter = PrivacyFilter::from_config(privacy_config);
    let mut packet = SessionEvidencePacket::default();
    let mut apps = HashSet::new();
    let mut titles = HashSet::new();
    let mut ocr_texts = HashSet::new();
    let mut domains = HashSet::new();
    let mut screenshot_paths = HashSet::new();

    for activity in activities {
        let privacy_action = if activity.window_title.trim() == REDACTED_WINDOW_TITLE {
            PrivacyAction::Anonymize
        } else {
            filter.check_privacy_full(
                &activity.app_name,
                &activity.window_title,
                activity.browser_url.as_deref(),
            )
        };
        if privacy_action == PrivacyAction::Skip {
            continue;
        }

        push_unique_limited(&mut packet.apps, &mut apps, activity.app_name.trim(), 8, 80);

        if privacy_action == PrivacyAction::Anonymize {
            continue;
        }

        if let Some(domain) = activity.browser_url.as_deref().and_then(extract_domain) {
            push_unique_limited(&mut packet.domains, &mut domains, &domain, 8, 120);
        }

        let title = filter.filter_text(activity.window_title.trim());
        push_unique_limited(&mut packet.window_titles, &mut titles, &title, 8, 160);

        if let Some(ocr_text) = activity.ocr_text.as_deref() {
            let filtered = filter.filter_text(ocr_text.trim());
            push_unique_limited(&mut packet.ocr_texts, &mut ocr_texts, &filtered, 6, 500);
        }

        if include_screenshots {
            push_unique_limited(
                &mut packet.screenshot_paths,
                &mut screenshot_paths,
                activity.screenshot_path.trim(),
                3,
                500,
            );
        }
    }

    packet
}

pub fn sanitize_activities_for_journal(
    activities: &[Activity],
    privacy_config: &PrivacyConfig,
) -> Vec<Activity> {
    let filter = PrivacyFilter::from_config(privacy_config);
    activities
        .iter()
        .filter_map(|activity| {
            let action = if activity.window_title.trim() == REDACTED_WINDOW_TITLE {
                PrivacyAction::Anonymize
            } else {
                filter.check_privacy_full(
                    &activity.app_name,
                    &activity.window_title,
                    activity.browser_url.as_deref(),
                )
            };
            match action {
                PrivacyAction::Skip => None,
                PrivacyAction::Anonymize => {
                    let mut sanitized = activity.clone();
                    sanitized.window_title = REDACTED_WINDOW_TITLE.to_string();
                    sanitized.screenshot_path.clear();
                    sanitized.ocr_text = None;
                    sanitized.browser_url = None;
                    sanitized.executable_path = None;
                    sanitized.screenshot_url = None;
                    Some(sanitized)
                }
                PrivacyAction::Record => {
                    let mut sanitized = activity.clone();
                    sanitized.window_title = filter.filter_text(&sanitized.window_title);
                    sanitized.ocr_text = sanitized
                        .ocr_text
                        .as_deref()
                        .map(|text| filter.filter_text(text));
                    Some(sanitized)
                }
            }
        })
        .collect()
}

fn extract_domain(url: &str) -> Option<String> {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(|host| host.to_lowercase()))
}

fn push_unique_limited(
    values: &mut Vec<String>,
    seen: &mut HashSet<String>,
    value: &str,
    max_count: usize,
    max_chars: usize,
) {
    if values.len() >= max_count || value.is_empty() {
        return;
    }
    let value = value.chars().take(max_chars).collect::<String>();
    if seen.insert(value.clone()) {
        values.push(value);
    }
}
