use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};

use crate::{
    ApiConfigResponse, ApiLogsResponse, ApiMetaResponse, ApiStatusResponse, ApiUpdateResponse,
    AppMetaView, ConfigUpdateRequest, ConfigView, InstallLayoutView, LogEntryView,
    ManagedServiceStatus, NodeConfig, NodeHealthView, RewardSourceMode, ServiceActionRequest,
    ServiceActionResponse, ServiceManager, ServiceRuntime, ServiceStatusView, UpdateActionRequest,
    UpdateActionResponse, UpdateStatusView,
};

const DASHBOARD_HTML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/desktop/web/index.html"
));
const DASHBOARD_CSS: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/desktop/web/app.css"));
const DASHBOARD_JS: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/desktop/web/app.js"));
const DEFAULT_CONTROL_API_BIND_ADDR: &str = "127.0.0.1:8787";

fn read_pid(path: &Path) -> Option<u32> {
    fs::read_to_string(path)
        .ok()
        .and_then(|content| content.lines().next().map(str::trim).map(str::to_string))
        .and_then(|content| content.parse::<u32>().ok())
}

fn process_is_running(pid: u32) -> bool {
    #[cfg(windows)]
    {
        let script = format!(
            "$p = Get-Process -Id {pid} -ErrorAction SilentlyContinue; if ($p) {{ exit 0 }} else {{ exit 1 }}"
        );
        std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &script,
            ])
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    #[cfg(not(windows))]
    {
        std::process::Command::new("sh")
            .args(["-c", &format!("kill -0 {pid} >/dev/null 2>&1")])
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
}

pub fn collect_status(
    config_path: impl AsRef<Path>,
) -> Result<ApiStatusResponse, Box<dyn std::error::Error>> {
    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path.as_ref())?;
    let summary = crate::load_status(&config)?;

    let data_dir = PathBuf::from(&config.runtime.data_dir);
    let daemon_pid = read_pid(&data_dir.join("daemon.pid"));
    let runtime = ServiceRuntime::from_observed_process(
        daemon_pid,
        daemon_pid.map(process_is_running).unwrap_or(false),
    );
    let service_snapshot = runtime.snapshot();

    Ok(ApiStatusResponse {
        service: ServiceStatusView {
            state: service_snapshot.state_label.to_string(),
            pid: service_snapshot.pid,
            stale: service_snapshot.stale,
            recoverable_without_manual_cleanup: service_snapshot.recoverable_without_manual_cleanup,
        },
        node: NodeHealthView {
            chain_id: config.chain_id,
            node_id: config.node_id_hex,
            reward_address: config.reward_address_hex,
            data_dir: config.runtime.data_dir,
            next_epoch_id: summary.next_epoch_id,
            next_slot_id: summary.next_slot_id,
            ticks_completed: summary.ticks_completed,
            low_impact_mode: summary.low_impact_mode,
            inline_verify_enabled: summary.inline_verify_enabled,
            inline_propose_enabled: summary.inline_propose_enabled,
        },
    })
}

pub fn collect_config(
    config_path: impl AsRef<Path>,
) -> Result<ApiConfigResponse, Box<dyn std::error::Error>> {
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path.as_ref())?;
    Ok(ApiConfigResponse {
        config: config_view_from_config(&config_path, config),
    })
}

pub fn collect_meta(
    config_path: impl AsRef<Path>,
) -> Result<ApiMetaResponse, Box<dyn std::error::Error>> {
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path.as_ref())?;
    let layout = crate::runtime_layout_for_config(&config_path, &config.runtime.data_dir);
    let platform = match crate::current_platform() {
        crate::Platform::Windows => "windows",
        crate::Platform::Linux => "linux",
        crate::Platform::Macos => "macos",
    };
    let service_manager = if cfg!(windows) {
        "windows-service"
    } else {
        "systemd"
    };

    Ok(ApiMetaResponse {
        app: AppMetaView {
            app_name: "PoLE".to_string(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            control_api_default_bind_addr: DEFAULT_CONTROL_API_BIND_ADDR.to_string(),
            remote_access_default_enabled: false,
            browser_open_supported: true,
            service_manager: service_manager.to_string(),
            install_layout: InstallLayoutView {
                platform: platform.to_string(),
                mode: "runtime".to_string(),
                root_dir: layout.root_dir.to_string_lossy().into_owned(),
                config_dir: layout.config_dir.to_string_lossy().into_owned(),
                data_dir: layout.data_dir.to_string_lossy().into_owned(),
                log_dir: layout.log_dir.to_string_lossy().into_owned(),
                update_dir: layout.update_dir.to_string_lossy().into_owned(),
            },
        },
    })
}

