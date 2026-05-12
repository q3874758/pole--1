use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::{
    ApiBlockchainResponse, ApiConfigResponse, ApiDashboardResponse, ApiLogsResponse, ApiMetaResponse,
    ApiStatusResponse, ApiStorageResponse, ApiTokenomicsResponse, ApiUpdateResponse,
    AppMetaView, BlockchainStatusView, ChallengeActivityView, ConfigUpdateRequest, ConfigView,
    DashboardView, InstallLayoutView, LogEntryView, ManagedServiceStatus, NodeConfig,
    NodeHealthView, P2pNetworkView, RewardSourceMode, ServiceActionRequest,
    ServiceActionResponse, ServiceManager, ServiceRuntime, ServiceStatusView,
    StorageInfoView, TokenomicsSummaryView, UpdateActionRequest, UpdateActionResponse,
    UpdateStatusView, TOTAL_SUPPLY,
};

/// Maximum HTTP request size (headers + body) to prevent memory exhaustion.
const MAX_REQUEST_SIZE: usize = 65_536;

/// Per-connection timeout for reads.
const CONNECTION_TIMEOUT_SECS: u64 = 30;

/// Returns the API token from the POLE_API_TOKEN env var, or None if not set.
/// When a token is configured, all mutating POST requests require it.
fn read_api_token() -> Option<String> {
    std::env::var("POLE_API_TOKEN").ok().filter(|t| !t.is_empty())
}

/// Verify that the Authorization header contains the expected Bearer token.
/// Returns true if no token is configured (auth disabled), or if the token matches.
fn verify_auth_token(request_headers: &str, expected_token: &str) -> bool {
    for line in request_headers.lines() {
        if let Some(value) = line.strip_prefix("Authorization: Bearer ") {
            return value.trim() == expected_token;
        }
    }
    // Also check lowercase
    for line in request_headers.lines() {
        if let Some(value) = line.strip_prefix("authorization: Bearer ") {
            return value.trim() == expected_token;
        }
    }
    false
}

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

pub fn collect_blockchain(
    _config_path: impl AsRef<Path>,
) -> Result<ApiBlockchainResponse, Box<dyn std::error::Error>> {
    use std::net::TcpStream;

    let http_online = TcpStream::connect(("127.0.0.1", 1317)).is_ok();
    let grpc_online = TcpStream::connect(("127.0.0.1", 9090)).is_ok();

    let (block_height, block_hash, block_time, chain_id) = if http_online {
        match reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build()
        {
            Ok(client) => {
                match client.get("http://127.0.0.1:1317/cosmos/base/tendermint/v1beta1/blocks/latest").send() {
                    Ok(resp) => {
                        match resp.text() {
                            Ok(body) => {
                                match serde_json::from_str::<serde_json::Value>(&body) {
                                    Ok(json) => {
                                        let block = &json["block"];
                                        let block_id = &json["block_id"];
                                        let height = json["sdk_block_height"]
                                            .as_u64()
                                            .or_else(|| json["block"]["header"]["height"].as_u64())
                                            .unwrap_or(0);
                                        let hash = block_id["hash"]
                                            .as_str()
                                            .unwrap_or("0000000000000000000000000000000000000000000000000000000000000000")
                                            .to_string();
                                        let time = block["header"]["time"]
                                            .as_str()
                                            .unwrap_or("-")
                                            .to_string();
                                        let cid = block["header"]["chain_id"]
                                            .as_str()
                                            .unwrap_or("unknown")
                                            .to_string();
                                        (height, hash, time, cid)
                                    }
                                    Err(_) => (0, String::new(), String::new(), String::new()),
                                }
                            }
                            Err(_) => (0, String::new(), String::new(), String::new()),
                        }
                    }
                    Err(_) => (0, String::new(), String::new(), String::new()),
                }
            }
            Err(_) => (0, String::new(), String::new(), String::new()),
        }
    } else {
        (0, String::new(), String::new(), String::new())
    };

    Ok(ApiBlockchainResponse {
        blockchain: BlockchainStatusView {
            online: http_online || grpc_online,
            block_height,
            block_hash,
            chain_id,
            http_online,
            grpc_online,
            block_time,
        },
    })
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

fn calculate_dir_size(path: &Path) -> u64 {
    if !path.exists() {
        return 0;
    }
    let mut total = 0u64;
    if path.is_dir() {
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                let file_path = entry.path();
                if file_path.is_dir() {
                    total += calculate_dir_size(&file_path);
                } else if let Ok(metadata) = entry.metadata() {
                    total += metadata.len();
                }
            }
        }
    } else if let Ok(metadata) = path.metadata() {
        total = metadata.len();
    }
    total
}

