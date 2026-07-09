use crate::database::Activity;

const MAX_MERGE_GAP_SECONDS: i64 = 5 * 60;
const CONTEXT_JUMP_SECONDS: i64 = 2 * 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkSession {
    pub started_at: i64,
    pub ended_at: i64,
    pub duration: i64,
    pub primary_app: String,
    pub activity_ids: Vec<i64>,
}

#[derive(Debug, Clone)]
struct ActivitySpan {
    id: Option<i64>,
    started_at: i64,
    ended_at: i64,
    duration: i64,
    app_name: String,
    context_category: Option<&'static str>,
    is_entertainment: bool,
}

pub fn sessionize_activities(activities: &[Activity]) -> Vec<WorkSession> {
    let mut spans: Vec<ActivitySpan> = activities.iter().map(ActivitySpan::from).collect();
    spans.sort_by(|left, right| {
        left.started_at
            .cmp(&right.started_at)
            .then_with(|| left.ended_at.cmp(&right.ended_at))
            .then_with(|| left.id.cmp(&right.id))
    });

    let mut sessions = Vec::new();
    let mut current: Vec<ActivitySpan> = Vec::new();

    for span in spans {
        if should_split_session(&current, &span) {
            sessions.push(build_session(&current));
            current.clear();
        }
        current.push(span);
    }

    if !current.is_empty() {
        sessions.push(build_session(&current));
    }

    sessions
}

impl From<&Activity> for ActivitySpan {
    fn from(activity: &Activity) -> Self {
        let duration = activity.duration.max(0);
        let category_key = crate::categorize::normalize_category_key(&activity.category);
        let is_entertainment = category_key == "entertainment"
            || activity
                .semantic_category
                .as_deref()
                .is_some_and(|semantic| semantic.contains("休息娱乐"));

        Self {
            id: activity.id,
            started_at: activity.timestamp.saturating_sub(duration),
            ended_at: activity.timestamp,
            duration,
            app_name: activity.app_name.clone(),
            context_category: context_category(&category_key),
            is_entertainment,
        }
    }
}

fn should_split_session(current: &[ActivitySpan], next: &ActivitySpan) -> bool {
    let Some(previous) = current.last() else {
        return false;
    };

    let gap = next.started_at.saturating_sub(previous.ended_at);
    if gap > MAX_MERGE_GAP_SECONDS {
        return true;
    }

    if session_is_entertainment(current) != next.is_entertainment {
        return true;
    }

    sustained_context_jump(current, next)
}

fn session_is_entertainment(spans: &[ActivitySpan]) -> bool {
    spans.iter().all(|span| span.is_entertainment)
}

fn sustained_context_jump(current: &[ActivitySpan], next: &ActivitySpan) -> bool {
    let Some(next_category) = next.context_category else {
        return false;
    };
    if next.duration <= CONTEXT_JUMP_SECONDS {
        return false;
    }

    let current_category = dominant_context_category(current);
    let Some((current_category, current_duration)) = current_category else {
        return false;
    };

    current_category != next_category && current_duration > CONTEXT_JUMP_SECONDS
}

fn dominant_context_category(spans: &[ActivitySpan]) -> Option<(&'static str, i64)> {
    let mut totals: Vec<(&'static str, i64)> = Vec::new();
    for span in spans {
        let Some(category) = span.context_category else {
            continue;
        };
        if let Some((_, duration)) = totals
            .iter_mut()
            .find(|(existing_category, _)| existing_category == &category)
        {
            *duration += span.duration;
        } else {
            totals.push((category, span.duration));
        }
    }

    totals.into_iter().max_by_key(|(_, duration)| *duration)
}

fn context_category(category_key: &str) -> Option<&'static str> {
    match category_key {
        "communication" => Some("communication"),
        "browser" => Some("browser"),
        "development" => Some("development"),
        "office" => Some("office"),
        _ => None,
    }
}

fn build_session(spans: &[ActivitySpan]) -> WorkSession {
    let started_at = spans
        .iter()
        .map(|span| span.started_at)
        .min()
        .unwrap_or_default();
    let ended_at = spans
        .iter()
        .map(|span| span.ended_at)
        .max()
        .unwrap_or_default();
    let duration = spans.iter().map(|span| span.duration).sum();
    let primary_app = primary_app(spans);
    let activity_ids = spans.iter().filter_map(|span| span.id).collect();

    WorkSession {
        started_at,
        ended_at,
        duration,
        primary_app,
        activity_ids,
    }
}

