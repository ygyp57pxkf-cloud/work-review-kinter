#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRule {
    pub project_key: String,
    pub project_name: String,
    pub obsidian_page: String,
    pub local_paths: Vec<String>,
    pub domains: Vec<String>,
    pub url_keywords: Vec<String>,
    pub window_keywords: Vec<String>,
    pub app_keywords: Vec<String>,
    pub negative_keywords: Vec<String>,
    pub priority: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectEvidence {
    pub app_name: String,
    pub window_title: String,
    pub browser_url: Option<String>,
    pub executable_path: Option<String>,
    pub ocr_text: Option<String>,
    pub category: Option<String>,
    pub semantic_category: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectMatch {
    pub project_key: String,
    pub project_name: String,
    pub obsidian_page: String,
    pub score: i32,
    pub evidence: Vec<String>,
}

pub fn default_project_rules() -> Vec<ProjectRule> {
    vec![
        ProjectRule {
            project_key: "it-service-robot".to_string(),
            project_name: "IT服务机器人".to_string(),
            obsidian_page: "IT服务机器人".to_string(),
            local_paths: strings(&["dify_wecom_gateway"]),
            domains: strings(&["aip-dev.sikadavco.cn"]),
            url_keywords: strings(&["dify_wecom_gateway", "dify", "wecom"]),
            window_keywords: strings(&["IT服务机器人", "Dify", "企微", "dify_wecom_gateway"]),
            app_keywords: strings(&["WeCom", "企业微信"]),
            negative_keywords: Vec::new(),
            priority: 100,
        },
        ProjectRule {
            project_key: "ai-committee-workbuddy".to_string(),
            project_name: "IT-AI委员会 / WorkBuddy".to_string(),
            obsidian_page: "AI协作中枢/项目与Agent资料/AI/IT-AI委员会".to_string(),
            local_paths: strings(&["workbuddy", "AI委员会"]),
            domains: Vec::new(),
            url_keywords: strings(&["WorkBuddy", "AI委员会"]),
            window_keywords: strings(&["WorkBuddy", "AI委员会", "BFMIT AI"]),
            app_keywords: strings(&["WorkBuddy"]),
            negative_keywords: Vec::new(),
            priority: 80,
        },
        ProjectRule {
            project_key: "jmcomic-bot".to_string(),
            project_name: "JMComic QQ群机器人".to_string(),
            obsidian_page: "私人/个人项目文档/JMComic QQ群机器人/JMComic QQ群机器人".to_string(),
            local_paths: strings(&["漫画", "JMComic", "downloadctl"]),
            domains: Vec::new(),
            url_keywords: strings(&["MangaDex", "FanFicFare", "gallery-dl", "downloadctl"]),
            window_keywords: strings(&["JMComic", "downloadctl", "MangaDex", "FanFicFare"]),
            app_keywords: strings(&["downloadctl"]),
            negative_keywords: Vec::new(),
            priority: 70,
        },
        ProjectRule {
            project_key: "work-review-fork".to_string(),
            project_name: "Work Review / Work Journal".to_string(),
            obsidian_page: "私人/个人项目文档/Work Journal/Work Journal".to_string(),
            local_paths: strings(&["timereview", "work-review-kinter", "Work-Review"]),
            domains: Vec::new(),
            url_keywords: strings(&["timereview", "work-review-kinter", "Work-Review"]),
            window_keywords: strings(&["timereview", "Work Review", "Work Journal", "work-review-kinter"]),
            app_keywords: strings(&["Work Journal", "Work Review"]),
            negative_keywords: Vec::new(),
            priority: 60,
        },
    ]
}

pub fn match_project(evidence: &ProjectEvidence, rules: &[ProjectRule]) -> Option<ProjectMatch> {
    if is_entertainment(evidence) {
        return None;
    }

    rules
        .iter()
        .filter_map(|rule| score_rule(evidence, rule))
        .max_by(|left, right| {
            left.score
                .cmp(&right.score)
                .then_with(|| left.project_key.cmp(&right.project_key))
        })
}

fn score_rule(evidence: &ProjectEvidence, rule: &ProjectRule) -> Option<ProjectMatch> {
    let combined_text = combined_text(evidence);
    if contains_any(&combined_text, &rule.negative_keywords) {
        return None;
    }

    let mut score = 0;
    let mut reasons = Vec::new();

    if let Some(url) = evidence.browser_url.as_deref() {
        let domain = PrivacyConfig::extract_domain(url);
        for rule_domain in &rule.domains {
            let normalized_rule_domain = PrivacyConfig::extract_domain(rule_domain);
            if !domain.is_empty()
                && !normalized_rule_domain.is_empty()
                && PrivacyConfig::domain_matches(&domain, &normalized_rule_domain)
            {
                score += 100;
                reasons.push(format!("domain:{normalized_rule_domain}"));
            }
        }

        score += score_keywords(url, &rule.url_keywords, 55, "url", &mut reasons);
    }

    if let Some(executable_path) = evidence.executable_path.as_deref() {
        score += score_keywords(
            executable_path,
            &rule.local_paths,
            70,
            "path",
            &mut reasons,
        );
    }

    score += score_keywords(
        &evidence.window_title,
        &rule.local_paths,
        70,
        "path",
        &mut reasons,
    );
    score += score_keywords(
        &evidence.window_title,
        &rule.window_keywords,
        60,
        "window",
        &mut reasons,
    );
    score += score_keywords(
        &evidence.app_name,
        &rule.app_keywords,
        35,
        "app",
        &mut reasons,
    );

    if let Some(ocr_text) = evidence.ocr_text.as_deref() {
        score += score_keywords(ocr_text, &rule.window_keywords, 35, "ocr", &mut reasons);
        score += score_keywords(ocr_text, &rule.local_paths, 40, "ocr", &mut reasons);
    }

    if score <= 0 {
        return None;
    }

    Some(ProjectMatch {
        project_key: rule.project_key.clone(),
        project_name: rule.project_name.clone(),
        obsidian_page: rule.obsidian_page.clone(),
        score: score + rule.priority,
        evidence: reasons,
    })
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| item.to_string()).collect()
}

