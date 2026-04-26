use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceStatusView {
    pub state: String,
    pub pid: Option<u32>,
    pub stale: bool,
    pub recoverable_without_manual_cleanup: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeHealthView {
    pub chain_id: String,
    pub node_id: String,
    pub reward_address: String,
    pub data_dir: String,
    pub next_epoch_id: u64,
    pub next_slot_id: u64,
    pub ticks_completed: u64,
    pub low_impact_mode: bool,
    pub inline_verify_enabled: bool,
    pub inline_propose_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiStatusResponse {
    pub service: ServiceStatusView,
    pub node: NodeHealthView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallLayoutView {
    pub platform: String,
    pub mode: String,
    pub root_dir: String,
    pub config_dir: String,
    pub data_dir: String,
    pub log_dir: String,
    pub update_dir: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppMetaView {
    pub app_name: String,
    pub app_version: String,
    pub control_api_default_bind_addr: String,
    pub remote_access_default_enabled: bool,
    pub browser_open_supported: bool,
    pub service_manager: String,
    pub install_layout: InstallLayoutView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiMetaResponse {
    pub app: AppMetaView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateStatusView {
    pub current_version: String,
    pub channel: String,
    pub update_dir: String,
    pub release_manifest_dir: String,
    pub latest_manifest_path: String,
    pub artifact_count: usize,
    pub update_available: bool,
    pub latest_available_version: Option<String>,
    pub pending_target_version: Option<String>,
    pub applied_target_version: Option<String>,
    pub selected_artifact_kind: Option<String>,
    pub selected_artifact_path: Option<String>,
    pub executed_artifact_path: Option<String>,
    pub planned_install_path: Option<String>,
    pub planned_backup_path: Option<String>,
    pub executed_install_path: Option<String>,
    pub install_target_mode: Option<String>,
    pub install_action_status: String,
    pub switch_execution_status: String,
    pub service_window_status: String,
    pub signing_status: String,
    pub rollback_status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiUpdateResponse {
    pub update: UpdateStatusView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct UpdateActionRequest {
    #[serde(default)]
    pub channel: Option<String>,
    #[serde(default)]
    pub install_root_override: Option<String>,
    #[serde(default)]
    pub installed_layout_root_override: Option<String>,
    #[serde(default)]
    pub use_installed_layout: bool,
    #[serde(default)]
    pub allow_system_install_write: bool,
    #[serde(default)]
    pub stop_service_before_install: bool,
    #[serde(default)]
    pub start_service_after_install: bool,
    #[serde(default)]
    pub stop_service_before_rollback: bool,
    #[serde(default)]
    pub start_service_after_rollback: bool,
    #[serde(default)]
    pub systemd_unit_root: Option<String>,
    #[serde(default)]
    pub systemctl_binary: Option<String>,
    #[serde(default)]
    pub windows_service_root: Option<String>,
    #[serde(default)]
    pub windows_sc_binary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateActionResponse {
    pub action: String,
    pub channel: String,
    pub status: String,
    pub target_version: Option<String>,
    pub pending_update_path: String,
    pub rollback_path: String,
    pub switch_plan_path: String,
    pub switch_execution_path: String,
    pub install_action_path: String,
    pub install_execution_path: String,
    pub installed_version_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigView {
    pub config_path: String,
    pub chain_id: String,
    pub node_id: String,
    pub reward_address: String,
    pub data_dir: String,
    pub target_app_ids: Vec<u32>,
    pub game_process_names: Vec<String>,
    pub low_impact_mode: bool,
    pub os_background_priority: bool,
    pub reward_source: String,
    pub emission_year: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiConfigResponse {
    pub config: ConfigView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ConfigUpdateRequest {
    #[serde(default)]
    pub target_app_ids: Option<Vec<u32>>,
    #[serde(default)]
    pub game_process_names: Option<Vec<String>>,
    #[serde(default)]
    pub low_impact_mode: Option<bool>,
    #[serde(default)]
    pub os_background_priority: Option<bool>,
    #[serde(default)]
    pub emission_year: Option<u32>,
    #[serde(default)]
    pub reward_source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntryView {
    pub source: String,
    pub path: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiLogsResponse {
    pub logs: Vec<LogEntryView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ServiceActionRequest {
    #[serde(default)]
    pub systemd_unit_root: Option<String>,
    #[serde(default)]
    pub systemctl_binary: Option<String>,
    #[serde(default)]
    pub windows_service_root: Option<String>,
    #[serde(default)]
    pub windows_sc_binary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceActionResponse {
    pub action: String,
    pub service_name: String,
    pub status: String,
}
