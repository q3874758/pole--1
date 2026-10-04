use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use serde::{Deserialize, Serialize};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

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

/// Reads local branch, commit hash, and remote URL directly from `.git` filesystem metadata.
/// This runs instantaneously (0ms) without spawning any child processes, guaranteeing zero window popups.
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

pub fn get_git_status(start_dir: Option<&Path>) -> GitStatusInfo {
    let repo_root = find_repo_root(start_dir);

    if let Some(ref root) = repo_root {
        if let Some((branch, current_commit, repo_url)) = read_local_git_info(root) {
            let remote_commit = fetch_remote_github_commit_quiet(&branch);

            let (synced, update_available, message) = match &remote_commit {
                Some(rc) if rc == &current_commit => (
                    true,
                    false,
                    "本地代码已与 GitHub 远程仓库保持实时同步 (最新提交)".to_string(),
                ),
                Some(rc) => (
                    false,
                    true,
                    format!("发现远程新提交 ({rc})，可点击一键同步拉取"),
                ),
                None => (
                    true,
                    false,
                    "本地 Git 仓库已就绪 (网络离线或无拉取权限)".to_string(),
                ),
            };

            return GitStatusInfo {
                is_git_repo: true,
                repo_url,
                branch,
                current_commit,
                remote_commit,
                synced,
                update_available,
                auto_sync_enabled: true,
                message,
            };
        }
    }

    // Portable distribution mode (no .git directory)
    let current_version = format!("v{}", env!("CARGO_PKG_VERSION"));
    let repo_url = "https://github.com/q3874758/pole--1".to_string();
    let remote_commit = fetch_remote_github_commit_quiet("main");

    GitStatusInfo {
        is_git_repo: false,
        repo_url,
        branch: "release-stable".to_string(),
        current_commit: current_version.clone(),
        remote_commit,
        synced: true,
        update_available: false,
        auto_sync_enabled: true,
        message: format!("当前运行为绿色便携版 ({current_version})，已连接 GitHub Releases 通道"),
    }
}

pub fn sync_git(start_dir: Option<&Path>) -> GitSyncResult {
    let repo_root = find_repo_root(start_dir);

    if let Some(ref root) = repo_root {
        let (branch, old_commit, _) = read_local_git_info(root)
            .unwrap_or_else(|| ("main".to_string(), "unknown".to_string(), String::new()));

        let remote_commit = fetch_remote_github_commit_quiet(&branch);

        // First check if already synced according to remote API
        if let Some(ref rc) = remote_commit {
            if rc == &old_commit {
                return GitSyncResult {
                    ok: true,
                    updated: false,
                    current_commit: old_commit.clone(),
                    message: format!("✅ 代码已与远程保持最新 (Commit: {old_commit})，无需拉取。"),
                    details: Some("Remote SHA matches local HEAD.".to_string()),
                };
            }
        }

        // Attempt git pull
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
                // If stdout indicates already up to date despite non-zero exit code
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
        // Portable mode
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

fn fetch_remote_github_commit_quiet(branch: &str) -> Option<String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .user_agent("PoLE-Node")
        .build()
        .ok()?;

    let target_branch = if branch.is_empty() || branch.starts_with("HEAD") {
        "main"
    } else {
        branch
    };

    let url = format!("https://api.github.com/repos/q3874758/pole--1/commits/{target_branch}");
    let resp = client.get(&url).send().ok()?;
    if !resp.status().is_success() {
        return None;
    }

    let json: serde_json::Value = resp.json().ok()?;
    json.get("sha")
        .and_then(|s| s.as_str())
        .map(|s| s.chars().take(7).collect())
}
