use std::path::PathBuf;

use pole_protocol_draft::{
    ManagedServiceStatus, ServiceManager, SystemdServiceManager, SystemdUnitDefinition,
    WindowsServiceDefinition, WindowsServiceManager, SYSTEMD_SERVICE_NAME, WINDOWS_SERVICE_NAME,
};
use serde_json::json;

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
fn systemd_unit_definition_renders_expected_unit_file() {
    let definition = SystemdUnitDefinition::new(
        "/opt/pole/pole-node",
        "/etc/pole/node.json",
        "/var/lib/pole",
    );
    let rendered = definition.render();

    assert_eq!(definition.service_name, SYSTEMD_SERVICE_NAME);
    assert!(rendered.contains("Description=PoLE node service"));
    assert!(rendered.contains("ExecStart=/opt/pole/pole-node service-run /etc/pole/node.json"));
    assert!(rendered.contains("WorkingDirectory=/var/lib/pole"));
    assert_eq!(
        definition.unit_path(),
        PathBuf::from("/etc/systemd/system/pole-node.service")
    );
    assert_eq!(
        definition.systemctl_start_command(),
        "systemctl start pole-node.service"
    );
    assert_eq!(
        definition.systemctl_stop_command(),
        "systemctl stop pole-node.service"
    );
    assert_eq!(
        definition.systemctl_status_command(),
        "systemctl is-active pole-node.service"
    );
}