pub fn collect_update(
    config_path: impl AsRef<Path>,
) -> Result<ApiUpdateResponse, Box<dyn std::error::Error>> {
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path.as_ref())?;
    let layout = crate::runtime_layout_for_config(&config_path, &config.runtime.data_dir);
    let managed_status =
        build_managed_service_status(&config_path, &config, &ServiceActionRequest::default()).ok();
    let overview = crate::collect_update_overview_with_status(
        env!("CARGO_PKG_VERSION"),
        "stable",
        &layout.update_dir,
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("dist")
            .join("release-manifests"),
        managed_status,
    );

    Ok(ApiUpdateResponse {
        update: UpdateStatusView {
            current_version: overview.current_version,
            channel: overview.channel,
            update_dir: overview.update_dir.to_string_lossy().into_owned(),
            release_manifest_dir: overview.release_manifest_dir.to_string_lossy().into_owned(),
            latest_manifest_path: overview.latest_manifest_path.to_string_lossy().into_owned(),
            artifact_count: overview.artifact_count,
            update_available: overview.update_available,
            latest_available_version: overview.latest_available_version,
            pending_target_version: overview.pending_target_version,
            applied_target_version: overview.applied_target_version,
            selected_artifact_kind: overview.selected_artifact_kind,
            selected_artifact_path: overview.selected_artifact_path,
            executed_artifact_path: overview.executed_artifact_path,
            planned_install_path: overview.planned_install_path,
            planned_backup_path: overview.planned_backup_path,
            executed_install_path: overview.executed_install_path,
            install_target_mode: overview.install_target_mode,
            install_action_status: overview.install_action_status,
            switch_execution_status: overview.switch_execution_status,
            service_window_status: overview.service_window_status,
            signing_status: overview.signing_status,
            rollback_status: overview.rollback_status,
        },
    })
}

