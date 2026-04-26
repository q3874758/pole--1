use std::path::PathBuf;
use std::thread;

use pole_protocol_draft::{
    collect_control_api_config, collect_control_api_logs, collect_control_api_meta,
    collect_control_api_status, collect_control_api_update, execute_control_api_service_action,
    execute_control_api_update_action, serve_control_api, update_control_api_config,
    ConfigUpdateRequest, NodeConfig, ServiceActionRequest, UpdateActionRequest,
};

fn temp_root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("pole-control-api-{name}-{}", std::process::id()))
}

#[test]
fn collect_status_reports_node_and_service_snapshot() {
    let root = temp_root("collect");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let mut config = NodeConfig::default();
    config.runtime.data_dir = root.join("pole-node-data").to_string_lossy().into_owned();
    config.save_json(&config_path).unwrap();
    std::fs::create_dir_all(&config.runtime.data_dir).unwrap();
    std::fs::write(
        PathBuf::from(&config.runtime.data_dir).join("daemon.pid"),
        std::process::id().to_string(),
    )
    .unwrap();

    let status = collect_control_api_status(&config_path).unwrap();
    assert_eq!(status.service.state, "running");
    assert_eq!(status.service.pid, Some(std::process::id()));
    assert_eq!(status.node.chain_id, "pole-local");
    assert_eq!(status.node.next_epoch_id, 1);

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn control_api_serves_status_endpoint() {
    let root = temp_root("serve");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let mut config = NodeConfig::default();
    config.runtime.data_dir = root.join("pole-node-data").to_string_lossy().into_owned();
    config.save_json(&config_path).unwrap();
    std::fs::create_dir_all(&config.runtime.data_dir).unwrap();

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let config_path_for_thread = config_path.clone();
    let handle = thread::spawn(move || {
        serve_control_api(listener, config_path_for_thread, Some(1)).unwrap();
    });

    let response = reqwest::blocking::get(format!("http://{addr}/api/status"))
        .unwrap()
        .text()
        .unwrap();
    handle.join().unwrap();

    assert!(response.contains("\"service\""));
    assert!(response.contains("\"node\""));
    assert!(response.contains("\"chain_id\":\"pole-local\""));

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn control_api_serves_dashboard_assets() {
    let root = temp_root("dashboard");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let mut config = NodeConfig::default();
    config.runtime.data_dir = root.join("pole-node-data").to_string_lossy().into_owned();
    config.save_json(&config_path).unwrap();
    std::fs::create_dir_all(&config.runtime.data_dir).unwrap();

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let config_path_for_thread = config_path.clone();
    let handle = thread::spawn(move || {
        serve_control_api(listener, config_path_for_thread, Some(3)).unwrap();
    });

    let index = reqwest::blocking::get(format!("http://{addr}/"))
        .unwrap()
        .text()
        .unwrap();
    let css = reqwest::blocking::get(format!("http://{addr}/app.css"))
        .unwrap()
        .text()
        .unwrap();
    let js = reqwest::blocking::get(format!("http://{addr}/app.js"))
        .unwrap()
        .text()
        .unwrap();
    handle.join().unwrap();

    assert!(index.contains("PoLE 控制台"));
    assert!(index.contains("概览"));
    assert!(index.contains("更新"));
    assert!(index.contains("安装根目录覆盖"));
    assert!(index.contains("使用已安装布局策略"));
    assert!(index.contains("安装前停止服务"));
    assert!(index.contains("安装后启动服务"));
    assert!(index.contains("回滚前停止服务"));
    assert!(index.contains("回滚后启动服务"));
    assert!(css.contains(".shell"));
    assert!(css.contains(".view-pane"));
    assert!(css.contains(".panel"));
    assert!(js.contains("refreshAll"));
    assert!(js.contains("syncActiveView"));
    assert!(js.contains("install_root_override"));
    assert!(js.contains("/api/status"));

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn collect_config_reports_basic_configuration_fields() {
    let root = temp_root("config");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let mut config = NodeConfig::default();
    config.runtime.data_dir = root.join("pole-node-data").to_string_lossy().into_owned();
    config.runtime.game_process_names = vec!["game.exe".into()];
    config.runtime.low_impact_mode = false;
    config.runtime.os_background_priority = false;
    config.save_json(&config_path).unwrap();

    let response = collect_control_api_config(&config_path).unwrap();
    assert_eq!(response.config.chain_id, "pole-local");
    assert_eq!(response.config.reward_source, "static");
    assert_eq!(response.config.game_process_names, vec!["game.exe"]);
    assert!(!response.config.low_impact_mode);
    assert!(!response.config.os_background_priority);

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn collect_meta_reports_runtime_layout_and_version() {
    let root = temp_root("meta");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let mut config = NodeConfig::default();
    config.runtime.data_dir = root.join("pole-node-data").to_string_lossy().into_owned();
    config.save_json(&config_path).unwrap();

    let response = collect_control_api_meta(&config_path).unwrap();
    assert_eq!(response.app.app_name, "PoLE");
    assert_eq!(response.app.app_version, env!("CARGO_PKG_VERSION"));
    assert_eq!(response.app.control_api_default_bind_addr, "127.0.0.1:8787");
    assert_eq!(
        response.app.install_layout.config_dir,
        root.to_string_lossy()
    );
    assert!(response
        .app
        .install_layout
        .log_dir
        .contains("pole-node-data"));

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn collect_update_reports_update_contract_defaults() {
    let root = temp_root("update-meta");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let mut config = NodeConfig::default();
    config.runtime.data_dir = root.join("pole-node-data").to_string_lossy().into_owned();
    config.save_json(&config_path).unwrap();

    let response = collect_control_api_update(&config_path).unwrap();
    assert_eq!(response.update.current_version, env!("CARGO_PKG_VERSION"));
    assert_eq!(response.update.channel, "stable");
    assert!(!response.update.update_available);
    assert_eq!(
        response.update.latest_available_version.as_deref(),
        Some(env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(response.update.signing_status, "manifest_signed");
    assert_eq!(response.update.rollback_status, "planned");
    assert_eq!(response.update.artifact_count, 2);
    assert_eq!(response.update.pending_target_version, None);
    assert_eq!(response.update.applied_target_version, None);
    assert_eq!(response.update.selected_artifact_kind, None);
    assert_eq!(response.update.selected_artifact_path, None);
    assert_eq!(response.update.executed_artifact_path, None);
    assert_eq!(response.update.planned_install_path, None);
    assert_eq!(response.update.planned_backup_path, None);
    assert_eq!(response.update.executed_install_path, None);
    assert_eq!(response.update.install_target_mode, None);
    assert_eq!(response.update.install_action_status, "not_planned");
    assert_eq!(response.update.switch_execution_status, "not_executed");
    assert_eq!(response.update.service_window_status, "safe_now");
    assert!(response
        .update
        .release_manifest_dir
        .contains("release-manifests"));
    assert!(response
        .update
        .latest_manifest_path
        .ends_with("stable.json"));

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn execute_update_action_stages_local_update_plan() {
    let root = temp_root("update-action");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let mut config = NodeConfig::default();
    config.runtime.data_dir = root.join("pole-node-data").to_string_lossy().into_owned();
    config.save_json(&config_path).unwrap();
    std::fs::create_dir_all(&config.runtime.data_dir).unwrap();

    let response =
        execute_control_api_update_action(&config_path, "stage", UpdateActionRequest::default())
            .unwrap();
    assert_eq!(response.action, "stage");
    assert_eq!(response.channel, "stable");
    assert_eq!(response.status, "up_to_date");
    assert!(response.pending_update_path.contains("pending-update.json"));
    assert!(response.rollback_path.contains("rollback.json"));
    assert!(response.switch_plan_path.contains("switch-plan.json"));
    assert!(response
        .switch_execution_path
        .contains("switch-executed.json"));
    assert!(response.install_action_path.contains("install-action.json"));
    assert!(response
        .install_execution_path
        .contains("install-executed.json"));
    assert!(response
        .installed_version_path
        .contains("installed-version.json"));

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn execute_update_action_applies_and_rolls_back_update_state() {
    let root = temp_root("update-action-state");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let mut config = NodeConfig::default();
    config.runtime.data_dir = root.join("pole-node-data").to_string_lossy().into_owned();
    config.save_json(&config_path).unwrap();
    std::fs::create_dir_all(&config.runtime.data_dir).unwrap();

    let stage =
        execute_control_api_update_action(&config_path, "stage", UpdateActionRequest::default())
            .unwrap();
    assert_eq!(stage.status, "up_to_date");

    let apply =
        execute_control_api_update_action(&config_path, "apply", UpdateActionRequest::default())
            .unwrap();
    assert_eq!(apply.status, "no_pending_update");

    let rollback =
        execute_control_api_update_action(&config_path, "rollback", UpdateActionRequest::default())
            .unwrap();
    assert_eq!(rollback.status, "no_rollback_available");

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn collect_logs_returns_existing_daemon_logs() {
    let root = temp_root("logs");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let data_dir = root.join("pole-node-data");
    let mut config = NodeConfig::default();
    config.runtime.data_dir = data_dir.to_string_lossy().into_owned();
    config.save_json(&config_path).unwrap();
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::write(data_dir.join("daemon.out.log"), "out-log").unwrap();
    std::fs::write(data_dir.join("daemon.err.log"), "err-log").unwrap();

    let response = collect_control_api_logs(&config_path).unwrap();
    assert_eq!(response.logs.len(), 2);
    assert!(response
        .logs
        .iter()
        .any(|entry| entry.text.contains("out-log")));
    assert!(response
        .logs
        .iter()
        .any(|entry| entry.text.contains("err-log")));

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn update_config_applies_safe_fields() {
    let root = temp_root("update");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let mut config = NodeConfig::default();
    config.runtime.data_dir = root.join("pole-node-data").to_string_lossy().into_owned();
    config.save_json(&config_path).unwrap();

    let response = update_control_api_config(
        &config_path,
        ConfigUpdateRequest {
            target_app_ids: Some(vec![730, 570]),
            game_process_names: Some(vec!["game.exe".into()]),
            low_impact_mode: Some(false),
            os_background_priority: Some(false),
            emission_year: Some(3),
            reward_source: Some("tokenomics".into()),
        },
    )
    .unwrap();

    assert_eq!(response.config.target_app_ids, vec![730, 570]);
    assert_eq!(response.config.game_process_names, vec!["game.exe"]);
    assert_eq!(response.config.emission_year, 3);
    assert_eq!(response.config.reward_source, "tokenomics");

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn control_api_serves_config_and_logs_endpoints() {
    let root = temp_root("serve-extra");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let data_dir = root.join("pole-node-data");
    let mut config = NodeConfig::default();
    config.runtime.data_dir = data_dir.to_string_lossy().into_owned();
    config.save_json(&config_path).unwrap();
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::write(data_dir.join("daemon.out.log"), "out-log").unwrap();

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let config_path_for_thread = config_path.clone();
    let handle = thread::spawn(move || {
        serve_control_api(listener, config_path_for_thread, Some(2)).unwrap();
    });

    let config_response = reqwest::blocking::get(format!("http://{addr}/api/config"))
        .unwrap()
        .text()
        .unwrap();
    assert!(config_response.contains("\"config\""));
    assert!(config_response.contains("\"chain_id\":\"pole-local\""));

    let logs_response = reqwest::blocking::get(format!("http://{addr}/api/logs"))
        .unwrap()
        .text()
        .unwrap();
    handle.join().unwrap();
    assert!(logs_response.contains("\"logs\""));
    assert!(logs_response.contains("out-log"));

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn control_api_serves_meta_endpoint() {
    let root = temp_root("serve-meta");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let mut config = NodeConfig::default();
    config.runtime.data_dir = root.join("pole-node-data").to_string_lossy().into_owned();
    config.save_json(&config_path).unwrap();
    std::fs::create_dir_all(&config.runtime.data_dir).unwrap();

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let config_path_for_thread = config_path.clone();
    let handle = thread::spawn(move || {
        serve_control_api(listener, config_path_for_thread, Some(1)).unwrap();
    });

    let response = reqwest::blocking::get(format!("http://{addr}/api/meta"))
        .unwrap()
        .text()
        .unwrap();
    handle.join().unwrap();

    assert!(response.contains("\"app\""));
    assert!(response.contains("\"app_name\":\"PoLE\""));
    assert!(response.contains("\"control_api_default_bind_addr\":\"127.0.0.1:8787\""));

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn control_api_serves_update_endpoint() {
    let root = temp_root("serve-update-meta");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let mut config = NodeConfig::default();
    config.runtime.data_dir = root.join("pole-node-data").to_string_lossy().into_owned();
    config.save_json(&config_path).unwrap();
    std::fs::create_dir_all(&config.runtime.data_dir).unwrap();

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let config_path_for_thread = config_path.clone();
    let handle = thread::spawn(move || {
        serve_control_api(listener, config_path_for_thread, Some(1)).unwrap();
    });

    let response = reqwest::blocking::get(format!("http://{addr}/api/update"))
        .unwrap()
        .text()
        .unwrap();
    handle.join().unwrap();

    assert!(response.contains("\"update\""));
    assert!(response.contains("\"channel\":\"stable\""));
    assert!(response.contains("\"signing_status\":\"manifest_signed\""));

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn control_api_serves_update_stage_endpoint() {
    let root = temp_root("serve-update-stage");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let mut config = NodeConfig::default();
    config.runtime.data_dir = root.join("pole-node-data").to_string_lossy().into_owned();
    config.save_json(&config_path).unwrap();
    std::fs::create_dir_all(&config.runtime.data_dir).unwrap();

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let config_path_for_thread = config_path.clone();
    let handle = thread::spawn(move || {
        serve_control_api(listener, config_path_for_thread, Some(1)).unwrap();
    });

    let client = reqwest::blocking::Client::new();
    let response = client
        .post(format!("http://{addr}/api/update/stage"))
        .header("Content-Type", "application/json")
        .body("{}")
        .send()
        .unwrap()
        .text()
        .unwrap();
    handle.join().unwrap();

    assert!(response.contains("\"action\":\"stage\""));
    assert!(response.contains("\"status\":\"up_to_date\""));

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn control_api_serves_update_apply_and_rollback_endpoints() {
    let root = temp_root("serve-update-actions");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let mut config = NodeConfig::default();
    config.runtime.data_dir = root.join("pole-node-data").to_string_lossy().into_owned();
    config.save_json(&config_path).unwrap();
    std::fs::create_dir_all(&config.runtime.data_dir).unwrap();

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let config_path_for_thread = config_path.clone();
    let handle = thread::spawn(move || {
        serve_control_api(listener, config_path_for_thread, Some(2)).unwrap();
    });

    let client = reqwest::blocking::Client::new();
    let apply = client
        .post(format!("http://{addr}/api/update/apply"))
        .header("Content-Type", "application/json")
        .body("{}")
        .send()
        .unwrap()
        .text()
        .unwrap();
    let rollback = client
        .post(format!("http://{addr}/api/update/rollback"))
        .header("Content-Type", "application/json")
        .body("{}")
        .send()
        .unwrap()
        .text()
        .unwrap();
    handle.join().unwrap();

    assert!(apply.contains("\"action\":\"apply\""));
    assert!(apply.contains("\"status\":\"no_pending_update\""));
    assert!(rollback.contains("\"action\":\"rollback\""));
    assert!(rollback.contains("\"status\":\"no_rollback_available\""));

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn control_api_serves_commit_install_endpoint() {
    let root = temp_root("serve-update-commit");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let mut config = NodeConfig::default();
    config.runtime.data_dir = root.join("pole-node-data").to_string_lossy().into_owned();
    config.save_json(&config_path).unwrap();
    std::fs::create_dir_all(&config.runtime.data_dir).unwrap();

    let update_dir = PathBuf::from(&config.runtime.data_dir).join("updates");
    let current_dir = update_dir.join("current");
    std::fs::create_dir_all(&current_dir).unwrap();
    std::fs::write(current_dir.join("artifact.bin"), b"artifact-bytes").unwrap();
    std::fs::write(
        update_dir.join("install-action.json"),
        serde_json::json!({
            "channel": "stable",
            "target_version": "0.2.0",
            "artifact_kind": "deb",
            "target_mode": "override_root",
            "staged_artifact_path": current_dir.join("artifact.bin").to_string_lossy().into_owned(),
            "target_install_path": "/opt/pole/pole-node",
            "backup_path": "/opt/pole/pole-node.bak",
            "strategy": "copy_then_swap",
            "planned_at_millis": 1
        })
        .to_string(),
    )
    .unwrap();

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let config_path_for_thread = config_path.clone();
    let handle = thread::spawn(move || {
        serve_control_api(listener, config_path_for_thread, Some(1)).unwrap();
    });

    let client = reqwest::blocking::Client::new();
    let install_root = root.join("install-root");
    std::fs::create_dir_all(&install_root).unwrap();
    let payload = serde_json::to_string(&UpdateActionRequest {
        channel: None,
        install_root_override: Some(install_root.to_string_lossy().into_owned()),
        installed_layout_root_override: None,
        use_installed_layout: false,
        allow_system_install_write: false,
        stop_service_before_install: false,
        start_service_after_install: false,
        stop_service_before_rollback: false,
        start_service_after_rollback: false,
        systemd_unit_root: None,
        systemctl_binary: None,
        windows_service_root: None,
        windows_sc_binary: None,
    })
    .unwrap();
    let response = client
        .post(format!("http://{addr}/api/update/commit-install"))
        .header("Content-Type", "application/json")
        .body(payload)
        .send()
        .unwrap()
        .text()
        .unwrap();
    handle.join().unwrap();

    assert!(response.contains("\"action\":\"commit-install\""));
    assert!(response.contains("\"status\":\"install_executed\""));
    assert!(response.contains("\"install_execution_path\":\""));
    assert!(install_root.join("pole-node").exists());

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn control_api_serves_commit_install_with_installed_layout_override() {
    let root = temp_root("serve-update-commit-layout");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let mut config = NodeConfig::default();
    config.runtime.data_dir = root.join("pole-node-data").to_string_lossy().into_owned();
    config.save_json(&config_path).unwrap();
    std::fs::create_dir_all(&config.runtime.data_dir).unwrap();

    let update_dir = PathBuf::from(&config.runtime.data_dir).join("updates");
    let current_dir = update_dir.join("current");
    std::fs::create_dir_all(&current_dir).unwrap();
    std::fs::write(current_dir.join("artifact.bin"), b"artifact-bytes").unwrap();
    std::fs::write(
        update_dir.join("install-action.json"),
        serde_json::json!({
            "channel": "stable",
            "target_version": "0.2.0",
            "artifact_kind": "deb",
            "target_mode": "installed_layout_override",
            "staged_artifact_path": current_dir.join("artifact.bin").to_string_lossy().into_owned(),
            "target_install_path": "/opt/pole/pole-node",
            "backup_path": "/opt/pole/pole-node.bak",
            "strategy": "copy_then_swap",
            "planned_at_millis": 1
        })
        .to_string(),
    )
    .unwrap();

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let config_path_for_thread = config_path.clone();
    let handle = thread::spawn(move || {
        serve_control_api(listener, config_path_for_thread, Some(1)).unwrap();
    });

    let client = reqwest::blocking::Client::new();
    let installed_root = root.join("installed-root");
    std::fs::create_dir_all(&installed_root).unwrap();
    let payload = serde_json::to_string(&UpdateActionRequest {
        channel: None,
        install_root_override: None,
        installed_layout_root_override: Some(installed_root.to_string_lossy().into_owned()),
        use_installed_layout: true,
        allow_system_install_write: false,
        stop_service_before_install: false,
        start_service_after_install: false,
        stop_service_before_rollback: false,
        start_service_after_rollback: false,
        systemd_unit_root: None,
        systemctl_binary: None,
        windows_service_root: None,
        windows_sc_binary: None,
    })
    .unwrap();
    let response = client
        .post(format!("http://{addr}/api/update/commit-install"))
        .header("Content-Type", "application/json")
        .body(payload)
        .send()
        .unwrap()
        .text()
        .unwrap();
    handle.join().unwrap();

    assert!(response.contains("\"action\":\"commit-install\""));
    assert!(response.contains("\"status\":\"install_executed\""));
    assert!(installed_root.join("pole-node").exists());

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn control_api_commit_install_can_stop_service_when_requested() {
    let root = temp_root("serve-update-commit-stop");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let mut config = NodeConfig::default();
    config.runtime.data_dir = root.join("pole-node-data").to_string_lossy().into_owned();
    config.save_json(&config_path).unwrap();
    std::fs::create_dir_all(&config.runtime.data_dir).unwrap();

    let update_dir = PathBuf::from(&config.runtime.data_dir).join("updates");
    let current_dir = update_dir.join("current");
    std::fs::create_dir_all(&current_dir).unwrap();
    std::fs::write(current_dir.join("artifact.bin"), b"artifact-bytes").unwrap();
    std::fs::write(
        update_dir.join("install-action.json"),
        serde_json::json!({
            "channel": "stable",
            "target_version": "0.2.0",
            "artifact_kind": "deb",
            "target_mode": "override_root",
            "staged_artifact_path": current_dir.join("artifact.bin").to_string_lossy().into_owned(),
            "target_install_path": "/opt/pole/pole-node",
            "backup_path": "/opt/pole/pole-node.bak",
            "strategy": "copy_then_swap",
            "planned_at_millis": 1
        })
        .to_string(),
    )
    .unwrap();

    #[cfg(not(windows))]
    let manager_root = root.join("systemd");
    #[cfg(windows)]
    let manager_root = root.join("windows-service");
    std::fs::create_dir_all(&manager_root).unwrap();
    #[cfg(not(windows))]
    std::fs::write(manager_root.join("pole-node.service"), "unit").unwrap();
    #[cfg(windows)]
    std::fs::write(manager_root.join("PoLENode.service.json"), "{}").unwrap();

    #[cfg(not(windows))]
    let control_binary = root.join("systemctl");
    #[cfg(windows)]
    let control_binary = root.join("sc.cmd");
    let control_log = root.join("control.log");

    #[cfg(not(windows))]
    {
        std::fs::write(
            &control_binary,
            format!(
                "#!/bin/sh\nif [ \"$1\" = \"is-active\" ]; then echo active; fi\nif [ \"$1\" = \"stop\" ] || [ \"$1\" = \"start\" ]; then echo \"$@\" >> \"{}\"; fi\nexit 0\n",
                control_log.display()
            ),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&control_binary).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&control_binary, perms).unwrap();
    }
    #[cfg(windows)]
    std::fs::write(
        &control_binary,
        format!(
            "@echo off\r\nif \"%1\"==\"query\" echo STATE              : 4  RUNNING\r\nif \"%1\"==\"stop\" echo %*>>\"{}\"\r\nif \"%1\"==\"start\" echo %*>>\"{}\"\r\nexit /b 0\r\n",
            control_log.display(),
            control_log.display()
        ),
    )
    .unwrap();

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let config_path_for_thread = config_path.clone();
    let handle = thread::spawn(move || {
        serve_control_api(listener, config_path_for_thread, Some(1)).unwrap();
    });

    let client = reqwest::blocking::Client::new();
    let install_root = root.join("install-root");
    std::fs::create_dir_all(&install_root).unwrap();
    let payload = serde_json::json!({
        "install_root_override": install_root.to_string_lossy().into_owned(),
        "installed_layout_root_override": null,
        "use_installed_layout": false,
        "allow_system_install_write": false,
        "stop_service_before_install": true,
        "start_service_after_install": true,
        "systemd_unit_root": if cfg!(not(windows)) { Some(manager_root.to_string_lossy().into_owned()) } else { None::<String> },
        "systemctl_binary": if cfg!(not(windows)) { Some(control_binary.to_string_lossy().into_owned()) } else { None::<String> },
        "windows_service_root": if cfg!(windows) { Some(manager_root.to_string_lossy().into_owned()) } else { None::<String> },
        "windows_sc_binary": if cfg!(windows) { Some(control_binary.to_string_lossy().into_owned()) } else { None::<String> }
    })
    .to_string();
    let response = client
        .post(format!("http://{addr}/api/update/commit-install"))
        .header("Content-Type", "application/json")
        .body(payload)
        .send()
        .unwrap()
        .text()
        .unwrap();
    handle.join().unwrap();

    assert!(response.contains("\"status\":\"install_executed_service_restarted\""));
    let log = std::fs::read_to_string(&control_log).unwrap();
    #[cfg(not(windows))]
    {
        assert!(log.contains("stop pole-node.service"));
        assert!(log.contains("start pole-node.service"));
    }
    #[cfg(windows)]
    {
        assert!(log.contains("stop PoLENode"));
        assert!(log.contains("start PoLENode"));
    }
}

#[test]
fn control_api_rollback_can_stop_and_restart_service_when_requested() {
    let root = temp_root("serve-update-rollback-service");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let mut config = NodeConfig::default();
    config.runtime.data_dir = root.join("pole-node-data").to_string_lossy().into_owned();
    config.save_json(&config_path).unwrap();
    std::fs::create_dir_all(&config.runtime.data_dir).unwrap();

    let update_dir = PathBuf::from(&config.runtime.data_dir).join("updates");
    std::fs::create_dir_all(&update_dir).unwrap();
    std::fs::write(
        update_dir.join("rollback.json"),
        serde_json::json!({
            "previous_version": "0.1.0",
            "channel": "stable",
            "manifest_path": "stable.json",
            "recorded_at_millis": 1
        })
        .to_string(),
    )
    .unwrap();

    #[cfg(not(windows))]
    let manager_root = root.join("systemd");
    #[cfg(windows)]
    let manager_root = root.join("windows-service");
    std::fs::create_dir_all(&manager_root).unwrap();
    #[cfg(not(windows))]
    std::fs::write(manager_root.join("pole-node.service"), "unit").unwrap();
    #[cfg(windows)]
    std::fs::write(manager_root.join("PoLENode.service.json"), "{}").unwrap();

    #[cfg(not(windows))]
    let control_binary = root.join("systemctl");
    #[cfg(windows)]
    let control_binary = root.join("sc.cmd");
    let control_log = root.join("control.log");

    #[cfg(not(windows))]
    {
        std::fs::write(
            &control_binary,
            format!(
                "#!/bin/sh\nif [ \"$1\" = \"is-active\" ]; then echo active; exit 0; fi\nif [ \"$1\" = \"stop\" ] || [ \"$1\" = \"start\" ]; then echo \"$@\" >> \"{}\"; exit 0; fi\nexit 0\n",
                control_log.display()
            ),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&control_binary).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&control_binary, perms).unwrap();
    }
    #[cfg(windows)]
    std::fs::write(
        &control_binary,
        format!(
            "@echo off\r\nif \"%1\"==\"query\" echo STATE              : 4  RUNNING\r\nif \"%1\"==\"stop\" echo %*>>\"{}\"\r\nif \"%1\"==\"start\" echo %*>>\"{}\"\r\nexit /b 0\r\n",
            control_log.display(),
            control_log.display()
        ),
    )
    .unwrap();

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let config_path_for_thread = config_path.clone();
    let handle = thread::spawn(move || {
        serve_control_api(listener, config_path_for_thread, Some(1)).unwrap();
    });

    let client = reqwest::blocking::Client::new();
    let payload = serde_json::json!({
        "stop_service_before_rollback": true,
        "start_service_after_rollback": true,
        "systemd_unit_root": if cfg!(not(windows)) { Some(manager_root.to_string_lossy().into_owned()) } else { None::<String> },
        "systemctl_binary": if cfg!(not(windows)) { Some(control_binary.to_string_lossy().into_owned()) } else { None::<String> },
        "windows_service_root": if cfg!(windows) { Some(manager_root.to_string_lossy().into_owned()) } else { None::<String> },
        "windows_sc_binary": if cfg!(windows) { Some(control_binary.to_string_lossy().into_owned()) } else { None::<String> }
    })
    .to_string();
    let response = client
        .post(format!("http://{addr}/api/update/rollback"))
        .header("Content-Type", "application/json")
        .body(payload)
        .send()
        .unwrap()
        .text()
        .unwrap();
    handle.join().unwrap();

    assert!(response.contains("\"status\":\"rolled_back_service_restarted\""));
    let log = std::fs::read_to_string(&control_log).unwrap();
    #[cfg(not(windows))]
    {
        assert!(log.contains("stop pole-node.service"));
        assert!(log.contains("start pole-node.service"));
    }
    #[cfg(windows)]
    {
        assert!(log.contains("stop PoLENode"));
        assert!(log.contains("start PoLENode"));
    }
}

#[test]
fn control_api_commit_install_rolls_back_when_service_restart_fails() {
    let root = temp_root("serve-update-commit-restart-fail");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let mut config = NodeConfig::default();
    config.runtime.data_dir = root.join("pole-node-data").to_string_lossy().into_owned();
    config.save_json(&config_path).unwrap();
    std::fs::create_dir_all(&config.runtime.data_dir).unwrap();

    let update_dir = PathBuf::from(&config.runtime.data_dir).join("updates");
    let current_dir = update_dir.join("current");
    std::fs::create_dir_all(&current_dir).unwrap();
    std::fs::write(current_dir.join("artifact.bin"), b"artifact-bytes").unwrap();
    std::fs::write(
        update_dir.join("install-action.json"),
        serde_json::json!({
            "channel": "stable",
            "target_version": "0.2.0",
            "artifact_kind": "deb",
            "target_mode": "override_root",
            "staged_artifact_path": current_dir.join("artifact.bin").to_string_lossy().into_owned(),
            "target_install_path": "/opt/pole/pole-node",
            "backup_path": "/opt/pole/pole-node.bak",
            "strategy": "copy_then_swap",
            "planned_at_millis": 1
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        update_dir.join("rollback.json"),
        serde_json::json!({
            "previous_version": "0.1.0",
            "channel": "stable",
            "manifest_path": "stable.json",
            "recorded_at_millis": 1
        })
        .to_string(),
    )
    .unwrap();

    #[cfg(not(windows))]
    let manager_root = root.join("systemd");
    #[cfg(windows)]
    let manager_root = root.join("windows-service");
    std::fs::create_dir_all(&manager_root).unwrap();
    #[cfg(not(windows))]
    std::fs::write(manager_root.join("pole-node.service"), "unit").unwrap();
    #[cfg(windows)]
    std::fs::write(manager_root.join("PoLENode.service.json"), "{}").unwrap();

    #[cfg(not(windows))]
    let control_binary = root.join("systemctl");
    #[cfg(windows)]
    let control_binary = root.join("sc.cmd");
    let control_log = root.join("control.log");

    #[cfg(not(windows))]
    {
        std::fs::write(
            &control_binary,
            format!(
                "#!/bin/sh\nif [ \"$1\" = \"is-active\" ]; then echo active; exit 0; fi\nif [ \"$1\" = \"stop\" ]; then echo \"$@\" >> \"{}\"; exit 0; fi\nif [ \"$1\" = \"start\" ]; then echo \"$@\" >> \"{}\"; exit 1; fi\nexit 0\n",
                control_log.display(),
                control_log.display()
            ),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&control_binary).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&control_binary, perms).unwrap();
    }
    #[cfg(windows)]
    std::fs::write(
        &control_binary,
        format!(
            "@echo off\r\nif \"%1\"==\"query\" echo STATE              : 4  RUNNING\r\nif \"%1\"==\"stop\" echo %*>>\"{}\" & exit /b 0\r\nif \"%1\"==\"start\" echo %*>>\"{}\" & exit /b 1\r\nexit /b 0\r\n",
            control_log.display(),
            control_log.display()
        ),
    )
    .unwrap();

    let install_root = root.join("install-root");
    std::fs::create_dir_all(&install_root).unwrap();
    std::fs::write(install_root.join("pole-node"), b"old-version").unwrap();

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let config_path_for_thread = config_path.clone();
    let handle = thread::spawn(move || {
        serve_control_api(listener, config_path_for_thread, Some(1)).unwrap();
    });

    let client = reqwest::blocking::Client::new();
    let payload = serde_json::json!({
        "install_root_override": install_root.to_string_lossy().into_owned(),
        "installed_layout_root_override": null,
        "use_installed_layout": false,
        "allow_system_install_write": false,
        "stop_service_before_install": true,
        "start_service_after_install": true,
        "systemd_unit_root": if cfg!(not(windows)) { Some(manager_root.to_string_lossy().into_owned()) } else { None::<String> },
        "systemctl_binary": if cfg!(not(windows)) { Some(control_binary.to_string_lossy().into_owned()) } else { None::<String> },
        "windows_service_root": if cfg!(windows) { Some(manager_root.to_string_lossy().into_owned()) } else { None::<String> },
        "windows_sc_binary": if cfg!(windows) { Some(control_binary.to_string_lossy().into_owned()) } else { None::<String> }
    })
    .to_string();
    let response = client
        .post(format!("http://{addr}/api/update/commit-install"))
        .header("Content-Type", "application/json")
        .body(payload)
        .send()
        .unwrap()
        .text()
        .unwrap();
    handle.join().unwrap();

    assert!(response.contains("\"status\":\"install_executed_service_restart_failed_rolled_back\""));
    assert_eq!(
        std::fs::read(install_root.join("pole-node")).unwrap(),
        b"old-version"
    );
    assert!(!install_root.join("pole-node.bak").exists());
}

#[test]
fn control_api_updates_config_endpoint() {
    let root = temp_root("serve-update");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let data_dir = root.join("pole-node-data");
    let mut config = NodeConfig::default();
    config.runtime.data_dir = data_dir.to_string_lossy().into_owned();
    config.save_json(&config_path).unwrap();
    std::fs::create_dir_all(&data_dir).unwrap();

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let config_path_for_thread = config_path.clone();
    let handle = thread::spawn(move || {
        serve_control_api(listener, config_path_for_thread, Some(1)).unwrap();
    });

    let payload = serde_json::to_string(&ConfigUpdateRequest {
        target_app_ids: Some(vec![730, 570]),
        game_process_names: Some(vec!["game.exe".into()]),
        low_impact_mode: Some(false),
        os_background_priority: None,
        emission_year: None,
        reward_source: Some("static".into()),
    })
    .unwrap();
    let client = reqwest::blocking::Client::new();
    let response = client
        .post(format!("http://{addr}/api/config"))
        .header("Content-Type", "application/json")
        .body(payload)
        .send()
        .unwrap()
        .text()
        .unwrap();
    handle.join().unwrap();

    assert!(response.contains("\"config\""));
    assert!(response.contains("\"target_app_ids\":[730,570]"));
    assert!(response.contains("\"game_process_names\":[\"game.exe\"]"));

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn control_api_returns_not_found_for_unknown_route() {
    let root = temp_root("404");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let mut config = NodeConfig::default();
    config.runtime.data_dir = root.join("pole-node-data").to_string_lossy().into_owned();
    config.save_json(&config_path).unwrap();
    std::fs::create_dir_all(&config.runtime.data_dir).unwrap();

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let config_path_for_thread = config_path.clone();
    let handle = thread::spawn(move || {
        serve_control_api(listener, config_path_for_thread, Some(1)).unwrap();
    });

    let response = reqwest::blocking::get(format!("http://{addr}/missing"))
        .unwrap()
        .status();
    handle.join().unwrap();

    assert_eq!(response.as_u16(), 404);

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn execute_service_action_installs_and_uninstalls_service() {
    let root = temp_root("service-action");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let data_dir = root.join("pole-node-data");
    let mut config = NodeConfig::default();
    config.runtime.data_dir = data_dir.to_string_lossy().into_owned();
    config.save_json(&config_path).unwrap();

    #[cfg(windows)]
    let request = ServiceActionRequest {
        windows_service_root: Some(root.join("windows-service").to_string_lossy().into_owned()),
        windows_sc_binary: Some(root.join("sc.cmd").to_string_lossy().into_owned()),
        ..ServiceActionRequest::default()
    };
    #[cfg(not(windows))]
    let request = ServiceActionRequest {
        systemd_unit_root: Some(root.join("systemd").to_string_lossy().into_owned()),
        systemctl_binary: Some(root.join("systemctl").to_string_lossy().into_owned()),
        ..ServiceActionRequest::default()
    };

    #[cfg(windows)]
    std::fs::write(root.join("sc.cmd"), "@echo off\r\nexit /b 0\r\n").unwrap();
    #[cfg(not(windows))]
    {
        std::fs::write(root.join("systemctl"), "#!/bin/sh\nexit 0\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(root.join("systemctl"))
            .unwrap()
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(root.join("systemctl"), perms).unwrap();
    }

    let install =
        execute_control_api_service_action(&config_path, "install", request.clone()).unwrap();
    assert_eq!(install.action, "install");
    assert_eq!(install.status, "stopped");

    #[cfg(windows)]
    {
        let registration_path = root.join("windows-service").join("PoLENode.service.json");
        let registration = std::fs::read_to_string(&registration_path).unwrap();
        assert!(registration.contains("pole-node"));
    }
    #[cfg(not(windows))]
    {
        let unit_path = root.join("systemd").join("pole-node.service");
        let unit = std::fs::read_to_string(&unit_path).unwrap();
        assert!(unit.contains("pole-node service-run"));
    }

    let uninstall =
        execute_control_api_service_action(&config_path, "uninstall", request.clone()).unwrap();
    assert_eq!(uninstall.action, "uninstall");
    assert_eq!(uninstall.status, "not_installed");

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn control_api_serves_service_action_endpoint() {
    let root = temp_root("service-http");
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("client.json");
    let data_dir = root.join("pole-node-data");
    let mut config = NodeConfig::default();
    config.runtime.data_dir = data_dir.to_string_lossy().into_owned();
    config.save_json(&config_path).unwrap();

    #[cfg(windows)]
    let payload = serde_json::to_string(&ServiceActionRequest {
        windows_service_root: Some(root.join("windows-service").to_string_lossy().into_owned()),
        windows_sc_binary: Some(root.join("sc.cmd").to_string_lossy().into_owned()),
        ..ServiceActionRequest::default()
    })
    .unwrap();
    #[cfg(not(windows))]
    let payload = serde_json::to_string(&ServiceActionRequest {
        systemd_unit_root: Some(root.join("systemd").to_string_lossy().into_owned()),
        systemctl_binary: Some(root.join("systemctl").to_string_lossy().into_owned()),
        ..ServiceActionRequest::default()
    })
    .unwrap();

    #[cfg(windows)]
    std::fs::write(root.join("sc.cmd"), "@echo off\r\nexit /b 0\r\n").unwrap();
    #[cfg(not(windows))]
    {
        std::fs::write(root.join("systemctl"), "#!/bin/sh\nexit 0\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(root.join("systemctl"))
            .unwrap()
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(root.join("systemctl"), perms).unwrap();
    }

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let config_path_for_thread = config_path.clone();
    let handle = thread::spawn(move || {
        serve_control_api(listener, config_path_for_thread, Some(1)).unwrap();
    });

    let client = reqwest::blocking::Client::new();
    let response = client
        .post(format!("http://{addr}/api/service/install"))
        .header("Content-Type", "application/json")
        .body(payload)
        .send()
        .unwrap()
        .text()
        .unwrap();
    handle.join().unwrap();

    assert!(response.contains("\"action\":\"install\""));
    assert!(response.contains("\"status\":\"stopped\""));

    std::fs::remove_dir_all(&root).unwrap();
}
