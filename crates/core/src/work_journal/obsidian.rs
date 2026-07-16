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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObsidianWriteResult {
    pub target_path: PathBuf,
    pub backup_path: Option<PathBuf>,
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

pub fn write_obsidian_daily_log(
    vault_path: &Path,
    daily_folder: &str,
    date: &str,
    markdown: &str,
    conflict_behavior: &str,
) -> Result<ObsidianWriteResult> {
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|_| AppError::Config(format!("无效日志日期: {date}")))?;
    if !vault_path.is_dir() {
        return Err(AppError::Config(
            "Obsidian vault 路径不存在或不是目录".to_string(),
        ));
    }

    let relative_folder = Path::new(daily_folder.trim());
    if relative_folder.is_absolute()
        || relative_folder.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(AppError::Privacy(
            "Obsidian 日志目录必须位于配置的 vault 内".to_string(),
        ));
    }

    let canonical_vault = vault_path.canonicalize()?;
    let target_dir = canonical_vault.join(relative_folder);
    std::fs::create_dir_all(&target_dir)?;
    let canonical_target_dir = target_dir.canonicalize()?;
    if !canonical_target_dir.starts_with(&canonical_vault) {
        return Err(AppError::Privacy(
            "拒绝写入 Obsidian vault 之外的路径".to_string(),
        ));
    }

    let base_target = canonical_target_dir.join(format!("{date}.md"));
    let target_path = match conflict_behavior {
        "append_under_marker" => base_target,
        "create_new" => next_available_target(&base_target, &canonical_vault)?,
        "manual_copy" | "preview_only" => {
            return Err(AppError::Config(
                "当前导出模式仅允许预览或手动复制".to_string(),
            ));
        }
        _ => {
            return Err(AppError::Config(
                "不支持的 Obsidian 冲突处理方式".to_string(),
            ))
        }
    };

    validate_target_path(&target_path, &canonical_vault, "Obsidian 日报")?;

    let target_exists = path_entry_exists(&target_path)?;
    let backup_path = if target_exists {
        let backup = target_path.with_file_name(format!(".{date}.work-journal.bak"));
        validate_target_path(&backup, &canonical_vault, "Obsidian 备份")?;
        fs::copy(&target_path, &backup)?;
        Some(backup)
    } else {
        None
    };
    let next_content = if target_exists && conflict_behavior == "append_under_marker" {
        let current = fs::read_to_string(&target_path)?;
        replace_or_append_marker(&current, date, markdown)
    } else {
        marked_block(date, markdown)
    };
    validate_target_path(&target_path, &canonical_vault, "Obsidian 日报")?;
    write_without_following_new_symlink(&target_path, next_content.as_bytes(), target_exists)?;

    Ok(ObsidianWriteResult {
        target_path,
        backup_path,
    })
}

fn next_available_target(base_target: &Path, canonical_vault: &Path) -> Result<PathBuf> {
    if !path_entry_exists(base_target)? {
        return Ok(base_target.to_path_buf());
    }
    validate_target_path(base_target, canonical_vault, "Obsidian 日报候选")?;
    let parent = base_target.parent().unwrap_or_else(|| Path::new("."));
    let stem = base_target
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("work-journal");
    for suffix in 2..10_000 {
        let candidate = parent.join(format!("{stem}-{suffix}.md"));
        if !path_entry_exists(&candidate)? {
            return Ok(candidate);
        }
        validate_target_path(&candidate, canonical_vault, "Obsidian 日报候选")?;
    }
    Err(AppError::Config(
        "Obsidian 日报候选文件名已用尽".to_string(),
    ))
}

fn path_entry_exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn validate_target_path(path: &Path, canonical_vault: &Path, label: &str) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() {
        return Err(AppError::Privacy(format!("拒绝通过符号链接写入{label}")));
    }
    if !metadata.is_file() {
        return Err(AppError::Privacy(format!("{label}目标不是普通文件")));
    }
    let canonical_path = path.canonicalize()?;
    if !canonical_path.starts_with(canonical_vault) {
        return Err(AppError::Privacy(format!("拒绝写入 vault 之外的{label}")));
    }
    Ok(())
}

fn write_without_following_new_symlink(
    path: &Path,
    content: &[u8],
    existed_before: bool,
) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true);
    if existed_before {
        options.truncate(true);
    } else {
        options.create_new(true);
    }
    let mut file = options.open(path)?;
    file.write_all(content)?;
    file.sync_all()?;
    Ok(())
}

fn replace_or_append_marker(existing: &str, date: &str, markdown: &str) -> String {
    let start_marker = format!("<!-- WORK-JOURNAL:START {date} -->");
    let end_marker = format!("<!-- WORK-JOURNAL:END {date} -->");
    let block = marked_block(date, markdown);
    if let Some(start) = existing.find(&start_marker) {
        if let Some(relative_end) = existing[start..].find(&end_marker) {
            let end = start + relative_end + end_marker.len();
            let mut content = existing.to_string();
            content.replace_range(start..end, block.trim_end());
            return content;
        }
    }

    let separator = if existing.trim().is_empty() {
        ""
    } else {
        "\n\n"
    };
    format!("{}{separator}{block}", existing.trim_end())
}

