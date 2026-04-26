use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::primitives::{ActivitySourceKind, Address, Amount, AppId, NodeId};
use crate::tokenomics::{
    base_player_reward_per_block_with_tail, LONG_TERM_TAIL_EMISSION_RATE_BPS,
    LONG_TERM_TAIL_START_YEAR,
};
use crate::{normalize_path, resolve_runtime_data_dir};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeConfig {
    pub chain_id: String,
    pub node_id_hex: String,
    pub reward_address_hex: String,
    pub capabilities: CapabilityConfig,
    pub collect: CollectConfig,
    pub runtime: RuntimeConfig,
    pub storage: StorageConfig,
    #[serde(default)]
    pub reward: RewardConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityConfig {
    pub collect: bool,
    pub store: bool,
    pub verify: bool,
    pub propose: bool,
    pub archive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectConfig {
    pub enabled: bool,
    pub default_epoch_id: u64,
    pub default_slot_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub data_dir: String,
    pub poll_interval_secs: u64,
    pub slots_per_epoch: u64,
    #[serde(default = "default_challenge_window_blocks")]
    pub challenge_window_blocks: u32,
    #[serde(default = "default_low_impact_mode")]
    pub low_impact_mode: bool,
    #[serde(default = "default_os_background_priority")]
    pub os_background_priority: bool,
    #[serde(default = "default_game_active_poll_interval_secs")]
    pub game_active_poll_interval_secs: u64,
    #[serde(default)]
    pub game_process_names: Vec<String>,
    pub target_app_ids: Vec<AppId>,
    #[serde(default)]
    pub p2p_simulation: P2pSimulationConfig,
    #[serde(default)]
    pub p2p_socket: P2pSocketConfig,
    #[serde(default)]
    pub p2p_libp2p: P2pLibp2pConfig,
    #[serde(default)]
    pub activity_sources: Vec<ActivitySourceConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivitySourceConfig {
    pub app_id: AppId,
    pub source_kind: ActivitySourceKind,
    #[serde(default)]
    pub endpoint_url: Option<String>,
    #[serde(default)]
    pub inline_json: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct P2pSimulationConfig {
    #[serde(default = "default_p2p_batch_listener_count")]
    pub batch_listener_count: usize,
    #[serde(default = "default_p2p_receipt_listener_count")]
    pub receipt_listener_count: usize,
    #[serde(default = "default_p2p_dual_listener_count")]
    pub dual_listener_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct P2pSocketConfig {
    #[serde(default = "default_p2p_socket_bind_addr")]
    pub bind_addr: String,
    #[serde(default)]
    pub peers: Vec<P2pSocketPeerConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct P2pSocketPeerConfig {
    pub peer_id_hex: String,
    pub addr: String,
    #[serde(default)]
    pub topics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct P2pLibp2pConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_p2p_libp2p_listen_addrs")]
    pub listen_addrs: Vec<String>,
    #[serde(default)]
    pub bootstrap_peers: Vec<P2pLibp2pBootstrapPeerConfig>,
    #[serde(default)]
    pub discovery: P2pLibp2pDiscoveryConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct P2pLibp2pBootstrapPeerConfig {
    pub peer_id: String,
    pub addr: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct P2pLibp2pDiscoveryConfig {
    #[serde(default = "default_p2p_libp2p_kademlia")]
    pub kademlia: bool,
    #[serde(default = "default_p2p_libp2p_mdns")]
    pub mdns: bool,
    #[serde(default)]
    pub rendezvous: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RewardConfig {
    #[serde(default = "default_reward_source")]
    pub reward_source: RewardSourceMode,
    #[serde(default = "default_emission_year")]
    pub emission_year: u32,
    #[serde(default = "default_reward_block_secs")]
    pub reward_block_secs: u64,
    #[serde(default = "default_player_block_reward")]
    pub player_block_reward: Amount,
    #[serde(default = "default_reward_adjustment_period_blocks")]
    pub reward_adjustment_period_blocks: u64,
    #[serde(default = "default_target_network_weight_units")]
    pub target_network_weight_units: Amount,
    #[serde(default = "default_reward_adjustment_cap_bps")]
    pub reward_adjustment_cap_bps: u16,
    #[serde(default = "default_collect_reward_bps")]
    pub collect_reward_bps: u16,
    #[serde(default = "default_store_reward_bps")]
    pub store_reward_bps: u16,
    #[serde(default = "default_verify_reward_bps")]
    pub verify_reward_bps: u16,
    #[serde(default = "default_propose_reward_bps")]
    pub propose_reward_bps: u16,
    #[serde(default = "default_tail_emission_start_year")]
    pub tail_emission_start_year: u32,
    #[serde(default = "default_tail_emission_rate_bps")]
    pub tail_emission_rate_bps: u16,
    #[serde(default)]
    pub game_mappings: Vec<RewardGameMapping>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RewardSourceMode {
    Static,
    Tokenomics,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RewardGameMapping {
    pub process_name: String,
    pub app_id: AppId,
    #[serde(default = "default_game_coefficient_ppm")]
    pub game_coefficient_ppm: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageConfig {
    pub quota_gb: u32,
    pub retention_epochs: u32,
}

#[derive(Debug)]
pub enum NodeConfigError {
    Io(io::Error),
    Json(serde_json::Error),
    InvalidHexLength {
        field: &'static str,
        expected: usize,
        actual: usize,
    },
    InvalidHexCharacter {
        field: &'static str,
        index: usize,
        byte: u8,
    },
    InvalidValue {
        field: &'static str,
        reason: String,
    },
}

impl fmt::Display for NodeConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "io error: {err}"),
            Self::Json(err) => write!(f, "json error: {err}"),
            Self::InvalidHexLength {
                field,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "invalid hex length for {field}: expected {expected}, got {actual}"
                )
            }
            Self::InvalidHexCharacter { field, index, byte } => {
                write!(
                    f,
                    "invalid hex character for {field} at {index}: 0x{byte:02x}"
                )
            }
            Self::InvalidValue { field, reason } => {
                write!(f, "invalid value for {field}: {reason}")
            }
        }
    }
}

impl std::error::Error for NodeConfigError {}

impl From<io::Error> for NodeConfigError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for NodeConfigError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            chain_id: "pole-local".to_string(),
            node_id_hex: hex_32([0x11; 32]),
            reward_address_hex: hex_32([0x22; 32]),
            capabilities: CapabilityConfig {
                collect: true,
                store: false,
                verify: false,
                propose: false,
                archive: false,
            },
            collect: CollectConfig {
                enabled: true,
                default_epoch_id: 1,
                default_slot_id: 1,
            },
            runtime: RuntimeConfig {
                data_dir: "./pole-node-data".to_string(),
                poll_interval_secs: 300,
                slots_per_epoch: 288,
                challenge_window_blocks: default_challenge_window_blocks(),
                low_impact_mode: default_low_impact_mode(),
                os_background_priority: default_os_background_priority(),
                game_active_poll_interval_secs: default_game_active_poll_interval_secs(),
                game_process_names: Vec::new(),
                target_app_ids: vec![730],
                p2p_simulation: P2pSimulationConfig::default(),
                p2p_socket: P2pSocketConfig::default(),
                p2p_libp2p: P2pLibp2pConfig::default(),
                activity_sources: Vec::new(),
            },
            storage: StorageConfig {
                quota_gb: 10,
                retention_epochs: 2,
            },
            reward: RewardConfig::default(),
        }
    }
}

fn default_challenge_window_blocks() -> u32 {
    20
}

fn default_low_impact_mode() -> bool {
    true
}

fn default_os_background_priority() -> bool {
    true
}

fn default_game_active_poll_interval_secs() -> u64 {
    900
}

fn default_reward_block_secs() -> u64 {
    3_600
}

fn default_reward_source() -> RewardSourceMode {
    RewardSourceMode::Static
}

fn default_emission_year() -> u32 {
    1
}

fn default_p2p_batch_listener_count() -> usize {
    1
}

fn default_p2p_receipt_listener_count() -> usize {
    1
}

fn default_p2p_dual_listener_count() -> usize {
    1
}

fn default_p2p_socket_bind_addr() -> String {
    "127.0.0.1:0".to_string()
}

fn default_p2p_libp2p_listen_addrs() -> Vec<String> {
    vec!["/ip4/0.0.0.0/tcp/0".to_string()]
}

fn default_p2p_libp2p_kademlia() -> bool {
    true
}

fn default_p2p_libp2p_mdns() -> bool {
    true
}

fn default_player_block_reward() -> Amount {
    1_000
}

fn default_target_network_weight_units() -> Amount {
    150_000_000_000_000
}

fn default_reward_adjustment_period_blocks() -> u64 {
    288
}

fn default_reward_adjustment_cap_bps() -> u16 {
    2_000
}

fn default_collect_reward_bps() -> u16 {
    5_000
}

fn default_store_reward_bps() -> u16 {
    2_500
}

fn default_verify_reward_bps() -> u16 {
    1_500
}

fn default_propose_reward_bps() -> u16 {
    1_000
}

fn default_tail_emission_start_year() -> u32 {
    LONG_TERM_TAIL_START_YEAR
}

fn default_tail_emission_rate_bps() -> u16 {
    LONG_TERM_TAIL_EMISSION_RATE_BPS
}

fn default_game_coefficient_ppm() -> u32 {
    1_000_000
}

impl Default for RewardConfig {
    fn default() -> Self {
        Self {
            reward_source: RewardSourceMode::Static,
            emission_year: default_emission_year(),
            reward_block_secs: default_reward_block_secs(),
            player_block_reward: default_player_block_reward(),
            reward_adjustment_period_blocks: default_reward_adjustment_period_blocks(),
            target_network_weight_units: default_target_network_weight_units(),
            reward_adjustment_cap_bps: default_reward_adjustment_cap_bps(),
            collect_reward_bps: default_collect_reward_bps(),
            store_reward_bps: default_store_reward_bps(),
            verify_reward_bps: default_verify_reward_bps(),
            propose_reward_bps: default_propose_reward_bps(),
            tail_emission_start_year: default_tail_emission_start_year(),
            tail_emission_rate_bps: default_tail_emission_rate_bps(),
            game_mappings: Vec::new(),
        }
    }
}

impl Default for P2pSimulationConfig {
    fn default() -> Self {
        Self {
            batch_listener_count: default_p2p_batch_listener_count(),
            receipt_listener_count: default_p2p_receipt_listener_count(),
            dual_listener_count: default_p2p_dual_listener_count(),
        }
    }
}

impl Default for P2pSocketConfig {
    fn default() -> Self {
        Self {
            bind_addr: default_p2p_socket_bind_addr(),
            peers: Vec::new(),
        }
    }
}

impl Default for P2pLibp2pConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            listen_addrs: default_p2p_libp2p_listen_addrs(),
            bootstrap_peers: Vec::new(),
            discovery: P2pLibp2pDiscoveryConfig::default(),
        }
    }
}

impl Default for P2pLibp2pDiscoveryConfig {
    fn default() -> Self {
        Self {
            kademlia: default_p2p_libp2p_kademlia(),
            mdns: default_p2p_libp2p_mdns(),
            rendezvous: false,
        }
    }
}

impl NodeConfig {
    pub fn load_json(path: impl AsRef<Path>) -> Result<Self, NodeConfigError> {
        let content = fs::read_to_string(path)?;
        let config: Self = serde_json::from_str(&content)?;
        config.validate()?;
        Ok(config)
    }

    pub fn load_json_with_runtime_paths(
        path: impl AsRef<Path>,
    ) -> Result<(PathBuf, Self), NodeConfigError> {
        let config_path = absolute_path(path.as_ref())?;
        let mut config = Self::load_json(&config_path)?;
        config.runtime.data_dir = resolve_runtime_data_dir(&config_path, &config.runtime.data_dir)
            .to_string_lossy()
            .into_owned();
        Ok((config_path, config))
    }

    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), NodeConfigError> {
        self.validate()?;
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let content = serde_json::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), NodeConfigError> {
        let _ = self.node_id()?;
        let _ = self.reward_address()?;

        if self.chain_id.trim().is_empty() {
            return Err(NodeConfigError::InvalidValue {
                field: "chain_id",
                reason: "must not be empty".to_string(),
            });
        }
        if self.runtime.data_dir.trim().is_empty() {
            return Err(NodeConfigError::InvalidValue {
                field: "runtime.data_dir",
                reason: "must not be empty".to_string(),
            });
        }
        if self.runtime.slots_per_epoch == 0 {
            return Err(NodeConfigError::InvalidValue {
                field: "runtime.slots_per_epoch",
                reason: "must be greater than 0".to_string(),
            });
        }
        if self.runtime.challenge_window_blocks == 0 {
            return Err(NodeConfigError::InvalidValue {
                field: "runtime.challenge_window_blocks",
                reason: "must be greater than 0".to_string(),
            });
        }
        if self.collect.enabled && !self.capabilities.collect {
            return Err(NodeConfigError::InvalidValue {
                field: "collect.enabled",
                reason: "cannot be true when capabilities.collect is false".to_string(),
            });
        }
        if self.runtime.target_app_ids.is_empty() {
            return Err(NodeConfigError::InvalidValue {
                field: "runtime.target_app_ids",
                reason: "must contain at least one app id".to_string(),
            });
        }
        if self.storage.quota_gb == 0 {
            return Err(NodeConfigError::InvalidValue {
                field: "storage.quota_gb",
                reason: "must be greater than 0".to_string(),
            });
        }
        if self.storage.retention_epochs == 0 {
            return Err(NodeConfigError::InvalidValue {
                field: "storage.retention_epochs",
                reason: "must be greater than 0".to_string(),
            });
        }
        if self.reward.reward_block_secs == 0 {
            return Err(NodeConfigError::InvalidValue {
                field: "reward.reward_block_secs",
                reason: "must be greater than 0".to_string(),
            });
        }
        if self.reward.emission_year == 0 {
            return Err(NodeConfigError::InvalidValue {
                field: "reward.emission_year",
                reason: "must be greater than 0".to_string(),
            });
        }
        if self.reward.tail_emission_start_year == 0 {
            return Err(NodeConfigError::InvalidValue {
                field: "reward.tail_emission_start_year",
                reason: "must be greater than 0".to_string(),
            });
        }
        if self.reward.tail_emission_rate_bps > 10_000 {
            return Err(NodeConfigError::InvalidValue {
                field: "reward.tail_emission_rate_bps",
                reason: "must be less than or equal to 10000 bps".to_string(),
            });
        }
        if self.runtime.poll_interval_secs > 0
            && !self
                .reward
                .reward_block_secs
                .is_multiple_of(self.runtime.poll_interval_secs)
        {
            return Err(NodeConfigError::InvalidValue {
                field: "reward.reward_block_secs",
                reason: format!(
                    "must be an exact multiple of runtime.poll_interval_secs ({})",
                    self.runtime.poll_interval_secs
                ),
            });
        }
        if self.reward.player_block_reward == 0 {
            return Err(NodeConfigError::InvalidValue {
                field: "reward.player_block_reward",
                reason: "must be greater than 0".to_string(),
            });
        }
        if self.reward.reward_adjustment_period_blocks == 0 {
            return Err(NodeConfigError::InvalidValue {
                field: "reward.reward_adjustment_period_blocks",
                reason: "must be greater than 0".to_string(),
            });
        }
        if self.reward.target_network_weight_units == 0 {
            return Err(NodeConfigError::InvalidValue {
                field: "reward.target_network_weight_units",
                reason: "must be greater than 0".to_string(),
            });
        }
        if self.reward.reward_adjustment_cap_bps > 10_000 {
            return Err(NodeConfigError::InvalidValue {
                field: "reward.reward_adjustment_cap_bps",
                reason: "must be less than or equal to 10000 bps".to_string(),
            });
        }
        let total_service_reward_bps = u32::from(self.reward.collect_reward_bps)
            + u32::from(self.reward.store_reward_bps)
            + u32::from(self.reward.verify_reward_bps)
            + u32::from(self.reward.propose_reward_bps);
        if total_service_reward_bps != 10_000 {
            return Err(NodeConfigError::InvalidValue {
                field: "reward.*_reward_bps",
                reason: format!(
                    "service reward split must sum to 10000 bps, got {total_service_reward_bps}"
                ),
            });
        }
        for mapping in &self.reward.game_mappings {
            if mapping.process_name.trim().is_empty() {
                return Err(NodeConfigError::InvalidValue {
                    field: "reward.game_mappings[].process_name",
                    reason: "must not be empty".to_string(),
                });
            }
            if mapping.game_coefficient_ppm == 0 {
                return Err(NodeConfigError::InvalidValue {
                    field: "reward.game_mappings[].game_coefficient_ppm",
                    reason: "must be greater than 0".to_string(),
                });
            }
        }
        Ok(())
    }

    pub fn node_id(&self) -> Result<NodeId, NodeConfigError> {
        decode_hex_32(&self.node_id_hex, "node_id_hex")
    }

    pub fn reward_address(&self) -> Result<Address, NodeConfigError> {
        decode_hex_32(&self.reward_address_hex, "reward_address_hex")
    }

    pub fn inline_verify_enabled(&self) -> bool {
        self.capabilities.verify && !self.runtime.low_impact_mode
    }

    pub fn inline_propose_enabled(&self) -> bool {
        self.capabilities.propose && !self.runtime.low_impact_mode
    }
}

