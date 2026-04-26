use pole_protocol_draft::{
    resolve_install_layout, InstallMode, Platform, WindowsServiceDefinition,
};
use serde_json::Value;
use std::path::PathBuf;

fn normalize_separators(path: &str) -> String {
    path.replace('\\', "/")
}

#[test]
fn windows_layout_manifest_matches_installed_layout_contract() {
    let manifest: Value =
        serde_json::from_str(include_str!("../packaging/windows/layout.json")).unwrap();
    let layout = resolve_install_layout(
        Platform::Windows,
        InstallMode::Installed,
        "C:/Program Files/PoLE",
    );

    assert_eq!(manifest["install_root"], "C:/Program Files/PoLE");
    assert_eq!(
        manifest["config_dir"],
        normalize_separators(layout.config_dir.to_string_lossy().as_ref())
    );
    assert_eq!(
        manifest["data_dir"],
        normalize_separators(layout.data_dir.to_string_lossy().as_ref())
    );
    assert_eq!(
        manifest["log_dir"],
        normalize_separators(layout.log_dir.to_string_lossy().as_ref())
    );
    assert_eq!(
        manifest["update_dir"],
        normalize_separators(layout.update_dir.to_string_lossy().as_ref())
    );
}

#[test]
fn windows_layout_manifest_matches_default_service_registration_root() {
    let manifest: Value =
        serde_json::from_str(include_str!("../packaging/windows/layout.json")).unwrap();
    let definition = WindowsServiceDefinition::new(
        "C:/Program Files/PoLE/pole-node.exe",
        "C:/Program Files/PoLE/config/node.json",
    );

    assert_eq!(
        manifest["service_registration_dir"],
        normalize_separators(definition.service_root.to_string_lossy().as_ref())
    );
    assert_eq!(
        definition.registration_path(),
        PathBuf::from("C:/ProgramData/PoLE/services/PoLENode.service.json")
    );
}

#[test]
fn windows_dashboard_launcher_targets_control_api_open() {
    let launcher = include_str!("../packaging/windows/open-dashboard.cmd");

    assert!(launcher.contains("pole-client.exe"));
    assert!(launcher.contains("control-api-open"));
    assert!(launcher.contains("%POLE_ROOT%\\config\\node.json"));
}
