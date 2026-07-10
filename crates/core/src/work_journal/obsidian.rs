#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObsidianExportDay {
    pub date: String,
    pub total_duration: i64,
    pub project_count: usize,
    pub sessions: Vec<ObsidianExportSession>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObsidianExportSession {
    pub started_at: String,
    pub ended_at: String,
    pub duration: i64,
    pub project_name: String,
    pub obsidian_page: String,
    pub task_summary: String,
    pub evidence: Vec<String>,
    pub needs_review: bool,
}

pub fn render_obsidian_daily_preview(day: &ObsidianExportDay) -> String {
    let mut markdown = String::new();
    markdown.push_str(&format!("# {} 工作日志\n\n", day.date));

    markdown.push_str("## 今日时间分布\n\n");
    markdown.push_str(&format!(
        "- 记录总时长：{}\n",
        format_duration(day.total_duration)
    ));
    markdown.push_str(&format!("- 关联项目数：{}\n", day.project_count));
    markdown.push_str(&format!("- 工作块数量：{}\n\n", day.sessions.len()));

    markdown.push_str("## 主要推进项目\n\n");
    if day.sessions.is_empty() {
        markdown.push_str("- 暂无可预览的工作 session。\n\n");
    } else {
        for session in &day.sessions {
            markdown.push_str(&format!(
                "- {}：{}{}\n",
                project_link(session),
                format_duration(session.duration),
                review_marker(session.needs_review)
            ));
        }
        markdown.push('\n');
    }

    markdown.push_str("## 具体完成事项\n\n");
    if day.sessions.is_empty() {
        markdown.push_str("- 待确认：今天暂无可归档的工作块。\n\n");
    } else {
        for session in &day.sessions {
            markdown.push_str(&format!(
                "- {}-{} {}：{}{}\n",
                session.started_at,
                session.ended_at,
                project_link(session),
                session.task_summary,
                review_marker(session.needs_review)
            ));
        }
        markdown.push('\n');
    }

    markdown.push_str("## 沟通与会议\n\n");
    markdown.push_str("- 待确认：请在审阅后补充会议结论或沟通背景。\n\n");

    markdown.push_str("## 卡点和待补记\n\n");
    if day.sessions.iter().any(|session| session.needs_review) {
        markdown.push_str("- 待确认：存在低置信或规则未确认的工作块，需要人工复核项目归因。\n\n");
    } else {
        markdown.push_str("- 暂无明显卡点。\n\n");
    }

    markdown.push_str("## 明日建议\n\n");
    markdown.push_str("- 优先延续今日主要推进项目，减少临时切换。\n\n");

    markdown.push_str("## 证据摘要\n\n");
    if day.sessions.is_empty() {
        markdown.push_str("- 暂无证据摘要。\n\n");
    } else {
        for session in &day.sessions {
            let evidence = if session.evidence.is_empty() {
                "无明确证据".to_string()
            } else {
                session.evidence.join("；")
            };
            markdown.push_str(&format!(
                "- {}-{} {}：{}\n",
                session.started_at,
                session.ended_at,
                project_link(session),
                evidence
            ));
        }
        markdown.push('\n');
    }

    markdown.push_str("## 隐私过滤说明\n\n");
    markdown.push_str("- 本预览基于本地隐私过滤后的应用活动、窗口、URL 和 OCR 摘要生成；当前步骤不会自动写入 Obsidian。\n");

    markdown
}

fn project_link(session: &ObsidianExportSession) -> String {
    if session.obsidian_page.trim().is_empty() {
        session.project_name.clone()
    } else {
        format!(
            "[[{}|{}]]",
            session.obsidian_page.trim(),
            session.project_name.trim()
        )
    }
}

fn review_marker(needs_review: bool) -> &'static str {
    if needs_review {
        "（待确认）"
    } else {
        ""
    }
}

fn format_duration(seconds: i64) -> String {
    let minutes = (seconds.max(0) + 59) / 60;
    let hours = minutes / 60;
    let remaining_minutes = minutes % 60;

    match (hours, remaining_minutes) {
        (0, minutes) => format!("{} 分钟", minutes),
        (hours, 0) => format!("{} 小时", hours),
        (hours, minutes) => format!("{} 小时 {} 分钟", hours, minutes),
    }
}
