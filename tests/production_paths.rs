use std::path::{Path, PathBuf};

use pole_protocol_draft::{
    default_data_dir_for_config, portable_layout_for_config, resolve_install_layout,
    resolve_runtime_data_dir, InstallMode, Platform,
};

#[test]
fn portable_layout_uses_config_directory_as_root() {
    let config_path = Path::new("E:/apps/pole/client.json");
    let layout = portable_layout_for_config(config_path);

    assert_eq!(layout.root_dir, PathBuf::from("E:/apps/pole"));
    assert_eq!(layout.config_dir, PathBuf::from("E:/apps/pole"));
    assert_eq!(
        layout.data_dir,
        PathBuf::from("E:/apps/pole/pole-node-data")
    );
    assert_eq!(
        layout.log_dir,
        PathBuf::from("E:/apps/pole/pole-node-data/logs")
    );
    assert_eq!(
        layout.update_dir,
        PathBuf::from("E:/apps/pole/pole-node-data/updates")
    );
}

#[test]
fn default_data_dir_matches_portable_layout_data_dir() {
    let config_path = Path::new("E:/apps/pole/client.json");
    let expected = portable_layout_for_config(config_path).data_dir;

    assert_eq!(
        PathBuf::from(default_data_dir_for_config(config_path)),
        expected
    );
}

#[test]
fn resolve_runtime_data_dir_uses_portable_root_for_relative_paths() {
    let config_path = Path::new("E:/apps/pole/client.json");
    let resolved = resolve_runtime_data_dir(config_path, "./custom-data");

    assert_eq!(resolved, PathBuf::from("E:/apps/pole/custom-data"));
}

#[test]
fn resolve_runtime_data_dir_uses_portable_default_for_empty_paths() {
    let config_path = Path::new("E:/apps/pole/client.json");
    let resolved = resolve_runtime_data_dir(config_path, "");

    assert_eq!(resolved, PathBuf::from("E:/apps/pole/pole-node-data"));
}

#[test]
fn windows_installed_layout_uses_app_local_subdirs() {
    let layout = resolve_install_layout(
        Platform::Windows,
        InstallMode::Installed,
        "C:/Program Files/PoLE",
    );

    assert_eq!(layout.root_dir, PathBuf::from("C:/Program Files/PoLE"));
    assert_eq!(
        layout.config_dir,
        PathBuf::from("C:/Program Files/PoLE/config")
    );
    assert_eq!(layout.data_dir, PathBuf::from("C:/Program Files/PoLE/data"));
    assert_eq!(layout.log_dir, PathBuf::from("C:/Program Files/PoLE/logs"));
    assert_eq!(
        layout.update_dir,
        PathBuf::from("C:/Program Files/PoLE/updates")
    );
}

#[test]
fn linux_installed_layout_uses_system_directories() {
    let layout = resolve_install_layout(Platform::Linux, InstallMode::Installed, "/opt/pole");

    assert_eq!(layout.root_dir, PathBuf::from("/opt/pole"));
    assert_eq!(layout.config_dir, PathBuf::from("/etc/pole"));
    assert_eq!(layout.data_dir, PathBuf::from("/var/lib/pole"));
    assert_eq!(layout.log_dir, PathBuf::from("/var/log/pole"));
    assert_eq!(layout.update_dir, PathBuf::from("/var/lib/pole/updates"));
}