fn is_entertainment(evidence: &ProjectEvidence) -> bool {
    evidence
        .category
        .as_deref()
        .map(crate::categorize::normalize_category_key)
        .is_some_and(|category| category == "entertainment")
        || evidence
            .semantic_category
            .as_deref()
            .is_some_and(|semantic| semantic.contains("休息娱乐"))
}

fn combined_text(evidence: &ProjectEvidence) -> String {
    [
        Some(evidence.app_name.as_str()),
        Some(evidence.window_title.as_str()),
        evidence.browser_url.as_deref(),
        evidence.executable_path.as_deref(),
        evidence.ocr_text.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join("\n")
}

fn contains_any(text: &str, keywords: &[String]) -> bool {
    let text = text.to_lowercase();
    keywords
        .iter()
        .any(|keyword| !keyword.trim().is_empty() && text.contains(&keyword.to_lowercase()))
}

fn score_keywords(
    text: &str,
    keywords: &[String],
    weight: i32,
    label: &str,
    reasons: &mut Vec<String>,
) -> i32 {
    let text = text.to_lowercase();
    let mut score = 0;

    for keyword in keywords {
        let keyword = keyword.trim();
        if keyword.is_empty() {
            continue;
        }
        if text.contains(&keyword.to_lowercase()) {
            score += weight;
            reasons.push(format!("{label}:{keyword}"));
        }
    }

    score
}

#[cfg(test)]
mod tests {
    use super::{default_project_rules, match_project, ProjectEvidence};

    fn evidence() -> ProjectEvidence {
        ProjectEvidence {
            app_name: "Google Chrome".to_string(),
            window_title: String::new(),
            browser_url: None,
            executable_path: None,
            ocr_text: None,
            category: None,
            semantic_category: None,
        }
    }

    #[test]
    fn default_rules_should_include_user_workflows() {
        let rules = default_project_rules();
        let keys: Vec<&str> = rules.iter().map(|rule| rule.project_key.as_str()).collect();

        assert!(keys.contains(&"it-service-robot"));
        assert!(keys.contains(&"ai-committee-workbuddy"));
        assert!(keys.contains(&"jmcomic-bot"));
        assert!(keys.contains(&"work-review-fork"));
    }

    #[test]
    fn aip_dev_domain_should_map_to_it_service_robot() {
        let mut item = evidence();
        item.browser_url = Some("https://aip-dev.sikadavco.cn/apps/dify_wecom_gateway".to_string());

        let matched = match_project(&item, &default_project_rules()).expect("应匹配 IT 服务机器人");

        assert_eq!(matched.project_key, "it-service-robot");
        assert_eq!(matched.project_name, "IT服务机器人");
        assert!(
            matched
                .evidence
                .iter()
                .any(|evidence| evidence.contains("aip-dev.sikadavco.cn"))
        );
    }

    #[test]
    fn codex_window_with_timereview_should_map_to_work_review_fork() {
        let mut item = evidence();
        item.app_name = "Codex".to_string();
        item.window_title =
            "/Users/kinter/KINTER/Windows_D盘完整备份/开发文件夹/timereview/work-review-kinter"
                .to_string();

        let matched = match_project(&item, &default_project_rules()).expect("应匹配 Work Journal");

        assert_eq!(matched.project_key, "work-review-fork");
        assert_eq!(matched.project_name, "Work Review / Work Journal");
    }

    #[test]
    fn entertainment_url_should_not_match_even_when_ocr_mentions_project() {
        let mut item = evidence();
        item.browser_url = Some("https://www.bilibili.com/video/BV123".to_string());
        item.ocr_text = Some("视频弹幕里提到 dify_wecom_gateway".to_string());
        item.category = Some("entertainment".to_string());
        item.semantic_category = Some("休息娱乐".to_string());

        let matched = match_project(&item, &default_project_rules());

        assert_eq!(matched, None);
    }
}
use crate::config::PrivacyConfig;
