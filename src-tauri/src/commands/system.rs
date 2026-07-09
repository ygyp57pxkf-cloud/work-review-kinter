//! Auto-extracted from the historical `commands.rs`. Behavior unchanged.

use crate::error::AppError;
#[cfg(target_os = "linux")]
use crate::linux_session::{current_linux_desktop_environment, current_linux_desktop_session, LinuxDesktopSession};
#[cfg(target_os = "linux")]
use super::avatar::{
    gnome_avatar_extension_needs_relogin, is_gnome_avatar_extension_enabled,
    is_gnome_avatar_extension_installed,
};
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{State};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LinuxSessionSupportInfo {
    pub platform: String,
    pub session_type: String,
    pub desktop_environment: String,
    pub active_window_provider: String,
    pub active_window_supported: bool,
    pub screenshot_supported: bool,
    pub browser_url_support_level: String,
    pub avatar_input_provider: String,
    pub avatar_input_support_level: String,
    pub avatar_keyboard_supported: bool,
    pub avatar_mouse_supported: bool,
    pub gnome_avatar_extension_installed: bool,
    pub gnome_avatar_extension_enabled: bool,
    pub gnome_avatar_extension_needs_relogin: bool,
}

#[tauri::command]
pub async fn get_runtime_platform() -> Result<String, AppError> {
    Ok(std::env::consts::OS.to_string())
}

#[tauri::command]
pub async fn get_linux_session_support() -> Result<LinuxSessionSupportInfo, AppError> {
    #[cfg(target_os = "linux")]
    {
        let session = current_linux_desktop_session();
        let desktop_environment = current_linux_desktop_environment();
        let active_window_provider =
            crate::monitor::current_linux_active_window_provider(session, desktop_environment);
        let avatar_input_support = crate::avatar_input::current_linux_avatar_input_support();
        let screenshot_support = crate::screenshot::current_linux_screenshot_support();
        let gnome_avatar_extension_installed = desktop_environment
            == crate::linux_session::LinuxDesktopEnvironment::Gnome
            && is_gnome_avatar_extension_installed();
        let gnome_avatar_extension_enabled = desktop_environment
            == crate::linux_session::LinuxDesktopEnvironment::Gnome
            && is_gnome_avatar_extension_enabled();
        let gnome_avatar_extension_needs_relogin = gnome_avatar_extension_needs_relogin(
            desktop_environment,
            gnome_avatar_extension_installed,
            gnome_avatar_extension_enabled,
            &avatar_input_support,
        );
        let active_window_supported = active_window_provider.is_some();
        let browser_url_support_level = if active_window_supported {
            "mixed"
        } else {
            "limited"
        };

        return Ok(LinuxSessionSupportInfo {
            platform: "linux".to_string(),
            session_type: session.as_str().to_string(),
            desktop_environment: desktop_environment.as_str().to_string(),
            active_window_provider: active_window_provider.unwrap_or("none").to_string(),
            active_window_supported,
            screenshot_supported: screenshot_support.supported,
            browser_url_support_level: browser_url_support_level.to_string(),
            avatar_input_provider: avatar_input_support.provider.to_string(),
            avatar_input_support_level: avatar_input_support.support_level.to_string(),
            avatar_keyboard_supported: avatar_input_support.keyboard_supported,
            avatar_mouse_supported: avatar_input_support.mouse_supported,
            gnome_avatar_extension_installed,
            gnome_avatar_extension_enabled,
            gnome_avatar_extension_needs_relogin,
        });
    }

    #[cfg(not(target_os = "linux"))]
    {
        Ok(LinuxSessionSupportInfo {
            platform: std::env::consts::OS.to_string(),
            session_type: "not_applicable".to_string(),
            desktop_environment: "not_applicable".to_string(),
            active_window_provider: "not_applicable".to_string(),
            active_window_supported: false,
            screenshot_supported: false,
            browser_url_support_level: "not_applicable".to_string(),
            avatar_input_provider: "not_applicable".to_string(),
            avatar_input_support_level: "not_applicable".to_string(),
            avatar_keyboard_supported: false,
            avatar_mouse_supported: false,
            gnome_avatar_extension_installed: false,
            gnome_avatar_extension_enabled: false,
            gnome_avatar_extension_needs_relogin: false,
        })
    }
}

/// 检查 macOS 系统权限状态（屏幕录制 + 辅助功能）
/// Windows 上始终返回全部已授权
#[tauri::command]
pub async fn check_permissions() -> Result<serde_json::Value, AppError> {
    let screen_capture = crate::screenshot::has_screen_capture_permission();
    let accessibility = crate::screenshot::has_accessibility_permission(false);
    let input_monitoring = crate::screenshot::has_input_monitoring_permission();

    #[cfg(target_os = "linux")]
    let screenshot_supported = crate::screenshot::current_linux_screenshot_support().supported;
    #[cfg(not(target_os = "linux"))]
    let screenshot_supported = screen_capture;

    #[cfg(target_os = "linux")]
    let avatar_input_supported =
        crate::avatar_input::current_linux_avatar_input_support().support_level != "none";
    #[cfg(target_os = "macos")]
    let avatar_input_supported = accessibility && input_monitoring;
    #[cfg(target_os = "windows")]
    let avatar_input_supported = true;
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    let avatar_input_supported = false;

    let all_granted = if cfg!(target_os = "macos") {
        screen_capture && accessibility && input_monitoring
    } else {
        screenshot_supported && avatar_input_supported
    };

    Ok(serde_json::json!({
        "screen_capture": screen_capture,
        "accessibility": accessibility,
        "input_monitoring": input_monitoring,
        "screenshot_supported": screenshot_supported,
        "avatar_input_supported": avatar_input_supported,
        "all_granted": all_granted,
        "platform": std::env::consts::OS,
    }))
}

