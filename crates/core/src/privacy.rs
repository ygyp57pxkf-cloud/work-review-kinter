use crate::config::{AppConfig, PrivacyConfig, PrivacyLevel};
use crate::database::DailyStats;
use regex::Regex;

/// 隐私检查结果
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PrivacyAction {
    /// 正常记录（截图 + 统计）
    Record,
    /// 内容脱敏（只统计时长，不保存截图）
    Anonymize,
    /// 完全跳过（不记录任何信息）
    Skip,
}

/// 隐私过滤器
pub struct PrivacyFilter {
    config: PrivacyConfig,
    sensitive_patterns: Vec<Regex>,
}

impl PrivacyFilter {
    /// 从配置创建隐私过滤器
    pub fn from_config(config: &PrivacyConfig) -> Self {
        let sensitive_patterns = vec![
            // 信用卡号
            Regex::new(r"\b\d{4}[\s-]?\d{4}[\s-]?\d{4}[\s-]?\d{4}\b").unwrap(),
            // 手机号
            Regex::new(r"\b1[3-9]\d{9}\b").unwrap(),
            // 身份证号
            Regex::new(r"\b\d{17}[\dXx]\b").unwrap(),
            // 邮箱
            Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b").unwrap(),
            // 密码字段
            Regex::new(r"(?i)(password|密码|pwd|passwd)[\s:：=]+\S+").unwrap(),
            // API Key
            Regex::new(r"(?i)(api[_-]?key|secret|token)[\s:：=]+\S+").unwrap(),
            // Authorization Bearer token
            Regex::new(r"(?i)\bbearer\s+[A-Za-z0-9._~+/=-]+").unwrap(),
            // 常见服务令牌前缀
            Regex::new(r"(?i)\b(?:sk-|ghp_|github_pat_|xox[baprs]-)[A-Za-z0-9_-]{8,}\b")
                .unwrap(),
        ];

        Self {
            config: config.clone(),
            sensitive_patterns,
        }
    }

    /// 检查应用和窗口的隐私操作
    /// 返回应该采取的隐私行动
    pub fn check_privacy(&self, app_name: &str, window_title: &str) -> PrivacyAction {
        // 1. 先检查应用级别的隐私规则
        let app_level = self.config.get_app_privacy_level(app_name);

        match app_level {
            PrivacyLevel::Ignored => {
                log::debug!("应用 {app_name} 设置为完全忽略");
                return PrivacyAction::Skip;
            }
            PrivacyLevel::Anonymized => {
                log::debug!("应用 {app_name} 设置为内容脱敏");
                return PrivacyAction::Anonymize;
            }
            PrivacyLevel::Full => {
                // 继续检查窗口标题
            }
        }

        // 2. 检查窗口标题关键词（匹配时使用脱敏模式）
        if self.config.should_anonymize_by_keyword(window_title) {
            log::debug!("窗口标题 {window_title} 匹配敏感关键词，使用脱敏模式");
            return PrivacyAction::Anonymize;
        }

        PrivacyAction::Record
    }

    /// 检查 URL 域名是否在黑名单中
    /// 如果匹配，返回 Skip；否则返回 Record
    pub fn check_url_privacy(&self, url: Option<&str>) -> PrivacyAction {
        if let Some(url) = url {
            if !url.is_empty() {
                let domain = PrivacyConfig::extract_domain(url);

                for excluded in &self.config.excluded_domains {
                    let excluded_domain = PrivacyConfig::extract_domain(excluded);

                    if !domain.is_empty() && !excluded_domain.is_empty() {
                        if PrivacyConfig::domain_matches(&domain, &excluded_domain) {
                            log::debug!("URL 域名 {domain} 匹配黑名单 {excluded_domain}, 跳过记录");
                            return PrivacyAction::Skip;
                        }
                    }
                }
            }
        }
        PrivacyAction::Record
    }