pub fn execute_update_action(
    config_path: impl AsRef<Path>,
    action: &str,
    request: UpdateActionRequest,
) -> Result<UpdateActionResponse, Box<dyn std::error::Error>> {
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path.as_ref())?;
    let layout = crate::runtime_layout_for_config(&config_path, &config.runtime.data_dir);
    let channel = request
        .channel
        .clone()
        .unwrap_or_else(|| "stable".to_string());
    let release_manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("dist")
        .join("release-manifests");
    let service_request = service_request_from_update_action(&request);
    let managed_status = build_managed_service_status(&config_path, &config, &service_request).ok();

    let result = match action {
        "stage" => crate::stage_update(
            env!("CARGO_PKG_VERSION"),
            &channel,
            &layout.update_dir,
            &release_manifest_dir,
        )?,
        "apply" => crate::apply_update_with_status(&layout.update_dir, managed_status)?,
        "commit-install" => {
            let service_was_active = matches!(
                managed_status,
                Some(
                    ManagedServiceStatus::Running { .. }
                        | ManagedServiceStatus::Starting { .. }
                        | ManagedServiceStatus::Failed { .. }
                )
            );
            let manager = (request.stop_service_before_install
                || request.start_service_after_install)
                .then(|| build_service_manager(&config_path, &config, &service_request))
                .transpose()?;
            if !request.stop_service_before_install && service_was_active {
                return Ok(UpdateActionResponse {
                    action: "commit-install".to_string(),
                    channel,
                    status: "service_stop_required".to_string(),
                    target_version: None,
                    pending_update_path: String::new(),
                    rollback_path: String::new(),
                    switch_plan_path: String::new(),
                    switch_execution_path: String::new(),
                    install_action_path: String::new(),
                    install_execution_path: String::new(),
                    installed_version_path: String::new(),
                });
            }
            if request.stop_service_before_install && service_was_active {
                if let Some(manager) = manager.as_ref() {
                    manager.stop()?;
                }
            }
            let mut result = crate::execute_install_action(
                &layout.update_dir,
                request.install_root_override.as_deref(),
                request.installed_layout_root_override.as_deref(),
                request.use_installed_layout,
                request.allow_system_install_write,
            )?;
            if request.start_service_after_install && result.status == "install_executed" {
                if let Some(manager) = manager.as_ref() {
                    if let Err(_err) = manager.start() {
                        let rollback_result = crate::rollback_update(&layout.update_dir)?;
                        result = rollback_result;
                        result.action = "commit-install".to_string();
                        result.status = if service_was_active && manager.start().is_ok() {
                            "install_executed_service_restart_failed_rolled_back_service_restarted"
                                .to_string()
                        } else {
                            "install_executed_service_restart_failed_rolled_back".to_string()
                        };
                    } else {
                        result.status = "install_executed_service_restarted".to_string();
                    }
                }
            }
            result
        }
        "rollback" => {
            let manager = (request.stop_service_before_rollback
                || request.start_service_after_rollback)
                .then(|| build_service_manager(&config_path, &config, &service_request))
                .transpose()?;
            let service_was_active = matches!(
                managed_status,
                Some(
                    ManagedServiceStatus::Running { .. }
                        | ManagedServiceStatus::Starting { .. }
                        | ManagedServiceStatus::Failed { .. }
                )
            );
            if !request.stop_service_before_rollback && service_was_active {
                return Ok(UpdateActionResponse {
                    action: "rollback".to_string(),
                    channel,
                    status: "service_stop_required".to_string(),
                    target_version: None,
                    pending_update_path: String::new(),
                    rollback_path: String::new(),
                    switch_plan_path: String::new(),
                    switch_execution_path: String::new(),
                    install_action_path: String::new(),
                    install_execution_path: String::new(),
                    installed_version_path: String::new(),
                });
            }
            if request.stop_service_before_rollback && service_was_active {
                if let Some(manager) = manager.as_ref() {
                    manager.stop()?;
                }
            }
            let mut result = crate::rollback_update(&layout.update_dir)?;
            if request.start_service_after_rollback && service_was_active {
                if let Some(manager) = manager.as_ref() {
                    if manager.start().is_ok() {
                        result.status = "rolled_back_service_restarted".to_string();
                    } else {
                        result.status = "rolled_back_service_restart_failed".to_string();
                    }
                }
            }
            result
        }
        other => return Err(format!("unknown update action {other}").into()),
    };

    Ok(UpdateActionResponse {
        action: result.action,
        channel: result.channel,
        status: result.status,
        target_version: result.target_version,
        pending_update_path: result.pending_update_path.to_string_lossy().into_owned(),
        rollback_path: result.rollback_path.to_string_lossy().into_owned(),
        switch_plan_path: result.switch_plan_path.to_string_lossy().into_owned(),
        switch_execution_path: result.switch_execution_path.to_string_lossy().into_owned(),
        install_action_path: result.install_action_path.to_string_lossy().into_owned(),
        install_execution_path: result.install_execution_path.to_string_lossy().into_owned(),
        installed_version_path: result.installed_version_path.to_string_lossy().into_owned(),
    })
}

fn config_view_from_config(config_path: &Path, config: NodeConfig) -> ConfigView {
    ConfigView {
        config_path: config_path.to_string_lossy().into_owned(),
        chain_id: config.chain_id,
        node_id: config.node_id_hex,
        reward_address: config.reward_address_hex,
        data_dir: config.runtime.data_dir,
        target_app_ids: config.runtime.target_app_ids,
        game_process_names: config.runtime.game_process_names,
        low_impact_mode: config.runtime.low_impact_mode,
        os_background_priority: config.runtime.os_background_priority,
        reward_source: config.reward.reward_source_label().to_string(),
        emission_year: config.reward.emission_year,
    }
}

pub fn update_config(
    config_path: impl AsRef<Path>,
    request: ConfigUpdateRequest,
) -> Result<ApiConfigResponse, Box<dyn std::error::Error>> {
    let (resolved_path, _) = NodeConfig::load_json_with_runtime_paths(config_path.as_ref())?;
    let mut config = NodeConfig::load_json(&resolved_path)?;

    if let Some(target_app_ids) = request.target_app_ids {
        config.runtime.target_app_ids = target_app_ids;
    }
    if let Some(game_process_names) = request.game_process_names {
        config.runtime.game_process_names = game_process_names;
    }
    if let Some(low_impact_mode) = request.low_impact_mode {
        config.runtime.low_impact_mode = low_impact_mode;
    }
    if let Some(os_background_priority) = request.os_background_priority {
        config.runtime.os_background_priority = os_background_priority;
    }
    if let Some(emission_year) = request.emission_year {
        config.reward.emission_year = emission_year;
    }
    if let Some(reward_source) = request.reward_source {
        config.reward.reward_source = match reward_source.as_str() {
            "static" => RewardSourceMode::Static,
            "tokenomics" => RewardSourceMode::Tokenomics,
            other => {
                return Err(format!(
                    "reward_source must be one of: static, tokenomics; got {other}"
                )
                .into())
            }
        };
    }

    config.save_json(&resolved_path)?;
    let (_resolved_path, normalized) = NodeConfig::load_json_with_runtime_paths(&resolved_path)?;
    Ok(ApiConfigResponse {
        config: config_view_from_config(&resolved_path, normalized),
    })
}