#[test]
fn packaged_systemd_unit_matches_default_rendering() {
    let definition = SystemdUnitDefinition::new(
        "/opt/pole/pole-node",
        "/etc/pole/node.json",
        "/var/lib/pole",
    );
    let packaged = include_str!("../packaging/linux/deb/pole-node.service");

    assert_eq!(packaged, definition.render());
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
    let root = std::env::temp_dir().join(format!("pole-service-platform-{}", std::process::id()));
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let windows = WindowsServiceManager::new(
        WindowsServiceDefinition::new(
            "C:/Program Files/PoLE/pole-node.exe",
            "C:/Program Files/PoLE/config/node.json",
        )
        .with_service_root(root.join("windows-services")),
    );
    let linux = SystemdServiceManager::new(SystemdUnitDefinition::new(
        "/opt/pole/pole-node",
        "/etc/pole/node.json",
        "/var/lib/pole",
    )
    .with_unit_root(root.join("systemd")));

    assert_eq!(
        windows.status().unwrap(),
        ManagedServiceStatus::NotInstalled
    );
    assert_eq!(linux.status().unwrap(), ManagedServiceStatus::NotInstalled);

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn systemd_manager_install_and_uninstall_track_unit_file() {
    let root = std::env::temp_dir().join(format!("pole-systemd-{}", std::process::id()));
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let definition = SystemdUnitDefinition::new(
        "/opt/pole/pole-node",
        "/etc/pole/node.json",
        "/var/lib/pole",
    )
    .with_unit_root(&root);
    let unit_path = definition.unit_path();
    let manager = SystemdServiceManager::new(definition);

    assert_eq!(
        manager.status().unwrap(),
        ManagedServiceStatus::NotInstalled
    );
    manager.install().unwrap();
    assert!(unit_path.exists());
    assert_eq!(manager.status().unwrap(), ManagedServiceStatus::Stopped);

    manager.uninstall().unwrap();
    assert!(!unit_path.exists());
    assert_eq!(
        manager.status().unwrap(),
        ManagedServiceStatus::NotInstalled
    );

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn systemd_manager_start_and_stop_use_configured_binary() {
    let root = std::env::temp_dir().join(format!("pole-systemd-start-stop-{}", std::process::id()));
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    #[cfg(windows)]
    let command_binary = root.join("systemctl.cmd");
    #[cfg(not(windows))]
    let command_binary = root.join("systemctl");
    let log_path = root.join("systemctl.log");

    #[cfg(windows)]
    std::fs::write(
        &command_binary,
        format!(
            "@echo off\necho %*>>\"{}\"\nexit /b 0\n",
            log_path.display()
        ),
    )
    .unwrap();
    #[cfg(not(windows))]
    {
        std::fs::write(
            &command_binary,
            format!(
                "#!/bin/sh\necho \"$@\" >> \"{}\"\nexit 0\n",
                log_path.display()
            ),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&command_binary).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&command_binary, perms).unwrap();
    }

    let definition = SystemdUnitDefinition::new(
        "/opt/pole/pole-node",
        "/etc/pole/node.json",
        "/var/lib/pole",
    )
    .with_unit_root(&root)
    .with_systemctl_binary(&command_binary);
    let manager = SystemdServiceManager::new(definition.clone());
    manager.install().unwrap();
    manager.start().unwrap();
    manager.stop().unwrap();

    let log = std::fs::read_to_string(&log_path).unwrap();
    assert!(log.contains("start pole-node.service"));
    assert!(log.contains("stop pole-node.service"));

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn windows_manager_install_and_uninstall_track_registration_file() {
    let root = std::env::temp_dir().join(format!("pole-windows-service-{}", std::process::id()));
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let definition = WindowsServiceDefinition::new(
        "C:/Program Files/PoLE/pole-node.exe",
        "C:/Program Files/PoLE/config/node.json",
    )
    .with_service_root(&root);
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

    std::fs::remove_dir_all(&root).unwrap();
}

#[cfg(windows)]
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
    let root = std::env::temp_dir().join(format!("pole-windows-start-stop-{}", std::process::id()));
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    #[cfg(windows)]
    let command_binary = root.join("sc.cmd");
    #[cfg(not(windows))]
    let command_binary = root.join("sc");
    let log_path = root.join("sc.log");

    #[cfg(windows)]
    std::fs::write(
        &command_binary,
        format!(
            "@echo off\necho %*>>\"{}\"\nexit /b 0\n",
            log_path.display()
        ),
    )
    .unwrap();
    #[cfg(not(windows))]
    {
        std::fs::write(
            &command_binary,
            format!(
                "#!/bin/sh\necho \"$@\" >> \"{}\"\nexit 0\n",
                log_path.display()
            ),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&command_binary).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&command_binary, perms).unwrap();
    }

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

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn systemd_manager_status_uses_binary_output() {
    let root = std::env::temp_dir().join(format!("pole-systemd-status-{}", std::process::id()));
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    #[cfg(windows)]
    let command_binary = root.join("systemctl.cmd");
    #[cfg(not(windows))]
    let command_binary = root.join("systemctl");

    #[cfg(windows)]
    std::fs::write(
        &command_binary,
        "@echo off\r\nif \"%1\"==\"is-active\" echo active\r\nexit /b 0\r\n",
    )
    .unwrap();
    #[cfg(not(windows))]
    {
        std::fs::write(
            &command_binary,
            "#!/bin/sh\nif [ \"$1\" = \"is-active\" ]; then echo active; fi\nexit 0\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&command_binary).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&command_binary, perms).unwrap();
    }

    let definition = SystemdUnitDefinition::new(
        "/opt/pole/pole-node",
        "/etc/pole/node.json",
        "/var/lib/pole",
    )
    .with_unit_root(&root)
    .with_systemctl_binary(&command_binary);
    let manager = SystemdServiceManager::new(definition);
    manager.install().unwrap();

    assert_eq!(
        manager.status().unwrap(),
        ManagedServiceStatus::Running { pid: None }
    );

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn windows_manager_status_uses_binary_output() {
    let root = std::env::temp_dir().join(format!("pole-windows-status-{}", std::process::id()));
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    #[cfg(windows)]
    let command_binary = root.join("sc.cmd");
    #[cfg(not(windows))]
    let command_binary = root.join("sc");

    #[cfg(windows)]
    std::fs::write(
        &command_binary,
        "@echo off\r\nif \"%1\"==\"query\" echo STATE              : 4  RUNNING\r\nexit /b 0\r\n",
    )
    .unwrap();
    #[cfg(not(windows))]
    {
        std::fs::write(
            &command_binary,
            "#!/bin/sh\nif [ \"$1\" = \"query\" ]; then echo 'STATE              : 4  RUNNING'; fi\nexit 0\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&command_binary).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&command_binary, perms).unwrap();
    }

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

    std::fs::remove_dir_all(&root).unwrap();
}
