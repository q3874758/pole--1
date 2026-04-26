use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Windows,
    Linux,
    Macos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallMode {
    Installed,
    Portable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallLayout {
    pub root_dir: PathBuf,
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub log_dir: PathBuf,
    pub update_dir: PathBuf,
}

pub fn current_platform() -> Platform {
    #[cfg(target_os = "windows")]
    {
        Platform::Windows
    }
    #[cfg(target_os = "linux")]
    {
        Platform::Linux
    }
    #[cfg(target_os = "macos")]
    {
        Platform::Macos
    }
}

pub fn resolve_install_layout(
    platform: Platform,
    mode: InstallMode,
    root: impl AsRef<Path>,
) -> InstallLayout {
    let root_dir = root.as_ref().to_path_buf();
    match mode {
        InstallMode::Portable => portable_install_layout(root_dir),
        InstallMode::Installed => installed_install_layout(platform, root_dir),
    }
}

pub fn portable_layout_for_config(config_path: &Path) -> InstallLayout {
    let root_dir = config_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let data_dir = root_dir.join("pole-node-data");
    InstallLayout {
        root_dir: root_dir.clone(),
        config_dir: root_dir,
        log_dir: data_dir.join("logs"),
        update_dir: data_dir.join("updates"),
        data_dir,
    }
}

pub fn runtime_layout_for_config(config_path: &Path, configured_data_dir: &str) -> InstallLayout {
    let mut layout = portable_layout_for_config(config_path);
    layout.data_dir = resolve_runtime_data_dir(config_path, configured_data_dir);
    layout.log_dir = layout.data_dir.join("logs");
    layout.update_dir = layout.data_dir.join("updates");
    layout
}

pub fn resolve_runtime_data_dir(config_path: &Path, configured_data_dir: &str) -> PathBuf {
    if configured_data_dir.trim().is_empty() {
        return portable_layout_for_config(config_path).data_dir;
    }

    let configured_path = Path::new(configured_data_dir);
    if configured_path.is_absolute() {
        return normalize_path(configured_path.to_path_buf());
    }

    let base_dir = portable_layout_for_config(config_path).root_dir;
    normalize_path(base_dir.join(configured_path))
}

pub fn normalize_path(path: PathBuf) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                let can_pop = normalized
                    .components()
                    .next_back()
                    .map(|last| matches!(last, Component::Normal(_)))
                    .unwrap_or(false);
                if can_pop {
                    normalized.pop();
                } else {
                    normalized.push(component.as_os_str());
                }
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn portable_install_layout(root_dir: PathBuf) -> InstallLayout {
    let data_dir = root_dir.join("pole-node-data");
    InstallLayout {
        root_dir: root_dir.clone(),
        config_dir: root_dir,
        log_dir: data_dir.join("logs"),
        update_dir: data_dir.join("updates"),
        data_dir,
    }
}

fn installed_install_layout(platform: Platform, root_dir: PathBuf) -> InstallLayout {
    match platform {
        Platform::Windows => InstallLayout {
            root_dir: root_dir.clone(),
            config_dir: root_dir.join("config"),
            data_dir: root_dir.join("data"),
            log_dir: root_dir.join("logs"),
            update_dir: root_dir.join("updates"),
        },
        Platform::Linux => {
            let data_dir = PathBuf::from("/var/lib/pole");
            InstallLayout {
                root_dir,
                config_dir: PathBuf::from("/etc/pole"),
                log_dir: PathBuf::from("/var/log/pole"),
                update_dir: data_dir.join("updates"),
                data_dir,
            }
        }
        Platform::Macos => {
            let data_dir = PathBuf::from("/Library/Application Support/PoLE");
            InstallLayout {
                root_dir,
                config_dir: PathBuf::from("/Library/Application Support/PoLE/config"),
                log_dir: PathBuf::from("/Library/Logs/PoLE"),
                update_dir: data_dir.join("updates"),
                data_dir,
            }
        }
    }
}
