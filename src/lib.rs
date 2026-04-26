#![forbid(unsafe_code)]

pub mod activity_collector;
pub mod app_paths;
pub mod cli_output;
pub mod cli_parsing;
pub mod cli_support;
pub mod control_api;
pub mod control_api_types;
pub mod executor;
mod governance_runtime;
pub mod install_layout;
mod json_file;
pub mod node_aggregator;
pub mod node_anomaly;
pub mod node_cli_support;
pub mod node_config;
pub mod node_daemon;
pub mod node_gvs;
pub mod node_pipeline;
pub mod node_prepare;
pub mod node_rewards;
pub mod node_runtime;
pub mod node_settlement;
pub mod node_storage_audit;
pub mod node_verifier;
pub mod p2p;
pub mod p2p_libp2p;
pub mod params;
pub mod primitives;
pub mod proto;
pub mod records;
pub mod service_runtime;
pub mod service_systemd;
pub mod service_windows;
pub mod signing;
pub mod state;
pub mod steam_collector;
pub mod steam_game_directory;
pub mod storage_book;
pub mod store;
pub mod tokenomics;
pub mod transactions;
pub mod transitions;
pub mod update_manifest;
pub mod updater;
pub mod wallet;

pub use activity_collector::{
    collect_configured_activity_source, parse_community_activity_response,
    parse_third_party_activity_response, ActivityCollector, ActivityCollectorError,
    CommunityJsonCollector, EaLiveCollector, EpicLiveCollector, GogLiveCollector,
    LiveActivityCollector, ThirdPartyJsonCollector,
};
pub use install_layout::{
    current_platform, normalize_path, portable_layout_for_config, resolve_install_layout,
    resolve_runtime_data_dir, runtime_layout_for_config, InstallLayout, InstallMode, Platform,
};
pub use cli_output::{
    dispatch_command, format_usage_block, parse_vote_choice, print_governance_index,
    print_governance_proposal_artifact, print_governance_scheduled_artifact,
    print_governance_summary, print_protocol_params_summary, print_reward_adjustment_index,
    print_reward_adjustment_summary, CommandHandler,
};
pub use cli_parsing::{
    decode_hex32, parse_socket_addr, parse_socket_peer_spec, parse_socket_peer_specs,
    parse_socket_topics, socket_peers_from_config, CliParseError,
};
pub use cli_support::{
    default_data_dir_for_config, effective_install_layout, is_reward_config_subcommand,
    latest_local_epoch, load_config_and_epoch_arg, looks_like_hex_32_arg,
    parse_config_path_and_rest, parse_config_path_and_rest_with_known_first_arg,
    parse_optional_u32_arg, parse_optional_u64_arg, print_command_header, print_data_dir_path,
    print_path_entry, resolve_challenge_window_blocks_arg, resolve_current_height_arg,
    resolve_epoch_id_arg, resolve_submission_height_arg,
};
pub use control_api::{
    collect_config as collect_control_api_config, collect_logs as collect_control_api_logs,
    collect_meta as collect_control_api_meta, collect_status as collect_control_api_status,
    collect_update as collect_control_api_update,
    execute_service_action as execute_control_api_service_action,
    execute_update_action as execute_control_api_update_action,
    handle_connection as handle_control_api_connection, serve as serve_control_api,
    update_config as update_control_api_config,
};
pub use control_api_types::{
    ApiConfigResponse, ApiLogsResponse, ApiMetaResponse, ApiStatusResponse, ApiUpdateResponse,
    AppMetaView, ConfigUpdateRequest, ConfigView, InstallLayoutView, LogEntryView, NodeHealthView,
    ServiceActionRequest, ServiceActionResponse, ServiceStatusView, UpdateActionRequest,
    UpdateActionResponse, UpdateStatusView,
};
pub use executor::{execute_block, Block, BlockExecutionError};
pub use governance_runtime::{execute_governance_vote, submit_protocol_params_update_proposal};
pub use node_aggregator::{
    aggregate_local_epoch, aggregate_record_root, EpochAggregationArtifact, NodeAggregationError,
};
pub use node_anomaly::{detect_sample_anomalies, SampleAnomaly, SampleAnomalyKind};
pub use node_cli_support::{
    current_unix_millis, maybe_write_payload, parse_simulation_topology_args, print_batch_summary,
    source_kind_label,
};
pub use node_config::{
    hex_32, ActivitySourceConfig, CapabilityConfig, CollectConfig, NodeConfig, NodeConfigError,
    P2pLibp2pBootstrapPeerConfig, P2pLibp2pConfig, P2pLibp2pDiscoveryConfig, P2pSimulationConfig,
    P2pSocketConfig, P2pSocketPeerConfig, RewardConfig, RewardGameMapping, RewardSourceMode,
    RuntimeConfig, StorageConfig,
};
pub use node_daemon::{
    adjustment_cycle_artifact_path, adjustment_cycle_index_path, adjustment_cycle_summary_path,
    batch_artifact_path, build_epoch_commit_from_local_data, detect_active_game_processes,
    detect_foreground_process_name, effective_collect_interval_secs,
    epoch_aggregation_artifact_path, epoch_commit_artifact_path, epoch_preparation_artifact_path,
    epoch_reward_artifact_path, epoch_settlement_artifact_path, epoch_verification_artifact_path,
    governance_index_artifact_path, governance_proposal_artifact_path,
    governance_scheduled_artifact_path, governance_summary_artifact_path, heartbeat_path,
    load_batches_for_epoch, load_status, local_chain_runtime_path, payload_path, progress_path,
    prune_retention, retention_book_path, reward_adjustment_artifact_path,
    reward_adjustment_index_path, reward_adjustment_summary_path, run_collect_loop_with_client,
    run_collect_loop_with_client_and_network, run_collect_tick_with_client,
    run_collect_tick_with_client_and_network, summarize_auto_settlement,
    summarize_collect_loop_with_client, summarize_collect_loop_with_client_and_network,
    AutoSettlementSummary, CollectLoopSummary, CollectTickArtifact, CollectTickResult,
    LocalNodeProgress, NodeDaemonError, NodeHeartbeat, NodeStatusSummary, PruneOutcome,
    RewardAdjustmentArtifact, RewardAdjustmentArtifactIndex, RewardAdjustmentArtifactSummary,
};
pub use node_gvs::{
    classify_tier, compute_coverage_bonus_ppm, compute_gvs_factors, compute_gvs_microunits,
    compute_time_decay_ppm, GvsFactors, GvsTier,
};
pub use node_pipeline::{
    cid_from_hash, merkle_root, stable_hash32, ActivitySample, AssembledBatch, BatchBuilder,
    NodePipelineError, SteamCurrentPlayersSample,
};
pub use node_prepare::prepare_local_epoch;
pub use node_rewards::{
    adjusted_player_block_reward, current_network_weight_units_for_block,
    effective_challenge_window_blocks, effective_min_retention_epochs,
    effective_player_block_reward, effective_reward_adjustment_cap_bps,
    effective_reward_block_secs, effective_target_network_weight_units,
    player_reward_tick_artifact_path, reward_local_epoch, reward_record_root, EpochRewardArtifact,
    NodeRewardError,
};
pub use node_runtime::{
    CollectAndStoreOutcome, EpochCommitInputs, LocalNodeRuntime, NodeRuntimeError,
};
pub use node_settlement::{
    export_governance_artifacts, export_governance_proposal_artifact,
    export_governance_scheduled_artifact, open_local_protocol_state, settle_local_epoch,
    suggested_settlement_height, EpochSettlementArtifact, GovernanceArtifactIndex,
    GovernanceArtifactSummary, GovernanceProposalArtifact, GovernanceProposalIndexEntry,
    GovernanceScheduledIndexEntry, GovernanceScheduledParamsArtifact, LocalChainRuntimeState,
    NodeSettlementError,
};
pub use node_storage_audit::{
    audit_local_retention, retention_audit_artifact_path, run_local_storage_challenge,
    NodeStorageAuditError, RetentionAuditArtifact, StorageChallengeArtifact,
};
pub use node_verifier::{verify_local_epoch, EpochVerificationReport, NodeVerificationError};
pub use p2p::*;
pub use p2p_libp2p::{
    build_libp2p_backend_skeleton, build_real_libp2p_swarm_report, run_libp2p_skeleton_loop,
    DiscoveryKind, Libp2pBackendError, Libp2pBackendSkeleton, Libp2pBootstrapPeer,
    Libp2pDiscoveryState, Libp2pLoopReport, Libp2pPeerEntry, Libp2pPeerTable,
    Libp2pRuntimeStateMachine, PeerConnectionState, RealLibp2pSwarmBuildReport,
    SkeletonRuntimePhase,
};
pub use params::*;
pub use primitives::*;
pub use proto::*;
pub use records::*;
pub use service_runtime::{
    ManagedServiceStatus, ServiceManager, ServiceManagerError, ServiceRuntime, ServiceSnapshot,
    ServiceState,
};
pub use service_systemd::{SystemdServiceManager, SystemdUnitDefinition, SYSTEMD_SERVICE_NAME};
pub use service_windows::{
    WindowsServiceDefinition, WindowsServiceManager, WINDOWS_SERVICE_DISPLAY_NAME,
    WINDOWS_SERVICE_NAME,
};
pub use signing::{
    development_manifest_signature, release_manifest_signing_payload,
    verify_release_manifest_signature, ManifestSignatureVerification,
};
pub use state::*;
pub use steam_collector::{
    current_players_url, fetch_current_players_live, fetch_current_players_with_client,
    parse_current_players_response, HttpTextClient, ReqwestHttpTextClient, SteamCollectorError,
};
pub use steam_game_directory::{
    canonical_process_name, infer_reward_game_mapping, infer_reward_game_mapping_from_roots,
    load_cached_reward_game_mapping, recognition_cache_path, store_cached_reward_game_mapping,
};
pub use storage_book::{LocalRetentionBook, StorageBookError};
pub use store::*;
pub use tokenomics::*;
pub use transactions::*;
pub use transitions::*;
pub use update_manifest::{
    load_release_manifest, load_release_manifest_for_channel, release_manifest_path,
    version_is_newer, ReleaseArtifact, ReleaseManifest,
};
pub use updater::{
    applied_update_record_path, apply_update, apply_update_with_status, collect_update_overview,
    collect_update_overview_with_status, execute_install_action, install_action_plan_path,
    install_execution_record_path, installed_version_record_path, load_applied_update_record,
    load_install_action_plan, load_install_execution_record, load_installed_version_record,
    load_pending_update_plan, load_rollback_metadata, load_switch_execution_record,
    load_switch_plan, pending_update_plan_path, rollback_metadata_path, rollback_update,
    stage_update, switch_execution_record_path, switch_plan_path, AppliedUpdateRecord,
    InstallActionPlanRecord, InstallExecutionRecord, InstalledVersionRecord, PendingUpdatePlan,
    RollbackMetadata, SwitchExecutionRecord, SwitchPlanRecord, UpdateExecutionResult,
    UpdateOverview,
};
pub use wallet::{
    create_wallet, derive_child_key, EncryptedKeystore, export_secret, generate_mnemonic,
    hex_decode, hex_encode, word_to_index, KeyPair, Mnemonic, recover_wallet, set_reward_address,
    show_address, show_address_with_password, sign_transaction, WalletError,
};