fn tail_text(path: &Path, max_chars: usize) -> String {
    let text = fs::read_to_string(path).unwrap_or_default();
    if text.len() <= max_chars {
        text
    } else {
        text[text.len().saturating_sub(max_chars)..].to_string()
    }
}

pub fn collect_logs(
    config_path: impl AsRef<Path>,
) -> Result<ApiLogsResponse, Box<dyn std::error::Error>> {
    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path.as_ref())?;
    let data_dir = PathBuf::from(&config.runtime.data_dir);
    let logs = [
        ("daemon_stdout", data_dir.join("daemon.out.log")),
        ("daemon_stderr", data_dir.join("daemon.err.log")),
    ]
    .into_iter()
    .filter(|(_, path)| path.exists())
    .map(|(source, path)| LogEntryView {
        source: source.to_string(),
        path: path.to_string_lossy().into_owned(),
        text: tail_text(&path, 8_192),
    })
    .collect();
    Ok(ApiLogsResponse { logs })
}

fn service_status_label(status: &crate::ManagedServiceStatus) -> &'static str {
    match status {
        crate::ManagedServiceStatus::NotInstalled => "not_installed",
        crate::ManagedServiceStatus::Stopped => "stopped",
        crate::ManagedServiceStatus::Starting { .. } => "starting",
        crate::ManagedServiceStatus::Running { .. } => "running",
        crate::ManagedServiceStatus::Failed { .. } => "failed",
    }
}

fn build_service_manager(
    config_path: &Path,
    _config: &NodeConfig,
    request: &ServiceActionRequest,
) -> Result<Box<dyn ServiceManager>, Box<dyn std::error::Error>> {
    let exe = resolve_service_executable_path()?;
    #[cfg(windows)]
    {
        let definition = crate::WindowsServiceDefinition::new(&exe, config_path);
        let definition = if let Some(service_root) = &request.windows_service_root {
            definition.with_service_root(service_root)
        } else {
            definition
        };
        let definition = if let Some(sc_binary) = &request.windows_sc_binary {
            definition.with_sc_binary(sc_binary)
        } else {
            definition
        };
        Ok(Box::new(crate::WindowsServiceManager::new(definition)))
    }
    #[cfg(not(windows))]
    {
        let definition =
            crate::SystemdUnitDefinition::new(&exe, config_path, &_config.runtime.data_dir);
        let definition = if let Some(unit_root) = &request.systemd_unit_root {
            definition.with_unit_root(unit_root)
        } else {
            definition
        };
        let definition = if let Some(systemctl_binary) = &request.systemctl_binary {
            definition.with_systemctl_binary(systemctl_binary)
        } else {
            definition
        };
        Ok(Box::new(crate::SystemdServiceManager::new(definition)))
    }
}

fn build_managed_service_status(
    config_path: &Path,
    config: &NodeConfig,
    request: &ServiceActionRequest,
) -> Result<ManagedServiceStatus, Box<dyn std::error::Error>> {
    let manager = build_service_manager(config_path, config, request)?;
    Ok(manager.status()?)
}

fn service_request_from_update_action(request: &UpdateActionRequest) -> ServiceActionRequest {
    ServiceActionRequest {
        systemd_unit_root: request.systemd_unit_root.clone(),
        systemctl_binary: request.systemctl_binary.clone(),
        windows_service_root: request.windows_service_root.clone(),
        windows_sc_binary: request.windows_sc_binary.clone(),
    }
}

fn resolve_service_executable_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let current = std::env::current_exe()?;
    let file_stem = current
        .file_stem()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if file_stem == "pole-node" {
        return Ok(current);
    }

    let extension = current
        .extension()
        .map(|value| format!(".{}", value.to_string_lossy()))
        .unwrap_or_default();
    let sibling = current
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!("pole-node{extension}"));
    Ok(sibling)
}

