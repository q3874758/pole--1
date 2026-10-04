use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelOption {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateChannelInfo {
    pub is_git_repo: bool,
    pub current_version: String,
    pub active_channel: String,
    pub available_channels: Vec<ChannelOption>,
    pub runtime_mode: String,
    pub branch: String,
    pub current_commit: String,
    pub remote_version: String,
    pub remote_commit: Option<String>,
    pub update_available: bool,
    pub synced: bool,
    pub release_name: Option<String>,
    pub release_notes: Option<String>,
    pub download_url: Option<String>,
    pub html_url: String,
    pub last_checked: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitStatusInfo {
    pub is_git_repo: bool,
    pub repo_url: String,
    pub branch: String,
    pub current_commit: String,
    pub remote_commit: Option<String>,
    pub synced: bool,
    pub update_available: bool,
    pub auto_sync_enabled: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitSyncResult {
    pub ok: bool,
    pub updated: bool,
    pub current_commit: String,
    pub message: String,
    pub details: Option<String>,
}

struct CacheEntry {
    fetched_at: Instant,
    channel: String,
    info: UpdateChannelInfo,
}

static UPDATE_CACHE: Mutex<Option<CacheEntry>> = Mutex::new(None);

fn strip_unc_prefix(path: PathBuf) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(stripped) = s.strip_prefix(r"\\?\") {
        PathBuf::from(stripped)
    } else {
        path
    }
}

pub fn find_repo_root(start_dir: Option<&Path>) -> Option<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(p) = start_dir {
        if let Ok(abs) = fs::canonicalize(p) {
            let clean = strip_unc_prefix(abs);
            if clean.is_dir() {
                candidates.push(clean);
            } else if let Some(parent) = clean.parent() {
                candidates.push(parent.to_path_buf());
            }
        } else if let Some(parent) = p.parent() {
            if !parent.as_os_str().is_empty() {
                candidates.push(parent.to_path_buf());
            }
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(strip_unc_prefix(parent.to_path_buf()));
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(strip_unc_prefix(cwd));
    }

    for cand in candidates {
        let mut cur = cand;
        for _ in 0..8 {
            if cur.join(".git").exists() {
                if let Ok(abs) = fs::canonicalize(&cur) {
                    return Some(strip_unc_prefix(abs));
                }
                return Some(cur);
            }
            if !cur.pop() || cur.as_os_str().is_empty() {
                break;
            }
        }
    }
    None
}

/// Reads local branch, commit hash, and remote URL directly from `.git` filesystem metadata (0ms).
pub fn read_local_git_info(repo_root: &Path) -> Option<(String, String, String)> {
    let git_dir = repo_root.join(".git");
    let head_path = git_dir.join("HEAD");
    let head_content = fs::read_to_string(head_path).ok()?;
    let head_trimmed = head_content.trim();

    let (branch, full_commit) = if let Some(ref_path) = head_trimmed.strip_prefix("ref: ") {
        let branch_name = ref_path.trim_start_matches("refs/heads/").to_string();
        let ref_file = git_dir.join(ref_path);
        let commit = if let Ok(c) = fs::read_to_string(&ref_file) {
            c.trim().to_string()
        } else {
            find_commit_in_packed_refs(&git_dir, ref_path).unwrap_or_else(|| "unknown".to_string())
        };
        (branch_name, commit)
    } else {
        ("HEAD (detached)".to_string(), head_trimmed.to_string())
    };

    let commit_short = if full_commit.len() >= 7 {
        full_commit[..7].to_string()
    } else {
        full_commit
    };

    let mut remote_url = "https://github.com/q3874758/pole--1".to_string();
    if let Ok(cfg) = fs::read_to_string(git_dir.join("config")) {
        let mut in_origin = false;
        for line in cfg.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("[remote \"origin\"]") {
                in_origin = true;
                continue;
            } else if trimmed.starts_with('[') {
                in_origin = false;
            }
            if in_origin && trimmed.starts_with("url =") {
                let url = trimmed.trim_start_matches("url =").trim();
                if !url.is_empty() {
                    remote_url = url.to_string();
                    break;
                }
            }
        }
    }

    Some((branch, commit_short, remote_url))
}

fn find_commit_in_packed_refs(git_dir: &Path, ref_path: &str) -> Option<String> {
    let packed = fs::read_to_string(git_dir.join("packed-refs")).ok()?;
    for line in packed.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.starts_with('^') {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() == 2 && parts[1] == ref_path {
            return Some(parts[0].to_string());
        }
    }
    None
}

fn resolve_channel_file(start_dir: Option<&Path>) -> PathBuf {
    let base_dir = if let Some(p) = start_dir {
        if p.is_dir() {
            p.to_path_buf()
        } else if let Some(parent) = p.parent() {
            if !parent.as_os_str().is_empty() {
                parent.to_path_buf()
            } else {
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
            }
        } else {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        }
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    };

    let data_dir = base_dir.join("pole-node-data");
    if data_dir.is_dir() {
        data_dir.join("update-channel.txt")
    } else {
        base_dir.join("update-channel.txt")
    }
}

pub fn get_active_channel(start_dir: Option<&Path>) -> String {
    let path = resolve_channel_file(start_dir);
    if let Ok(c) = fs::read_to_string(&path) {
        let trimmed = c.trim().to_ascii_lowercase();
        if trimmed == "beta" || trimmed == "dev" || trimmed == "stable" {
            return trimmed;
        }
    }
    "stable".to_string()
}

pub fn set_active_channel(start_dir: Option<&Path>, channel: &str) -> UpdateChannelInfo {
    let normalized = match channel.to_ascii_lowercase().as_str() {
        "beta" => "beta",
        "dev" => "dev",
        _ => "stable",
    };
    let path = resolve_channel_file(start_dir);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&path, normalized);

    if let Ok(mut lock) = UPDATE_CACHE.lock() {
        *lock = None;
    }
    get_update_channel_status(start_dir, true)
}