impl RewardConfig {
    pub fn base_player_block_reward(&self) -> Amount {
        match self.reward_source {
            RewardSourceMode::Static => self.player_block_reward,
            RewardSourceMode::Tokenomics => base_player_reward_per_block_with_tail(
                self.emission_year,
                self.reward_block_secs,
                self.tail_emission_start_year,
                self.tail_emission_rate_bps,
            )
            .max(1),
        }
    }

    pub fn reward_source_label(&self) -> &'static str {
        match self.reward_source {
            RewardSourceMode::Static => "static",
            RewardSourceMode::Tokenomics => "tokenomics",
        }
    }
}

fn absolute_path(path: &Path) -> Result<PathBuf, NodeConfigError> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    Ok(normalize_path(absolute))
}

pub fn hex_32(bytes: [u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for byte in bytes {
        out.push(char::from(b"0123456789abcdef"[(byte >> 4) as usize]));
        out.push(char::from(b"0123456789abcdef"[(byte & 0x0f) as usize]));
    }
    out
}

fn decode_hex_32(input: &str, field: &'static str) -> Result<[u8; 32], NodeConfigError> {
    if input.len() != 64 {
        return Err(NodeConfigError::InvalidHexLength {
            field,
            expected: 64,
            actual: input.len(),
        });
    }

    let mut out = [0u8; 32];
    let bytes = input.as_bytes();
    for index in 0..32 {
        let hi = decode_nibble(bytes[index * 2], field, index * 2)?;
        let lo = decode_nibble(bytes[index * 2 + 1], field, index * 2 + 1)?;
        out[index] = (hi << 4) | lo;
    }
    Ok(out)
}

fn decode_nibble(byte: u8, field: &'static str, index: usize) -> Result<u8, NodeConfigError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(NodeConfigError::InvalidHexCharacter { field, index, byte }),
    }
}