    /// 综合检查：应用 + 窗口标题 + URL
    pub fn check_privacy_full(
        &self,
        app_name: &str,
        window_title: &str,
        browser_url: Option<&str>,
    ) -> PrivacyAction {
        // 1. 先检查应用和窗口标题
        let app_action = self.check_privacy(app_name, window_title);
        if app_action == PrivacyAction::Skip {
            return PrivacyAction::Skip;
        }

        // 2. 检查 URL 域名黑名单
        let url_action = self.check_url_privacy(browser_url);
        if url_action == PrivacyAction::Skip {
            return PrivacyAction::Skip;
        }

        // 3. 返回应用级别的结果（可能是 Record 或 Anonymize）
        app_action
    }

    /// 兼容旧接口：检查是否应该跳过
    pub fn should_skip(&self, app_name: &str, window_title: &str) -> bool {
        self.check_privacy(app_name, window_title) == PrivacyAction::Skip
    }

    /// 过滤OCR文本中的敏感信息
    pub fn filter_text(&self, text: &str) -> String {
        let mut filtered = text.to_string();

        for pattern in &self.sensitive_patterns {
            filtered = pattern.replace_all(&filtered, "[已过滤]").to_string();
        }

        filtered
    }

    /// 更新配置
    pub fn update_config(&mut self, config: &PrivacyConfig) {
        self.config = config.clone();
    }
}

/// 判断 app 是否命中忽略名单。匹配前先经过 `normalize_display_app_name` 归一化，
/// 再做大小写不敏感的子串/包含匹配。归一化是防御性的：即便上游传入的是裸可执行名
/// （如 `chrome.exe`），也能与配置里以显示名（如 `Google Chrome`）登记的规则对齐。
/// 对于上游已经归一化好的显示名，归一化是幂等的，行为保持一致。
pub fn matches_ignored_app(app_name: &str, ignored_apps: &[String]) -> bool {
    let app_lower = crate::categorize::normalize_display_app_name(app_name)
        .to_lowercase()
        .trim()
        .to_string();
    if app_lower.is_empty() {
        return false;
    }

    ignored_apps
        .iter()
        .any(|ignored| app_lower.contains(ignored) || ignored.contains(&app_lower))
}

/// 判断目标 URL/域名是否命中排除域名名单。先提取域名，再做精确后缀匹配或
/// 合并域名（merged domain）的兼容匹配。
pub fn matches_excluded_domain(target: &str, excluded_domains: &[String]) -> bool {
    let domain = PrivacyConfig::extract_domain(target);
    if domain.is_empty() {
        return false;
    }

    excluded_domains.iter().any(|excluded| {
        let excluded_domain = PrivacyConfig::extract_domain(excluded);
        !excluded_domain.is_empty()
            && (PrivacyConfig::domain_matches(&domain, &excluded_domain)
                || merged_domain_matches_excluded(&domain, &excluded_domain))
    })
}

/// 合并域名（merged domain）兼容匹配：当目标域名本身是一个「合并入口」
/// （如 `github.com` 被合并成 `github.io` 之类的二级入口）时，允许与排除名单中
/// 的同类入口做前缀匹配。
pub fn merged_domain_matches_excluded(domain: &str, excluded_domain: &str) -> bool {
    if !crate::categorize::is_merged_domain(domain) {
        return false;
    }

    let domain = domain.trim_end_matches('.').to_lowercase();
    let excluded_domain = excluded_domain.trim_end_matches('.').to_lowercase();
    let domain_labels: Vec<&str> = domain.split('.').collect();
    let excluded_labels: Vec<&str> = excluded_domain.split('.').collect();

    domain_labels.len() == 2
        && excluded_labels.len() == 2
        && domain_labels[0] == excluded_labels[0]
        && domain_labels[1].starts_with(excluded_labels[1])
        && domain_labels[1].len() > excluded_labels[1].len()
}

/// 从 `AppConfig` 收集隐私过滤所需的两组名单：
/// 返回 `(ignored_apps, excluded_domains)`。
pub fn collect_privacy_filters(config: &AppConfig) -> (Vec<String>, Vec<String>) {
    (
        config.privacy.collect_ignored_app_names(),
        config.privacy.collect_excluded_domains(),
    )
}