pub fn default_available_channels() -> Vec<ChannelOption> {
    vec![
        ChannelOption {
            id: "stable".to_string(),
            name: "🌟 稳定通道 (Stable Releases)".to_string(),
            description: "推荐：接收经过严格验证的稳定发行版本，最适合日常游戏节点".to_string(),
        },
        ChannelOption {
            id: "beta".to_string(),
            name: "🧪 测试通道 (Beta Pre-releases)".to_string(),
            description: "尝鲜：优先获取即将发布的测试补丁与新功能预览".to_string(),
        },
        ChannelOption {
            id: "dev".to_string(),
            name: "⚡ 开发通道 (Git Main 实时提交)".to_string(),
            description: "极速：直接与 GitHub main 分支代码同步，第一时间体验最新特性".to_string(),
        },
    ]
}

pub fn get_update_channel_status(start_dir: Option<&Path>, force_refresh: bool) -> UpdateChannelInfo {
    let active_channel = get_active_channel(start_dir);

    if !force_refresh {
        if let Ok(lock) = UPDATE_CACHE.lock() {
            if let Some(ref entry) = *lock {
                if entry.channel == active_channel && entry.fetched_at.elapsed() < Duration::from_secs(300) {
                    return entry.info.clone();
                }
            }
        }
    }

    let info = check_update_for_channel(start_dir, &active_channel);
    if let Ok(mut lock) = UPDATE_CACHE.lock() {
        *lock = Some(CacheEntry {
            fetched_at: Instant::now(),
            channel: active_channel,
            info: info.clone(),
        });
    }
    info
}