fn daemon_pid_path(config: &NodeConfig) -> PathBuf {
    PathBuf::from(&config.runtime.data_dir).join("daemon.pid")
}

fn kill_runtime_process(pid: u32) -> bool {
    #[cfg(windows)]
    {
        std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    #[cfg(not(windows))]
    {
        std::process::Command::new("sh")
            .args(["-c", &format!("kill -TERM {pid} >/dev/null 2>&1")])
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
}

fn spawn_user_mode_runtime(config_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let current_exe = std::env::current_exe()?;
    let config_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
    let mut command = std::process::Command::new(current_exe);
    command
        .arg("player-autostart")
        .arg(config_path)
        .current_dir(config_dir);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    command.spawn()?;
    Ok(())
}

fn execute_service_action_with_fallback(
    config_path: &Path,
    config: &NodeConfig,
    action: &str,
    manager: &dyn ServiceManager,
) -> Result<String, Box<dyn std::error::Error>> {
    match action {
        "install" => {
            manager.install()?;
            Ok(service_status_label(&manager.status()?).to_string())
        }
        "uninstall" => {
            manager.uninstall()?;
            Ok(service_status_label(&manager.status()?).to_string())
        }
        "status" => Ok(service_status_label(&manager.status()?).to_string()),
        "start" => match manager.start() {
            Ok(()) => Ok(service_status_label(&manager.status()?).to_string()),
            Err(err) if cfg!(windows) => {
                let message = err.to_string().to_ascii_lowercase();
                if message.contains("administrator privileges") {
                    spawn_user_mode_runtime(config_path)?;
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    let daemon_pid = read_pid(&daemon_pid_path(config));
                    let runtime = ServiceRuntime::from_observed_process(
                        daemon_pid,
                        daemon_pid.map(process_is_running).unwrap_or(false),
                    );
                    Ok(service_status_label(&runtime.managed_status()).to_string())
                } else {
                    Err(err.into())
                }
            }
            Err(err) => Err(err.into()),
        },
        "stop" => match manager.stop() {
            Ok(()) => Ok(service_status_label(&manager.status()?).to_string()),
            Err(err) if cfg!(windows) => {
                let message = err.to_string().to_ascii_lowercase();
                if message.contains("administrator privileges") {
                    let daemon_pid_path = daemon_pid_path(config);
                    if let Some(pid) = read_pid(&daemon_pid_path) {
                        if kill_runtime_process(pid) {
                            let _ = fs::remove_file(daemon_pid_path);
                            return Ok("stopped".to_string());
                        }
                    }
                }
                Err(err.into())
            }
            Err(err) => Err(err.into()),
        },
        other => Err(format!("unknown service action {other}").into()),
    }
}

pub fn execute_service_action(
    config_path: impl AsRef<Path>,
    action: &str,
    request: ServiceActionRequest,
) -> Result<ServiceActionResponse, Box<dyn std::error::Error>> {
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path.as_ref())?;
    let manager = build_service_manager(&config_path, &config, &request)?;
    let status = execute_service_action_with_fallback(&config_path, &config, action, manager.as_ref())?;
    Ok(ServiceActionResponse {
        action: action.to_string(),
        service_name: manager.service_name().to_string(),
        status,
    })
}

fn write_response(
    stream: &mut TcpStream,
    status_line: &str,
    content_type: &str,
    body: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    write!(
        stream,
        "{status_line}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )?;
    Ok(())
}

fn write_json_response(
    stream: &mut TcpStream,
    status_line: &str,
    body: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    write_response(stream, status_line, "application/json; charset=utf-8", body)
}

pub fn handle_connection(
    mut stream: TcpStream,
    config_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut buffer = [0u8; 4096];
    let bytes_read = stream.read(&mut buffer)?;
    let request = String::from_utf8_lossy(&buffer[..bytes_read]);
    let method = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().next())
        .unwrap_or("GET");
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");
    let path = path.split('?').next().unwrap_or(path);
    let body = request.split("\r\n\r\n").nth(1).unwrap_or("");

    match (method, path) {
        ("GET", "/") | ("GET", "/index.html") => {
            write_response(
                &mut stream,
                "HTTP/1.1 200 OK",
                "text/html; charset=utf-8",
                DASHBOARD_HTML,
            )?;
        }
        ("GET", "/app.css") => {
            write_response(
                &mut stream,
                "HTTP/1.1 200 OK",
                "text/css; charset=utf-8",
                DASHBOARD_CSS,
            )?;
        }
        ("GET", "/app.js") => {
            write_response(
                &mut stream,
                "HTTP/1.1 200 OK",
                "text/javascript; charset=utf-8",
                DASHBOARD_JS,
            )?;
        }
        ("GET", "/api/status") => {
            let body = serde_json::to_string(&collect_status(config_path)?)?;
            write_json_response(&mut stream, "HTTP/1.1 200 OK", &body)?;
        }
        ("GET", "/api/meta") => {
            let body = serde_json::to_string(&collect_meta(config_path)?)?;
            write_json_response(&mut stream, "HTTP/1.1 200 OK", &body)?;
        }
        ("GET", "/api/update") => {
            let body = serde_json::to_string(&collect_update(config_path)?)?;
            write_json_response(&mut stream, "HTTP/1.1 200 OK", &body)?;
        }
        ("POST", "/api/update/stage") => {
            let request = if body.trim().is_empty() {
                UpdateActionRequest::default()
            } else {
                serde_json::from_str::<UpdateActionRequest>(body)?
            };
            let body =
                serde_json::to_string(&execute_update_action(config_path, "stage", request)?)?;
            write_json_response(&mut stream, "HTTP/1.1 200 OK", &body)?;
        }
        ("POST", "/api/update/apply") | ("POST", "/api/update/rollback") => {
            let action = path.trim_start_matches("/api/update/");
            let request = if body.trim().is_empty() {
                UpdateActionRequest::default()
            } else {
                serde_json::from_str::<UpdateActionRequest>(body)?
            };
            let body =
                serde_json::to_string(&execute_update_action(config_path, action, request)?)?;
            write_json_response(&mut stream, "HTTP/1.1 200 OK", &body)?;
        }
        ("POST", "/api/update/commit-install") => {
            let request = if body.trim().is_empty() {
                UpdateActionRequest::default()
            } else {
                serde_json::from_str::<UpdateActionRequest>(body)?
            };
            let body = serde_json::to_string(&execute_update_action(
                config_path,
                "commit-install",
                request,
            )?)?;
            write_json_response(&mut stream, "HTTP/1.1 200 OK", &body)?;
        }
        ("GET", "/api/config") => {
            let body = serde_json::to_string(&collect_config(config_path)?)?;
            write_json_response(&mut stream, "HTTP/1.1 200 OK", &body)?;
        }
        ("POST", "/api/config") => {
            let request = serde_json::from_str::<ConfigUpdateRequest>(body)?;
            let body = serde_json::to_string(&update_config(config_path, request)?)?;
            write_json_response(&mut stream, "HTTP/1.1 200 OK", &body)?;
        }
        ("GET", "/api/logs") => {
            let body = serde_json::to_string(&collect_logs(config_path)?)?;
            write_json_response(&mut stream, "HTTP/1.1 200 OK", &body)?;
        }
        ("POST", "/api/service/install")
        | ("POST", "/api/service/uninstall")
        | ("POST", "/api/service/start")
        | ("POST", "/api/service/stop")
        | ("POST", "/api/service/status") => {
            let action = path.trim_start_matches("/api/service/");
            let request = if body.trim().is_empty() {
                ServiceActionRequest::default()
            } else {
                match serde_json::from_str::<ServiceActionRequest>(body) {
                    Ok(r) => r,
                    Err(e) => {
                        write_json_response(&mut stream, "HTTP/1.1 400 Bad Request", &format!("{{\"error\":\"invalid request: {}\"}}", e))?;
                        return Ok(());
                    }
                }
            };
match execute_service_action(config_path, action, request) {
                Ok(body) => {
                    let json = serde_json::to_string(&body)?;
                    write_json_response(&mut stream, "HTTP/1.1 200 OK", &json)?;
                }
                Err(e) => {
                    write_json_response(&mut stream, "HTTP/1.1 500 Internal Server Error", &format!("{{\"error\":\"{}\"}}", e))?;
                }
            }
        }
        _ => {
            write_json_response(
                &mut stream,
                "HTTP/1.1 404 Not Found",
                "{\"error\":\"not_found\"}",
            )?;
        }
    }

    Ok(())
}

pub fn serve(
    listener: TcpListener,
    config_path: PathBuf,
    max_requests: Option<usize>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut served = 0usize;
    for stream in listener.incoming() {
        handle_connection(stream?, &config_path)?;
        served += 1;
        if let Some(limit) = max_requests {
            if served >= limit {
                break;
            }
        }
    }
    Ok(())
}