fn count_files_in_dir(path: &Path, pattern: &str) -> usize {
    if !path.exists() {
        return 0;
    }
    let mut count = 0usize;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let file_path = entry.path();
            if file_path.is_file() && file_path.to_string_lossy().contains(pattern) {
                count += 1;
            }
        }
    }
    count
}

fn count_dirs_in_dir(path: &Path, prefix: &str) -> usize {
    if !path.exists() {
        return 0;
    }
    let mut count = 0usize;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let dir_path = entry.path();
            if dir_path.is_dir() {
                let name = dir_path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                if name.starts_with(prefix) {
                    count += 1;
                }
            }
        }
    }
    count
}

pub fn collect_storage(
    config_path: impl AsRef<Path>,
) -> Result<ApiStorageResponse, Box<dyn std::error::Error>> {
    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path.as_ref())?;
    let data_dir = PathBuf::from(&config.runtime.data_dir);

    let total_size = calculate_dir_size(&data_dir);
    let batch_count = count_files_in_dir(&data_dir.join("batches"), ".json");
    let prepared_epoch_count = count_dirs_in_dir(&data_dir.join("prepared-epochs"), "epoch-");
    let settlement_count = count_dirs_in_dir(&data_dir.join("settlements"), "epoch-");
    let log_files_count = count_files_in_dir(&data_dir.join("logs"), ".log");
    let db_size = calculate_dir_size(&data_dir.join("local-chain"));

    let payload_count = count_files_in_dir(&data_dir.join("payloads"), ".json")
        + count_files_in_dir(&data_dir.join("payloads"), ".bin");

    Ok(ApiStorageResponse {
        storage: StorageInfoView {
            data_dir: config.runtime.data_dir,
            total_size_bytes: total_size,
            total_size_formatted: format_bytes(total_size),
            batch_count,
            epoch_count: prepared_epoch_count,
            payload_count,
            prepared_epoch_count,
            settlement_count,
            log_files_count,
            db_size_bytes: db_size,
        },
    })
}

pub fn collect_tokenomics(
    config_path: impl AsRef<Path>,
) -> Result<ApiTokenomicsResponse, Box<dyn std::error::Error>> {
    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path.as_ref())?;

    let total_supply = TOTAL_SUPPLY.to_string();
    let emission_year = config.reward.emission_year;

    let player_reward_budget_per_hour = format!("{} / 小时", config.reward.target_network_weight_units);
    let service_reward_budget_per_hour = format!("{} / 小时", config.reward.target_network_weight_units);
    let player_block_reward = format!("{} 原子单位", config.reward.player_block_reward);

    Ok(ApiTokenomicsResponse {
        tokenomics: TokenomicsSummaryView {
            total_supply,
            annual_emission_rate_bps: 500,
            current_year: 2026,
            emission_year,
            player_reward_budget_per_hour,
            service_reward_budget_per_hour,
            player_block_reward,
            tail_emission_active: false,
            tail_emission_rate_bps: 10,
        },
    })
}