#[cfg(target_os = "macos")]
fn macos_permission_settings_url(permission: &str) -> Option<&'static str> {
    match permission {
        "screen_capture" => {
            Some("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
        }
        "accessibility" => {
            Some("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        }
        "input_monitoring" => {
            Some("x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent")
        }
        _ => None,
    }
}

/// 打开系统权限设置页
#[tauri::command]
pub async fn open_permission_settings(permission: String) -> Result<(), AppError> {
    #[cfg(target_os = "macos")]
    {
        match permission.as_str() {
            "screen_capture" => crate::screenshot::request_screen_capture_permission(),
            "accessibility" => {
                crate::screenshot::has_accessibility_permission(true);
            }
            "input_monitoring" => crate::screenshot::request_input_monitoring_permission(),
            _ => {}
        }

        let target = macos_permission_settings_url(&permission)
            .ok_or_else(|| AppError::Unknown(format!("不支持的权限类型: {permission}")))?;

        std::process::Command::new("open")
            .arg(target)
            .spawn()
            .map_err(|e| AppError::Unknown(format!("打开系统权限设置失败: {e}")))?;

        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = permission;
        Err(AppError::Unknown(
            "当前平台暂不支持直接跳转系统权限设置".to_string(),
        ))
    }
}

/// 设置 Dock 图标可见性 (仅 macOS)
#[tauri::command]
#[allow(unused_variables)]
#[allow(unexpected_cfgs)]
pub fn set_dock_visibility(visible: bool) -> Result<(), AppError> {
    #[cfg(target_os = "macos")]
    {
        apply_dock_visibility(visible, true);
        log::info!("Dock 图标可见性已设置为: {visible}");
    }

    #[cfg(not(target_os = "macos"))]
    {
        log::warn!("set_dock_visibility 仅支持 macOS");
    }

    Ok(())
}

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
fn refresh_dock_icon(activate: bool) {
    use cocoa::appkit::{NSApp, NSImage};
    use cocoa::base::nil;
    use cocoa::foundation::NSString;
    use objc::runtime::Object;

    unsafe {
        let app: *mut Object = NSApp();

        // 使用 NSBundle.mainBundle 获取图标路径
        let bundle: *mut Object = objc::msg_send![objc::class!(NSBundle), mainBundle];
        let resource: *mut Object = objc::msg_send![
            bundle,
            pathForResource: NSString::alloc(nil).init_str("icon")
            ofType: NSString::alloc(nil).init_str("icns")
        ];

        // 如果 bundle 中找不到，尝试硬编码路径
        let path_to_use = if resource != nil {
            resource
        } else {
            NSString::alloc(nil)
                .init_str("/Applications/Work Review.app/Contents/Resources/icon.icns")
        };

        let image: *mut Object = NSImage::alloc(nil).initByReferencingFile_(path_to_use);
        if image != nil {
            let _: () = objc::msg_send![app, setApplicationIconImage: image];
            log::info!("已重新设置 Dock 图标");
        }

        if activate {
            let _: () = objc::msg_send![app, activateIgnoringOtherApps: true];
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn apply_dock_visibility(visible: bool, activate: bool) {
    use cocoa::appkit::{NSApp, NSApplication, NSApplicationActivationPolicy};
    use objc::runtime::Object;

    unsafe {
        let app: *mut Object = NSApp();

        if visible {
            // 显示 Dock 图标: 切换回 Regular 策略
            app.setActivationPolicy_(
                NSApplicationActivationPolicy::NSApplicationActivationPolicyRegular,
            );

            // 切换 ActivationPolicy 后主动重载图标，避免启动后 Dock 残留旧图标缓存
            refresh_dock_icon(activate);
        } else {
            // 隐藏 Dock 图标: 切换到 Accessory 策略
            app.setActivationPolicy_(
                NSApplicationActivationPolicy::NSApplicationActivationPolicyAccessory,
            );
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn apply_dock_visibility(_visible: bool, _activate: bool) {}

/// 获取应用图标（Base64 PNG）
/// 返回应用的图标，如果获取失败返回空字符串
#[tauri::command]
pub async fn get_app_icon(
    app_name: String,
    executable_path: Option<String>,
) -> Result<String, AppError> {
    get_app_icon_impl(&app_name, executable_path.as_deref()).await
}

#[cfg(any(target_os = "macos", test))]
fn normalize_macos_app_lookup_name(value: &str) -> String {
    let trimmed = value.trim().trim_end_matches(".app");
    let mut normalized = String::new();
    let mut last_was_space = false;

    for ch in trimmed.chars().flat_map(|c| c.to_lowercase()) {
        if ch.is_alphanumeric() {
            normalized.push(ch);
            last_was_space = false;
        } else if !last_was_space {
            normalized.push(' ');
            last_was_space = true;
        }
    }

    normalized.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(any(target_os = "macos", test))]
fn push_normalized_macos_lookup_name(target: &mut Vec<String>, value: &str) {
    let normalized = normalize_macos_app_lookup_name(value);
    if normalized.is_empty() || target.iter().any(|existing| existing == &normalized) {
        return;
    }
    target.push(normalized);
}

#[cfg(any(target_os = "macos", test))]
fn macos_lookup_name_variants(value: &str) -> Vec<String> {
    const ALIAS_GROUPS: &[&[&str]] = &[&["腾讯视频", "QQLive", "Tencent Video"]];

    let normalized = normalize_macos_app_lookup_name(value);
    if normalized.is_empty() {
        return Vec::new();
    }

    let mut variants = Vec::new();
    push_normalized_macos_lookup_name(&mut variants, value);

    for group in ALIAS_GROUPS {
        if !group
            .iter()
            .any(|alias| normalize_macos_app_lookup_name(alias) == normalized)
        {
            continue;
        }

        for alias in *group {
            push_normalized_macos_lookup_name(&mut variants, alias);
        }
    }

    variants
}

#[cfg(any(target_os = "macos", test))]
fn macos_significant_name_tokens(value: &str) -> Vec<String> {
    const STOPWORDS: &[&str] = &["app", "browser", "desktop", "helper", "tools"];

    let mut tokens = Vec::new();
    for token in normalize_macos_app_lookup_name(value).split_whitespace() {
        if token.len() < 2 || STOPWORDS.contains(&token) {
            continue;
        }
        if !tokens.iter().any(|existing| existing == token) {
            tokens.push(token.to_string());
        }
    }
    tokens
}

#[cfg(target_os = "macos")]
fn macos_bundle_path_from_executable(executable_path: &str) -> Option<PathBuf> {
    let path = Path::new(executable_path);
    for ancestor in path.ancestors() {
        let is_app_bundle = ancestor
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("app"))
            .unwrap_or(false);
        if is_app_bundle {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

#[cfg(any(target_os = "macos", test))]
fn score_normalized_macos_app_bundle_name(normalized_app: &str, normalized_bundle: &str) -> i32 {
    if normalized_app.is_empty() || normalized_bundle.is_empty() {
        return 0;
    }

    let mut score = 0;
    if normalized_app == normalized_bundle {
        score += 1000;
    } else if normalized_app.contains(normalized_bundle)
        || normalized_bundle.contains(normalized_app)
    {
        score += 500;
    }

    let app_tokens = macos_significant_name_tokens(normalized_app);
    let bundle_tokens = macos_significant_name_tokens(normalized_bundle);
    let overlap_count = bundle_tokens
        .iter()
        .filter(|token| app_tokens.iter().any(|candidate| candidate == *token))
        .count() as i32;
    score += overlap_count * 160;

    if let Some(first_token) = app_tokens.first() {
        if normalized_bundle.starts_with(first_token) {
            score += 80;
        }
    }

    score
}

#[cfg(any(target_os = "macos", test))]
fn macos_score_app_bundle_name(app_name: &str, bundle_name: &str) -> i32 {
    let app_variants = macos_lookup_name_variants(app_name);
    let bundle_variants = macos_lookup_name_variants(bundle_name);

    let mut best_score = 0;
    for normalized_app in &app_variants {
        for normalized_bundle in &bundle_variants {
            best_score = best_score.max(score_normalized_macos_app_bundle_name(
                normalized_app,
                normalized_bundle,
            ));
        }
    }

    best_score
}

#[cfg(target_os = "macos")]
fn collect_macos_app_bundles(root: &Path, depth: usize, bundles: &mut Vec<PathBuf>) {
    if depth == 0 || !root.exists() {
        return;
    }

    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let is_app_bundle = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("app"))
            .unwrap_or(false);
        if is_app_bundle {
            bundles.push(path);
            continue;
        }

        collect_macos_app_bundles(&path, depth.saturating_sub(1), bundles);
    }
}

#[cfg(target_os = "macos")]
fn macos_icon_app_path_candidates(app_name: &str, executable_path: Option<&str>) -> Vec<String> {
    let mut candidates: Vec<(i32, String)> = Vec::new();

    if let Some(path) = executable_path.and_then(macos_bundle_path_from_executable) {
        // 仅当 executable_path 解析出的 bundle 与 app_name 匹配时才赋予最高优先级。
        // 活动采集偶发写入脏数据（如浏览器活动的 executable_path 记录成 IDE 路径），
        // 若无条件信任，浏览器会错误显示编译器图标；不匹配时改用名称评分兜底。
        let bundle_name = path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if macos_score_app_bundle_name(app_name, bundle_name) > 0 {
            candidates.push((i32::MAX, path.to_string_lossy().to_string()));
        }
    }

    let mut search_roots = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
        PathBuf::from("/System/Applications/Utilities"),
    ];
    if let Some(home_dir) = dirs::home_dir() {
        search_roots.push(home_dir.join("Applications"));
    }

    let mut bundles = Vec::new();
    for root in search_roots {
        collect_macos_app_bundles(&root, 3, &mut bundles);
    }

    for bundle in bundles {
        let Some(bundle_name) = bundle.file_stem().and_then(|name| name.to_str()) else {
            continue;
        };
        let score = macos_score_app_bundle_name(app_name, bundle_name);
        if score <= 0 {
            continue;
        }
        candidates.push((score, bundle.to_string_lossy().to_string()));
    }

    candidates.sort_by(|(score_a, path_a), (score_b, path_b)| {
        score_b
            .cmp(score_a)
            .then_with(|| path_a.len().cmp(&path_b.len()))
            .then_with(|| path_a.cmp(path_b))
    });

    let mut deduped = Vec::new();
    for (_, path) in candidates {
        if deduped.iter().any(|existing| existing == &path) {
            continue;
        }
        deduped.push(path);
    }
    deduped
}

/// macOS 实现：使用 mdfind 获取应用图标（带磁盘缓存）
#[cfg(target_os = "macos")]
async fn get_app_icon_impl(
    app_name: &str,
    executable_path: Option<&str>,
) -> Result<String, AppError> {
    use std::path::Path;
    use std::process::Command;

    // 缓存目录：/tmp/work_review_icons_v2/
    // v2：失效历史缓存。旧版缓存 key 仅含 app_name，曾把"executable_path 与 app_name 不一致"
    // （如浏览器活动记录成 IDE 路径）的脏数据解析结果持久化，导致浏览器长期显示编译器图标。
    let cache_dir = Path::new("/tmp/work_review_icons_v2");
    if !cache_dir.exists() {
        let _ = std::fs::create_dir_all(cache_dir);
    }

    // 安全文件名：将空格和特殊字符替换为下划线
    let safe_name: String = app_name
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    let cache_file = cache_dir.join(format!("{safe_name}.b64"));

    // 检查缓存：如果缓存存在且非空，直接返回
    if cache_file.exists() {
        if let Ok(cached) = std::fs::read_to_string(&cache_file) {
            if !cached.is_empty() {
                log::debug!("从缓存读取图标: {app_name}");
                return Ok(cached);
            }
        }
    }

    let app_path = macos_icon_app_path_candidates(app_name, executable_path)
        .into_iter()
        .find(|candidate| Path::new(candidate).exists())
        .unwrap_or_default();

    if app_path.is_empty() {
        log::debug!("未找到应用路径: {app_name}");
        return Ok(String::new());
    }

    log::debug!("找到应用路径: {app_name} -> {app_path}");

    // 获取 Info.plist 中的图标文件名
    let info_plist = format!("{app_path}/Contents/Info.plist");
    let icon_name = if Path::new(&info_plist).exists() {
        // 使用 defaults read 读取 CFBundleIconFile
        let defaults_output = Command::new("defaults")
            .args(["read", &info_plist, "CFBundleIconFile"])
            .output();

        if let Ok(output) = defaults_output {
            if output.status.success() {
                let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
                // 确保有 .icns 扩展名
                if name.ends_with(".icns") {
                    name
                } else {
                    format!("{name}.icns")
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    // 构造图标文件路径
    let icns_path = if !icon_name.is_empty() {
        format!("{app_path}/Contents/Resources/{icon_name}")
    } else {
        // 尝试查找任何 .icns 文件
        let find_output = Command::new("find")
            .args([
                &format!("{app_path}/Contents/Resources"),
                "-name",
                "*.icns",
                "-maxdepth",
                "1",
            ])
            .output()
            .map_err(|e| AppError::Unknown(format!("查找图标失败: {e}")))?;

        String::from_utf8_lossy(&find_output.stdout)
            .lines()
            .next()
            .unwrap_or("")
            .to_string()
    };

    if icns_path.is_empty() || !Path::new(&icns_path).exists() {
        log::debug!("未找到图标文件: {app_name}");
        return Ok(String::new());
    }

    log::debug!("找到图标文件: {icns_path}");

    // 使用 sips 转换为 PNG
    let temp_png = format!(
        "/tmp/app_icon_{}_{}.png",
        app_name.replace(' ', "_"),
        std::process::id()
    );

    let sips_output = Command::new("sips")
        .args([
            "-s", "format", "png", "-Z", "128", &icns_path, "--out", &temp_png,
        ])
        .output();

    if let Ok(result) = sips_output {
        if result.status.success() {
            if let Ok(png_data) = std::fs::read(&temp_png) {
                let _ = std::fs::remove_file(&temp_png);
                let base64_str =
                    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &png_data);
                // 保存到缓存
                let _ = std::fs::write(&cache_file, &base64_str);
                log::debug!("图标已缓存: {} ({} bytes)", app_name, base64_str.len());
                return Ok(base64_str);
            }
        } else {
            log::debug!("sips 转换失败: {}", String::from_utf8_lossy(&result.stderr));
        }
    }

    let _ = std::fs::remove_file(&temp_png);
    Ok(String::new())
}

/// Windows 实现：使用 Shell API 获取高清应用图标
/// 优先提取 256x256 (JUMBO) 图标，降级到 48x48 (EXTRALARGE)，最后回退到 32x32
/// 带磁盘缓存，避免重复启动 PowerShell
#[cfg(any(target_os = "windows", test))]
fn sanitize_icon_cache_name(value: &str) -> String {
    let safe_name: String = value
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();

    if safe_name.is_empty() {
        "icon".to_string()
    } else {
        safe_name
    }
}

#[cfg(any(target_os = "windows", test))]
fn build_windows_icon_cache_key(app_name: &str, executable_path: Option<&str>) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let safe_name = sanitize_icon_cache_name(app_name);
    let Some(path) = executable_path
        .map(str::trim)
        .filter(|path| !path.is_empty())
    else {
        return safe_name;
    };

    let mut hasher = DefaultHasher::new();
    path.to_lowercase().hash(&mut hasher);
    format!("{safe_name}_{:016x}", hasher.finish())
}

#[cfg(any(target_os = "windows", test))]
fn merge_windows_icon_lookup_candidates(
    executable_path: Option<&str>,
    known_icon_paths: Vec<String>,
) -> Vec<String> {
    let mut candidates = Vec::new();
    let mut push_candidate = |value: &str| {
        let candidate = value.trim().trim_matches('"').replace('/', "\\");
        if !candidate.is_empty() && !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    };

    if let Some(path) = executable_path {
        push_candidate(path);
    }

    for path in known_icon_paths {
        push_candidate(&path);
    }

    candidates
}

#[cfg(target_os = "windows")]
fn windows_known_icon_paths(app_name: &str) -> Vec<String> {
    let trimmed = app_name
        .trim()
        .trim_end_matches(".exe")
        .trim_end_matches(".EXE")
        .trim();
    let normalized = trimmed.to_lowercase();
    let compact = normalized
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>();

    let program_files = std::env::var("ProgramFiles").unwrap_or_default();
    let program_files_x86 = std::env::var("ProgramFiles(x86)").unwrap_or_default();
    let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_default();
    let app_data = std::env::var("APPDATA").unwrap_or_default();
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".to_string());

    let mut paths = Vec::new();
    let mut push_path = |path: String| {
        if !path.is_empty() && !paths.contains(&path) {
            paths.push(path);
        }
    };

    match compact.as_str() {
        "explorer" | "fileexplorer" => {
            push_path(format!(r"{}\explorer.exe", windir));
        }
        "msedge" | "edge" | "microsoftedge" => {
            push_path(format!(
                r"{}\Microsoft\Edge\Application\msedge.exe",
                program_files_x86
            ));
            push_path(format!(
                r"{}\Microsoft\Edge\Application\msedge.exe",
                program_files
            ));
        }
        "chrome" | "googlechrome" => {
            push_path(format!(
                r"{}\Google\Chrome\Application\chrome.exe",
                program_files
            ));
            push_path(format!(
                r"{}\Google\Chrome\Application\chrome.exe",
                program_files_x86
            ));
        }
        "wechat" | "weixin" => {
            push_path(format!(r"{}\Tencent\WeChat\WeChat.exe", program_files_x86));
            push_path(format!(r"{}\Tencent\WeChat\WeChat.exe", program_files));
        }
        "wecom" | "wxwork" => {
            push_path(format!(r"{}\Tencent\WeCom\WXWork.exe", program_files_x86));
            push_path(format!(r"{}\Tencent\WeCom\WXWork.exe", program_files));
        }
        "obsidian" => {
            push_path(format!(
                r"{}\Programs\Obsidian\Obsidian.exe",
                local_app_data
            ));
        }
        "pixpin" => {
            push_path(format!(r"{}\PixPin\PixPin.exe", local_app_data));
        }
        "xshell" => {
            push_path(format!(
                r"{}\NetSarang Computer\7\Xshell.exe",
                program_files_x86
            ));
            push_path(format!(
                r"{}\NetSarang Computer\7\Xshell.exe",
                program_files
            ));
            push_path(format!(
                r"{}\NetSarang Computer\8\Xshell.exe",
                program_files_x86
            ));
            push_path(format!(
                r"{}\NetSarang Computer\8\Xshell.exe",
                program_files
            ));
        }
        "wechatappex" => {
            push_path(format!(
                r"{}\Tencent\WeChat\XPlugin\Plugins\WeChatAppEx\WeChatAppEx.exe",
                app_data
            ));
        }
        _ => {}
    }

    paths
}

#[cfg(target_os = "windows")]
fn encode_windows_icon_path(value: &str) -> Vec<u16> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(target_os = "windows")]
unsafe fn get_windows_icon_from_shell_image_list(
    path: &str,
    list_type: i32,
) -> Option<winapi::shared::windef::HICON> {
    use std::mem::zeroed;
    use std::ptr::null_mut;
    use winapi::ctypes::c_void;
    use winapi::um::commoncontrols::IImageList;
    use winapi::um::shellapi::{SHGetFileInfoW, SHGetImageList, SHFILEINFOW, SHGFI_SYSICONINDEX};
    use winapi::Interface;

    let wide_path = encode_windows_icon_path(path);
    let mut file_info: SHFILEINFOW = zeroed();
    let lookup_result = SHGetFileInfoW(
        wide_path.as_ptr(),
        0,
        &mut file_info,
        std::mem::size_of::<SHFILEINFOW>() as u32,
        SHGFI_SYSICONINDEX,
    );
    if lookup_result == 0 {
        return None;
    }

    let mut image_list: *mut IImageList = null_mut();
    let hr = SHGetImageList(
        list_type,
        &IImageList::uuidof(),
        &mut image_list as *mut _ as *mut *mut c_void,
    );
    if hr < 0 || image_list.is_null() {
        return None;
    }

    let mut icon = null_mut();
    let hr = (*image_list).GetIcon(file_info.iIcon, 0, &mut icon);
    (*image_list).Release();

    if hr < 0 || icon.is_null() {
        None
    } else {
        Some(icon)
    }
}

#[cfg(target_os = "windows")]
unsafe fn get_windows_associated_icon(path: &str) -> Option<winapi::shared::windef::HICON> {
    use std::ptr::null_mut;
    use winapi::shared::minwindef::WORD;
    use winapi::um::shellapi::ExtractAssociatedIconW;

    let mut wide_path = encode_windows_icon_path(path);
    if wide_path.len() < 260 {
        wide_path.resize(260, 0);
    }

    let mut icon_index: WORD = 0;
    let icon = ExtractAssociatedIconW(null_mut(), wide_path.as_mut_ptr(), &mut icon_index);
    if icon.is_null() {
        None
    } else {
        Some(icon)
    }
}

#[cfg(target_os = "windows")]
unsafe fn render_windows_icon_pixels(
    icon: winapi::shared::windef::HICON,
) -> Option<(Vec<u8>, u32, u32)> {
    const DI_NORMAL: u32 = 0x0003;

    use std::mem::zeroed;
    use std::ptr::{copy_nonoverlapping, null_mut, write_bytes};
    use winapi::shared::minwindef::UINT;
    use winapi::shared::windef::HGDIOBJ;
    use winapi::um::wingdi::{
        CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetObjectW, SelectObject,
        BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    };
    use winapi::um::winuser::{DrawIconEx, GetDC, GetIconInfo, ReleaseDC, ICONINFO};

    let mut icon_info: ICONINFO = zeroed();
    if GetIconInfo(icon, &mut icon_info) == 0 {
        return None;
    }

    let rendered = (|| {
        let source_bitmap = if !icon_info.hbmColor.is_null() {
            icon_info.hbmColor
        } else {
            icon_info.hbmMask
        };
        if source_bitmap.is_null() {
            return None;
        }

        let mut bitmap: BITMAP = zeroed();
        let get_object_result = GetObjectW(
            source_bitmap as *mut _,
            std::mem::size_of::<BITMAP>() as i32,
            &mut bitmap as *mut _ as *mut _,
        );
        if get_object_result == 0 {
            return None;
        }

        let width = bitmap.bmWidth.abs();
        let mut height = bitmap.bmHeight.abs();
        if icon_info.hbmColor.is_null() {
            height /= 2;
        }
        if width <= 0 || height <= 0 {
            return None;
        }

        let screen_dc = GetDC(null_mut());
        if screen_dc.is_null() {
            return None;
        }

        let mem_dc = CreateCompatibleDC(screen_dc);
        if mem_dc.is_null() {
            ReleaseDC(null_mut(), screen_dc);
            return None;
        }

        let mut bitmap_info: BITMAPINFO = zeroed();
        bitmap_info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bitmap_info.bmiHeader.biWidth = width;
        bitmap_info.bmiHeader.biHeight = -height;
        bitmap_info.bmiHeader.biPlanes = 1;
        bitmap_info.bmiHeader.biBitCount = 32;
        bitmap_info.bmiHeader.biCompression = BI_RGB;

        let mut dib_bits = null_mut();
        let dib = CreateDIBSection(
            screen_dc,
            &bitmap_info,
            DIB_RGB_COLORS as UINT,
            &mut dib_bits,
            null_mut(),
            0,
        );
        if dib.is_null() || dib_bits.is_null() {
            DeleteDC(mem_dc);
            ReleaseDC(null_mut(), screen_dc);
            return None;
        }

        let old_object = SelectObject(mem_dc, dib as HGDIOBJ);
        if old_object.is_null() {
            DeleteObject(dib as HGDIOBJ);
            DeleteDC(mem_dc);
            ReleaseDC(null_mut(), screen_dc);
            return None;
        }

        let pixel_len = width as usize * height as usize * 4;
        write_bytes(dib_bits as *mut u8, 0, pixel_len);

        let draw_result = DrawIconEx(mem_dc, 0, 0, icon, width, height, 0, null_mut(), DI_NORMAL);
        let mut pixels = None;
        if draw_result != 0 {
            let mut buffer = vec![0; pixel_len];
            copy_nonoverlapping(dib_bits as *const u8, buffer.as_mut_ptr(), pixel_len);
            pixels = Some((buffer, width as u32, height as u32));
        }

        SelectObject(mem_dc, old_object);
        DeleteObject(dib as HGDIOBJ);
        DeleteDC(mem_dc);
        ReleaseDC(null_mut(), screen_dc);
        pixels
    })();

    if !icon_info.hbmColor.is_null() {
        DeleteObject(icon_info.hbmColor as HGDIOBJ);
    }
    if !icon_info.hbmMask.is_null() {
        DeleteObject(icon_info.hbmMask as HGDIOBJ);
    }

    rendered
}

#[cfg(target_os = "windows")]
fn encode_windows_icon_base64(mut pixels: Vec<u8>, width: u32, height: u32) -> Option<String> {
    if width == 0 || height == 0 {
        return None;
    }

    for chunk in pixels.chunks_exact_mut(4) {
        chunk.swap(0, 2);
    }

    let image = image::RgbaImage::from_raw(width, height, pixels)?;
    let mut dynamic_image = image::DynamicImage::ImageRgba8(image);
    if width > 128 || height > 128 {
        dynamic_image = dynamic_image.resize_exact(128, 128, image::imageops::FilterType::Lanczos3);
    }

    let mut cursor = std::io::Cursor::new(Vec::new());
    dynamic_image
        .write_to(&mut cursor, image::ImageFormat::Png)
        .ok()?;

    Some(base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        cursor.into_inner(),
    ))
}

#[cfg(target_os = "windows")]
fn convert_windows_icon_to_base64(icon: winapi::shared::windef::HICON) -> Option<(String, u32)> {
    use winapi::um::winuser::DestroyIcon;

    let rendered = unsafe { render_windows_icon_pixels(icon) };
    unsafe {
        DestroyIcon(icon);
    }

    let (pixels, width, height) = rendered?;
    let encoded = encode_windows_icon_base64(pixels, width, height)?;
    Some((encoded, width.max(height)))
}

#[cfg(target_os = "windows")]
fn extract_windows_icon_base64(path: &str) -> Option<String> {
    use winapi::um::shellapi::{SHIL_EXTRALARGE, SHIL_JUMBO};

    let mut jumbo_fallback = None;

    if let Some(icon) = unsafe { get_windows_icon_from_shell_image_list(path, SHIL_JUMBO as i32) } {
        if let Some((encoded, size)) = convert_windows_icon_to_base64(icon) {
            if size >= 48 {
                return Some(encoded);
            }
            jumbo_fallback = Some(encoded);
        }
    }

    let mut extra_large_fallback = None;
    if let Some(icon) =
        unsafe { get_windows_icon_from_shell_image_list(path, SHIL_EXTRALARGE as i32) }
    {
        if let Some((encoded, size)) = convert_windows_icon_to_base64(icon) {
            if size >= 32 {
                return Some(encoded);
            }
            extra_large_fallback = Some(encoded);
        }
    }

    if let Some(icon) = unsafe { get_windows_associated_icon(path) } {
        if let Some((encoded, _)) = convert_windows_icon_to_base64(icon) {
            return Some(encoded);
        }
    }

    extra_large_fallback.or(jumbo_fallback)
}

#[cfg(target_os = "windows")]
async fn get_app_icon_impl(
    app_name: &str,
    executable_path: Option<&str>,
) -> Result<String, AppError> {
    const WINDOWS_ICON_CACHE_VERSION: &str = "v5";

    // 磁盘缓存：检查是否已有缓存
    let cache_dir = std::env::temp_dir().join("work_review_icons");
    let _ = std::fs::create_dir_all(&cache_dir);
    let cache_key = build_windows_icon_cache_key(
        &crate::monitor::normalize_display_app_name(app_name),
        executable_path,
    );
    let cache_file = cache_dir.join(format!("{cache_key}_{WINDOWS_ICON_CACHE_VERSION}.b64"));

    if cache_file.exists() {
        if let Ok(metadata) = std::fs::metadata(&cache_file) {
            // 缓存有效期 24 小时
            if let Ok(modified) = metadata.modified() {
                if modified.elapsed().unwrap_or_default().as_secs() < 86400 {
                    if let Ok(cached) = std::fs::read_to_string(&cache_file) {
                        if cached.len() > 100 {
                            return Ok(cached);
                        }
                    }
                }
            }
        }
    }

    let icon_lookup_candidates = merge_windows_icon_lookup_candidates(
        executable_path,
        windows_known_icon_paths(app_name)
            .into_iter()
            .filter(|path| std::path::Path::new(path).exists())
            .collect::<Vec<_>>(),
    );
    if icon_lookup_candidates.is_empty() {
        return Ok(String::new());
    }

    // 仅对明确的可执行路径提取图标，不扫描注册表、开始菜单快捷方式或全部运行进程。
    for candidate_path in icon_lookup_candidates {
        if !Path::new(&candidate_path).exists() {
            continue;
        }

        if let Some(base64_str) = extract_windows_icon_base64(&candidate_path) {
            if base64_str.len() > 100 {
                let _ = std::fs::write(&cache_file, &base64_str);
                return Ok(base64_str);
            }
        }
    }

    Ok(String::new())
}

/// 其他平台：返回空字符串
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
async fn get_app_icon_impl(
    _app_name: &str,
    _executable_path: Option<&str>,
) -> Result<String, AppError> {
    Ok(String::new())
}

/// 保存背景图片（接收 base64 编码的图片数据）
#[tauri::command]
pub async fn save_background_image(
    data: String,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), AppError> {
    let (data_dir, config_path) = {
        let s = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
        (s.data_dir.clone(), s.config_path.clone())
    };

    // 解码 base64
    let image_bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &data)
        .map_err(|e| AppError::Unknown(format!("base64 解码失败: {e}")))?;

    // 保存为 JPEG（压缩体积）
    let img = image::load_from_memory(&image_bytes)
        .map_err(|e| AppError::Unknown(format!("图片解析失败: {e}")))?;

    // 限制最大尺寸为 1920px 宽
    let img = if img.width() > 1920 {
        img.resize(1920, 1920, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };

    let bg_path = data_dir.join("background.jpg");
    img.save_with_format(&bg_path, image::ImageFormat::Jpeg)
        .map_err(|e| AppError::Unknown(format!("保存背景图失败: {e}")))?;

    // 更新配置
    let mut s = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
    s.config.background_image = Some("background.jpg".to_string());
    s.config.save(&config_path)?;

    Ok(())
}

/// 获取背景图片（返回 base64）
#[tauri::command]
pub async fn get_background_image(
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<Option<String>, AppError> {
    let (data_dir, bg_filename) = {
        let s = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
        (s.data_dir.clone(), s.config.background_image.clone())
    };

    let filename = match bg_filename {
        Some(f) if !f.is_empty() => f,
        _ => return Ok(None),
    };

    let bg_path = data_dir.join(&filename);
    if !bg_path.exists() {
        return Ok(None);
    }

    let bytes =
        std::fs::read(&bg_path).map_err(|e| AppError::Unknown(format!("读取背景图失败: {e}")))?;
    let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
    Ok(Some(b64))
}

/// 清除背景图片
#[tauri::command]
pub async fn clear_background_image(
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), AppError> {
    let (data_dir, config_path) = {
        let s = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
        (s.data_dir.clone(), s.config_path.clone())
    };

    // 删除文件
    let bg_path = data_dir.join("background.jpg");
    if bg_path.exists() {
        let _ = std::fs::remove_file(&bg_path);
    }

    // 更新配置
    let mut s = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
    s.config.background_image = None;
    s.config.save(&config_path)?;

    Ok(())
}



#[cfg(test)]
mod tests {
    use super::*;
    

    #[cfg(target_os = "macos")]
    #[test]
    fn mac_permission_settings_url_should_match_expected_panels() {
        assert_eq!(
            super::macos_permission_settings_url("screen_capture"),
            Some("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
        );
        assert_eq!(
            super::macos_permission_settings_url("accessibility"),
            Some("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        );
        assert_eq!(
            super::macos_permission_settings_url("input_monitoring"),
            Some("x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent")
        );
        assert_eq!(super::macos_permission_settings_url("unknown"), None);
    }

    #[test]
    fn windows图标候选应优先真实路径并去重() {
        let candidates = merge_windows_icon_lookup_candidates(
            Some(r"D:\Portable\Code\Code.exe"),
            vec![
                r"C:\Program Files\Microsoft VS Code\Code.exe".to_string(),
                r"D:\Portable\Code\Code.exe".to_string(),
                r"C:\Program Files\Microsoft VS Code\Code.exe".to_string(),
            ],
        );

        assert_eq!(
            candidates,
            vec![
                r"D:\Portable\Code\Code.exe".to_string(),
                r"C:\Program Files\Microsoft VS Code\Code.exe".to_string(),
            ]
        );
    }

    #[test]
    fn windows图标缓存key应包含真实路径特征() {
        let portable_key =
            build_windows_icon_cache_key("VS Code", Some(r"D:\Portable\Code\Code.exe"));
        let installed_key = build_windows_icon_cache_key(
            "VS Code",
            Some(r"C:\Program Files\Microsoft VS Code\Code.exe"),
        );

        assert_ne!(portable_key, installed_key);
        assert!(portable_key.starts_with("VS_Code_"));
        assert!(installed_key.starts_with("VS_Code_"));
    }

    #[test]
    fn macos图标名称归一化应兼容分隔符与后缀() {
        assert_eq!(
            normalize_macos_app_lookup_name("Zen Browser"),
            "zen browser"
        );
        assert_eq!(
            normalize_macos_app_lookup_name("antigravity_tools.app"),
            "antigravity tools"
        );
        assert_eq!(
            normalize_macos_app_lookup_name("Antigravity-Tools"),
            "antigravity tools"
        );
    }

    #[test]
    fn macos应用包名评分应兼容缩写与分隔符差异() {
        assert!(
            macos_score_app_bundle_name("Foo Browser", "Foo")
                > macos_score_app_bundle_name("Foo Browser", "Bar")
        );
        assert!(
            macos_score_app_bundle_name("antigravity_tools", "Antigravity")
                > macos_score_app_bundle_name("antigravity_tools", "Calculator")
        );
        assert!(
            macos_score_app_bundle_name("antigravity_tools", "Antigravity Tools")
                >= macos_score_app_bundle_name("antigravity_tools", "Antigravity")
        );
    }

    #[test]
    fn macos应用包名评分应兼容中文显示名与英文包名别名() {
        assert!(macos_score_app_bundle_name("腾讯视频", "QQLive") > 0);
        assert!(
            macos_score_app_bundle_name("腾讯视频", "QQLive")
                > macos_score_app_bundle_name("腾讯视频", "QQ")
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn 图标解析应忽略与app_name矛盾的executable_path() {
        // 回归：活动采集偶发写入脏数据（app_name 是浏览器，executable_path 却指向 IDE）。
        // 修复前 executable_path 无条件得 i32::MAX，浏览器会错误显示编译器图标。
        let cands = super::macos_icon_app_path_candidates(
            "Safari",
            Some("/Applications/Google Chrome.app"),
        );
        let first = cands.first();
        assert!(
            first.map(|p| p.contains("Safari.app")).unwrap_or(false),
            "浏览器不应因脏 executable_path 显示编译器图标, 实际首位: {:?}",
            first
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn 图标解析在executable_path匹配时仍优先使用它() {
        // executable_path 与 app_name 一致时应优先使用（最快、最准）。
        let cands = super::macos_icon_app_path_candidates(
            "Microsoft Edge",
            Some("/Applications/Microsoft Edge.app"),
        );
        assert_eq!(
            cands.first().map(|p| p.as_str()),
            Some("/Applications/Microsoft Edge.app")
        );
    }

}