fn marked_block(date: &str, markdown: &str) -> String {
    format!(
        "<!-- WORK-JOURNAL:START {date} -->\n{}\n<!-- WORK-JOURNAL:END {date} -->\n",
        markdown.trim()
    )
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

#[cfg(test)]
mod tests {
    use super::write_obsidian_daily_log;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_vault(name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("work-journal-{name}-{unique}"));
        std::fs::create_dir_all(&path).expect("创建临时 vault 失败");
        path
    }

    #[test]
    fn obsidian_export_should_replace_stable_marker_without_duplicates() {
        let vault = temp_vault("obsidian-replace");
        let first = write_obsidian_daily_log(
            &vault,
            "Daily",
            "2026-07-10",
            "# 第一版",
            "append_under_marker",
        )
        .expect("首次导出失败");
        let second = write_obsidian_daily_log(
            &vault,
            "Daily",
            "2026-07-10",
            "# 第二版",
            "append_under_marker",
        )
        .expect("重复导出失败");
        let content = std::fs::read_to_string(&second.target_path).expect("读取导出文件失败");

        assert_eq!(first.target_path, second.target_path);
        assert_eq!(
            content
                .matches("<!-- WORK-JOURNAL:START 2026-07-10 -->")
                .count(),
            1
        );
        assert_eq!(
            content
                .matches("<!-- WORK-JOURNAL:END 2026-07-10 -->")
                .count(),
            1
        );
        assert!(content.contains("# 第二版"));
        assert!(!content.contains("# 第一版"));
        assert!(second.backup_path.is_some());

        let _ = std::fs::remove_dir_all(vault);
    }

    #[test]
    fn obsidian_export_should_refuse_path_outside_vault() {
        let vault = temp_vault("obsidian-boundary");
        let result = write_obsidian_daily_log(
            &vault,
            "../outside",
            "2026-07-10",
            "# 不应写入",
            "append_under_marker",
        );

        assert!(result.is_err());
        let _ = std::fs::remove_dir_all(vault);
    }

    #[cfg(unix)]
    #[test]
    fn obsidian_export_should_refuse_symlinked_target_file() {
        use std::os::unix::fs::symlink;

        let vault = temp_vault("obsidian-target-symlink");
        let outside = temp_vault("obsidian-target-outside").join("outside.md");
        std::fs::write(&outside, "outside").expect("创建外部文件失败");
        let daily = vault.join("Daily");
        std::fs::create_dir_all(&daily).expect("创建日报目录失败");
        symlink(&outside, daily.join("2026-07-10.md")).expect("创建目标链接失败");

        let result = write_obsidian_daily_log(
            &vault,
            "Daily",
            "2026-07-10",
            "# 不应写入",
            "append_under_marker",
        );

        assert!(result.is_err());
        assert_eq!(
            std::fs::read_to_string(&outside).expect("读取外部文件失败"),
            "outside"
        );
        let outside_root = outside.parent().expect("外部目录应存在").to_path_buf();
        let _ = std::fs::remove_dir_all(vault);
        let _ = std::fs::remove_dir_all(outside_root);
    }

    #[cfg(unix)]
    #[test]
    fn obsidian_export_should_refuse_symlinked_backup_file() {
        use std::os::unix::fs::symlink;

        let vault = temp_vault("obsidian-backup-symlink");
        let outside = temp_vault("obsidian-backup-outside").join("outside.md");
        std::fs::write(&outside, "outside").expect("创建外部文件失败");
        let daily = vault.join("Daily");
        std::fs::create_dir_all(&daily).expect("创建日报目录失败");
        std::fs::write(daily.join("2026-07-10.md"), "existing").expect("创建既有日报失败");
        symlink(&outside, daily.join(".2026-07-10.work-journal.bak")).expect("创建备份链接失败");

        let result = write_obsidian_daily_log(
            &vault,
            "Daily",
            "2026-07-10",
            "# 不应写入",
            "append_under_marker",
        );

        assert!(result.is_err());
        assert_eq!(
            std::fs::read_to_string(&outside).expect("读取外部文件失败"),
            "outside"
        );
        let outside_root = outside.parent().expect("外部目录应存在").to_path_buf();
        let _ = std::fs::remove_dir_all(vault);
        let _ = std::fs::remove_dir_all(outside_root);
    }

    #[cfg(unix)]
    #[test]
    fn obsidian_create_new_should_refuse_symlinked_candidate_file() {
        use std::os::unix::fs::symlink;

        let vault = temp_vault("obsidian-candidate-symlink");
        let outside = temp_vault("obsidian-candidate-outside").join("outside.md");
        std::fs::write(&outside, "outside").expect("创建外部文件失败");
        let daily = vault.join("Daily");
        std::fs::create_dir_all(&daily).expect("创建日报目录失败");
        std::fs::write(daily.join("2026-07-10.md"), "existing").expect("创建现有日报失败");
        symlink(&outside, daily.join("2026-07-10-2.md")).expect("创建候选链接失败");

        let result =
            write_obsidian_daily_log(&vault, "Daily", "2026-07-10", "# 不应写入", "create_new");

        assert!(result.is_err());
        assert_eq!(
            std::fs::read_to_string(&outside).expect("读取外部文件失败"),
            "outside"
        );
        let outside_root = outside.parent().expect("外部目录应存在").to_path_buf();
        let _ = std::fs::remove_dir_all(vault);
        let _ = std::fs::remove_dir_all(outside_root);
    }
}
use crate::error::{AppError, Result};
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Component, Path, PathBuf};