fn primary_app(spans: &[ActivitySpan]) -> String {
    let mut totals: Vec<(String, i64, usize)> = Vec::new();
    for (index, span) in spans.iter().enumerate() {
        if let Some((_, duration, _)) = totals
            .iter_mut()
            .find(|(app_name, _, _)| app_name == &span.app_name)
        {
            *duration += span.duration;
        } else {
            totals.push((span.app_name.clone(), span.duration, index));
        }
    }

    totals
        .into_iter()
        .max_by(|left, right| {
            left.1
                .cmp(&right.1)
                .then_with(|| right.2.cmp(&left.2))
        })
        .map(|(app_name, _, _)| app_name)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::sessionize_activities;
    use crate::database::Activity;

    fn activity(
        id: i64,
        started_at: i64,
        duration: i64,
        app_name: &str,
        category: &str,
        semantic_category: Option<&str>,
    ) -> Activity {
        Activity {
            id: Some(id),
            timestamp: started_at + duration,
            app_name: app_name.to_string(),
            window_title: format!("{app_name} window"),
            screenshot_path: format!("shot-{id}.jpg"),
            ocr_text: None,
            category: category.to_string(),
            duration,
            browser_url: None,
            executable_path: None,
            semantic_category: semantic_category.map(str::to_string),
            semantic_confidence: semantic_category.map(|_| 90),
            screenshot_url: None,
        }
    }

    #[test]
    fn chrome_docs_browsing_should_be_one_session() {
        let activities = vec![
            activity(1, 1_000, 600, "Google Chrome", "browser", Some("资料阅读")),
            activity(2, 1_620, 600, "Google Chrome", "browser", Some("资料阅读")),
        ];

        let sessions = sessionize_activities(&activities);

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].primary_app, "Google Chrome");
        assert_eq!(sessions[0].activity_ids, vec![1, 2]);
        assert_eq!(sessions[0].duration, 1_200);
    }

    #[test]
    fn short_communication_bursts_should_attach_to_focused_coding_session() {
        let activities = vec![
            activity(1, 2_000, 120, "WeCom", "communication", Some("即时聊天")),
            activity(2, 2_130, 1_500, "Codex", "development", Some("编码开发")),
            activity(3, 3_650, 60, "WeCom", "communication", Some("即时聊天")),
        ];

        let sessions = sessionize_activities(&activities);

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].primary_app, "Codex");
        assert_eq!(sessions[0].activity_ids, vec![1, 2, 3]);
        assert_eq!(sessions[0].duration, 1_680);
    }

    #[test]
    fn entertainment_block_should_not_mix_into_work_session() {
        let activities = vec![
            activity(1, 4_000, 900, "Codex", "development", Some("编码开发")),
            activity(2, 4_920, 600, "Bilibili", "entertainment", Some("休息娱乐")),
        ];

        let sessions = sessionize_activities(&activities);

        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].primary_app, "Codex");
        assert_eq!(sessions[0].activity_ids, vec![1]);
        assert_eq!(sessions[1].primary_app, "Bilibili");
        assert_eq!(sessions[1].activity_ids, vec![2]);
    }

    #[test]
    fn gap_over_five_minutes_should_split_sessions() {
        let activities = vec![
            activity(1, 6_000, 600, "Codex", "development", Some("编码开发")),
            activity(2, 6_901, 600, "Codex", "development", Some("编码开发")),
        ];

        let sessions = sessionize_activities(&activities);

        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].activity_ids, vec![1]);
        assert_eq!(sessions[1].activity_ids, vec![2]);
    }

    #[test]
    fn sustained_context_jump_should_split_between_work_categories() {
        let activities = vec![
            activity(1, 8_000, 600, "Google Chrome", "browser", Some("资料阅读")),
            activity(2, 8_610, 600, "Codex", "development", Some("编码开发")),
        ];

        let sessions = sessionize_activities(&activities);

        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].primary_app, "Google Chrome");
        assert_eq!(sessions[1].primary_app, "Codex");
    }
}