/// 把忽略应用名单应用到 `DailyStats`：移除命中名单的应用/浏览器，并重算合计时长。
pub fn apply_ignored_apps_to_stats(mut stats: DailyStats, ignored_apps: &[String]) -> DailyStats {
    if ignored_apps.is_empty() {
        return stats;
    }

    let filtered_app_usage: Vec<_> = stats
        .app_usage
        .into_iter()
        .filter(|app| !matches_ignored_app(&app.app_name, ignored_apps))
        .collect();

    stats.total_duration = filtered_app_usage.iter().map(|app| app.duration).sum();
    stats.app_usage = filtered_app_usage;
    stats
        .browser_usage
        .retain(|browser| !matches_ignored_app(&browser.browser_name, ignored_apps));
    stats.browser_duration = stats
        .browser_usage
        .iter()
        .map(|browser| browser.duration)
        .sum();

    if stats.work_time_duration > stats.total_duration {
        stats.work_time_duration = stats.total_duration;
    }

    stats
}

/// 把排除域名名单应用到 `DailyStats`：移除命中名单的 URL/域名/浏览器子域名，
/// 重算浏览器时长并丢弃时长归零的浏览器。
pub fn apply_excluded_domains_to_stats(
    mut stats: DailyStats,
    excluded_domains: &[String],
) -> DailyStats {
    if excluded_domains.is_empty() {
        return stats;
    }

    stats
        .url_usage
        .retain(|url| !matches_excluded_domain(&url.domain, excluded_domains));
    stats
        .domain_usage
        .retain(|domain| !matches_excluded_domain(&domain.domain, excluded_domains));

    for browser in &mut stats.browser_usage {
        browser
            .domains
            .retain(|domain| !matches_excluded_domain(&domain.domain, excluded_domains));
        browser.duration = browser.domains.iter().map(|domain| domain.duration).sum();
    }
    stats.browser_usage.retain(|browser| browser.duration > 0);
    stats.browser_usage.sort_by(|left, right| {
        right
            .duration
            .cmp(&left.duration)
            .then_with(|| left.browser_name.cmp(&right.browser_name))
    });
    stats.browser_duration = stats
        .browser_usage
        .iter()
        .map(|browser| browser.duration)
        .sum();

    stats
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppPrivacyRule;

    #[test]
    fn test_privacy_levels() {
        let mut config = PrivacyConfig::default();
        config.app_rules.push(AppPrivacyRule {
            app_name: "WeChat".to_string(),
            level: PrivacyLevel::Anonymized,
        });

        let filter = PrivacyFilter::from_config(&config);

        // 完全忽略
        assert_eq!(
            filter.check_privacy("1Password", "Main"),
            PrivacyAction::Skip
        );

        // 内容脱敏（通过规则）
        assert_eq!(
            filter.check_privacy("WeChat", "Chat"),
            PrivacyAction::Anonymize
        );

        // 正常记录
        assert_eq!(
            filter.check_privacy("VS Code", "main.rs"),
            PrivacyAction::Record
        );
    }

    #[test]
    fn test_keyword_anonymize() {
        let config = PrivacyConfig::default();
        let filter = PrivacyFilter::from_config(&config);

        // 关键词触发脱敏
        assert_eq!(
            filter.check_privacy("Chrome", "Bank Login"),
            PrivacyAction::Anonymize
        );
        assert_eq!(
            filter.check_privacy("Firefox", "Enter password"),
            PrivacyAction::Anonymize
        );

        // 正常记录
        assert_eq!(
            filter.check_privacy("Chrome", "GitHub"),
            PrivacyAction::Record
        );
    }

    #[test]
    fn test_filter_text() {
        let config = PrivacyConfig::default();
        let filter = PrivacyFilter::from_config(&config);

        let text = "My phone is 13812345678 and card is 1234-5678-9012-3456";
        let filtered = filter.filter_text(text);

        assert!(filtered.contains("[已过滤]"));
        assert!(!filtered.contains("13812345678"));
    }
}