pub fn collect_dashboard(
    config_path: impl AsRef<Path>,
) -> Result<ApiDashboardResponse, Box<dyn std::error::Error>> {
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path.as_ref())?;
    let summary = crate::load_status(&config)?;

    let data_dir = PathBuf::from(&config.runtime.data_dir);
    let daemon_pid = read_pid(&data_dir.join("daemon.pid"));
    let runtime = ServiceRuntime::from_observed_process(
        daemon_pid,
        daemon_pid.map(process_is_running).unwrap_or(false),
    );
    let service_snapshot = runtime.snapshot();

    let total_size = calculate_dir_size(&data_dir);
    let batch_count = count_files_in_dir(&data_dir.join("batches"), ".json");
    let prepared_epoch_count = count_dirs_in_dir(&data_dir.join("prepared-epochs"), "epoch-");
    let settlement_count = count_dirs_in_dir(&data_dir.join("settlements"), "epoch-");
    let log_files_count = count_files_in_dir(&data_dir.join("logs"), ".log");
    let db_size = calculate_dir_size(&data_dir.join("local-chain"));
    let payload_count = count_files_in_dir(&data_dir.join("payloads"), ".json")
        + count_files_in_dir(&data_dir.join("payloads"), ".bin");

    let layout = crate::runtime_layout_for_config(&config_path, &config.runtime.data_dir);
    let platform = match crate::current_platform() {
        crate::Platform::Windows => "windows",
        crate::Platform::Linux => "linux",
        crate::Platform::Macos => "macos",
    };
    let service_manager = if cfg!(windows) { "windows-service" } else { "systemd" };

    let update_status = crate::collect_update_overview(
        "0.1.0",
        "stable",
        &config.runtime.data_dir,
        PathBuf::from(&config.runtime.data_dir).join("releases"),
    );

    Ok(ApiDashboardResponse {
        dashboard: DashboardView {
            service: ServiceStatusView {
                state: service_snapshot.state_label.to_string(),
                pid: service_snapshot.pid,
                stale: service_snapshot.stale,
                recoverable_without_manual_cleanup: service_snapshot.recoverable_without_manual_cleanup,
            },
            node: NodeHealthView {
                chain_id: config.chain_id.clone(),
                node_id: config.node_id_hex.clone(),
                reward_address: config.reward_address_hex.clone(),
                data_dir: config.runtime.data_dir.clone(),
                next_epoch_id: summary.next_epoch_id,
                next_slot_id: summary.next_slot_id,
                ticks_completed: summary.ticks_completed,
                low_impact_mode: summary.low_impact_mode,
                inline_verify_enabled: summary.inline_verify_enabled,
                inline_propose_enabled: summary.inline_propose_enabled,
            },
            storage: StorageInfoView {
                data_dir: config.runtime.data_dir.clone(),
                total_size_bytes: total_size,
                total_size_formatted: format_bytes(total_size),
                batch_count,
                epoch_count: prepared_epoch_count,
                payload_count,
                prepared_epoch_count,
                settlement_count,
                log_files_count,
                db_size_bytes: db_size,
            },
            tokenomics: TokenomicsSummaryView {
                total_supply: TOTAL_SUPPLY.to_string(),
                annual_emission_rate_bps: 500,
                current_year: 2026,
                emission_year: config.reward.emission_year,
                player_reward_budget_per_hour: format!("{} 权重单位", config.reward.target_network_weight_units),
                service_reward_budget_per_hour: format!("{} 权重单位", config.reward.target_network_weight_units),
                player_block_reward: format!("{} 原子", config.reward.player_block_reward),
                tail_emission_active: false,
                tail_emission_rate_bps: 10,
            },
            network: P2pNetworkView {
                mode: "socket".to_string(),
                local_peer_id: config.node_id_hex.clone(),
                connected_peers: 0,
                peers: Vec::new(),
            },
            challenge_activity: ChallengeActivityView {
                active_challenges: 0,
                completed_challenges: 0,
                failed_challenges: 0,
                last_challenge_epoch: 0,
            },
            meta: AppMetaView {
                app_name: "PoLE".to_string(),
                app_version: "0.1.0".to_string(),
                control_api_default_bind_addr: "127.0.0.1:8787".to_string(),
                remote_access_default_enabled: false,
                browser_open_supported: true,
                service_manager: service_manager.to_string(),
                install_layout: InstallLayoutView {
                    platform: platform.to_string(),
                    mode: "portable".to_string(),
                    root_dir: layout.root_dir.to_string_lossy().to_string(),
                    config_dir: layout.config_dir.to_string_lossy().to_string(),
                    data_dir: layout.data_dir.to_string_lossy().to_string(),
                    log_dir: layout.log_dir.to_string_lossy().to_string(),
                    update_dir: layout.update_dir.to_string_lossy().to_string(),
                },
            },
            config: ConfigView {
                config_path: config_path.to_string_lossy().to_string(),
                chain_id: config.chain_id.clone(),
                node_id: config.node_id_hex.clone(),
                reward_address: config.reward_address_hex.clone(),
                data_dir: config.runtime.data_dir.clone(),
                target_app_ids: config.runtime.target_app_ids.clone(),
                game_process_names: config.runtime.game_process_names.clone(),
                low_impact_mode: config.runtime.low_impact_mode,
                os_background_priority: config.runtime.os_background_priority,
                reward_source: format!("{:?}", config.reward.reward_source),
                emission_year: config.reward.emission_year,
            },
            update_available: update_status.update_available,
            current_version: update_status.current_version,
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
        "{status_line}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\nX-Frame-Options: DENY\r\nReferrer-Policy: no-referrer\r\n\r\n{}",
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
    // Set read timeout to prevent slow-loris style attacks
    stream.set_read_timeout(Some(Duration::from_secs(CONNECTION_TIMEOUT_SECS)))?;

    // Read request with size limit
    let mut buffer = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];
    loop {
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        if buffer.len() + n > MAX_REQUEST_SIZE {
            write_json_response(
                &mut stream,
                "HTTP/1.1 413 Payload Too Large",
                "{\"error\":\"request_too_large\"}",
            )?;
            return Ok(());
        }
        buffer.extend_from_slice(&chunk[..n]);
        // Stop reading if we've got a complete HTTP header + body
        if buffer.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }

    let request = String::from_utf8_lossy(&buffer);
    let request_headers = request.split("\r\n\r\n").next().unwrap_or("");
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

    // Authentication: check token for all mutating (POST) endpoints
    let requires_auth = method == "POST";
    if requires_auth {
        if let Some(expected_token) = read_api_token() {
            if !verify_auth_token(request_headers, &expected_token) {
                write_json_response(
                    &mut stream,
                    "HTTP/1.1 401 Unauthorized",
                    "{\"error\":\"unauthorized\"}",
                )?;
                return Ok(());
            }
        }
    }

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
        ("GET", "/api/dashboard") => {
            let body = serde_json::to_string(&collect_dashboard(config_path)?)?;
            write_json_response(&mut stream, "HTTP/1.1 200 OK", &body)?;
        }
        ("GET", "/api/blockchain") => {
            let body = serde_json::to_string(&collect_blockchain(config_path)?)?;
            write_json_response(&mut stream, "HTTP/1.1 200 OK", &body)?;
        }
        ("GET", "/api/storage") => {
            let body = serde_json::to_string(&collect_storage(config_path)?)?;
            write_json_response(&mut stream, "HTTP/1.1 200 OK", &body)?;
        }
        ("GET", "/api/tokenomics") => {
            let body = serde_json::to_string(&collect_tokenomics(config_path)?)?;
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
    if read_api_token().is_some() {
        eprintln!("[control-api] API token authentication enabled (POLE_API_TOKEN set)");
    } else {
        eprintln!("[control-api] WARNING: No POLE_API_TOKEN set — mutating endpoints are unprotected");
    }
    let mut served = 0usize;
    for stream in listener.incoming() {
        match handle_connection(stream?, &config_path) {
            Ok(()) => {}
            Err(e) => {
                eprintln!("[control-api] connection error: {e}");
            }
        }
        served += 1;
        if let Some(limit) = max_requests {
            if served >= limit {
                break;
            }
        }
    }
    Ok(())
}
