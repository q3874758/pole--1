use std::path::PathBuf;

use pole_protocol_draft::{
    ManagedServiceStatus, ServiceManager, WindowsServiceDefinition, WindowsServiceManager,
    WINDOWS_SERVICE_NAME,
};
use serde_json::json;

fn temp_root(name: &str) -> PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "pole-test-{name}-{}-{id}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn windows_service_definition_renders_binary_path() {
    let definition = WindowsServiceDefinition::new(
        "C:/Program Files/PoLE/pole-node.exe",
        "C:/Program Files/PoLE/config/node.json",
    );

    assert_eq!(definition.service_name, WINDOWS_SERVICE_NAME);
    assert_eq!(
        definition.binary_path(),
        "\"C:/Program Files/PoLE/pole-node.exe\" service-run \"C:/Program Files/PoLE/config/node.json\""
    );
    assert_eq!(
        definition.sc_create_command(),
        "sc.exe create PoLENode binPath= \"C:/Program Files/PoLE/pole-node.exe\" service-run \"C:/Program Files/PoLE/config/node.json\" DisplayName= PoLE Node Service"
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&definition.render_registration_payload())
            .unwrap(),
        json!({
            "service_name": "PoLENode",
            "display_name": "PoLE Node Service",
            "binary_path": "\"C:/Program Files/PoLE/pole-node.exe\" service-run \"C:/Program Files/PoLE/config/node.json\""
        })
    );
}

#[test]
fn packaged_windows_service_payload_matches_default_rendering() {
    let definition = WindowsServiceDefinition::new(
        "C:/Program Files/PoLE/pole-node.exe",
        "C:/Program Files/PoLE/config/node.json",
    );
    let packaged = include_str!("../packaging/windows/pole-node-service.json");

    assert_eq!(
        serde_json::from_str::<serde_json::Value>(packaged).unwrap(),
        serde_json::from_str::<serde_json::Value>(&definition.render_registration_payload())
            .unwrap()
    );
}

#[test]
fn packaged_windows_service_scripts_match_cli_contract() {
    let install = include_str!("../packaging/windows/install-service.cmd");
    let uninstall = include_str!("../packaging/windows/uninstall-service.cmd");
    let start = include_str!("../packaging/windows/start-service.cmd");
    let stop = include_str!("../packaging/windows/stop-service.cmd");

    assert!(install.contains("pole-node.exe\" service-install "));
    assert!(uninstall.contains("pole-node.exe\" service-uninstall "));
    assert!(start.contains("pole-node.exe\" service-start "));
    assert!(stop.contains("pole-node.exe\" service-stop "));
    assert!(install.contains("C:\\Program Files\\PoLE\\config\\node.json"));
    assert!(uninstall.contains("C:\\Program Files\\PoLE\\config\\node.json"));
}

#[test]
fn service_managers_default_to_not_installed_status() {
    let root = temp_root("service-platform");

    let windows = WindowsServiceManager::new(
        WindowsServiceDefinition::new(
            "C:/Program Files/PoLE/pole-node.exe",
            "C:/Program Files/PoLE/config/node.json",
        )
        .with_service_root(root.join("windows-services")),
    );

    assert_eq!(
        windows.status().unwrap(),
        ManagedServiceStatus::NotInstalled
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn windows_manager_install_and_uninstall_track_registration_file() {
    let root = temp_root("windows-service");

    let sc_binary = root.join("sc.cmd");
    std::fs::write(
        &sc_binary,
        "@echo off\r\nif \"%1\"==\"query\" echo STATE              : 1  STOPPED\r\necho %*>>\"%~dp0sc.log\"\r\nexit /b 0\r\n",
    )
    .unwrap();

    let definition = WindowsServiceDefinition::new(
        "C:/Program Files/PoLE/pole-node.exe",
        "C:/Program Files/PoLE/config/node.json",
    )
    .with_service_root(&root)
    .with_sc_binary(&sc_binary);
    let registration_path = definition.registration_path();
    let manager = WindowsServiceManager::new(definition);

    assert_eq!(
        manager.status().unwrap(),
        ManagedServiceStatus::NotInstalled
    );
    manager.install().unwrap();
    assert!(registration_path.exists());
    assert_eq!(manager.status().unwrap(), ManagedServiceStatus::Stopped);

    manager.uninstall().unwrap();
    assert!(!registration_path.exists());
    assert_eq!(
        manager.status().unwrap(),
        ManagedServiceStatus::NotInstalled
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn windows_service_definition_renders_start_and_stop_commands() {
    let definition = WindowsServiceDefinition::new(
        "C:/Program Files/PoLE/pole-node.exe",
        "C:/Program Files/PoLE/config/node.json",
    );

    assert_eq!(definition.sc_start_command(), "sc.exe start PoLENode");
    assert_eq!(definition.sc_stop_command(), "sc.exe stop PoLENode");
    assert_eq!(definition.sc_query_command(), "sc.exe query PoLENode");
}

#[test]
fn windows_manager_start_and_stop_use_configured_binary() {
    let root = temp_root("windows-start-stop");

    let command_binary = root.join("sc.cmd");
    let log_path = root.join("sc.log");

    std::fs::write(
        &command_binary,
        "@echo off\r\necho %*>>\"%~dp0sc.log\"\r\nexit /b 0\r\n",
    )
    .unwrap();

    let definition = WindowsServiceDefinition::new(
        "C:/Program Files/PoLE/pole-node.exe",
        "C:/Program Files/PoLE/config/node.json",
    )
    .with_service_root(&root)
    .with_sc_binary(&command_binary);
    let manager = WindowsServiceManager::new(definition);
    manager.install().unwrap();
    manager.start().unwrap();
    manager.stop().unwrap();

    let log = std::fs::read_to_string(&log_path).unwrap();
    assert!(log.contains("start PoLENode"));
    assert!(log.contains("stop PoLENode"));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn windows_manager_status_uses_binary_output() {
    let root = temp_root("windows-status");

    let command_binary = root.join("sc.cmd");

    std::fs::write(
        &command_binary,
        "@echo off\r\nif \"%1\"==\"query\" echo STATE              : 4  RUNNING\r\nexit /b 0\r\n",
    )
    .unwrap();

    let definition = WindowsServiceDefinition::new(
        "C:/Program Files/PoLE/pole-node.exe",
        "C:/Program Files/PoLE/config/node.json",
    )
    .with_service_root(&root)
    .with_sc_binary(&command_binary);
    let manager = WindowsServiceManager::new(definition);
    manager.install().unwrap();

    assert_eq!(
        manager.status().unwrap(),
        ManagedServiceStatus::Running { pid: None }
    );

    let _ = std::fs::remove_dir_all(&root);
}