fn check_update_for_channel(start_dir: Option<&Path>, channel: &str) -> UpdateChannelInfo {
    let repo_root = find_repo_root(start_dir);
    let is_git_repo = repo_root.is_some();
    let current_version = format!("v{}", env!("CARGO_PKG_VERSION"));
    let available_channels = default_available_channels();
    let runtime_mode = if is_git_repo {
        "Git 源码工作区".to_string()
    } else {
        "绿色便携免安装版".to_string()
    };

    let (branch, current_commit) = if let Some(ref root) = repo_root {
        if let Some((b, c, _)) = read_local_git_info(root) {
            (b, c)
        } else {
            ("main".to_string(), current_version.clone())
        }
    } else {
        ("main".to_string(), current_version.clone())
    };

    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let now_str = format!("{}:{:02}:{:02}", (now_secs / 3600) % 24, (now_secs / 60) % 60, now_secs % 60);

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(2500))
        .user_agent("PoLE-Node")
        .build();

    match channel {
        "dev" => {
            let mut remote_commit = None;
            if let Ok(ref c) = client {
                if let Ok(resp) = c.get("https://api.github.com/repos/q3874758/pole--1/commits/main").send() {
                    if resp.status().is_success() {
                        if let Ok(json) = resp.json::<serde_json::Value>() {
                            if let Some(sha) = json.get("sha").and_then(|s| s.as_str()) {
                                remote_commit = Some(sha.chars().take(7).collect::<String>());
                            }
                        }
                    }
                }
            }

            let (update_available, synced, message) = match &remote_commit {
                Some(rc) if rc == &current_commit => (
                    false,
                    true,
                    format!("本地已与 GitHub main 分支保持最新 (Commit: {rc})"),
                ),
                Some(rc) => (
                    true,
                    false,
                    format!("发现 GitHub main 分支新提交 ({rc})，可点击立即同步更新"),
                ),
                None => (
                    false,
                    true,
                    format!("已连接开发通道 (当前: {current_commit})"),
                ),
            };

            UpdateChannelInfo {
                is_git_repo,
                current_version,
                active_channel: "dev".to_string(),
                available_channels,
                runtime_mode,
                branch,
                current_commit,
                remote_version: remote_commit.clone().unwrap_or_else(|| "main".to_string()),
                remote_commit,
                update_available,
                synced,
                release_name: Some("GitHub main 分支实时提交".to_string()),
                release_notes: None,
                download_url: Some("https://github.com/q3874758/pole--1/archive/refs/heads/main.zip".to_string()),
                html_url: "https://github.com/q3874758/pole--1".to_string(),
                last_checked: now_str,
                message,
            }
        }
        "beta" => {
            let mut remote_tag = None;
            let mut release_name = None;
            let mut release_notes = None;
            let mut download_url = None;

            if let Ok(ref c) = client {
                if let Ok(resp) = c.get("https://api.github.com/repos/q3874758/pole--1/releases").send() {
                    if resp.status().is_success() {
                        if let Ok(json) = resp.json::<serde_json::Value>() {
                            if let Some(first) = json.as_array().and_then(|arr| arr.first()) {
                                remote_tag = first.get("tag_name").and_then(|s| s.as_str()).map(|s| s.to_string());
                                release_name = first.get("name").and_then(|s| s.as_str()).map(|s| s.to_string());
                                release_notes = first.get("body").and_then(|s| s.as_str()).map(|s| s.to_string());
                                if let Some(assets) = first.get("assets").and_then(|a| a.as_array()) {
                                    for asset in assets {
                                        if let Some(name) = asset.get("name").and_then(|n| n.as_str()) {
                                            if name.ends_with(".zip") {
                                                download_url = asset.get("browser_download_url").and_then(|u| u.as_str()).map(|u| u.to_string());
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            let remote_v = remote_tag.unwrap_or_else(|| current_version.clone());
            let update_available = is_version_higher(&remote_v, &current_version);
            let synced = !update_available;
            let message = if update_available {
                format!("测试通道发现新版本 {remote_v}，可体验最新特性！")
            } else {
                format!("当前已是测试通道最新版本 ({remote_v})")
            };

            UpdateChannelInfo {
                is_git_repo,
                current_version,
                active_channel: "beta".to_string(),
                available_channels,
                runtime_mode,
                branch,
                current_commit,
                remote_version: remote_v,
                remote_commit: None,
                update_available,
                synced,
                release_name,
                release_notes,
                download_url,
                html_url: "https://github.com/q3874758/pole--1/releases".to_string(),
                last_checked: now_str,
                message,
            }
        }
        _ => {
            // Stable channel
            let mut remote_tag = None;
            let mut release_name = None;
            let mut release_notes = None;
            let mut download_url = None;

            if let Ok(ref c) = client {
                if let Ok(resp) = c.get("https://api.github.com/repos/q3874758/pole--1/releases/latest").send() {
                    if resp.status().is_success() {
                        if let Ok(json) = resp.json::<serde_json::Value>() {
                            remote_tag = json.get("tag_name").and_then(|s| s.as_str()).map(|s| s.to_string());
                            release_name = json.get("name").and_then(|s| s.as_str()).map(|s| s.to_string());
                            release_notes = json.get("body").and_then(|s| s.as_str()).map(|s| s.to_string());
                            if let Some(assets) = json.get("assets").and_then(|a| a.as_array()) {
                                for asset in assets {
                                    if let Some(name) = asset.get("name").and_then(|n| n.as_str()) {
                                        if name.ends_with(".zip") {
                                            download_url = asset.get("browser_download_url").and_then(|u| u.as_str()).map(|u| u.to_string());
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            let remote_v = remote_tag.unwrap_or_else(|| current_version.clone());
            let update_available = is_version_higher(&remote_v, &current_version);
            let synced = !update_available;
            let message = if update_available {
                format!("稳定通道发现新版本 {remote_v}，包含重要功能与安全优化！")
            } else {
                format!("当前已是最新稳定版本 ({remote_v})")
            };

            UpdateChannelInfo {
                is_git_repo,
                current_version,
                active_channel: "stable".to_string(),
                available_channels,
                runtime_mode,
                branch,
                current_commit,
                remote_version: remote_v,
                remote_commit: None,
                update_available,
                synced,
                release_name,
                release_notes,
                download_url,
                html_url: "https://github.com/q3874758/pole--1/releases".to_string(),
                last_checked: now_str,
                message,
            }
        }
    }
}

fn is_version_higher(remote: &str, current: &str) -> bool {
    let clean_remote = remote.trim_start_matches('v').trim();
    let clean_current = current.trim_start_matches('v').trim();

    let r_parts: Vec<u64> = clean_remote.split('.').filter_map(|s| s.parse().ok()).collect();
    let c_parts: Vec<u64> = clean_current.split('.').filter_map(|s| s.parse().ok()).collect();

    for i in 0..std::cmp::max(r_parts.len(), c_parts.len()) {
        let r_val = r_parts.get(i).copied().unwrap_or(0);
        let c_val = c_parts.get(i).copied().unwrap_or(0);
        if r_val > c_val {
            return true;
        } else if r_val < c_val {
            return false;
        }
    }
    false
}

pub fn apply_update_channel(start_dir: Option<&Path>) -> GitSyncResult {
    let repo_root = find_repo_root(start_dir);
    if repo_root.is_some() {
        sync_git(start_dir)
    } else {
        // Portable mode: check if update is available
        let info = get_update_channel_status(start_dir, true);
        if info.update_available {
            GitSyncResult {
                ok: true,
                updated: false,
                current_commit: info.current_version.clone(),
                message: format!("🎉 发现新版本 {}！已获取下载链接，请下载解压后覆盖更新。", info.remote_version),
                details: info.download_url,
            }
        } else {
            GitSyncResult {
                ok: true,
                updated: false,
                current_commit: info.current_version.clone(),
                message: format!("✅ 当前版本 {} 已是最新，无需更新！", info.current_version),
                details: None,
            }
        }
    }
}

/// Compatibility wrapper for existing `/api/git/status` endpoint (0ms cached response).
pub fn get_git_status(start_dir: Option<&Path>) -> GitStatusInfo {
    let status = get_update_channel_status(start_dir, false);
    GitStatusInfo {
        is_git_repo: status.is_git_repo,
        repo_url: status.html_url,
        branch: status.branch,
        current_commit: status.current_commit,
        remote_commit: status.remote_commit,
        synced: status.synced,
        update_available: status.update_available,
        auto_sync_enabled: true,
        message: status.message,
    }
}

pub fn sync_git(start_dir: Option<&Path>) -> GitSyncResult {
    let repo_root = find_repo_root(start_dir);

    if let Some(ref root) = repo_root {
        let (branch, old_commit, _) = read_local_git_info(root)
            .unwrap_or_else(|| ("main".to_string(), "unknown".to_string(), String::new()));

        let mut pull_cmd = Command::new("git");
        pull_cmd.current_dir(root);
        pull_cmd.args(&["pull", "--ff-only", "origin", &branch]);
        pull_cmd.stdin(Stdio::null());
        pull_cmd.stdout(Stdio::piped());
        pull_cmd.stderr(Stdio::piped());

        #[cfg(windows)]
        {
            pull_cmd.creation_flags(CREATE_NO_WINDOW);
        }

        match pull_cmd.output() {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let new_info = read_local_git_info(root);
                let new_commit = new_info.map(|(_, c, _)| c).unwrap_or(old_commit.clone());
                let updated = new_commit != old_commit
                    || (!stdout.contains("Already up to date") && !stdout.contains("最新"));
                let message = if updated {
                    format!("🎉 同步成功！已更新至最新提交 {new_commit}")
                } else {
                    format!("✅ 代码已是最新 (Commit: {new_commit})，无需更新")
                };

                if let Ok(mut lock) = UPDATE_CACHE.lock() {
                    *lock = None;
                }

                GitSyncResult {
                    ok: true,
                    updated,
                    current_commit: new_commit,
                    message,
                    details: Some(stdout),
                }
            }
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let combined = if stderr.trim().is_empty() { stdout } else { stderr };
                if combined.contains("Already up to date") {
                    GitSyncResult {
                        ok: true,
                        updated: false,
                        current_commit: old_commit.clone(),
                        message: format!("✅ 代码已是最新 (Commit: {old_commit})，无需更新"),
                        details: Some(combined),
                    }
                } else {
                    GitSyncResult {
                        ok: false,
                        updated: false,
                        current_commit: old_commit,
                        message: format!("Git 同步未完成: {}", combined.trim()),
                        details: Some(combined),
                    }
                }
            }
            Err(e) => {
                GitSyncResult {
                    ok: false,
                    updated: false,
                    current_commit: old_commit,
                    message: format!("执行 git 进程失败: {e}"),
                    details: None,
                }
            }
        }
    } else {
        let current_version = format!("v{}", env!("CARGO_PKG_VERSION"));
        GitSyncResult {
            ok: true,
            updated: false,
            current_commit: current_version.clone(),
            message: format!("绿色便携版已连接 GitHub Releases 通道，当前版本 {current_version} 为最新。"),
            details: None,
        }
    }
}
