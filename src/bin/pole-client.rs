use std::env;
use std::fs;
use std::fs::File;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use rand::RngCore;
use serde::{Deserialize, Serialize};

use pole_protocol_draft::{
    aggregate_local_epoch, allocation_breakdown, annual_emission_schedule_with_tail,
    build_epoch_commit_from_local_data, build_inmemory_simulation_network,
    build_libp2p_backend_skeleton, build_real_libp2p_swarm_report, chain_bridge, current_players_url,
    decode_hex32, default_data_dir_for_config, detect_active_game_processes,
    detect_foreground_process_name, dispatch_command, effective_challenge_window_blocks,
    effective_collect_interval_secs, effective_install_layout, effective_player_block_reward,
    effective_reward_adjustment_cap_bps, effective_reward_block_secs,
    effective_target_network_weight_units, export_governance_proposal_artifact,
    export_governance_scheduled_artifact, format_usage_block, governance_index_artifact_path,
    governance_summary_artifact_path, heartbeat_path, hex_32, hex_encode, infer_reward_game_mapping,
    is_reward_config_subcommand, KeyPair, latest_local_epoch, load_batches_for_epoch,
    load_cached_reward_game_mapping, load_config_and_epoch_arg, load_status, looks_like_hex_32_arg,
    node_prepare, open_local_protocol_state,
    parse_config_path_and_rest, parse_config_path_and_rest_with_known_first_arg,
    parse_optional_u64_arg, parse_socket_peer_specs, parse_socket_topics, parse_vote_choice,
    prepare_local_epoch,
    print_command_header, print_data_dir_path, print_governance_index,
    print_governance_proposal_artifact, print_governance_scheduled_artifact,
    print_governance_summary, print_path_entry, print_reward_adjustment_index,
    print_reward_adjustment_summary, progress_path, prune_retention, recognition_cache_path,
    resolve_challenge_window_blocks_arg, resolve_current_height_arg, resolve_epoch_id_arg,
    resolve_submission_height_arg, retention_book_path, reward_local_epoch,
    run_collect_tick_with_client, serve_control_api, settle_local_epoch, socket_peers_from_config,
    stable_hash32, store_cached_reward_game_mapping, submit_protocol_params_update_proposal,
    suggested_settlement_height, summarize_collect_loop_with_client,
    summarize_collect_loop_with_client_and_network, verify_local_epoch, ActivitySourceConfig,
    ActivitySourceKind, CollectLoopSummary, CollectTickResult, FilesystemP2pNetwork,
    GovernanceArtifactIndex, GovernanceArtifactSummary, LocalNodeProgress, NodeConfig, P2pNetwork,
    ProtocolStore, ReqwestHttpTextClient, RewardAdjustmentArtifactSummary, RewardSourceMode,
    ServiceRuntime, ServiceSnapshot, SocketP2pNetwork, INITIAL_EMISSION_RATE_BPS,
    LONG_TERM_TAIL_EMISSION_RATE_BPS, LONG_TERM_TAIL_START_YEAR, TOTAL_SUPPLY,
};
type ClientCommandHandler = pole_protocol_draft::CommandHandler;

const DEFAULT_CONFIG_PATH: &str = "./node.json";
const DEFAULT_CONTROL_API_BIND_ADDR: &str = "127.0.0.1:8787";
const CLIENT_USAGE_COMMANDS: &[&str] = &[
    "  pole-client init [config-path] [minimal|player|validator]",
    "  pole-client player-start [config-path]",
    "  pole-client player-autostart [config-path]",
    "  pole-client status [config-path]",
    "  pole-client doctor [config-path]",
    "  pole-client tokenomics [years]",
    "  pole-client collect [config-path]",
    "  pole-client watch [config-path] [ticks]",
    "  pole-client watch-p2p-sim [config-path] [ticks]",
    "  pole-client watch-p2p-fs [config-path] [ticks]",
    "  pole-client watch-p2p-socket [config-path] [ticks]",
    "  pole-client activity-sources-list [config-path]",
    "  pole-client activity-sources-add [config-path] <app-id> <steam|epic|ea|gog|community> <endpoint-url|inline-json>",
    "  pole-client activity-sources-remove [config-path] <app-id> <steam|epic|ea|gog|community>",
    "  pole-client activity-sources-sync [config-path]",
    "  pole-client reward-config-show [config-path]",
    "  pole-client reward-config-set [config-path] mode <static|tokenomics>",
    "  pole-client reward-config-set [config-path] emission-year <year>",
    "  pole-client reward-config-set [config-path] tail-policy <start-year> <tail-rate-bps>",
    "  pole-client reward-config-set [config-path] service-split <collect_bps> <store_bps> <verify_bps> <propose_bps>",
    "  pole-client governance-propose-params [config-path] <proposal-id-hex> <effective-epoch> <emission-year> <effective-player-block-reward> [tail-start-year tail-rate-bps]",
    "  pole-client governance-propose-slow-params [config-path] <proposal-id-hex> <effective-epoch> <reward-block-secs> <effective-player-block-reward>",
    "  pole-client governance-propose-retention [config-path] <proposal-id-hex> <effective-epoch> <min-retention-epochs> <challenge-window-blocks>",
    "  pole-client governance-propose-app-weight [config-path] <proposal-id-hex> <effective-epoch> <app-id> <game-coefficient-ppm>",
    "  pole-client governance-propose-tier-weights [config-path] <proposal-id-hex> <effective-epoch> <tier1_weight_ppm> <tier2_min_ppm> <tier2_max_ppm> <tier3_min_ppm> <tier3_max_ppm>",
    "  pole-client governance-propose-service-split [config-path] <proposal-id-hex> <effective-epoch> <collect_bps> <store_bps> <verify_bps> <propose_bps>",
    "  pole-client governance-propose-thresholds [config-path] <proposal-id-hex> <effective-epoch> <quorum_bps> <approval_bps>",
    "  pole-client governance-vote [config-path] <proposal-id-hex> <yes|no|abstain> <voting-power>",
    "  pole-client governance-show-proposal [config-path] <proposal-id-hex>",
    "  pole-client governance-show-scheduled [config-path] [epoch-id]",
    "  pole-client governance-show-index [config-path]",
    "  pole-client governance-show-summary [config-path]",
    "  pole-client reward-adjustment-show-index [config-path]",
    "  pole-client reward-adjustment-show-summary [config-path]",
    "  pole-client adjustment-cycle-show-index [config-path]",
    "  pole-client adjustment-cycle-show-summary [config-path]",
    "  pole-client control-api-serve [config-path] [bind-addr]",
    "  pole-client control-api-open [config-path] [bind-addr]",
    "  pole-client libp2p-diagnose [config-path]",
    "  pole-client libp2p-skeleton [config-path]",
    "  pole-client p2p-socket-show [config-path]",
    "  pole-client p2p-socket-add-peer <config-path> <peer-id-hex> <peer-addr> [topics]",
    "  pole-client repair-identity [config-path]",
    "  pole-client capture-foreground-process <config-path>",
    "  pole-client set-game-processes <config-path> <process-name>...",
    "  pole-client aggregate [config-path] [epoch-id]",
    "  pole-client rewards [config-path] [epoch-id]",
    "  pole-client verify [config-path] [epoch-id]",
    "  pole-client build-epoch [config-path] [epoch-id] [current-height] [challenge-window-blocks]",
    "  pole-client prepare-epoch [config-path] [epoch-id] [current-height] [challenge-window-blocks]",
    "  pole-client suggest-settlement-height [config-path]",
    "  pole-client settle-epoch [config-path] [epoch-id] [submission-height] [challenge-window-blocks]",
    "  pole-client prune [config-path] [current-epoch]",
    "  pole-client paths [config-path]",
    "  pole-client wallet-create [data-dir] [password]",
    "  pole-client wallet-recover [data-dir] [password] <24-word-mnemonic...>",
    "  pole-client wallet-address [data-dir]",
    "  pole-client wallet-set-reward-address [config-path] [data-dir] [password]",
    "  pole-client submit-batch [config-path] [epoch-id]",
    "  pole-client submit-epoch [config-path] [epoch-id] [current-height] [challenge-window-blocks]",
    "  pole-client export-tx [config-path] [type] [epoch-id] [current-height] [challenge-window-blocks]",
];
const CLIENT_COMMANDS: &[(&str, ClientCommandHandler)] = &[
    ("init", init_cmd),
    ("player-start", player_start_cmd),
    ("player-autostart", player_autostart_cmd),
    ("status", status_cmd),
    ("doctor", doctor_cmd),
    ("tokenomics", tokenomics_cmd),
    ("collect", collect_cmd),
    ("watch", watch_cmd),
    ("watch-p2p-sim", watch_p2p_sim_cmd),
    ("watch-p2p-fs", watch_p2p_fs_cmd),
    ("watch-p2p-socket", watch_p2p_socket_cmd),
    ("activity-sources-list", activity_sources_list_cmd),
    ("activity-sources-add", activity_sources_add_cmd),
    ("activity-sources-remove", activity_sources_remove_cmd),
    ("activity-sources-sync", activity_sources_sync_cmd),
    ("reward-config-show", reward_config_show_cmd),
    ("reward-config-set", reward_config_set_cmd),
    ("governance-propose-params", governance_propose_params_cmd),
    (
        "governance-propose-reward-tuning",
        governance_propose_reward_tuning_cmd,
    ),
    (
        "governance-propose-slow-params",
        governance_propose_slow_params_cmd,
    ),
    (
        "governance-propose-retention",
        governance_propose_retention_cmd,
    ),
    (
        "governance-propose-tier-weights",
        governance_propose_tier_weights_cmd,
    ),
    (
        "governance-propose-app-weight",
        governance_propose_app_weight_cmd,
    ),
    (
        "governance-propose-service-split",
        governance_propose_service_split_cmd,
    ),
    (
        "governance-propose-thresholds",
        governance_propose_thresholds_cmd,
    ),
    ("governance-vote", governance_vote_cmd),
    ("governance-show-proposal", governance_show_proposal_cmd),
    ("governance-show-scheduled", governance_show_scheduled_cmd),
    ("governance-show-index", governance_show_index_cmd),
    ("governance-show-summary", governance_show_summary_cmd),
    (
        "reward-adjustment-show-index",
        reward_adjustment_show_index_cmd,
    ),
    (
        "reward-adjustment-show-summary",
        reward_adjustment_show_summary_cmd,
    ),
    (
        "adjustment-cycle-show-index",
        reward_adjustment_show_index_cmd,
    ),
    (
        "adjustment-cycle-show-summary",
        reward_adjustment_show_summary_cmd,
    ),
    ("control-api-serve", control_api_serve_cmd),
    ("control-api-open", control_api_open_cmd),
    ("libp2p-diagnose", libp2p_diagnose_cmd),
    ("libp2p-skeleton", libp2p_skeleton_cmd),
    ("p2p-socket-show", p2p_socket_show_cmd),
    ("p2p-socket-add-peer", p2p_socket_add_peer_cmd),
    ("repair-identity", repair_identity_cmd),
    ("capture-foreground-process", capture_foreground_process_cmd),
    ("set-game-processes", set_game_processes_cmd),
    ("aggregate", aggregate_cmd),
    ("rewards", rewards_cmd),
    ("verify", verify_cmd),
    ("build-epoch", build_epoch_cmd),
    ("prepare-epoch", prepare_epoch_cmd),
    ("suggest-settlement-height", suggest_settlement_height_cmd),
    ("settle-epoch", settle_epoch_cmd),
    ("prune", prune_cmd),
    ("paths", paths_cmd),
    ("wallet-create", wallet_create_cmd),
    ("wallet-recover", wallet_recover_cmd),
    ("wallet-address", wallet_address_cmd),
    ("wallet-set-reward-address", wallet_set_reward_address_cmd),
    ("submit-batch", submit_batch_cmd),
    ("submit-epoch", submit_epoch_cmd),
    ("export-tx", export_tx_cmd),
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DaemonMetadata {
    pid: u32,
    background_mode: String,
    config_path: String,
    pid_file: String,
    stdout_log: String,
    stderr_log: String,
    started_at_millis: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClientProfile {
    Minimal,
    Player,
    Validator,
}

impl ClientProfile {
    fn parse(input: &str) -> Option<Self> {
        match input {
            "minimal" => Some(Self::Minimal),
            "player" => Some(Self::Player),
            "validator" => Some(Self::Validator),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Player => "player",
            Self::Validator => "validator",
        }
    }
}

fn main() {
    if let Err(err) = run(&env::args().collect::<Vec<_>>()) {
        eprintln!("pole-client error: {err}");
        std::process::exit(1);
    }
}

pub fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    dispatch_command(args, CLIENT_COMMANDS, print_usage)
}

fn init_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mut config_path = DEFAULT_CONFIG_PATH.to_string();
    let mut profile = ClientProfile::Player;

    match (args.get(2), args.get(3), args.get(4)) {
        (None, None, None) => {}
        (Some(arg), None, None) => {
            if let Some(parsed) = ClientProfile::parse(arg) {
                profile = parsed;
            } else {
                config_path = arg.clone();
            }
        }
        (Some(first), Some(second), None) => {
            config_path = first.clone();
            profile = ClientProfile::parse(second)
                .ok_or("usage: pole-client init [config-path] [minimal|player|validator]")?;
        }
        _ => {
            return Err("usage: pole-client init [config-path] [minimal|player|validator]".into());
        }
    }

    let config_path = PathBuf::from(config_path);
    if config_path.exists() {
        return Err(format!("config already exists at {}", config_path.to_string_lossy()).into());
    }

    let mut config = NodeConfig::default();
    config.runtime.data_dir = default_data_dir_for_config(&config_path);

    let identity_keypair = generate_identity_keypair();
    config.node_id_hex = node_id_hex_from_identity(&identity_keypair);

    apply_profile(&mut config, profile);
    sync_activity_sources(&mut config);
    config.save_json(&config_path)?;
    fs::create_dir_all(&config.runtime.data_dir)?;

    write_identity_file(Path::new(&config.runtime.data_dir), &identity_keypair)?;

    println!("PoLE client initialized");
    println!("config_path={}", config_path.to_string_lossy());
    println!("profile={}", profile.name());
    println!("chain_id={}", config.chain_id);
    println!("node_id={}", config.node_id_hex);
    println!("data_dir={}", config.runtime.data_dir);
    println!("capabilities={}", format_capabilities(&config));
    println!(
        "next_step={}",
        command_hint("status", config_path.to_string_lossy().as_ref())
    );

    Ok(())
}

fn repair_identity_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() > 3 {
        return Err("usage: pole-client repair-identity [config-path]".into());
    }

    let config_path = args
        .get(2)
        .map(String::as_str)
        .unwrap_or(DEFAULT_CONFIG_PATH);
    let (config_path, mut config) = NodeConfig::load_json_with_runtime_paths(config_path)?;

    if !has_placeholder_node_identity(&config) {
        println!("identity_ok=true");
        println!("config_path={}", config_path.to_string_lossy());
        println!("node_id={}", config.node_id_hex);
        return Ok(());
    }

    eprintln!("WARNING: Repairing node identity will invalidate all previously stored data.");
    eprintln!("WARNING: All previously submitted observations and payloads will become unverifiable.");
    eprintln!("WARNING: Clear the data directory ({}) after repair to avoid verification errors.", config.runtime.data_dir);

    let identity_keypair = generate_identity_keypair();
    config.node_id_hex = node_id_hex_from_identity(&identity_keypair);
    config.save_json(&config_path)?;
    write_identity_file(Path::new(&config.runtime.data_dir), &identity_keypair)?;

    println!("identity_repaired=true");
    println!("config_path={}", config_path.to_string_lossy());
    println!("node_id={}", config.node_id_hex);
    println!("data_dir={}", config.runtime.data_dir);
    println!(
        "identity_path={}",
        Path::new(&config.runtime.data_dir)
            .join("identity.json")
            .to_string_lossy()
    );

    Ok(())
}

fn player_start_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() > 3 {
        return Err("usage: pole-client player-start [config-path]".into());
    }

    let config_path = args
        .get(2)
        .map(PathBuf::from)
        .unwrap_or_else(default_player_start_config_path);
    let config_path = resolve_input_path(&config_path)?;
    let mut config = load_or_init_player_config(&config_path)?;

    let foreground_process = detect_foreground_process_name();
    let captured_process = foreground_process
        .as_deref()
        .filter(|name| should_capture_foreground_process(name))
        .map(canonical_process_name);
    if let Some(process_name) = &captured_process {
        let merged_processes = merge_process_names(
            &config.runtime.game_process_names,
            std::slice::from_ref(process_name),
        );
        save_game_process_names(&mut config, &config_path, merged_processes)?;
    }

    let autostart_outcome = ensure_player_autostart(&config_path)?;
    let background_mode = background_watch_mode_from_env();
    let background_ticks = background_watch_ticks_from_env()?;
    let daemon_outcome = if env::var("POLE_CLIENT_SKIP_BACKGROUND_START")
        .ok()
        .as_deref()
        == Some("1")
    {
        BackgroundStartOutcome::Skipped
    } else {
        start_background_watch(&config_path, background_ticks, background_mode)?
    };

    println!("PoLE player mode started");
    println!("config_path={}", config_path.to_string_lossy());
    println!("data_dir={}", config.runtime.data_dir);
    println!("capabilities={}", format_capabilities(&config));
    println!("foreground_process={foreground_process:?}");
    println!("captured_game_process={captured_process:?}");
    println!("game_process_names={:?}", config.runtime.game_process_names);
    println!("reward_block_secs={}", effective_reward_block_secs(&config));
    println!("player_block_reward={}", config.reward.player_block_reward);
    println!(
        "effective_player_block_reward={}",
        effective_player_block_reward(&config)
    );
    println!(
        "reward_adjustment_period_blocks={}",
        config.reward.reward_adjustment_period_blocks
    );
    println!("reward_game_mappings={}", config.reward.game_mappings.len());
    println!(
        "activity_source_count={}",
        config.runtime.activity_sources.len()
    );
    println!("libp2p_enabled={}", config.runtime.p2p_libp2p.enabled);
    println!(
        "libp2p_listen_addrs={:?}",
        config.runtime.p2p_libp2p.listen_addrs
    );
    println!(
        "libp2p_bootstrap_peer_count={}",
        config.runtime.p2p_libp2p.bootstrap_peers.len()
    );
    println!("background_mode={}", background_mode.subcommand());
    match autostart_outcome {
        AutostartRegistrationOutcome::Registered { launcher_path } => {
            println!("autostart_enabled=true");
            println!("autostart_registered=true");
            println!("autostart_launcher={}", launcher_path.to_string_lossy());
        }
        AutostartRegistrationOutcome::AlreadyRegistered { launcher_path } => {
            println!("autostart_enabled=true");
            println!("autostart_registered=true");
            println!("autostart_already_registered=true");
            println!("autostart_launcher={}", launcher_path.to_string_lossy());
        }
        #[cfg(not(windows))]
        AutostartRegistrationOutcome::Unsupported => {
            println!("autostart_enabled=false");
            println!("autostart_supported=false");
        }
    }
    print_background_start_outcome(daemon_outcome);
    print_next_step("status", &config_path);

    Ok(())
}

fn player_autostart_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() > 3 {
        return Err("usage: pole-client player-autostart [config-path]".into());
    }

    let config_path = args
        .get(2)
        .map(PathBuf::from)
        .unwrap_or_else(default_player_start_config_path);
    let config_path = resolve_input_path(&config_path)?;
    let config = load_or_init_player_config(&config_path)?;
    let background_mode = background_watch_mode_from_env();
    let background_ticks = background_watch_ticks_from_env()?;
    let daemon_outcome = start_background_watch(&config_path, background_ticks, background_mode)?;

    println!("PoLE player autostart ensured");
    println!("config_path={}", config_path.to_string_lossy());
    println!("data_dir={}", config.runtime.data_dir);
    println!("capabilities={}", format_capabilities(&config));
    println!("background_mode={}", background_mode.subcommand());
    print_background_start_outcome(daemon_outcome);

    Ok(())
}

fn status_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() > 3 {
        return Err("usage: pole-client status [config-path]".into());
    }

    let config_path = args
        .get(2)
        .map(String::as_str)
        .unwrap_or(DEFAULT_CONFIG_PATH);
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path)?;
    let summary = load_status(&config)?;
    let active_game_processes = detect_active_game_processes(&config);
    let foreground_process = detect_foreground_process_name();
    let game_throttle_active = effective_collect_interval_secs(&config, &active_game_processes)
        > config.runtime.poll_interval_secs;
    let daemon_status =
        DaemonFiles::from_data_dir(Path::new(&config.runtime.data_dir)).probe_status();
    let autostart_launcher = probe_player_autostart(&config_path);

    println!("PoLE client status");
    println!("config_path={}", config_path.to_string_lossy());
    println!("chain_id={}", config.chain_id);
    println!("node_id={}", config.node_id_hex);
    println!(
        "node_identity_placeholder={}",
        has_placeholder_node_identity(&config)
    );
    println!("reward_address={}", config.reward_address_hex);
    println!("capabilities={}", format_capabilities(&config));
    println!("targets={:?}", config.runtime.target_app_ids);
    println!("data_dir={}", config.runtime.data_dir);
    println!("low_impact_mode={}", summary.low_impact_mode);
    println!("os_background_priority={}", summary.os_background_priority);
    println!("inline_verify_enabled={}", summary.inline_verify_enabled);
    println!("inline_propose_enabled={}", summary.inline_propose_enabled);
    println!(
        "game_process_awareness_configured={}",
        !config.runtime.game_process_names.is_empty()
    );
    println!(
        "game_active_poll_interval_secs={}",
        config.runtime.game_active_poll_interval_secs
    );
    println!("game_process_names={:?}", config.runtime.game_process_names);
    println!("reward_block_secs={}", effective_reward_block_secs(&config));
    println!("reward_source={}", summary.reward_source);
    println!("emission_year={}", summary.emission_year);
    println!("player_block_reward={}", config.reward.player_block_reward);
    println!(
        "effective_player_block_reward={}",
        summary.effective_player_block_reward
    );
    println!(
        "challenge_window_blocks={}",
        summary.challenge_window_blocks
    );
    println!(
        "reward_adjustment_period_blocks={}",
        summary.reward_adjustment_period_blocks
    );
    println!(
        "target_network_weight_units={}",
        summary.target_network_weight_units
    );
    println!(
        "reward_adjustment_cap_bps={}",
        summary.reward_adjustment_cap_bps
    );
    println!("reward_game_mappings={}", config.reward.game_mappings.len());
    println!(
        "activity_source_count={}",
        config.runtime.activity_sources.len()
    );
    println!("libp2p_enabled={}", config.runtime.p2p_libp2p.enabled);
    println!(
        "libp2p_listen_addrs={:?}",
        config.runtime.p2p_libp2p.listen_addrs
    );
    println!(
        "libp2p_bootstrap_peer_count={}",
        config.runtime.p2p_libp2p.bootstrap_peers.len()
    );
    println!("foreground_process={foreground_process:?}");
    println!("active_game_processes={:?}", active_game_processes);
    println!("game_throttle_active={game_throttle_active}");
    println!(
        "effective_poll_interval_secs={}",
        effective_collect_interval_secs(&config, &active_game_processes)
    );
    println!("autostart_enabled={}", autostart_launcher.is_some());
    if let Some(path) = autostart_launcher {
        println!("autostart_launcher={}", path.to_string_lossy());
    }
    println!(
        "daemon_running={}",
        daemon_status.snapshot.state_label == "running"
    );
    if let Some(pid) = daemon_status.snapshot.pid {
        println!("daemon_pid={pid}");
    }
    print_daemon_metadata_summary(daemon_status.metadata.as_ref());
    print_client_node_status(summary);

    Ok(())
}

fn print_client_node_status(summary: pole_protocol_draft::NodeStatusSummary) {
    println!("next_epoch={}", summary.next_epoch_id);
    println!("next_slot={}", summary.next_slot_id);
    println!("ticks_completed={}", summary.ticks_completed);
    println!(
        "reward_blocks_completed={}",
        summary.reward_blocks_completed
    );
    println!("reward_source={}", summary.reward_source);
    println!("emission_year={}", summary.emission_year);
    println!(
        "configured_fixed_player_reward={}",
        summary.configured_fixed_player_reward
    );
    println!(
        "effective_fixed_player_reward={}",
        summary.effective_fixed_player_reward
    );
    println!(
        "configured_player_block_reward={}",
        summary.configured_player_block_reward
    );
    println!(
        "effective_player_block_reward={}",
        summary.effective_player_block_reward
    );
    println!(
        "effective_min_retention_epochs={}",
        summary.effective_min_retention_epochs
    );
    println!(
        "effective_app_weight_override_count={}",
        summary.effective_app_weight_override_count
    );
    println!(
        "challenge_window_blocks={}",
        summary.challenge_window_blocks
    );
    println!(
        "target_network_weight_units={}",
        summary.target_network_weight_units
    );
    println!(
        "reward_adjustment_cap_bps={}",
        summary.reward_adjustment_cap_bps
    );
    println!(
        "current_adjustment_cycle_index={}",
        summary.current_adjustment_cycle_index
    );
    println!(
        "current_fixed_player_reward={}",
        summary.current_fixed_player_reward
    );
    println!(
        "adjustment_cycle_blocks={}",
        summary.adjustment_cycle_blocks
    );
    println!(
        "current_fixed_player_reward_basis_cycle_index={}",
        summary.current_fixed_player_reward_basis_cycle_index
    );
    println!(
        "current_fixed_player_reward_basis_total_network_weight_units={}",
        summary.current_fixed_player_reward_basis_total_network_weight_units
    );
    println!(
        "current_reward_adjustment_artifact_path={}",
        summary.current_reward_adjustment_artifact_path
    );
    println!(
        "current_reward_adjustment_artifact_exists={}",
        summary.current_reward_adjustment_artifact_exists
    );
    println!(
        "reward_adjustment_artifact_count={}",
        summary.reward_adjustment_artifact_count
    );
    println!(
        "adjustment_cycle_artifact_count={}",
        summary.adjustment_cycle_artifact_count
    );
    println!(
        "reward_adjustment_artifact_index_path={}",
        summary.reward_adjustment_artifact_index_path
    );
    println!(
        "reward_adjustment_artifact_summary_path={}",
        summary.reward_adjustment_artifact_summary_path
    );
    println!(
        "previous_adjustment_cycle_total_network_weight_units={}",
        summary.previous_adjustment_cycle_total_network_weight_units
    );
    println!(
        "current_adjustment_cycle_total_network_weight_units={}",
        summary.current_adjustment_cycle_total_network_weight_units
    );
    println!(
        "current_reward_adjustment_period_index={}",
        summary.current_reward_adjustment_period_index
    );
    println!(
        "current_fixed_block_reward={}",
        summary.current_fixed_block_reward
    );
    println!(
        "current_fixed_block_reward_basis_period_index={}",
        summary.current_fixed_block_reward_basis_period_index
    );
    println!(
        "current_fixed_block_reward_basis_network_weight_units={}",
        summary.current_fixed_block_reward_basis_network_weight_units
    );
    println!(
        "previous_reward_adjustment_period_network_weight_units={}",
        summary.previous_reward_adjustment_period_network_weight_units
    );
    println!(
        "current_reward_adjustment_period_network_weight_units={}",
        summary.current_reward_adjustment_period_network_weight_units
    );
    println!(
        "pending_reward_block_seconds={}",
        summary.pending_reward_block_seconds
    );
    println!(
        "pending_reward_block_ticks={}",
        summary.pending_reward_block_ticks
    );
    println!(
        "pending_reward_block_remaining_seconds={}",
        summary.pending_reward_block_remaining_seconds
    );
    println!(
        "pending_reward_block_entry_count={}",
        summary.pending_reward_block_entry_count
    );
    println!("stored_payloads={}", summary.stored_payload_count);
    println!("used_bytes={}", summary.used_bytes);
    println!("quota_bytes={}", summary.quota_bytes);
    println!(
        "configured_p2p_batch_listeners={}",
        summary.configured_p2p_batch_listener_count
    );
    println!(
        "configured_p2p_receipt_listeners={}",
        summary.configured_p2p_receipt_listener_count
    );
    println!(
        "configured_p2p_dual_listeners={}",
        summary.configured_p2p_dual_listener_count
    );
    println!("last_tick_at_millis={:?}", summary.last_tick_at_millis);
    println!("last_payload_cid={:?}", summary.last_payload_cid);
    println!(
        "last_aggregate_epoch_id={:?}",
        summary.last_aggregate_epoch_id
    );
    println!(
        "last_aggregate_gvs_tier={:?}",
        summary.last_aggregate_gvs_tier
    );
    println!(
        "last_aggregate_source_kind={:?}",
        summary.last_aggregate_source_kind
    );
    println!(
        "last_aggregate_source_confidence_ppm={:?}",
        summary.last_aggregate_source_confidence_ppm
    );
    println!(
        "last_p2p_batch_recipients={:?}",
        summary.last_p2p_batch_recipients
    );
    println!(
        "last_p2p_receipt_recipients={:?}",
        summary.last_p2p_receipt_recipients
    );
    println!("last_p2p_retrieval_ok={:?}", summary.last_p2p_retrieval_ok);
    println!(
        "last_p2p_retrieval_error={:?}",
        summary.last_p2p_retrieval_error
    );
    println!("last_p2p_transport={:?}", summary.last_p2p_transport);
    println!(
        "last_p2p_known_peer_count={:?}",
        summary.last_p2p_known_peer_count
    );
    println!(
        "last_p2p_learned_remote_peer_count={:?}",
        summary.last_p2p_learned_remote_peer_count
    );
    println!(
        "last_p2p_batch_listener_count={:?}",
        summary.last_p2p_batch_listener_count
    );
    println!(
        "last_p2p_receipt_listener_count={:?}",
        summary.last_p2p_receipt_listener_count
    );
    println!(
        "last_p2p_challenge_listener_count={:?}",
        summary.last_p2p_challenge_listener_count
    );
    println!(
        "last_p2p_coordination_sent_count={:?}",
        summary.last_p2p_coordination_sent_count
    );
    println!(
        "last_p2p_coordination_received_count={:?}",
        summary.last_p2p_coordination_received_count
    );
    println!(
        "last_p2p_hello_sent_count={:?}",
        summary.last_p2p_hello_sent_count
    );
    println!(
        "last_p2p_hint_sent_count={:?}",
        summary.last_p2p_hint_sent_count
    );
    println!(
        "last_p2p_goodbye_sent_count={:?}",
        summary.last_p2p_goodbye_sent_count
    );
    println!(
        "last_p2p_hello_received_count={:?}",
        summary.last_p2p_hello_received_count
    );
    println!(
        "last_p2p_hint_received_count={:?}",
        summary.last_p2p_hint_received_count
    );
    println!(
        "last_p2p_goodbye_received_count={:?}",
        summary.last_p2p_goodbye_received_count
    );
    println!(
        "last_p2p_challenge_recipients={:?}",
        summary.last_p2p_challenge_recipients
    );
    println!(
        "last_p2p_challenge_kind={:?}",
        summary.last_p2p_challenge_kind
    );
    println!(
        "last_p2p_challenge_epoch_id={:?}",
        summary.last_p2p_challenge_epoch_id
    );
    println!(
        "last_p2p_challenge_payload_cid={:?}",
        summary.last_p2p_challenge_payload_cid
    );
    println!(
        "p2p_challenge_events_total={:?}",
        summary.p2p_challenge_events_total
    );
    println!(
        "p2p_bad_batch_challenge_events={:?}",
        summary.p2p_bad_batch_challenge_events
    );
    println!(
        "p2p_omission_challenge_events={:?}",
        summary.p2p_omission_challenge_events
    );
    println!(
        "p2p_bad_aggregate_challenge_events={:?}",
        summary.p2p_bad_aggregate_challenge_events
    );
    println!(
        "p2p_bad_reward_challenge_events={:?}",
        summary.p2p_bad_reward_challenge_events
    );
    println!(
        "p2p_bad_storage_challenge_events={:?}",
        summary.p2p_bad_storage_challenge_events
    );
    println!(
        "recent_p2p_challenge_events={:?}",
        summary.recent_p2p_challenge_events
    );
    println!(
        "recent_p2p_bad_batch_challenge_events={:?}",
        summary.recent_p2p_bad_batch_challenge_events
    );
    println!(
        "recent_p2p_omission_challenge_events={:?}",
        summary.recent_p2p_omission_challenge_events
    );
    println!(
        "recent_p2p_bad_aggregate_challenge_events={:?}",
        summary.recent_p2p_bad_aggregate_challenge_events
    );
    println!(
        "recent_p2p_bad_reward_challenge_events={:?}",
        summary.recent_p2p_bad_reward_challenge_events
    );
    println!(
        "recent_p2p_bad_storage_challenge_events={:?}",
        summary.recent_p2p_bad_storage_challenge_events
    );
    println!(
        "p2p_challenge_delivered_events_total={:?}",
        summary.p2p_challenge_delivered_events_total
    );
    println!(
        "p2p_challenge_zero_recipient_events_total={:?}",
        summary.p2p_challenge_zero_recipient_events_total
    );
    println!(
        "recent_p2p_challenge_delivered_events={:?}",
        summary.recent_p2p_challenge_delivered_events
    );
    println!(
        "recent_p2p_challenge_zero_recipient_events={:?}",
        summary.recent_p2p_challenge_zero_recipient_events
    );
    println!(
        "recent_p2p_challenge_recipient_sum={:?}",
        summary.recent_p2p_challenge_recipient_sum
    );
    println!(
        "last_retention_all_retrievable={:?}",
        summary.last_retention_all_retrievable
    );
    println!(
        "last_retention_retained_payload_count={:?}",
        summary.last_retention_retained_payload_count
    );
    println!(
        "last_retention_retrievable_payload_count={:?}",
        summary.last_retention_retrievable_payload_count
    );
    println!(
        "last_retention_missing_payload_count={:?}",
        summary.last_retention_missing_payload_count
    );
    println!(
        "last_retention_corrupted_payload_count={:?}",
        summary.last_retention_corrupted_payload_count
    );
    println!(
        "last_storage_challenge_all_passed={:?}",
        summary.last_storage_challenge_all_passed
    );
    println!(
        "last_storage_challenge_checked_payload_count={:?}",
        summary.last_storage_challenge_checked_payload_count
    );
    println!(
        "last_storage_challenge_failed_payload_count={:?}",
        summary.last_storage_challenge_failed_payload_count
    );
    println!(
        "last_storage_challenge_error={:?}",
        summary.last_storage_challenge_error
    );
    println!(
        "last_auto_settlement_pending_epoch_count={:?}",
        summary.last_auto_settlement_pending_epoch_count
    );
    println!(
        "last_auto_settled_epoch={:?}",
        summary.last_auto_settled_epoch
    );
    println!(
        "last_auto_settlement_reward_claimed={:?}",
        summary.last_auto_settlement_reward_claimed
    );
    println!(
        "last_auto_settlement_error={:?}",
        summary.last_auto_settlement_error
    );
}

fn print_daemon_metadata_summary(metadata: Option<&DaemonMetadata>) {
    println!("daemon_meta_exists={}", metadata.is_some());
    if let Some(metadata) = metadata {
        println!("daemon_background_mode={}", metadata.background_mode);
        println!("daemon_config_path={}", metadata.config_path);
        println!("daemon_pid_file={}", metadata.pid_file);
        println!("daemon_stdout_log={}", metadata.stdout_log);
        println!("daemon_stderr_log={}", metadata.stderr_log);
        println!("daemon_started_at_millis={}", metadata.started_at_millis);
    }
}

fn print_client_network_and_retention_diagnostics(
    summary: &pole_protocol_draft::NodeStatusSummary,
) {
    println!(
        "configured_p2p_batch_listeners={}",
        summary.configured_p2p_batch_listener_count
    );
    println!(
        "configured_p2p_receipt_listeners={}",
        summary.configured_p2p_receipt_listener_count
    );
    println!(
        "configured_p2p_dual_listeners={}",
        summary.configured_p2p_dual_listener_count
    );
    println!(
        "last_p2p_batch_recipients={:?}",
        summary.last_p2p_batch_recipients
    );
    println!(
        "last_p2p_receipt_recipients={:?}",
        summary.last_p2p_receipt_recipients
    );
    println!("last_p2p_retrieval_ok={:?}", summary.last_p2p_retrieval_ok);
    println!(
        "last_p2p_retrieval_error={:?}",
        summary.last_p2p_retrieval_error
    );
    println!("last_p2p_transport={:?}", summary.last_p2p_transport);
    println!(
        "last_p2p_known_peer_count={:?}",
        summary.last_p2p_known_peer_count
    );
    println!(
        "last_p2p_learned_remote_peer_count={:?}",
        summary.last_p2p_learned_remote_peer_count
    );
    println!(
        "last_p2p_batch_listener_count={:?}",
        summary.last_p2p_batch_listener_count
    );
    println!(
        "last_p2p_receipt_listener_count={:?}",
        summary.last_p2p_receipt_listener_count
    );
    println!(
        "last_p2p_challenge_listener_count={:?}",
        summary.last_p2p_challenge_listener_count
    );
    println!(
        "last_p2p_coordination_sent_count={:?}",
        summary.last_p2p_coordination_sent_count
    );
    println!(
        "last_p2p_coordination_received_count={:?}",
        summary.last_p2p_coordination_received_count
    );
    println!(
        "last_p2p_hello_sent_count={:?}",
        summary.last_p2p_hello_sent_count
    );
    println!(
        "last_p2p_hint_sent_count={:?}",
        summary.last_p2p_hint_sent_count
    );
    println!(
        "last_p2p_goodbye_sent_count={:?}",
        summary.last_p2p_goodbye_sent_count
    );
    println!(
        "last_p2p_hello_received_count={:?}",
        summary.last_p2p_hello_received_count
    );
    println!(
        "last_p2p_hint_received_count={:?}",
        summary.last_p2p_hint_received_count
    );
    println!(
        "last_p2p_goodbye_received_count={:?}",
        summary.last_p2p_goodbye_received_count
    );
    println!(
        "last_p2p_challenge_recipients={:?}",
        summary.last_p2p_challenge_recipients
    );
    println!(
        "last_p2p_challenge_kind={:?}",
        summary.last_p2p_challenge_kind
    );
    println!(
        "last_p2p_challenge_epoch_id={:?}",
        summary.last_p2p_challenge_epoch_id
    );
    println!(
        "last_p2p_challenge_payload_cid={:?}",
        summary.last_p2p_challenge_payload_cid
    );
    println!(
        "p2p_challenge_events_total={:?}",
        summary.p2p_challenge_events_total
    );
    println!(
        "p2p_bad_batch_challenge_events={:?}",
        summary.p2p_bad_batch_challenge_events
    );
    println!(
        "p2p_omission_challenge_events={:?}",
        summary.p2p_omission_challenge_events
    );
    println!(
        "p2p_bad_aggregate_challenge_events={:?}",
        summary.p2p_bad_aggregate_challenge_events
    );
    println!(
        "p2p_bad_reward_challenge_events={:?}",
        summary.p2p_bad_reward_challenge_events
    );
    println!(
        "p2p_bad_storage_challenge_events={:?}",
        summary.p2p_bad_storage_challenge_events
    );
    println!(
        "recent_p2p_challenge_events={:?}",
        summary.recent_p2p_challenge_events
    );
    println!(
        "recent_p2p_bad_batch_challenge_events={:?}",
        summary.recent_p2p_bad_batch_challenge_events
    );
    println!(
        "recent_p2p_omission_challenge_events={:?}",
        summary.recent_p2p_omission_challenge_events
    );
    println!(
        "recent_p2p_bad_aggregate_challenge_events={:?}",
        summary.recent_p2p_bad_aggregate_challenge_events
    );
    println!(
        "recent_p2p_bad_reward_challenge_events={:?}",
        summary.recent_p2p_bad_reward_challenge_events
    );
    println!(
        "recent_p2p_bad_storage_challenge_events={:?}",
        summary.recent_p2p_bad_storage_challenge_events
    );
    println!(
        "p2p_challenge_delivered_events_total={:?}",
        summary.p2p_challenge_delivered_events_total
    );
    println!(
        "p2p_challenge_zero_recipient_events_total={:?}",
        summary.p2p_challenge_zero_recipient_events_total
    );
    println!(
        "recent_p2p_challenge_delivered_events={:?}",
        summary.recent_p2p_challenge_delivered_events
    );
    println!(
        "recent_p2p_challenge_zero_recipient_events={:?}",
        summary.recent_p2p_challenge_zero_recipient_events
    );
    println!(
        "recent_p2p_challenge_recipient_sum={:?}",
        summary.recent_p2p_challenge_recipient_sum
    );
    println!(
        "last_retention_all_retrievable={:?}",
        summary.last_retention_all_retrievable
    );
    println!(
        "last_retention_retained_payload_count={:?}",
        summary.last_retention_retained_payload_count
    );
    println!(
        "last_retention_retrievable_payload_count={:?}",
        summary.last_retention_retrievable_payload_count
    );
    println!(
        "last_retention_missing_payload_count={:?}",
        summary.last_retention_missing_payload_count
    );
    println!(
        "last_retention_corrupted_payload_count={:?}",
        summary.last_retention_corrupted_payload_count
    );
    println!(
        "last_storage_challenge_all_passed={:?}",
        summary.last_storage_challenge_all_passed
    );
    println!(
        "last_storage_challenge_checked_payload_count={:?}",
        summary.last_storage_challenge_checked_payload_count
    );
    println!(
        "last_storage_challenge_failed_payload_count={:?}",
        summary.last_storage_challenge_failed_payload_count
    );
    println!(
        "last_storage_challenge_error={:?}",
        summary.last_storage_challenge_error
    );
    println!(
        "last_auto_settlement_pending_epoch_count={:?}",
        summary.last_auto_settlement_pending_epoch_count
    );
    println!(
        "last_auto_settled_epoch={:?}",
        summary.last_auto_settled_epoch
    );
    println!(
        "last_auto_settlement_reward_claimed={:?}",
        summary.last_auto_settlement_reward_claimed
    );
    println!(
        "last_auto_settlement_error={:?}",
        summary.last_auto_settlement_error
    );
}

fn print_epoch_commit_artifact_roots(
    accepted_batches_root_hex: &str,
    observations_root_hex: &str,
    aggregates_root_hex: &str,
    rewards_root_hex: &str,
    availability_root_hex: &str,
    challenge_deadline_height: u64,
) {
    println!("accepted_batches_root={accepted_batches_root_hex}");
    println!("observations_root={observations_root_hex}");
    println!("aggregates_root={aggregates_root_hex}");
    println!("rewards_root={rewards_root_hex}");
    println!("availability_root={availability_root_hex}");
    println!("challenge_deadline_height={challenge_deadline_height}");
}

fn doctor_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() > 3 {
        return Err("usage: pole-client doctor [config-path]".into());
    }

    let config_path = args
        .get(2)
        .map(String::as_str)
        .unwrap_or(DEFAULT_CONFIG_PATH);
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path)?;
    let data_dir = PathBuf::from(&config.runtime.data_dir);
    let progress = progress_path(&config);
    let retention = retention_book_path(&config);
    let heartbeat = heartbeat_path(&config);
    let active_game_processes = detect_active_game_processes(&config);
    let foreground_process = detect_foreground_process_name();
    let daemon_status =
        DaemonFiles::from_data_dir(Path::new(&config.runtime.data_dir)).probe_status();
    let autostart_launcher = probe_player_autostart(&config_path);

    let node_id_ok = config.node_id().is_ok();
    let placeholder_node_identity = has_placeholder_node_identity(&config);
    let reward_address_ok = config.reward_address().is_ok();
    let targets_ok = !config.runtime.target_app_ids.is_empty();
    let collect_consistent = !config.collect.enabled || config.capabilities.collect;
    let game_safe_mode = config.runtime.low_impact_mode && config.runtime.os_background_priority;
    let data_dir_exists = data_dir.exists();
    let data_dir_parent_exists = data_dir
        .parent()
        .map(|parent| parent.exists())
        .unwrap_or(true);

    let overall_ok = node_id_ok
        && !placeholder_node_identity
        && reward_address_ok
        && targets_ok
        && collect_consistent
        && data_dir_parent_exists;

    println!("PoLE client doctor");
    println!("config_path={}", config_path.to_string_lossy());
    println!("overall_ok={overall_ok}");
    println!("node_id_ok={node_id_ok}");
    println!("node_identity_placeholder={placeholder_node_identity}");
    println!("reward_address_ok={reward_address_ok}");
    println!("target_app_ids_ok={targets_ok}");
    println!("collect_config_consistent={collect_consistent}");
    println!("capabilities={}", format_capabilities(&config));
    println!("low_impact_mode={}", config.runtime.low_impact_mode);
    println!(
        "os_background_priority={}",
        config.runtime.os_background_priority
    );
    println!("inline_verify_enabled={}", config.inline_verify_enabled());
    println!("inline_propose_enabled={}", config.inline_propose_enabled());
    println!("game_safe_mode={game_safe_mode}");
    println!(
        "game_process_awareness_configured={}",
        !config.runtime.game_process_names.is_empty()
    );
    println!(
        "game_active_poll_interval_secs={}",
        config.runtime.game_active_poll_interval_secs
    );
    println!("game_process_names={:?}", config.runtime.game_process_names);
    println!("reward_block_secs={}", effective_reward_block_secs(&config));
    println!("reward_source={}", config.reward.reward_source_label());
    println!("emission_year={}", config.reward.emission_year);
    println!(
        "tail_emission_start_year={}",
        config.reward.tail_emission_start_year
    );
    println!(
        "tail_emission_rate_bps={}",
        config.reward.tail_emission_rate_bps
    );
    println!("player_block_reward={}", config.reward.player_block_reward);
    println!(
        "effective_player_block_reward={}",
        effective_player_block_reward(&config)
    );
    println!(
        "challenge_window_blocks={}",
        effective_challenge_window_blocks(&config)
    );
    println!(
        "reward_adjustment_period_blocks={}",
        config.reward.reward_adjustment_period_blocks
    );
    println!(
        "target_network_weight_units={}",
        effective_target_network_weight_units(&config)
    );
    println!(
        "reward_adjustment_cap_bps={}",
        effective_reward_adjustment_cap_bps(&config)
    );
    println!("reward_game_mappings={}", config.reward.game_mappings.len());
    println!("foreground_process={foreground_process:?}");
    println!("active_game_processes={:?}", active_game_processes);
    println!(
        "game_throttle_active={}",
        effective_collect_interval_secs(&config, &active_game_processes)
            > config.runtime.poll_interval_secs
    );
    println!(
        "effective_poll_interval_secs={}",
        effective_collect_interval_secs(&config, &active_game_processes)
    );
    println!("autostart_enabled={}", autostart_launcher.is_some());
    if let Some(path) = autostart_launcher {
        println!("autostart_launcher={}", path.to_string_lossy());
    }
    println!(
        "daemon_running={}",
        daemon_status.snapshot.state_label == "running"
    );
    if let Some(pid) = daemon_status.snapshot.pid {
        println!("daemon_pid={pid}");
    }
    print_daemon_metadata_summary(daemon_status.metadata.as_ref());
    let summary = load_status(&config)?;
    print_client_network_and_retention_diagnostics(&summary);
    println!("data_dir={}", data_dir.to_string_lossy());
    println!("data_dir_exists={data_dir_exists}");
    println!("data_dir_parent_exists={data_dir_parent_exists}");
    println!("runtime_state_exists={}", progress.exists());
    println!("retention_book_exists={}", retention.exists());
    println!("heartbeat_exists={}", heartbeat.exists());

    if !overall_ok {
        println!(
            "hint=run `pole-client init` for a fresh workspace or fix the config fields above"
        );
        if placeholder_node_identity {
            println!("hint_identity=run `pole-client repair-identity {}` to replace placeholder node id", config_path.to_string_lossy());
        }
    }

    Ok(())
}

fn tokenomics_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() > 3 {
        return Err("usage: pole-client tokenomics [years]".into());
    }

    let years = args
        .get(2)
        .map(|value| value.parse::<u32>())
        .transpose()?
        .unwrap_or(10);
    let breakdown = allocation_breakdown();
    let schedule = annual_emission_schedule_with_tail(
        years,
        LONG_TERM_TAIL_START_YEAR,
        LONG_TERM_TAIL_EMISSION_RATE_BPS,
    );

    println!("PoLE tokenomics");
    println!("total_supply={TOTAL_SUPPLY}");
    println!("initial_emission_rate_bps={INITIAL_EMISSION_RATE_BPS}");
    println!("tail_emission_start_year={LONG_TERM_TAIL_START_YEAR}");
    println!("tail_emission_rate_bps={LONG_TERM_TAIL_EMISSION_RATE_BPS}");
    println!("player_rewards_allocation={}", breakdown.player_rewards);
    println!("service_rewards_allocation={}", breakdown.service_rewards);
    println!("treasury_allocation={}", breakdown.treasury);
    println!("team_allocation={}", breakdown.team);
    println!("early_supporters_allocation={}", breakdown.early_supporters);
    for row in schedule {
        println!(
            "year={} nominal_rate_bps={} annual_emission={} cumulative_emission={}",
            row.year, row.nominal_rate_bps, row.annual_emission, row.cumulative_emission
        );
    }

    Ok(())
}

fn collect_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() > 3 {
        return Err("usage: pole-client collect [config-path]".into());
    }

    let config_path = args
        .get(2)
        .map(String::as_str)
        .unwrap_or(DEFAULT_CONFIG_PATH);
    let (config_path, mut config) = NodeConfig::load_json_with_runtime_paths(config_path)?;

    let mut config_changed = false;
    if let Some(foreground) = detect_foreground_process_name() {
        if should_capture_foreground_process(&foreground) {
            let game_process = canonical_process_name(&foreground);
            let merged = merge_process_names(&config.runtime.game_process_names, &[game_process.clone()]);
            if !merged.is_empty() && merged != config.runtime.game_process_names {
                configure_game_process_awareness(&mut config);
                config.runtime.game_process_names = merged;
                sync_reward_game_mappings(&mut config);
                sync_activity_sources(&mut config);
                config.save_json(&config_path)?;
                config_changed = true;
                println!("game_process_detected={}", game_process);
                println!("game_mappings_updated={}", !config.reward.game_mappings.is_empty());
            }
        }
    }

    let mut progress = LocalNodeProgress::load_or_default(progress_path(&config), &config)?;
    let client = ReqwestHttpTextClient;
    let result = run_collect_tick_with_client(&config, &mut progress, &client)?;

    println!("PoLE client collect");
    println!("config_path={}", config_path.to_string_lossy());
    println!("tick_epoch={}", result.artifact.epoch_id);
    println!("tick_slot={}", result.artifact.slot_id);
    println!("payload_cid={}", result.artifact.payload_cid);
    println!("obs_count={}", result.artifact.obs_count);
    println!(
        "player_reward_block_count={}",
        result.artifact.player_reward_block_count
    );
    println!(
        "player_reward_total={}",
        result.artifact.player_reward_total
    );
    println!(
        "reward_process_name={:?}",
        result.artifact.reward_process_name
    );
    println!("low_impact_mode={}", config.runtime.low_impact_mode);
    println!("inline_verify_enabled={}", config.inline_verify_enabled());
    println!("inline_propose_enabled={}", config.inline_propose_enabled());
    println!("next_epoch={}", result.progress.next_epoch_id);
    println!("next_slot={}", result.progress.next_slot_id);
    if config_changed {
        println!("game_awareness_enabled=true");
    }
    print_tick_auto_settlement_summary(&result);
    print_tick_retention_summary(&result);
    if let Some(artifact) = &result.aggregation_artifact {
        println!("aggregate_count={}", artifact.aggregate_count);
        println!("aggregates_root={}", artifact.aggregate_root_hex);
    }
    if let Some(artifact) = &result.reward_artifact {
        println!("reward_count={}", artifact.reward_count);
        println!("rewards_root={}", artifact.reward_root_hex);
    }
    if let Some(report) = &result.verification_report {
        println!("verify_all_valid={}", report.all_valid);
    }
    if let Some(artifact) = &result.epoch_commit_artifact {
        println!(
            "epoch_commit_deadline_height={}",
            artifact.challenge_deadline_height
        );
    }

    Ok(())
}

fn watch_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let (config_path, start_index) = parse_config_path_and_rest(args, 2, DEFAULT_CONFIG_PATH);
    if args.len() > start_index + 1 {
        return Err("usage: pole-client watch [config-path] [ticks]".into());
    }

    let ticks = args
        .get(start_index)
        .map(|value| value.parse::<u64>())
        .transpose()?;
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path)?;
    let client = ReqwestHttpTextClient;
    let summary = summarize_collect_loop_with_client(&config, &client, ticks)?;
    print_watch_execution(&config_path, &config, &summary, None)?;

    Ok(())
}

fn watch_p2p_sim_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let (config_path, start_index) = parse_config_path_and_rest(args, 2, DEFAULT_CONFIG_PATH);
    if args.len() > start_index + 1 {
        return Err("usage: pole-client watch-p2p-sim [config-path] [ticks]".into());
    }

    let ticks = args
        .get(start_index)
        .map(|value| value.parse::<u64>())
        .transpose()?;
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path)?;
    let client = ReqwestHttpTextClient;
    let mut network = build_inmemory_simulation_network(config.runtime.p2p_simulation);
    let summary =
        summarize_collect_loop_with_client_and_network(&config, &client, ticks, &mut network)?;
    print_watch_execution(&config_path, &config, &summary, Some("inmemory-sim"))?;

    Ok(())
}

fn watch_p2p_fs_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let (config_path, start_index) = parse_config_path_and_rest(args, 2, DEFAULT_CONFIG_PATH);
    if args.len() > start_index + 1 {
        return Err("usage: pole-client watch-p2p-fs [config-path] [ticks]".into());
    }

    let ticks = args
        .get(start_index)
        .map(|value| value.parse::<u64>())
        .transpose()?;
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path)?;
    let client = ReqwestHttpTextClient;
    let mut network = FilesystemP2pNetwork::new(default_filesystem_network_dir(&config));
    let summary =
        summarize_collect_loop_with_client_and_network(&config, &client, ticks, &mut network)?;
    print_watch_execution(&config_path, &config, &summary, Some("filesystem"))?;

    Ok(())
}

fn watch_p2p_socket_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let (config_path, start_index) = parse_config_path_and_rest(args, 2, DEFAULT_CONFIG_PATH);
    if args.len() > start_index + 1 {
        return Err("usage: pole-client watch-p2p-socket [config-path] [ticks]".into());
    }

    let ticks = args
        .get(start_index)
        .map(|value| value.parse::<u64>())
        .transpose()?;
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path)?;
    let client = ReqwestHttpTextClient;
    let mut network = build_socket_watch_network(&config)?;
    let summary =
        summarize_collect_loop_with_client_and_network(&config, &client, ticks, &mut network)?;
    print_watch_execution(&config_path, &config, &summary, Some("socket"))?;

    Ok(())
}

fn activity_sources_list_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() > 3 {
        return Err("usage: pole-client activity-sources-list [config-path]".into());
    }
    let config_path = args
        .get(2)
        .map(String::as_str)
        .unwrap_or(DEFAULT_CONFIG_PATH);
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path)?;
    println!("PoLE client activity sources");
    println!("config_path={}", config_path.to_string_lossy());
    println!(
        "activity_source_count={}",
        config.runtime.activity_sources.len()
    );
    for source in &config.runtime.activity_sources {
        println!(
            "app_id={} source_kind={:?} endpoint_url={:?} inline_json_present={}",
            source.app_id,
            source.source_kind,
            source.endpoint_url,
            source.inline_json.is_some()
        );
    }
    Ok(())
}

fn activity_sources_add_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 5 && args.len() != 6 {
        return Err(
            "usage: pole-client activity-sources-add [config-path] <app-id> <steam|epic|ea|gog|community> <endpoint-url|inline-json>"
                .into(),
        );
    }
    let (config_path_arg, start_index) = parse_config_path_and_rest(args, 2, DEFAULT_CONFIG_PATH);
    if args.len() != start_index + 3 {
        return Err(
            "usage: pole-client activity-sources-add [config-path] <app-id> <steam|epic|ea|gog|community> <endpoint-url|inline-json>"
                .into(),
        );
    }
    let (config_path, mut config) = NodeConfig::load_json_with_runtime_paths(config_path_arg)?;
    let app_id: u32 = args[start_index].parse()?;
    let source_kind = parse_activity_source_kind(&args[start_index + 1])?;
    let source_value = args[start_index + 2].clone();
    config
        .runtime
        .activity_sources
        .retain(|source| !(source.app_id == app_id && source.source_kind == source_kind));
    config.runtime.activity_sources.push(ActivitySourceConfig {
        app_id,
        source_kind,
        endpoint_url: (source_kind != ActivitySourceKind::Community)
            .then_some(source_value.clone()),
        inline_json: (source_kind == ActivitySourceKind::Community).then_some(source_value),
    });
    config.save_json(&config_path)?;
    println!("activity_source_added=true");
    println!("config_path={}", config_path.to_string_lossy());
    println!("app_id={app_id}");
    println!("source_kind={:?}", source_kind);
    Ok(())
}

fn activity_sources_remove_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 4 && args.len() != 5 {
        return Err(
            "usage: pole-client activity-sources-remove [config-path] <app-id> <steam|epic|ea|gog|community>"
                .into(),
        );
    }
    let (config_path_arg, start_index) = parse_config_path_and_rest(args, 2, DEFAULT_CONFIG_PATH);
    if args.len() != start_index + 2 {
        return Err(
            "usage: pole-client activity-sources-remove [config-path] <app-id> <steam|epic|ea|gog|community>"
                .into(),
        );
    }
    let (config_path, mut config) = NodeConfig::load_json_with_runtime_paths(config_path_arg)?;
    let app_id: u32 = args[start_index].parse()?;
    let source_kind = parse_activity_source_kind(&args[start_index + 1])?;
    let original_len = config.runtime.activity_sources.len();
    config
        .runtime
        .activity_sources
        .retain(|source| !(source.app_id == app_id && source.source_kind == source_kind));
    config.save_json(&config_path)?;
    println!(
        "activity_source_removed={}",
        config.runtime.activity_sources.len() != original_len
    );
    println!("config_path={}", config_path.to_string_lossy());
    println!("app_id={app_id}");
    println!("source_kind={:?}", source_kind);
    Ok(())
}

fn reward_config_show_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() > 3 {
        return Err("usage: pole-client reward-config-show [config-path]".into());
    }
    let config_path = args
        .get(2)
        .map(String::as_str)
        .unwrap_or(DEFAULT_CONFIG_PATH);
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path)?;
    println!("PoLE client reward config");
    println!("config_path={}", config_path.to_string_lossy());
    println!("reward_source={}", config.reward.reward_source_label());
    println!("emission_year={}", config.reward.emission_year);
    println!(
        "tail_emission_start_year={}",
        config.reward.tail_emission_start_year
    );
    println!(
        "tail_emission_rate_bps={}",
        config.reward.tail_emission_rate_bps
    );
    println!("reward_block_secs={}", effective_reward_block_secs(&config));
    println!(
        "configured_player_block_reward={}",
        config.reward.player_block_reward
    );
    println!(
        "effective_player_block_reward={}",
        effective_player_block_reward(&config)
    );
    println!(
        "reward_adjustment_period_blocks={}",
        config.reward.reward_adjustment_period_blocks
    );
    println!(
        "target_network_weight_units={}",
        effective_target_network_weight_units(&config)
    );
    println!(
        "reward_adjustment_cap_bps={}",
        effective_reward_adjustment_cap_bps(&config)
    );
    println!("collect_reward_bps={}", config.reward.collect_reward_bps);
    println!("store_reward_bps={}", config.reward.store_reward_bps);
    println!("verify_reward_bps={}", config.reward.verify_reward_bps);
    println!("propose_reward_bps={}", config.reward.propose_reward_bps);
    Ok(())
}

fn reward_config_set_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() < 4 {
        return Err("usage: pole-client reward-config-set [config-path] <mode|emission-year|tail-policy|service-split> <value...>".into());
    }
    let (config_path_arg, start_index) = parse_config_path_and_rest_with_known_first_arg(
        args,
        2,
        DEFAULT_CONFIG_PATH,
        is_reward_config_subcommand,
    );
    let (config_path, mut config) = NodeConfig::load_json_with_runtime_paths(config_path_arg)?;
    let subcommand = args
        .get(start_index)
        .ok_or("missing reward-config-set subcommand")?;

    match subcommand.as_str() {
        "mode" => {
            let mode = args.get(start_index + 1).ok_or(
                "usage: pole-client reward-config-set [config-path] mode <static|tokenomics>",
            )?;
            config.reward.reward_source = parse_reward_source_mode(mode)?;
        }
        "emission-year" => {
            let year: u32 = args
                .get(start_index + 1)
                .ok_or("usage: pole-client reward-config-set [config-path] emission-year <year>")?
                .parse()?;
            config.reward.emission_year = year;
        }
        "tail-policy" => {
            if args.len() != start_index + 3 {
                return Err("usage: pole-client reward-config-set [config-path] tail-policy <start-year> <tail-rate-bps>".into());
            }
            config.reward.tail_emission_start_year = args[start_index + 1].parse()?;
            config.reward.tail_emission_rate_bps = args[start_index + 2].parse()?;
        }
        "service-split" => {
            if args.len() != start_index + 5 {
                return Err("usage: pole-client reward-config-set [config-path] service-split <collect_bps> <store_bps> <verify_bps> <propose_bps>".into());
            }
            config.reward.collect_reward_bps = args[start_index + 1].parse()?;
            config.reward.store_reward_bps = args[start_index + 2].parse()?;
            config.reward.verify_reward_bps = args[start_index + 3].parse()?;
            config.reward.propose_reward_bps = args[start_index + 4].parse()?;
        }
        _ => {
            return Err("usage: pole-client reward-config-set [config-path] <mode|emission-year|tail-policy|service-split> <value...>".into());
        }
    }

    config.save_json(&config_path)?;
    println!("reward_config_updated=true");
    println!("config_path={}", config_path.to_string_lossy());
    println!("reward_source={}", config.reward.reward_source_label());
    println!("emission_year={}", config.reward.emission_year);
    println!(
        "tail_emission_start_year={}",
        config.reward.tail_emission_start_year
    );
    println!(
        "tail_emission_rate_bps={}",
        config.reward.tail_emission_rate_bps
    );
    println!(
        "effective_player_block_reward={}",
        effective_player_block_reward(&config)
    );
    println!("collect_reward_bps={}", config.reward.collect_reward_bps);
    println!("store_reward_bps={}", config.reward.store_reward_bps);
    println!("verify_reward_bps={}", config.reward.verify_reward_bps);
    println!("propose_reward_bps={}", config.reward.propose_reward_bps);
    Ok(())
}

fn parse_reward_source_mode(input: &str) -> Result<RewardSourceMode, Box<dyn std::error::Error>> {
    match input {
        "static" => Ok(RewardSourceMode::Static),
        "tokenomics" => Ok(RewardSourceMode::Tokenomics),
        _ => Err("reward source mode must be one of: static, tokenomics".into()),
    }
}

fn governance_propose_params_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 6 && args.len() != 7 && args.len() != 8 && args.len() != 9 {
        return Err("usage: pole-client governance-propose-params [config-path] <proposal-id-hex> <effective-epoch> <emission-year> <effective-player-block-reward> [tail-start-year tail-rate-bps]".into());
    }
    let (config_path_arg, start_index) = parse_config_path_and_rest_with_known_first_arg(
        args,
        2,
        DEFAULT_CONFIG_PATH,
        looks_like_hex_32_arg,
    );
    if args.len() != start_index + 4 && args.len() != start_index + 6 {
        return Err("usage: pole-client governance-propose-params [config-path] <proposal-id-hex> <effective-epoch> <emission-year> <effective-player-block-reward> [tail-start-year tail-rate-bps]".into());
    }
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path_arg)?;
    let proposal_id = decode_hex32(&args[start_index], "proposal_id")?;
    let effective_epoch: u64 = args[start_index + 1].parse()?;
    let emission_year: u32 = args[start_index + 2].parse()?;
    let effective_player_block_reward: u128 = args[start_index + 3].parse()?;
    let tail_policy = if args.len() == start_index + 6 {
        Some((
            args[start_index + 4].parse::<u32>()?,
            args[start_index + 5].parse::<u16>()?,
        ))
    } else {
        None
    };

    let mut params = open_local_protocol_state(&config, config.runtime.challenge_window_blocks)?
        .1
        .params
        .clone();
    params.rewards.emission_year = emission_year;
    params.rewards.effective_player_block_reward = effective_player_block_reward;
    if let Some((tail_start_year, tail_rate_bps)) = tail_policy {
        params.rewards.tail_emission_start_year = tail_start_year;
        params.rewards.tail_emission_rate_bps = tail_rate_bps;
    }
    let effects =
        submit_protocol_params_update_proposal(&config, proposal_id, effective_epoch, params)?;

    print_command_header("governance-propose-params", &config_path);
    println!("proposal_id={}", pole_protocol_draft::hex_32(proposal_id));
    println!("effective_epoch={effective_epoch}");
    println!("emission_year={emission_year}");
    println!("effective_player_block_reward={effective_player_block_reward}");
    if let Some((tail_start_year, tail_rate_bps)) = tail_policy {
        println!("tail_emission_start_year={tail_start_year}");
        println!("tail_emission_rate_bps={tail_rate_bps}");
    }
    println!("effect_count={}", effects.len());
    Ok(())
}

fn governance_propose_service_split_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 8 && args.len() != 9 {
        return Err("usage: pole-client governance-propose-service-split [config-path] <proposal-id-hex> <effective-epoch> <collect_bps> <store_bps> <verify_bps> <propose_bps>".into());
    }
    let (config_path_arg, start_index) = parse_config_path_and_rest_with_known_first_arg(
        args,
        2,
        DEFAULT_CONFIG_PATH,
        looks_like_hex_32_arg,
    );
    if args.len() != start_index + 6 {
        return Err("usage: pole-client governance-propose-service-split [config-path] <proposal-id-hex> <effective-epoch> <collect_bps> <store_bps> <verify_bps> <propose_bps>".into());
    }
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path_arg)?;
    let proposal_id = decode_hex32(&args[start_index], "proposal_id")?;
    let effective_epoch: u64 = args[start_index + 1].parse()?;
    let collect_bps: u16 = args[start_index + 2].parse()?;
    let store_bps: u16 = args[start_index + 3].parse()?;
    let verify_bps: u16 = args[start_index + 4].parse()?;
    let propose_bps: u16 = args[start_index + 5].parse()?;

    let mut params = open_local_protocol_state(&config, config.runtime.challenge_window_blocks)?
        .1
        .params
        .clone();
    params.rewards.collect_reward_bps = collect_bps;
    params.rewards.store_reward_bps = store_bps;
    params.rewards.verify_reward_bps = verify_bps;
    params.rewards.propose_reward_bps = propose_bps;
    let effects =
        submit_protocol_params_update_proposal(&config, proposal_id, effective_epoch, params)?;

    print_command_header("governance-propose-service-split", &config_path);
    println!("proposal_id={}", pole_protocol_draft::hex_32(proposal_id));
    println!("effective_epoch={effective_epoch}");
    println!("collect_reward_bps={collect_bps}");
    println!("store_reward_bps={store_bps}");
    println!("verify_reward_bps={verify_bps}");
    println!("propose_reward_bps={propose_bps}");
    println!("effect_count={}", effects.len());
    Ok(())
}

fn governance_propose_reward_tuning_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 8 && args.len() != 9 {
        return Err("usage: pole-client governance-propose-reward-tuning [config-path] <proposal-id-hex> <effective-epoch> <target_network_weight_units> <reward_adjustment_cap_bps> <challenge_window_blocks> <effective-player-block-reward>".into());
    }
    let (config_path_arg, start_index) = parse_config_path_and_rest_with_known_first_arg(
        args,
        2,
        DEFAULT_CONFIG_PATH,
        looks_like_hex_32_arg,
    );
    if args.len() != start_index + 6 {
        return Err("usage: pole-client governance-propose-reward-tuning [config-path] <proposal-id-hex> <effective-epoch> <target_network_weight_units> <reward_adjustment_cap_bps> <challenge_window_blocks> <effective-player-block-reward>".into());
    }
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path_arg)?;
    let proposal_id = decode_hex32(&args[start_index], "proposal_id")?;
    let effective_epoch: u64 = args[start_index + 1].parse()?;
    let target_network_weight_units: u128 = args[start_index + 2].parse()?;
    let reward_adjustment_cap_bps: u16 = args[start_index + 3].parse()?;
    let challenge_window_blocks: u32 = args[start_index + 4].parse()?;
    let effective_player_block_reward: u128 = args[start_index + 5].parse()?;

    let mut params = open_local_protocol_state(&config, config.runtime.challenge_window_blocks)?
        .1
        .params
        .clone();
    params.rewards.effective_player_block_reward = effective_player_block_reward;
    params.rewards.target_network_weight_units = target_network_weight_units;
    params.rewards.reward_adjustment_cap_bps = reward_adjustment_cap_bps;
    params.challenge_window_blocks = challenge_window_blocks;
    let effects =
        submit_protocol_params_update_proposal(&config, proposal_id, effective_epoch, params)?;

    print_command_header("governance-propose-reward-tuning", &config_path);
    println!("proposal_id={}", pole_protocol_draft::hex_32(proposal_id));
    println!("effective_epoch={effective_epoch}");
    println!("target_network_weight_units={target_network_weight_units}");
    println!("reward_adjustment_cap_bps={reward_adjustment_cap_bps}");
    println!("challenge_window_blocks={challenge_window_blocks}");
    println!("effective_player_block_reward={effective_player_block_reward}");
    println!("effect_count={}", effects.len());
    Ok(())
}

fn governance_propose_thresholds_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 6 && args.len() != 7 {
        return Err("usage: pole-client governance-propose-thresholds [config-path] <proposal-id-hex> <effective-epoch> <quorum_bps> <approval_bps>".into());
    }
    let (config_path_arg, start_index) = parse_config_path_and_rest_with_known_first_arg(
        args,
        2,
        DEFAULT_CONFIG_PATH,
        looks_like_hex_32_arg,
    );
    if args.len() != start_index + 4 {
        return Err("usage: pole-client governance-propose-thresholds [config-path] <proposal-id-hex> <effective-epoch> <quorum_bps> <approval_bps>".into());
    }
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path_arg)?;
    let proposal_id = decode_hex32(&args[start_index], "proposal_id")?;
    let effective_epoch: u64 = args[start_index + 1].parse()?;
    let quorum_bps: u16 = args[start_index + 2].parse()?;
    let approval_bps: u16 = args[start_index + 3].parse()?;

    let mut params = open_local_protocol_state(&config, config.runtime.challenge_window_blocks)?
        .1
        .params
        .clone();
    params.governance.params_update_quorum_bps = quorum_bps;
    params.governance.params_update_approval_bps = approval_bps;
    let effects =
        submit_protocol_params_update_proposal(&config, proposal_id, effective_epoch, params)?;

    print_command_header("governance-propose-thresholds", &config_path);
    println!("proposal_id={}", pole_protocol_draft::hex_32(proposal_id));
    println!("effective_epoch={effective_epoch}");
    println!("params_update_quorum_bps={quorum_bps}");
    println!("params_update_approval_bps={approval_bps}");
    println!("effect_count={}", effects.len());
    Ok(())
}

fn governance_propose_slow_params_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 6 && args.len() != 7 {
        return Err("usage: pole-client governance-propose-slow-params [config-path] <proposal-id-hex> <effective-epoch> <reward-block-secs> <effective-player-block-reward>".into());
    }
    let (config_path_arg, start_index) = parse_config_path_and_rest_with_known_first_arg(
        args,
        2,
        DEFAULT_CONFIG_PATH,
        looks_like_hex_32_arg,
    );
    if args.len() != start_index + 4 {
        return Err("usage: pole-client governance-propose-slow-params [config-path] <proposal-id-hex> <effective-epoch> <reward-block-secs> <effective-player-block-reward>".into());
    }
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path_arg)?;
    let proposal_id = decode_hex32(&args[start_index], "proposal_id")?;
    let effective_epoch: u64 = args[start_index + 1].parse()?;
    let reward_block_secs: u64 = args[start_index + 2].parse()?;
    let effective_player_block_reward: u128 = args[start_index + 3].parse()?;

    let mut params = open_local_protocol_state(&config, config.runtime.challenge_window_blocks)?
        .1
        .params
        .clone();
    params.rewards.reward_block_secs = reward_block_secs;
    params.rewards.effective_player_block_reward = effective_player_block_reward;
    let effects =
        submit_protocol_params_update_proposal(&config, proposal_id, effective_epoch, params)?;

    print_command_header("governance-propose-slow-params", &config_path);
    println!("proposal_id={}", pole_protocol_draft::hex_32(proposal_id));
    println!("effective_epoch={effective_epoch}");
    println!("reward_block_secs={reward_block_secs}");
    println!("effective_player_block_reward={effective_player_block_reward}");
    println!("effect_count={}", effects.len());
    Ok(())
}

fn governance_propose_retention_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 6 && args.len() != 7 {
        return Err("usage: pole-client governance-propose-retention [config-path] <proposal-id-hex> <effective-epoch> <min-retention-epochs> <challenge-window-blocks>".into());
    }
    let (config_path_arg, start_index) = parse_config_path_and_rest_with_known_first_arg(
        args,
        2,
        DEFAULT_CONFIG_PATH,
        looks_like_hex_32_arg,
    );
    if args.len() != start_index + 4 {
        return Err("usage: pole-client governance-propose-retention [config-path] <proposal-id-hex> <effective-epoch> <min-retention-epochs> <challenge-window-blocks>".into());
    }
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path_arg)?;
    let proposal_id = decode_hex32(&args[start_index], "proposal_id")?;
    let effective_epoch: u64 = args[start_index + 1].parse()?;
    let min_retention_epochs: u32 = args[start_index + 2].parse()?;
    let challenge_window_blocks: u32 = args[start_index + 3].parse()?;

    let mut params = open_local_protocol_state(&config, config.runtime.challenge_window_blocks)?
        .1
        .params
        .clone();
    params.min_retention_epochs = min_retention_epochs;
    params.challenge_window_blocks = challenge_window_blocks;
    let effects =
        submit_protocol_params_update_proposal(&config, proposal_id, effective_epoch, params)?;

    print_command_header("governance-propose-retention", &config_path);
    println!("proposal_id={}", pole_protocol_draft::hex_32(proposal_id));
    println!("effective_epoch={effective_epoch}");
    println!("min_retention_epochs={min_retention_epochs}");
    println!("challenge_window_blocks={challenge_window_blocks}");
    println!("effect_count={}", effects.len());
    Ok(())
}

fn governance_propose_tier_weights_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 9 && args.len() != 10 {
        return Err("usage: pole-client governance-propose-tier-weights [config-path] <proposal-id-hex> <effective-epoch> <tier1_weight_ppm> <tier2_min_ppm> <tier2_max_ppm> <tier3_min_ppm> <tier3_max_ppm>".into());
    }
    let (config_path_arg, start_index) = parse_config_path_and_rest_with_known_first_arg(
        args,
        2,
        DEFAULT_CONFIG_PATH,
        looks_like_hex_32_arg,
    );
    if args.len() != start_index + 7 {
        return Err("usage: pole-client governance-propose-tier-weights [config-path] <proposal-id-hex> <effective-epoch> <tier1_weight_ppm> <tier2_min_ppm> <tier2_max_ppm> <tier3_min_ppm> <tier3_max_ppm>".into());
    }
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path_arg)?;
    let proposal_id = decode_hex32(&args[start_index], "proposal_id")?;
    let effective_epoch: u64 = args[start_index + 1].parse()?;
    let tier1_weight_ppm: u32 = args[start_index + 2].parse()?;
    let tier2_weight_min_ppm: u32 = args[start_index + 3].parse()?;
    let tier2_weight_max_ppm: u32 = args[start_index + 4].parse()?;
    let tier3_weight_min_ppm: u32 = args[start_index + 5].parse()?;
    let tier3_weight_max_ppm: u32 = args[start_index + 6].parse()?;

    let mut params = open_local_protocol_state(&config, config.runtime.challenge_window_blocks)?
        .1
        .params
        .clone();
    params.rewards.tier1_weight_ppm = tier1_weight_ppm;
    params.rewards.tier2_weight_min_ppm = tier2_weight_min_ppm;
    params.rewards.tier2_weight_max_ppm = tier2_weight_max_ppm;
    params.rewards.tier3_weight_min_ppm = tier3_weight_min_ppm;
    params.rewards.tier3_weight_max_ppm = tier3_weight_max_ppm;
    let effects =
        submit_protocol_params_update_proposal(&config, proposal_id, effective_epoch, params)?;

    print_command_header("governance-propose-tier-weights", &config_path);
    println!("proposal_id={}", pole_protocol_draft::hex_32(proposal_id));
    println!("effective_epoch={effective_epoch}");
    println!("tier1_weight_ppm={tier1_weight_ppm}");
    println!("tier2_weight_min_ppm={tier2_weight_min_ppm}");
    println!("tier2_weight_max_ppm={tier2_weight_max_ppm}");
    println!("tier3_weight_min_ppm={tier3_weight_min_ppm}");
    println!("tier3_weight_max_ppm={tier3_weight_max_ppm}");
    println!("effect_count={}", effects.len());
    Ok(())
}

fn governance_propose_app_weight_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 6 && args.len() != 7 {
        return Err("usage: pole-client governance-propose-app-weight [config-path] <proposal-id-hex> <effective-epoch> <app-id> <game-coefficient-ppm>".into());
    }
    let (config_path_arg, start_index) = parse_config_path_and_rest_with_known_first_arg(
        args,
        2,
        DEFAULT_CONFIG_PATH,
        looks_like_hex_32_arg,
    );
    if args.len() != start_index + 4 {
        return Err("usage: pole-client governance-propose-app-weight [config-path] <proposal-id-hex> <effective-epoch> <app-id> <game-coefficient-ppm>".into());
    }
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path_arg)?;
    let proposal_id = decode_hex32(&args[start_index], "proposal_id")?;
    let effective_epoch: u64 = args[start_index + 1].parse()?;
    let app_id: u32 = args[start_index + 2].parse()?;
    let game_coefficient_ppm: u32 = args[start_index + 3].parse()?;

    let mut params = open_local_protocol_state(&config, config.runtime.challenge_window_blocks)?
        .1
        .params
        .clone();
    params
        .rewards
        .app_weight_overrides
        .retain(|entry| entry.app_id != app_id);
    params
        .rewards
        .app_weight_overrides
        .push(pole_protocol_draft::AppWeightOverride {
            app_id,
            game_coefficient_ppm,
        });
    params
        .rewards
        .app_weight_overrides
        .sort_by_key(|entry| entry.app_id);
    let effects =
        submit_protocol_params_update_proposal(&config, proposal_id, effective_epoch, params)?;

    print_command_header("governance-propose-app-weight", &config_path);
    println!("proposal_id={}", pole_protocol_draft::hex_32(proposal_id));
    println!("effective_epoch={effective_epoch}");
    println!("app_id={app_id}");
    println!("game_coefficient_ppm={game_coefficient_ppm}");
    println!("effect_count={}", effects.len());
    Ok(())
}

fn governance_vote_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 5 && args.len() != 6 {
        return Err("usage: pole-client governance-vote [config-path] <proposal-id-hex> <yes|no|abstain> <voting-power>".into());
    }
    let (config_path_arg, start_index) = parse_config_path_and_rest_with_known_first_arg(
        args,
        2,
        DEFAULT_CONFIG_PATH,
        looks_like_hex_32_arg,
    );
    if args.len() != start_index + 3 {
        return Err("usage: pole-client governance-vote [config-path] <proposal-id-hex> <yes|no|abstain> <voting-power>".into());
    }
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path_arg)?;
    let proposal_id = decode_hex32(&args[start_index], "proposal_id")?;
    let choice = parse_vote_choice(&args[start_index + 1])?;
    let voting_power: u128 = args[start_index + 2].parse()?;

    let (effects, scheduled) =
        pole_protocol_draft::execute_governance_vote(&config, proposal_id, choice, voting_power)?;

    print_command_header("governance-vote", &config_path);
    println!("proposal_id={}", pole_protocol_draft::hex_32(proposal_id));
    println!("choice={:?}", choice);
    println!("voting_power={voting_power}");
    println!("effect_count={}", effects.len());
    println!("scheduled_next_epoch={scheduled}");
    Ok(())
}

fn governance_show_proposal_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 3 && args.len() != 4 {
        return Err(
            "usage: pole-client governance-show-proposal [config-path] <proposal-id-hex>".into(),
        );
    }
    let (config_path_arg, start_index) = parse_config_path_and_rest_with_known_first_arg(
        args,
        2,
        DEFAULT_CONFIG_PATH,
        looks_like_hex_32_arg,
    );
    if args.len() != start_index + 1 {
        return Err(
            "usage: pole-client governance-show-proposal [config-path] <proposal-id-hex>".into(),
        );
    }
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path_arg)?;
    let proposal_id = decode_hex32(&args[start_index], "proposal_id")?;
    let (_, state) = open_local_protocol_state(&config, config.runtime.challenge_window_blocks)?;
    let Some((artifact, artifact_path, index_path)) =
        export_governance_proposal_artifact(&config, &state.store, &proposal_id)?
    else {
        return Err("governance params update proposal not found".into());
    };

    print_command_header("governance-show-proposal", &config_path);
    print_governance_proposal_artifact(&artifact);
    println!("artifact_path={}", artifact_path.to_string_lossy());
    println!("artifact_index_path={}", index_path.to_string_lossy());
    Ok(())
}

fn governance_show_scheduled_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() > 4 {
        return Err("usage: pole-client governance-show-scheduled [config-path] [epoch-id]".into());
    }
    let (config_path_arg, start_index) = parse_config_path_and_rest(args, 2, DEFAULT_CONFIG_PATH);
    if args.len() > start_index + 1 {
        return Err("usage: pole-client governance-show-scheduled [config-path] [epoch-id]".into());
    }
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path_arg)?;
    let (_, state) = open_local_protocol_state(&config, config.runtime.challenge_window_blocks)?;
    let epoch_id =
        parse_optional_u64_arg(args, start_index)?.unwrap_or(state.current_epoch.saturating_add(1));

    print_command_header("governance-show-scheduled", &config_path);
    let scheduled_params = state.store.scheduled_protocol_params(&epoch_id);
    let (artifact, artifact_path, index_path) = export_governance_scheduled_artifact(
        &config,
        state.current_epoch,
        epoch_id,
        scheduled_params,
    )?;
    print_governance_scheduled_artifact(&artifact);
    println!("artifact_path={}", artifact_path.to_string_lossy());
    println!("artifact_index_path={}", index_path.to_string_lossy());
    Ok(())
}

fn governance_show_index_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() > 3 {
        return Err("usage: pole-client governance-show-index [config-path]".into());
    }
    let (config_path_arg, start_index) = parse_config_path_and_rest(args, 2, DEFAULT_CONFIG_PATH);
    if args.len() != start_index {
        return Err("usage: pole-client governance-show-index [config-path]".into());
    }
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path_arg)?;
    let index_path = governance_index_artifact_path(&config);
    let index = GovernanceArtifactIndex::load_or_default_json(&index_path)?;

    print_command_header("governance-show-index", &config_path);
    println!("artifact_index_path={}", index_path.to_string_lossy());
    print_governance_index(&index);
    Ok(())
}

fn governance_show_summary_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() > 3 {
        return Err("usage: pole-client governance-show-summary [config-path]".into());
    }
    let (config_path_arg, start_index) = parse_config_path_and_rest(args, 2, DEFAULT_CONFIG_PATH);
    if args.len() != start_index {
        return Err("usage: pole-client governance-show-summary [config-path]".into());
    }
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path_arg)?;
    let summary_path = governance_summary_artifact_path(&config);
    let summary = GovernanceArtifactSummary::load_or_default_json(&summary_path)?;

    print_command_header("governance-show-summary", &config_path);
    println!("artifact_summary_path={}", summary_path.to_string_lossy());
    println!("artifact_index_path={}", summary.artifact_index_path);
    print_governance_summary(&summary);
    Ok(())
}

fn reward_adjustment_show_index_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() > 3 {
        return Err("usage: pole-client reward-adjustment-show-index [config-path]".into());
    }
    let (config_path_arg, start_index) = parse_config_path_and_rest(args, 2, DEFAULT_CONFIG_PATH);
    if args.len() != start_index {
        return Err("usage: pole-client reward-adjustment-show-index [config-path]".into());
    }
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path_arg)?;
    let index_path = pole_protocol_draft::reward_adjustment_index_path(&config);
    let index =
        pole_protocol_draft::RewardAdjustmentArtifactIndex::load_or_default_json(&index_path)?;

    print_command_header("reward-adjustment-show-index", &config_path);
    println!("artifact_index_path={}", index_path.to_string_lossy());
    print_reward_adjustment_index(&index);
    Ok(())
}

fn reward_adjustment_show_summary_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() > 3 {
        return Err("usage: pole-client reward-adjustment-show-summary [config-path]".into());
    }
    let (config_path_arg, start_index) = parse_config_path_and_rest(args, 2, DEFAULT_CONFIG_PATH);
    if args.len() != start_index {
        return Err("usage: pole-client reward-adjustment-show-summary [config-path]".into());
    }
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path_arg)?;
    let summary_path = pole_protocol_draft::reward_adjustment_summary_path(&config);
    let summary = RewardAdjustmentArtifactSummary::load_or_default_json(&summary_path)?;

    print_command_header("reward-adjustment-show-summary", &config_path);
    println!("artifact_summary_path={}", summary_path.to_string_lossy());
    println!("artifact_index_path={}", summary.artifact_index_path);
    print_reward_adjustment_summary(&summary);
    Ok(())
}

fn activity_sources_sync_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() > 3 {
        return Err("usage: pole-client activity-sources-sync [config-path]".into());
    }
    let config_path = args
        .get(2)
        .map(String::as_str)
        .unwrap_or(DEFAULT_CONFIG_PATH);
    let (config_path, mut config) = NodeConfig::load_json_with_runtime_paths(config_path)?;
    sync_activity_sources(&mut config);
    config.save_json(&config_path)?;
    println!("activity_sources_synced=true");
    println!("config_path={}", config_path.to_string_lossy());
    println!(
        "activity_source_count={}",
        config.runtime.activity_sources.len()
    );
    Ok(())
}

fn control_api_serve_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() > 4 {
        return Err("usage: pole-client control-api-serve [config-path] [bind-addr]".into());
    }
    let (config_path_arg, start_index) = parse_config_path_and_rest(args, 2, DEFAULT_CONFIG_PATH);
    if args.len() > start_index + 1 {
        return Err("usage: pole-client control-api-serve [config-path] [bind-addr]".into());
    }
    let (config_path, _config) = NodeConfig::load_json_with_runtime_paths(config_path_arg)?;
    let bind_addr = args
        .get(start_index)
        .map(String::as_str)
        .unwrap_or(DEFAULT_CONTROL_API_BIND_ADDR);
    let listener = std::net::TcpListener::bind(bind_addr)?;
    println!("PoLE client control api");
    println!("config_path={}", config_path.to_string_lossy());
    println!("bind_addr={bind_addr}");
    println!("dashboard_url=http://{bind_addr}/");
    println!("status_url=http://{bind_addr}/api/status");
    let max_requests = env::var("POLE_CLIENT_CONTROL_API_MAX_REQUESTS")
        .ok()
        .map(|value| value.parse::<usize>())
        .transpose()?;
    serve_control_api(listener, config_path, max_requests)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ControlApiLaunchOutcome {
    Started { pid: u32 },
    AlreadyRunning,
}

fn control_api_open_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() > 4 {
        return Err("usage: pole-client control-api-open [config-path] [bind-addr]".into());
    }
    let (config_path_arg, start_index) = parse_config_path_and_rest(args, 2, DEFAULT_CONFIG_PATH);
    if args.len() > start_index + 1 {
        return Err("usage: pole-client control-api-open [config-path] [bind-addr]".into());
    }
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path_arg)?;
    let bind_addr = args
        .get(start_index)
        .map(String::as_str)
        .unwrap_or(DEFAULT_CONTROL_API_BIND_ADDR);
    let dashboard_url = format!("http://{bind_addr}/");
    let status_url = format!("http://{bind_addr}/api/status");
    let outcome = ensure_control_api_server(&config_path, &config, bind_addr)?;
    open_dashboard_url(&dashboard_url)?;

    println!("PoLE client control api launcher");
    println!("config_path={}", config_path.to_string_lossy());
    println!("bind_addr={bind_addr}");
    println!("dashboard_url={dashboard_url}");
    println!("status_url={status_url}");
    match outcome {
        ControlApiLaunchOutcome::Started { pid } => {
            println!("control_api_started=true");
            println!("control_api_pid={pid}");
        }
        ControlApiLaunchOutcome::AlreadyRunning => {
            println!("control_api_started=false");
            println!("control_api_already_running=true");
        }
    }
    Ok(())
}

fn parse_activity_source_kind(
    input: &str,
) -> Result<ActivitySourceKind, Box<dyn std::error::Error>> {
    match input.to_ascii_lowercase().as_str() {
        "steam" => Ok(ActivitySourceKind::Steam),
        "epic" => Ok(ActivitySourceKind::Epic),
        "ea" => Ok(ActivitySourceKind::Ea),
        "gog" => Ok(ActivitySourceKind::Gog),
        "community" => Ok(ActivitySourceKind::Community),
        _ => Err(format!("unknown activity source kind {input}").into()),
    }
}

fn libp2p_diagnose_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() > 3 {
        return Err("usage: pole-client libp2p-diagnose [config-path]".into());
    }
    let config_path = args
        .get(2)
        .map(String::as_str)
        .unwrap_or(DEFAULT_CONFIG_PATH);
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path)?;
    println!("PoLE client libp2p diagnose");
    println!("config_path={}", config_path.to_string_lossy());
    println!("libp2p_enabled={}", config.runtime.p2p_libp2p.enabled);
    println!(
        "libp2p_listen_addrs={:?}",
        config.runtime.p2p_libp2p.listen_addrs
    );
    println!(
        "libp2p_bootstrap_peer_count={}",
        config.runtime.p2p_libp2p.bootstrap_peers.len()
    );
    println!(
        "libp2p_kademlia={}",
        config.runtime.p2p_libp2p.discovery.kademlia
    );
    println!("libp2p_mdns={}", config.runtime.p2p_libp2p.discovery.mdns);
    println!(
        "libp2p_rendezvous={}",
        config.runtime.p2p_libp2p.discovery.rendezvous
    );
    Ok(())
}

fn libp2p_skeleton_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() > 3 {
        return Err("usage: pole-client libp2p-skeleton [config-path]".into());
    }
    let config_path = args
        .get(2)
        .map(String::as_str)
        .unwrap_or(DEFAULT_CONFIG_PATH);
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path)?;
    let skeleton = build_libp2p_backend_skeleton(&config.runtime.p2p_libp2p)?;
    let real_swarm = build_real_libp2p_swarm_report(&config.runtime.p2p_libp2p)?;
    println!("PoLE client libp2p skeleton");
    println!("config_path={}", config_path.to_string_lossy());
    println!("local_peer_id={}", skeleton.local_peer_id);
    println!("listen_addrs={:?}", skeleton.listen_addrs);
    println!("bootstrap_peer_count={}", skeleton.bootstrap_peers.len());
    println!("kademlia_enabled={}", skeleton.kademlia_enabled);
    println!("mdns_enabled={}", skeleton.mdns_enabled);
    println!("rendezvous_enabled={}", skeleton.rendezvous_enabled);
    println!("real_swarm_local_peer_id={}", real_swarm.local_peer_id);
    println!("real_swarm_listener_count={}", real_swarm.listener_count);
    Ok(())
}

fn p2p_socket_show_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() > 3 {
        return Err("usage: pole-client p2p-socket-show [config-path]".into());
    }
    let config_path = args
        .get(2)
        .map(String::as_str)
        .unwrap_or(DEFAULT_CONFIG_PATH);
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path)?;
    let topics = default_socket_topic_labels();

    println!("PoLE client p2p socket");
    println!("config_path={}", config_path.to_string_lossy());
    println!("local_peer_id={}", config.node_id_hex);
    println!("bind_addr={}", config.runtime.p2p_socket.bind_addr);
    println!("peer_count={}", config.runtime.p2p_socket.peers.len());
    println!(
        "local_peer_spec={}@{}@{}",
        config.node_id_hex,
        config.runtime.p2p_socket.bind_addr,
        topics.join(",")
    );
    for peer in &config.runtime.p2p_socket.peers {
        println!(
            "peer={}@{}@{}",
            peer.peer_id_hex,
            peer.addr,
            peer.topics.join(",")
        );
    }
    Ok(())
}

fn p2p_socket_add_peer_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 5 && args.len() != 6 {
        return Err(
            "usage: pole-client p2p-socket-add-peer <config-path> <peer-id-hex> <peer-addr> [topics]"
                .into(),
        );
    }

    let config_path = PathBuf::from(&args[2]);
    let mut config = NodeConfig::load_json(&config_path)?;
    let peer_id_hex = args[3].trim().to_ascii_lowercase();
    let peer_addr = args[4].trim().to_string();
    let topics = if let Some(value) = args.get(5) {
        parse_socket_topics(value)?
            .into_iter()
            .map(socket_topic_label)
            .map(str::to_string)
            .collect::<Vec<_>>()
    } else {
        default_socket_topic_labels()
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>()
    };

    let _ = decode_hex32(&peer_id_hex, "peer_id_hex")?;
    let _ = pole_protocol_draft::parse_socket_addr(&peer_addr, "peer_addr")?;

    config.runtime.p2p_socket.peers.retain(|peer| peer.peer_id_hex != peer_id_hex);
    config.runtime.p2p_socket.peers.push(pole_protocol_draft::P2pSocketPeerConfig {
        peer_id_hex: peer_id_hex.clone(),
        addr: peer_addr.clone(),
        topics: topics.clone(),
    });
    config
        .runtime
        .p2p_socket
        .peers
        .sort_by(|left, right| left.peer_id_hex.cmp(&right.peer_id_hex));
    config.save_json(&config_path)?;

    println!("peer_added=true");
    println!("config_path={}", config_path.to_string_lossy());
    println!("peer_id={peer_id_hex}");
    println!("peer_addr={peer_addr}");
    println!("topics={}", topics.join(","));
    println!("peer_count={}", config.runtime.p2p_socket.peers.len());
    Ok(())
}

fn print_watch_execution(
    config_path: &Path,
    config: &NodeConfig,
    summary: &CollectLoopSummary,
    attached_network_mode: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("PoLE client watch");
    println!("config_path={}", config_path.to_string_lossy());
    println!("low_impact_mode={}", config.runtime.low_impact_mode);
    println!(
        "os_background_priority={}",
        config.runtime.os_background_priority
    );
    println!("inline_verify_enabled={}", config.inline_verify_enabled());
    println!("inline_propose_enabled={}", config.inline_propose_enabled());
    println!(
        "game_active_poll_interval_secs={}",
        config.runtime.game_active_poll_interval_secs
    );
    println!("game_process_names={:?}", config.runtime.game_process_names);
    print_watch_auto_settlement_summary(summary);
    print_watch_retention_summary(summary);
    println!(
        "p2p_transport={}",
        attached_network_mode.unwrap_or("disabled")
    );
    println!(
        "p2p_simulation_enabled={}",
        attached_network_mode == Some("inmemory-sim")
    );
    if attached_network_mode.is_some() {
        let summary = load_status(config)?;
        print_client_network_and_retention_diagnostics(&summary);
    }
    println!("ticks_run={}", summary.ticks_completed);
    if let Some(last) = summary.last_result.as_ref() {
        println!("last_epoch={}", last.artifact.epoch_id);
        println!("last_slot={}", last.artifact.slot_id);
        println!("last_payload_cid={}", last.artifact.payload_cid);
        if let Some(artifact) = &last.aggregation_artifact {
            println!("last_aggregate_count={}", artifact.aggregate_count);
            println!("last_aggregates_root={}", artifact.aggregate_root_hex);
        }
        if let Some(artifact) = &last.reward_artifact {
            println!("last_reward_count={}", artifact.reward_count);
            println!("last_rewards_root={}", artifact.reward_root_hex);
        }
    }

    Ok(())
}

fn print_tick_auto_settlement_summary(result: &CollectTickResult) {
    let pending_epochs = result.unresolved_auto_settlement_epochs();
    println!("auto_settlement_enabled={}", result.auto_settlement_enabled);
    println!("auto_settlement_pending_epochs={}", pending_epochs.len());
    println!("auto_settled_epochs={}", result.settlement_artifacts.len());
    if result.auto_settlement_skipped() {
        println!("auto_settlement_skipped=propose_capability_required");
    }
    if let Some(error) = &result.auto_settlement_error {
        println!("last_auto_settlement_error={error}");
    }
    if let Some(artifact) = result.last_settlement_artifact() {
        println!("last_auto_settled_epoch={}", artifact.epoch_id);
        println!(
            "last_auto_settlement_reward_claimed={}",
            artifact.local_reward_claimed
        );
        println!(
            "last_auto_settlement_reward_balance={}",
            artifact.local_reward_balance
        );
    }
}

fn print_tick_retention_summary(result: &CollectTickResult) {
    println!(
        "retention_retained_payloads={}",
        result.retention_audit_artifact.retained_payload_count
    );
    println!(
        "retention_retrievable_payloads={}",
        result.retention_audit_artifact.retrievable_payload_count
    );
    println!(
        "retention_missing_payloads={}",
        result.retention_audit_artifact.missing_payload_count
    );
    println!(
        "retention_corrupted_payloads={}",
        result.retention_audit_artifact.corrupted_payload_count
    );
    println!(
        "retention_all_retrievable={}",
        result.retention_integrity_healthy()
    );
    println!(
        "storage_challenge_ok={}",
        result.storage_challenge_healthy()
    );
    if let Some(artifact) = &result.storage_challenge_artifact {
        println!(
            "storage_challenge_checked_payloads={}",
            artifact.checked_payload_count
        );
        println!(
            "storage_challenge_failed_payloads={}",
            artifact.failed_payload_count
        );
    }
    if let Some(error) = &result.storage_challenge_error {
        println!("storage_challenge_error={error}");
    }
    println!(
        "retention_prune_epoch={}",
        result.retention_prune_outcome.current_epoch
    );
    println!(
        "retention_pruned_payloads={}",
        result.pruned_payload_count()
    );
}

fn print_watch_auto_settlement_summary(summary: &CollectLoopSummary) {
    let auto_settlement = &summary.auto_settlement_summary;

    println!("auto_settlement_enabled={}", auto_settlement.enabled);
    println!(
        "auto_settlement_pending_epochs={}",
        auto_settlement.pending_epochs.len()
    );
    println!(
        "auto_settled_epochs={}",
        auto_settlement.settled_epoch_count
    );
    if auto_settlement.skipped() {
        println!("auto_settlement_skipped=propose_capability_required");
    }
    if let Some(error) = &auto_settlement.last_error {
        println!("last_auto_settlement_error={error}");
    }
    if let Some(artifact) = &auto_settlement.last_settlement_artifact {
        println!("last_auto_settled_epoch={}", artifact.epoch_id);
        println!(
            "last_auto_settlement_reward_claimed={}",
            artifact.local_reward_claimed
        );
        println!(
            "last_auto_settlement_reward_balance={}",
            artifact.local_reward_balance
        );
    }
}

fn print_watch_retention_summary(summary: &CollectLoopSummary) {
    let last_retention_audit = summary.last_retention_audit_artifact.as_ref();

    if let Some(audit) = last_retention_audit {
        println!(
            "retention_retained_payloads={}",
            audit.retained_payload_count
        );
        println!(
            "retention_retrievable_payloads={}",
            audit.retrievable_payload_count
        );
        println!("retention_missing_payloads={}", audit.missing_payload_count);
        println!(
            "retention_corrupted_payloads={}",
            audit.corrupted_payload_count
        );
        println!("retention_all_retrievable={}", audit.all_retrievable);
    }
    if let Some(last_storage_challenge) = summary.last_storage_challenge_artifact.as_ref() {
        println!(
            "storage_challenge_checked_payloads={}",
            last_storage_challenge.checked_payload_count
        );
        println!(
            "storage_challenge_failed_payloads={}",
            last_storage_challenge.failed_payload_count
        );
        println!(
            "storage_challenge_all_passed={}",
            last_storage_challenge.all_passed
        );
    }
    if let Some(error) = summary.last_storage_challenge_error.as_ref() {
        println!("storage_challenge_error={error}");
    }
    println!("retention_prune_epoch={}", summary.last_prune_epoch);
    println!(
        "retention_pruned_payloads={}",
        summary.total_pruned_payloads
    );
}

fn capture_foreground_process_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 3 {
        return Err("usage: pole-client capture-foreground-process <config-path>".into());
    }

    let config_path = PathBuf::from(&args[2]);
    let mut config = NodeConfig::load_json(&config_path)?;
    let foreground_process =
        detect_foreground_process_name().ok_or("could not detect a foreground process to add")?;

    config.runtime.low_impact_mode = true;
    config.runtime.os_background_priority = true;
    if config.runtime.game_active_poll_interval_secs < config.runtime.poll_interval_secs {
        config.runtime.game_active_poll_interval_secs = config.runtime.poll_interval_secs;
    }
    config.runtime.game_process_names = merge_process_names(
        &config.runtime.game_process_names,
        std::slice::from_ref(&foreground_process),
    );
    sync_reward_game_mappings(&mut config);
    sync_activity_sources(&mut config);
    config.save_json(&config_path)?;

    println!("PoLE client foreground process captured");
    println!("config_path={}", config_path.to_string_lossy());
    println!("foreground_process={foreground_process}");
    println!("game_process_names={:?}", config.runtime.game_process_names);
    println!(
        "next_step={}",
        command_hint("status", config_path.to_string_lossy().as_ref())
    );

    Ok(())
}

fn set_game_processes_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() < 4 {
        return Err("usage: pole-client set-game-processes <config-path> <process-name>...".into());
    }

    let config_path = PathBuf::from(&args[2]);
    let mut config = NodeConfig::load_json(&config_path)?;
    let process_names = args[3..]
        .iter()
        .map(|name| canonical_process_name(name))
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>();
    if process_names.is_empty() {
        return Err("at least one process name is required".into());
    }

    config.runtime.low_impact_mode = true;
    config.runtime.os_background_priority = true;
    if config.runtime.game_active_poll_interval_secs < config.runtime.poll_interval_secs {
        config.runtime.game_active_poll_interval_secs = config.runtime.poll_interval_secs;
    }
    config.runtime.game_process_names = merge_process_names(&[], &process_names);
    sync_reward_game_mappings(&mut config);
    sync_activity_sources(&mut config);
    config.save_json(&config_path)?;

    println!("PoLE client game-process list updated");
    println!("config_path={}", config_path.to_string_lossy());
    println!("low_impact_mode={}", config.runtime.low_impact_mode);
    println!(
        "os_background_priority={}",
        config.runtime.os_background_priority
    );
    println!(
        "game_active_poll_interval_secs={}",
        config.runtime.game_active_poll_interval_secs
    );
    println!("game_process_names={:?}", process_names);
    println!(
        "next_step={}",
        command_hint("status", config_path.to_string_lossy().as_ref())
    );

    Ok(())
}

fn aggregate_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let (_, start_index) = parse_config_path_and_rest(args, 2, DEFAULT_CONFIG_PATH);
    if args.len() > start_index + 1 {
        return Err("usage: pole-client aggregate [config-path] [epoch-id]".into());
    }

    let (config_path, config, epoch_id) =
        load_config_and_epoch_arg(args, 2, 0, DEFAULT_CONFIG_PATH)?;
    let artifact = aggregate_local_epoch(&config, epoch_id)?;

    print_command_header("aggregate", &config_path);
    println!("epoch_id={}", artifact.epoch_id);
    println!("aggregate_count={}", artifact.aggregate_count);
    println!(
        "total_observation_count={}",
        artifact.total_observation_count
    );
    println!(
        "deduped_observation_count={}",
        artifact.deduped_observation_count
    );
    println!(
        "accepted_observation_count={}",
        artifact.accepted_observation_count
    );
    println!("aggregate_root={}", artifact.aggregate_root_hex);
    for record in artifact.records {
        println!(
            "slot={} app_id={} total_obs={} unique_collectors={} trimmed={} accepted={} tier={:?} source={:?} source_confidence_ppm={} median_players={} gvs_microunits={}",
            record.slot_id,
            record.app_id,
            record.total_observations,
            record.unique_collectors,
            record.trimmed_observations,
            record.aggregate.accepted_observations,
            record.aggregate.gvs_tier,
            record.aggregate.primary_source_kind,
            record.aggregate.source_confidence_ppm,
            record.aggregate.median_players,
            record.aggregate.gvs_microunits
        );
    }

    Ok(())
}

fn rewards_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let (_, start_index) = parse_config_path_and_rest(args, 2, DEFAULT_CONFIG_PATH);
    if args.len() > start_index + 1 {
        return Err("usage: pole-client rewards [config-path] [epoch-id]".into());
    }

    let (config_path, config, epoch_id) =
        load_config_and_epoch_arg(args, 2, 0, DEFAULT_CONFIG_PATH)?;
    let artifact = reward_local_epoch(&config, epoch_id)?;

    print_command_header("rewards", &config_path);
    println!("epoch_id={}", artifact.epoch_id);
    println!("reward_block_secs={}", artifact.reward_block_secs);
    println!(
        "reward_adjustment_period_blocks={}",
        artifact.reward_adjustment_period_blocks
    );
    println!("player_block_reward={}", artifact.player_block_reward);
    println!(
        "player_reward_block_count={}",
        artifact.player_reward_block_count
    );
    println!(
        "completed_hour_reward_block_count={}",
        artifact.player_reward_block_count
    );
    println!("player_reward_pool={}", artifact.player_reward_pool);
    println!(
        "local_player_weight_units={}",
        artifact.local_player_weight_units
    );
    println!(
        "total_network_weight_units={}",
        artifact.total_network_weight_units
    );
    println!(
        "local_player_reward_total={}",
        artifact.local_player_reward_total
    );
    println!("total_gvs_units={}", artifact.total_gvs_units);
    println!("collect_pool={}", artifact.collect_pool);
    println!("store_pool={}", artifact.store_pool);
    println!("verify_pool={}", artifact.verify_pool);
    println!("propose_pool={}", artifact.propose_pool);
    println!("reward_count={}", artifact.reward_count);
    println!("reward_root={}", artifact.reward_root_hex);
    println!("total_distributed={}", artifact.total_distributed);
    for entry in artifact.records {
        println!(
            "node_id={} player_blocks={} player_weight={} player_reward={} record_player_reward={} collect_score={} storage_score={} collect_reward={} store_reward={} verify_reward={} propose_reward={} slash_debit={} net_reward={}",
            entry.node_id_hex,
            entry.player_block_count,
            entry.player_weight_units,
            entry.player_reward_units,
            entry.reward.player_reward,
            entry.collect_score_units,
            entry.storage_score_units,
            entry.reward.collect_reward,
            entry.reward.store_reward,
            entry.reward.verify_reward,
            entry.reward.propose_reward,
            entry.reward.slash_debit,
            entry.reward.net_reward
        );
    }

    Ok(())
}

fn verify_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let (_, start_index) = parse_config_path_and_rest(args, 2, DEFAULT_CONFIG_PATH);
    if args.len() > start_index + 1 {
        return Err("usage: pole-client verify [config-path] [epoch-id]".into());
    }

    let (config_path, config, epoch_id) =
        load_config_and_epoch_arg(args, 2, 0, DEFAULT_CONFIG_PATH)?;
    let report = verify_local_epoch(&config, epoch_id)?;

    print_command_header("verify", &config_path);
    println!("epoch_id={}", report.epoch_id);
    println!("batch_count={}", report.batch_count);
    println!("stored_payload_count={}", report.stored_payload_count);
    println!("all_valid={}", report.all_valid);
    for batch in report.reports {
        println!(
            "slot={} payload_hash_ok={} batch_root_ok={} obs_count_ok={} retention_record={} retention_hash_ok={}",
            batch.slot_id,
            batch.payload_hash_matches,
            batch.batch_root_matches,
            batch.obs_count_matches,
            batch.retention_record_present,
            batch.retention_hash_matches
        );
    }

    Ok(())
}

fn build_epoch_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let (config_path, start_index) = parse_config_path_and_rest(args, 2, DEFAULT_CONFIG_PATH);
    if args.len() > start_index + 3 {
        return Err(
            "usage: pole-client build-epoch [config-path] [epoch-id] [current-height] [challenge-window-blocks]"
                .into(),
        );
    }

    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path)?;
    let progress = LocalNodeProgress::load_or_default(progress_path(&config), &config)?;
    let epoch_id = resolve_epoch_id_arg(args, start_index, &config)?;
    let current_height = resolve_current_height_arg(args, start_index + 1, &progress)?;
    let challenge_window_blocks =
        resolve_challenge_window_blocks_arg(args, start_index + 2, &config)?;

    let (epoch_commit, artifact) = build_epoch_commit_from_local_data(
        &config,
        epoch_id,
        current_height,
        challenge_window_blocks,
        [0u8; 32],
        [0u8; 32],
    )?;

    println!("PoLE client build-epoch");
    println!("config_path={}", config_path.to_string_lossy());
    println!("epoch_id={}", epoch_commit.epoch_id);
    println!("batch_count={}", artifact.batch_count);
    println!("payload_count={}", artifact.payload_count);
    print_epoch_commit_artifact_roots(
        &artifact.accepted_batches_root_hex,
        &artifact.observations_root_hex,
        &artifact.aggregates_root_hex,
        &artifact.rewards_root_hex,
        &artifact.availability_root_hex,
        artifact.challenge_deadline_height,
    );

    Ok(())
}

fn prepare_epoch_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let (config_path, start_index) = parse_config_path_and_rest(args, 2, DEFAULT_CONFIG_PATH);
    if args.len() > start_index + 3 {
        return Err(
            "usage: pole-client prepare-epoch [config-path] [epoch-id] [current-height] [challenge-window-blocks]"
                .into(),
        );
    }

    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path)?;
    let progress = LocalNodeProgress::load_or_default(progress_path(&config), &config)?;
    let epoch_id = resolve_epoch_id_arg(args, start_index, &config)?;
    let current_height = resolve_current_height_arg(args, start_index + 1, &progress)?;
    let challenge_window_blocks =
        resolve_challenge_window_blocks_arg(args, start_index + 2, &config)?;

    let artifact = prepare_local_epoch(&config, epoch_id, current_height, challenge_window_blocks)?;

    println!("PoLE client prepare-epoch");
    println!("config_path={}", config_path.to_string_lossy());
    println!("epoch_id={}", artifact.epoch_id);
    println!("batch_count={}", artifact.batch_count);
    println!("payload_count={}", artifact.payload_count);
    println!("reward_block_secs={}", effective_reward_block_secs(&config));
    println!(
        "reward_adjustment_period_blocks={}",
        config.reward.reward_adjustment_period_blocks
    );
    println!(
        "verification_batch_count={}",
        artifact.verification_batch_count
    );
    println!("stored_payload_count={}", artifact.stored_payload_count);
    println!("aggregate_count={}", artifact.aggregate_count);
    println!("reward_count={}", artifact.reward_count);
    println!(
        "player_reward_block_count={}",
        artifact.player_reward_block_count
    );
    println!(
        "completed_hour_reward_block_count={}",
        artifact.player_reward_block_count
    );
    println!(
        "total_observation_count={}",
        artifact.total_observation_count
    );
    println!(
        "accepted_observation_count={}",
        artifact.accepted_observation_count
    );
    println!(
        "local_player_reward_total={}",
        artifact.local_player_reward_total
    );
    println!("total_gvs_units={}", artifact.total_gvs_units);
    println!("total_distributed={}", artifact.total_distributed);
    println!("verification_all_valid={}", artifact.verification_all_valid);
    print_epoch_commit_artifact_roots(
        &artifact.accepted_batches_root_hex,
        &artifact.observations_root_hex,
        &artifact.aggregates_root_hex,
        &artifact.rewards_root_hex,
        &artifact.availability_root_hex,
        artifact.challenge_deadline_height,
    );
    println!("ready_for_submission={}", artifact.ready_for_submission);

    Ok(())
}

fn suggest_settlement_height_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() > 3 {
        return Err("usage: pole-client suggest-settlement-height [config-path]".into());
    }

    let config_path = args
        .get(2)
        .map(String::as_str)
        .unwrap_or(DEFAULT_CONFIG_PATH);
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path)?;
    let height = suggested_settlement_height(&config)?;

    println!("PoLE client suggest-settlement-height");
    println!("config_path={}", config_path.to_string_lossy());
    println!("suggested_submission_height={height}");

    Ok(())
}

fn settle_epoch_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let (config_path, start_index) = parse_config_path_and_rest(args, 2, DEFAULT_CONFIG_PATH);
    if args.len() > start_index + 3 {
        return Err(
            "usage: pole-client settle-epoch [config-path] [epoch-id] [submission-height] [challenge-window-blocks]"
                .into(),
        );
    }

    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path)?;
    let progress = LocalNodeProgress::load_or_default(progress_path(&config), &config)?;
    let epoch_id = resolve_epoch_id_arg(args, start_index, &config)?;
    let submission_height = resolve_submission_height_arg(args, start_index + 1, &config)?;
    let challenge_window_blocks =
        resolve_challenge_window_blocks_arg(args, start_index + 2, &config)?;

    let artifact = settle_local_epoch(
        &config,
        epoch_id,
        submission_height,
        challenge_window_blocks,
    )?;

    println!("PoLE client settle-epoch");
    println!("config_path={}", config_path.to_string_lossy());
    println!("epoch_id={}", artifact.epoch_id);
    println!("submission_height={}", artifact.submission_height);
    println!(
        "challenge_window_blocks={}",
        artifact.challenge_window_blocks
    );
    println!(
        "challenge_deadline_height={}",
        artifact.challenge_deadline_height
    );
    println!("finalization_height={}", artifact.finalization_height);
    println!(
        "prepared_ready_for_submission={}",
        artifact.prepared_ready_for_submission
    );
    println!("batch_count={}", artifact.batch_count);
    println!("payload_count={}", artifact.payload_count);
    println!("batch_submission_count={}", artifact.batch_submission_count);
    println!(
        "batch_already_present_count={}",
        artifact.batch_already_present_count
    );
    println!("reward_record_count={}", artifact.reward_record_count);
    print_epoch_commit_artifact_roots(
        &artifact.accepted_batches_root_hex,
        &artifact.observations_root_hex,
        &artifact.aggregates_root_hex,
        &artifact.rewards_root_hex,
        &artifact.availability_root_hex,
        artifact.challenge_deadline_height,
    );
    println!("randomness_seed={}", artifact.randomness_seed_hex);
    println!("commit_applied={}", artifact.commit_applied);
    println!("commit_already_present={}", artifact.commit_already_present);
    println!("epoch_finalized={}", artifact.epoch_finalized);
    println!(
        "epoch_already_finalized={}",
        artifact.epoch_already_finalized
    );
    println!("local_reward_available={}", artifact.local_reward_available);
    println!("local_reward_claimed={}", artifact.local_reward_claimed);
    println!(
        "local_reward_already_claimed={}",
        artifact.local_reward_already_claimed
    );
    println!("local_reward_balance={}", artifact.local_reward_balance);
    println!("current_epoch_after={}", artifact.current_epoch_after);
    println!(
        "local_chain_runtime_path={}",
        artifact.local_chain_runtime_path
    );
    println!("local_chain_store_path={}", artifact.local_chain_store_path);
    println!("prepared_epoch_path={}", artifact.prepared_epoch_path);
    println!("progress_next_epoch={}", progress.next_epoch_id);

    Ok(())
}

fn submit_batch_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let (config_path, start_index) = parse_config_path_and_rest(args, 2, DEFAULT_CONFIG_PATH);
    if args.len() > start_index + 1 {
        return Err("usage: pole-client submit-batch [config-path] [epoch-id]".into());
    }

    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path)?;
    let epoch_id = resolve_epoch_id_arg(args, start_index, &config)?;

    let batches = load_batches_for_epoch(&config, epoch_id)?;
    if batches.is_empty() {
        return Err(format!("no batches found for epoch {}", epoch_id).into());
    }

    let collector_hex = hex_encode(&config.node_id()?);
    let mut batch_count = 0;
    for batch in &batches {
        if batch.batch_commit.epoch_id == epoch_id {
            let tx_json = chain_bridge::generate_tx_json_for_batch(&collector_hex, &batch.batch_commit)?;
            println!("{}", tx_json);
            batch_count += 1;
        }
    }

    println!("PoLE client submit-batch");
    println!("config_path={}", config_path.to_string_lossy());
    println!("epoch_id={}", epoch_id);
    println!("batch_count={}", batch_count);

    Ok(())
}

fn submit_epoch_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let (config_path, start_index) = parse_config_path_and_rest(args, 2, DEFAULT_CONFIG_PATH);
    if args.len() > start_index + 3 {
        return Err(
            "usage: pole-client submit-epoch [config-path] [epoch-id] [current-height] [challenge-window-blocks]"
                .into(),
        );
    }

    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path)?;
    let progress = LocalNodeProgress::load_or_default(progress_path(&config), &config)?;
    let epoch_id = resolve_epoch_id_arg(args, start_index, &config)?;
    let current_height = resolve_current_height_arg(args, start_index + 1, &progress)?;
    let challenge_window_blocks =
        resolve_challenge_window_blocks_arg(args, start_index + 2, &config)?;

    let preparation = node_prepare::compute_local_epoch_preparation(
        &config,
        epoch_id,
        current_height,
        challenge_window_blocks,
    )?;

    let proposer_hex = hex_encode(&config.node_id()?);
    let tx_json = chain_bridge::generate_tx_json_for_epoch_commit(
        &proposer_hex,
        &preparation.epoch_commit,
    )?;

    println!("PoLE client submit-epoch");
    println!("config_path={}", config_path.to_string_lossy());
    println!("epoch_id={}", epoch_id);
    println!("tx_json={}", tx_json);

    Ok(())
}

fn export_tx_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let (config_path, start_index) = parse_config_path_and_rest(args, 2, DEFAULT_CONFIG_PATH);
    if args.len() > start_index + 4 {
        return Err(
            "usage: pole-client export-tx [config-path] [type] [epoch-id] [current-height] [challenge-window-blocks]\n  types: batch, epoch"
                .into(),
        );
    }

    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path)?;
    let progress = LocalNodeProgress::load_or_default(progress_path(&config), &config)?;
    let tx_type = args
        .get(start_index)
        .ok_or("tx type required (batch|epoch)")?;
    let epoch_id = resolve_epoch_id_arg(args, start_index + 1, &config)?;

    match tx_type.as_str() {
        "batch" => {
            let batches = load_batches_for_epoch(&config, epoch_id)?;
            let collector_hex = hex_encode(&config.node_id()?);
            for batch in batches {
                if batch.batch_commit.epoch_id == epoch_id {
                    let tx_json = chain_bridge::generate_tx_json_for_batch(
                        &collector_hex,
                        &batch.batch_commit,
                    )?;
                    println!("{}", tx_json);
                }
            }
        }
        "epoch" => {
            if args.len() < start_index + 4 {
                return Err("epoch export requires current-height and challenge-window-blocks".into());
            }
            let current_height = resolve_current_height_arg(args, start_index + 2, &progress)?;
            let challenge_window_blocks =
                resolve_challenge_window_blocks_arg(args, start_index + 3, &config)?;

            let preparation = node_prepare::compute_local_epoch_preparation(
                &config,
                epoch_id,
                current_height,
                challenge_window_blocks,
            )?;
            let proposer_hex = hex_encode(&config.node_id()?);
            let tx_json = chain_bridge::generate_tx_json_for_epoch_commit(
                &proposer_hex,
                &preparation.epoch_commit,
            )?;
            println!("{}", tx_json);
        }
        _ => {
            return Err("tx type must be 'batch' or 'epoch'".into());
        }
    }

    Ok(())
}

fn prune_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let (config_path, start_index) = parse_config_path_and_rest(args, 2, DEFAULT_CONFIG_PATH);
    if args.len() > start_index + 1 {
        return Err("usage: pole-client prune [config-path] [current-epoch]".into());
    }

    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path)?;
    let current_epoch = match args.get(start_index) {
        Some(value) => value.parse::<u64>()?,
        None => latest_local_epoch(&config)?,
    };
    let outcome = prune_retention(&config, current_epoch)?;

    println!("PoLE client prune");
    println!("config_path={}", config_path.to_string_lossy());
    println!("current_epoch={}", outcome.current_epoch);
    println!("removed_payloads={}", outcome.removed_payloads.len());
    for payload in outcome.removed_payloads {
        println!("removed={payload}");
    }

    Ok(())
}

fn paths_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() > 3 {
        return Err("usage: pole-client paths [config-path]".into());
    }

    let config_path = args
        .get(2)
        .map(String::as_str)
        .unwrap_or(DEFAULT_CONFIG_PATH);
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path)?;
    let layout = effective_install_layout(&config_path, &config);
    let data_dir = layout.data_dir.clone();

    println!("PoLE client paths");
    print_path_entry("config_path", &config_path);
    print_path_entry("config_dir", &layout.config_dir);
    print_path_entry("data_dir", &data_dir);
    print_path_entry("logs_dir", &layout.log_dir);
    print_path_entry("updates_dir", &layout.update_dir);
    print_path_entry("runtime_state", progress_path(&config));
    print_path_entry("retention_book", retention_book_path(&config));
    print_path_entry("heartbeat", heartbeat_path(&config));
    print_data_dir_path("payloads_dir", &data_dir, "payloads");
    print_data_dir_path("batches_dir", &data_dir, "batches");
    print_data_dir_path("epochs_dir", &data_dir, "epochs");
    print_data_dir_path("aggregates_dir", &data_dir, "aggregates");
    print_data_dir_path("rewards_dir", &data_dir, "rewards");
    print_data_dir_path(
        "player_reward_blocks_dir",
        &data_dir,
        "player-reward-blocks",
    );
    print_data_dir_path("settlements_dir", &data_dir, "settlements");
    print_path_entry(
        "local_chain_runtime",
        data_dir.join("local-chain").join("runtime.json"),
    );
    print_path_entry(
        "local_chain_store",
        data_dir.join("local-chain").join("store.bin"),
    );
    print_data_dir_path("prepared_epochs_dir", &data_dir, "prepared-epochs");
    print_data_dir_path("verifications_dir", &data_dir, "verifications");

    Ok(())
}

fn apply_profile(config: &mut NodeConfig, profile: ClientProfile) {
    config.reward.reward_source = pole_protocol_draft::RewardSourceMode::Tokenomics;
    config.reward.emission_year = 1;

    match profile {
        ClientProfile::Minimal => {
            config.capabilities.collect = true;
            config.capabilities.store = false;
            config.capabilities.verify = false;
            config.capabilities.propose = false;
            config.capabilities.archive = false;
            config.runtime.low_impact_mode = true;
            config.runtime.os_background_priority = true;
            config.runtime.game_active_poll_interval_secs = 900;
        }
        ClientProfile::Player => {
            config.capabilities.collect = true;
            config.capabilities.store = true;
            config.capabilities.verify = false;
            config.capabilities.propose = false;
            config.capabilities.archive = false;
            config.runtime.low_impact_mode = true;
            config.runtime.os_background_priority = true;
            config.runtime.game_active_poll_interval_secs = 900;
        }
        ClientProfile::Validator => {
            config.capabilities.collect = true;
            config.capabilities.store = true;
            config.capabilities.verify = true;
            config.capabilities.propose = true;
            config.capabilities.archive = false;
            config.runtime.low_impact_mode = false;
            config.runtime.os_background_priority = false;
            config.runtime.game_active_poll_interval_secs = config.runtime.poll_interval_secs;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum BackgroundStartOutcome {
    Started { runtime: ServiceRuntime },
    AlreadyRunning { runtime: ServiceRuntime },
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AutostartRegistrationOutcome {
    Registered {
        launcher_path: PathBuf,
    },
    AlreadyRegistered {
        launcher_path: PathBuf,
    },
    #[cfg(not(windows))]
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackgroundWatchMode {
    Watch,
    WatchP2pSim,
    WatchP2pFs,
    WatchP2pSocket,
}

#[derive(Debug, Clone)]
struct DaemonFiles {
    pid_file: PathBuf,
    meta_file: PathBuf,
    stdout_log: PathBuf,
    stderr_log: PathBuf,
}

#[derive(Debug, Clone)]
struct DaemonStatusProbe {
    metadata: Option<DaemonMetadata>,
    snapshot: ServiceSnapshot,
}

impl BackgroundWatchMode {
    fn subcommand(self) -> &'static str {
        match self {
            Self::Watch => "watch",
            Self::WatchP2pSim => "watch-p2p-sim",
            Self::WatchP2pFs => "watch-p2p-fs",
            Self::WatchP2pSocket => "watch-p2p-socket",
        }
    }
}

impl DaemonFiles {
    fn from_data_dir(data_dir: &Path) -> Self {
        Self {
            pid_file: data_dir.join("daemon.pid"),
            meta_file: data_dir.join("daemon.meta.json"),
            stdout_log: data_dir.join("daemon.out.log"),
            stderr_log: data_dir.join("daemon.err.log"),
        }
    }

    fn read_pid(&self) -> Option<u32> {
        fs::read_to_string(&self.pid_file)
            .ok()
            .and_then(|content| content.lines().next().map(str::trim).map(str::to_string))
            .and_then(|content| content.parse::<u32>().ok())
    }

    fn read_metadata(&self) -> Option<DaemonMetadata> {
        let content = fs::read_to_string(&self.meta_file).ok()?;
        serde_json::from_str(&content).ok()
    }

    fn probe_status(&self) -> DaemonStatusProbe {
        let metadata = self.read_metadata();
        let pid = self.read_pid();
        let persisted_pid = metadata.as_ref().map(|entry| entry.pid);
        let running = pid
            .or(persisted_pid)
            .map(process_is_running)
            .unwrap_or(false);
        let runtime = ServiceRuntime::from_persisted_observation(pid, persisted_pid, running);
        let snapshot = runtime.snapshot();
        DaemonStatusProbe { metadata, snapshot }
    }

    fn clear_stale_state(&self) {
        let _ = fs::remove_file(&self.pid_file);
        let _ = fs::remove_file(&self.meta_file);
    }

    fn create_logs(&self) -> Result<(File, File), Box<dyn std::error::Error>> {
        Ok((
            File::create(&self.stdout_log)?,
            File::create(&self.stderr_log)?,
        ))
    }

    fn write_pid(&self, pid: u32) -> Result<(), Box<dyn std::error::Error>> {
        fs::write(&self.pid_file, pid.to_string())?;
        Ok(())
    }

    fn write_metadata(
        &self,
        pid: u32,
        background_mode: &str,
        config_path: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let metadata = DaemonMetadata {
            pid,
            background_mode: background_mode.to_string(),
            config_path: config_path.to_string_lossy().into_owned(),
            pid_file: self.pid_file.to_string_lossy().into_owned(),
            stdout_log: self.stdout_log.to_string_lossy().into_owned(),
            stderr_log: self.stderr_log.to_string_lossy().into_owned(),
            started_at_millis: unix_time_millis()?,
        };
        let content = serde_json::to_string_pretty(&metadata)?;
        fs::write(&self.meta_file, content)?;
        Ok(())
    }
}

fn default_player_start_config_path() -> PathBuf {
    if let Some(local_app_data) = env::var_os("LOCALAPPDATA") {
        PathBuf::from(local_app_data)
            .join("PoLE")
            .join("player")
            .join("node.json")
    } else {
        PathBuf::from(DEFAULT_CONFIG_PATH)
    }
}

fn resolve_input_path(path: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(env::current_dir()?.join(path))
    }
}

fn load_or_init_player_config(
    config_path: &Path,
) -> Result<NodeConfig, Box<dyn std::error::Error>> {
    let mut config = if config_path.exists() {
        NodeConfig::load_json(config_path)?
    } else {
        let mut created = NodeConfig::default();
        created.runtime.data_dir = default_data_dir_for_config(config_path);
        created
    };

    apply_profile(&mut config, ClientProfile::Player);
    if config.runtime.data_dir.trim().is_empty() {
        config.runtime.data_dir = default_data_dir_for_config(config_path);
    }
    sync_reward_game_mappings(&mut config);
    sync_activity_sources(&mut config);
    config.save_json(config_path)?;

    let (_, resolved) = NodeConfig::load_json_with_runtime_paths(config_path)?;
    fs::create_dir_all(&resolved.runtime.data_dir)?;
    Ok(resolved)
}

fn ensure_player_autostart(
    config_path: &Path,
) -> Result<AutostartRegistrationOutcome, Box<dyn std::error::Error>> {
    #[cfg(windows)]
    {
        let startup_dir = player_startup_dir()?;
        fs::create_dir_all(&startup_dir)?;

        let launcher_path = startup_dir.join(player_launcher_filename(config_path));
        let launcher_contents = render_windows_startup_launcher(&env::current_exe()?, config_path);

        if fs::read_to_string(&launcher_path).ok().as_deref() == Some(launcher_contents.as_str()) {
            return Ok(AutostartRegistrationOutcome::AlreadyRegistered { launcher_path });
        }

        fs::write(&launcher_path, launcher_contents)?;
        Ok(AutostartRegistrationOutcome::Registered { launcher_path })
    }

    #[cfg(not(windows))]
    {
        let _ = config_path;
        Ok(AutostartRegistrationOutcome::Unsupported)
    }
}

fn probe_player_autostart(config_path: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let launcher_path = player_startup_dir()
            .ok()?
            .join(player_launcher_filename(config_path));
        launcher_path.exists().then_some(launcher_path)
    }

    #[cfg(not(windows))]
    {
        let _ = config_path;
        None
    }
}

#[cfg(windows)]
fn player_startup_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let app_data = env::var_os("APPDATA")
        .ok_or("APPDATA is not set; cannot configure Windows startup launcher")?;
    Ok(PathBuf::from(app_data)
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join("Startup"))
}

fn player_launcher_filename(config_path: &Path) -> String {
    let normalized = config_path.to_string_lossy().replace('\\', "/");
    format!(
        "PoLE Player Mode-{:016x}.vbs",
        stable_hash64(normalized.as_bytes())
    )
}

#[cfg(windows)]
fn render_windows_startup_launcher(exe_path: &Path, config_path: &Path) -> String {
    let command_line = format!(
        "\"{}\" player-autostart \"{}\"",
        exe_path.to_string_lossy(),
        config_path.to_string_lossy()
    );
    let escaped = command_line.replace('"', "\"\"");
    format!(
        "Set WshShell = CreateObject(\"WScript.Shell\")\r\nWshShell.Run \"{escaped}\", 0, False\r\n"
    )
}

fn stable_hash64(input: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in input {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn should_capture_foreground_process(process_name: &str) -> bool {
    let normalized = process_name
        .trim()
        .to_ascii_lowercase()
        .trim_end_matches(".exe")
        .to_string();
    !normalized.is_empty()
        && !matches!(
            normalized.as_str(),
            "pole-client"
                | "cmd"
                | "powershell"
                | "pwsh"
                | "conhost"
                | "windowsterminal"
                | "explorer"
                | "devenv"
                | "code"
                | "idea64"
                | "notepad"
                | "notepad++"
        )
}

fn generate_identity_keypair() -> KeyPair {
    let mut seed = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut seed);
    KeyPair::from_seed(&seed)
}

fn node_id_hex_from_identity(identity_keypair: &KeyPair) -> String {
    hex_32(stable_hash32(&identity_keypair.public))
}

fn write_identity_file(
    data_dir: &Path,
    identity_keypair: &KeyPair,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(data_dir)?;
    let identity_path = data_dir.join("identity.json");
    let identity_json = serde_json::to_string_pretty(identity_keypair)?;
    fs::write(identity_path, identity_json)?;
    Ok(())
}

fn has_placeholder_node_identity(config: &NodeConfig) -> bool {
    config.node_id_hex == hex_32([0x11; 32]) || config.node_id_hex == hex_32([0x31; 32])
}

fn socket_topic_label(topic: pole_protocol_draft::P2pTopic) -> &'static str {
    match topic {
        pole_protocol_draft::P2pTopic::Observations => "observations",
        pole_protocol_draft::P2pTopic::Batches => "batches",
        pole_protocol_draft::P2pTopic::Receipts => "receipts",
        pole_protocol_draft::P2pTopic::Challenges => "challenges",
    }
}

fn default_socket_topic_labels() -> Vec<&'static str> {
    vec!["observations", "batches", "receipts", "challenges"]
}

fn start_background_watch(
    config_path: &Path,
    ticks: Option<u64>,
    mode: BackgroundWatchMode,
) -> Result<BackgroundStartOutcome, Box<dyn std::error::Error>> {
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path)?;
    let data_dir = PathBuf::from(&config.runtime.data_dir);
    fs::create_dir_all(&data_dir)?;

    let daemon_files = DaemonFiles::from_data_dir(&data_dir);

    if let Some(existing_pid) = daemon_files.read_pid() {
        if process_is_running(existing_pid) {
            return Ok(BackgroundStartOutcome::AlreadyRunning {
                runtime: ServiceRuntime::from_observed_process(Some(existing_pid), true),
            });
        }
        daemon_files.clear_stale_state();
    }

    let current_exe = env::current_exe()?;
    let config_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
    let (stdout, stderr) = daemon_files.create_logs()?;

    let mut command = Command::new(current_exe);
    command.arg(mode.subcommand()).arg(&config_path);
    if let Some(ticks) = ticks {
        command.arg(ticks.to_string());
    }
    command.current_dir(config_dir);
    command.stdout(Stdio::from(stdout));
    command.stderr(Stdio::from(stderr));

    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let child = command.spawn()?;
    daemon_files.write_pid(child.id())?;
    daemon_files.write_metadata(child.id(), mode.subcommand(), &config_path)?;
    Ok(BackgroundStartOutcome::Started {
        runtime: ServiceRuntime::from_observed_process(Some(child.id()), true),
    })
}

fn control_api_log_paths(config: &NodeConfig) -> (PathBuf, PathBuf) {
    let data_dir = PathBuf::from(&config.runtime.data_dir);
    (
        data_dir.join("control-api.out.log"),
        data_dir.join("control-api.err.log"),
    )
}

fn ensure_control_api_server(
    config_path: &Path,
    config: &NodeConfig,
    bind_addr: &str,
) -> Result<ControlApiLaunchOutcome, Box<dyn std::error::Error>> {
    let dashboard_addr = bind_addr.parse()?;
    if std::net::TcpListener::bind(bind_addr).is_ok() {
        let pid = spawn_control_api_server(config_path, config, bind_addr)?;
        wait_for_control_api_ready(dashboard_addr, Duration::from_secs(5))?;
        Ok(ControlApiLaunchOutcome::Started { pid })
    } else if probe_control_api_ready(dashboard_addr) {
        Ok(ControlApiLaunchOutcome::AlreadyRunning)
    } else {
        Err(
            format!("bind address {bind_addr} is unavailable and control api is not reachable")
                .into(),
        )
    }
}

fn spawn_control_api_server(
    config_path: &Path,
    config: &NodeConfig,
    bind_addr: &str,
) -> Result<u32, Box<dyn std::error::Error>> {
    let current_exe = env::current_exe()?;
    let config_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(&config.runtime.data_dir)?;
    let (stdout_log, stderr_log) = control_api_log_paths(config);
    let stdout = File::create(stdout_log)?;
    let stderr = File::create(stderr_log)?;

    let mut command = Command::new(current_exe);
    command
        .arg("control-api-serve")
        .arg(config_path)
        .arg(bind_addr)
        .current_dir(config_dir)
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));

    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let child = command.spawn()?;
    Ok(child.id())
}

fn wait_for_control_api_ready(
    addr: std::net::SocketAddr,
    timeout: Duration,
) -> Result<(), Box<dyn std::error::Error>> {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if probe_control_api_ready(addr) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err(format!("timed out waiting for control api at {addr}").into())
}

fn probe_control_api_ready(addr: std::net::SocketAddr) -> bool {
    let mut stream = match TcpStream::connect_timeout(&addr, Duration::from_millis(250)) {
        Ok(stream) => stream,
        Err(_) => return false,
    };
    let request = format!(
        "GET /api/status HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        addr
    );
    if std::io::Write::write_all(&mut stream, request.as_bytes()).is_err() {
        return false;
    }
    let mut response = String::new();
    if std::io::Read::read_to_string(&mut stream, &mut response).is_err() {
        return false;
    }
    response.contains("\"service\"") && response.contains("\"node\"")
}

fn open_dashboard_url(url: &str) -> Result<(), Box<dyn std::error::Error>> {
    if let Ok(opener) = env::var("POLE_CLIENT_BROWSER_OPENER") {
        #[cfg(windows)]
        let status = {
            let lower = opener.to_ascii_lowercase();
            if lower.ends_with(".cmd") || lower.ends_with(".bat") {
                Command::new("cmd")
                    .args(["/C", opener.as_str(), url])
                    .status()?
            } else {
                Command::new(opener.as_str()).arg(url).status()?
            }
        };
        #[cfg(not(windows))]
        let status = Command::new(opener).arg(url).status()?;
        if status.success() {
            return Ok(());
        }
        return Err(format!("browser opener failed for {url}").into());
    }

    #[cfg(windows)]
    {
        let status = Command::new("cmd")
            .args(["/C", "start", "", url])
            .status()?;
        if status.success() {
            return Ok(());
        }
        Err(format!("failed to open dashboard url {url}").into())
    }

    #[cfg(target_os = "macos")]
    {
        let status = Command::new("open").arg(url).status()?;
        if status.success() {
            return Ok(());
        }
        Err(format!("failed to open dashboard url {url}").into())
    }

    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        let status = Command::new("xdg-open").arg(url).status()?;
        if status.success() {
            return Ok(());
        }
        Err(format!("failed to open dashboard url {url}").into())
    }
}

fn background_watch_mode_from_env() -> BackgroundWatchMode {
    match env::var("POLE_CLIENT_BACKGROUND_MODE").ok().as_deref() {
        Some("watch-p2p-sim") => BackgroundWatchMode::WatchP2pSim,
        Some("watch-p2p-fs") => BackgroundWatchMode::WatchP2pFs,
        Some("watch-p2p-socket") => BackgroundWatchMode::WatchP2pSocket,
        _ => BackgroundWatchMode::Watch,
    }
}

fn default_filesystem_network_dir(config: &NodeConfig) -> PathBuf {
    PathBuf::from(&config.runtime.data_dir).join("p2p-fs-network")
}

fn build_socket_watch_network(
    config: &NodeConfig,
) -> Result<SocketP2pNetwork, Box<dyn std::error::Error>> {
    let bind_addr = env::var("POLE_CLIENT_SOCKET_BIND_ADDR")
        .unwrap_or_else(|_| config.runtime.p2p_socket.bind_addr.clone())
        .parse()?;
    let peers = if let Ok(specs) = env::var("POLE_CLIENT_SOCKET_PEER_SPECS") {
        parse_socket_peer_specs(&specs)?
    } else if config.runtime.p2p_socket.peers.is_empty() {
        Vec::new()
    } else {
        socket_peers_from_config(&config.runtime.p2p_socket.peers)?
    };
    let mut network = SocketP2pNetwork::bind(config.node_id()?, bind_addr, peers)?;
    network.bootstrap_peer(
        config.node_id()?,
        &[
            pole_protocol_draft::P2pTopic::Batches,
            pole_protocol_draft::P2pTopic::Receipts,
        ],
    )?;
    Ok(network)
}

fn background_watch_ticks_from_env() -> Result<Option<u64>, Box<dyn std::error::Error>> {
    env::var("POLE_CLIENT_BACKGROUND_TICKS")
        .ok()
        .map(|value| value.parse::<u64>().map_err(Into::into))
        .transpose()
}

fn unix_time_millis() -> Result<u64, Box<dyn std::error::Error>> {
    let duration = SystemTime::now().duration_since(UNIX_EPOCH)?;
    Ok(duration.as_millis() as u64)
}

fn process_is_running(pid: u32) -> bool {
    #[cfg(windows)]
    {
        let script = format!(
            "$p = Get-Process -Id {pid} -ErrorAction SilentlyContinue; if ($p) {{ exit 0 }} else {{ exit 1 }}"
        );
        Command::new("powershell")
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
        let _ = pid;
        false
    }
}

fn configure_game_process_awareness(config: &mut NodeConfig) {
    config.runtime.low_impact_mode = true;
    config.runtime.os_background_priority = true;
    if config.runtime.game_active_poll_interval_secs < config.runtime.poll_interval_secs {
        config.runtime.game_active_poll_interval_secs = config.runtime.poll_interval_secs;
    }
}

fn save_game_process_names(
    config: &mut NodeConfig,
    config_path: &Path,
    process_names: Vec<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    configure_game_process_awareness(config);
    config.runtime.game_process_names = process_names;
    sync_reward_game_mappings(config);
    sync_activity_sources(config);
    config.save_json(config_path)?;
    Ok(())
}

fn print_background_start_outcome(outcome: BackgroundStartOutcome) {
    match outcome {
        BackgroundStartOutcome::Started { runtime } => {
            println!("background_started=true");
            if let Some(pid) = runtime.current_pid() {
                println!("daemon_pid={pid}");
            }
        }
        BackgroundStartOutcome::AlreadyRunning { runtime } => {
            println!("background_started=true");
            if let Some(pid) = runtime.current_pid() {
                println!("daemon_pid={pid}");
            }
            println!("daemon_already_running=true");
        }
        BackgroundStartOutcome::Skipped => {
            println!("background_started=false");
            println!("background_start_skipped=true");
        }
    }
}

fn merge_process_names(existing: &[String], incoming: &[String]) -> Vec<String> {
    let mut merged: Vec<String> = Vec::new();
    for name in existing.iter().chain(incoming.iter()) {
        let canonical = canonical_process_name(name);
        if canonical.is_empty() {
            continue;
        }
        if !merged
            .iter()
            .any(|item| item.eq_ignore_ascii_case(&canonical))
        {
            merged.push(canonical);
        }
    }
    merged
}

fn sync_reward_game_mappings(config: &mut NodeConfig) {
    let normalized_processes = merge_process_names(&[], &config.runtime.game_process_names);
    let default_app_id = config
        .runtime
        .target_app_ids
        .first()
        .copied()
        .unwrap_or(730);
    let cache_path = recognition_cache_path(Path::new(&config.runtime.data_dir));
    let mut mappings = Vec::new();

    for process_name in &normalized_processes {
        if let Some(existing) = config.reward.game_mappings.iter().find(|mapping| {
            canonical_process_name(&mapping.process_name).eq_ignore_ascii_case(process_name)
        }) {
            let mut mapping = existing.clone();
            mapping.process_name = canonical_process_name(process_name);
            mappings.push(mapping);
        } else if let Some(mut cached) = load_cached_reward_game_mapping(&cache_path, process_name)
        {
            cached.process_name = canonical_process_name(process_name);
            mappings.push(cached);
        } else if let Some(mut inferred) = infer_reward_game_mapping(process_name) {
            inferred.process_name = canonical_process_name(process_name);
            let _ = store_cached_reward_game_mapping(&cache_path, &inferred);
            mappings.push(inferred);
        } else {
            let fallback = pole_protocol_draft::RewardGameMapping {
                process_name: canonical_process_name(process_name),
                app_id: default_app_id,
                game_coefficient_ppm: 1_000_000,
            };
            let _ = store_cached_reward_game_mapping(&cache_path, &fallback);
            mappings.push(fallback);
        }
    }

    config.runtime.game_process_names = normalized_processes;
    let mut inferred_targets = Vec::new();
    for mapping in &mappings {
        if !inferred_targets.contains(&mapping.app_id) {
            inferred_targets.push(mapping.app_id);
        }
    }
    if !inferred_targets.is_empty() {
        config.runtime.target_app_ids = inferred_targets;
    }
    config.reward.game_mappings = mappings;
}

fn sync_activity_sources(config: &mut NodeConfig) {
    let mut reconciled = config
        .runtime
        .activity_sources
        .iter()
        .filter(|source| source.source_kind != ActivitySourceKind::Steam)
        .cloned()
        .collect::<Vec<_>>();

    for app_id in &config.runtime.target_app_ids {
        if !reconciled.iter().any(|source| {
            source.app_id == *app_id && source.source_kind == ActivitySourceKind::Steam
        }) {
            reconciled.push(ActivitySourceConfig {
                app_id: *app_id,
                source_kind: ActivitySourceKind::Steam,
                endpoint_url: Some(current_players_url(*app_id)),
                inline_json: None,
            });
        }
    }

    reconciled.sort_by_key(|source| (source.app_id, source_kind_order(source.source_kind)));
    config.runtime.activity_sources = reconciled;
}

fn source_kind_order(kind: ActivitySourceKind) -> u8 {
    match kind {
        ActivitySourceKind::Steam => 0,
        ActivitySourceKind::Epic => 1,
        ActivitySourceKind::Ea => 2,
        ActivitySourceKind::Gog => 3,
        ActivitySourceKind::Community => 4,
    }
}

fn canonical_process_name(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let base = if trimmed.len() >= 4 && trimmed[trimmed.len() - 4..].eq_ignore_ascii_case(".exe") {
        &trimmed[..trimmed.len() - 4]
    } else {
        trimmed
    };
    format!("{base}.exe")
}

fn format_capabilities(config: &NodeConfig) -> String {
    let mut capabilities = Vec::new();
    if config.capabilities.collect {
        capabilities.push("collect");
    }
    if config.capabilities.store {
        capabilities.push("store");
    }
    if config.capabilities.verify {
        capabilities.push("verify");
    }
    if config.capabilities.propose {
        capabilities.push("propose");
    }
    if config.capabilities.archive {
        capabilities.push("archive");
    }

    if capabilities.is_empty() {
        "none".to_string()
    } else {
        capabilities.join(",")
    }
}

fn command_hint(subcommand: &str, config_path: &str) -> String {
    if config_path == DEFAULT_CONFIG_PATH {
        format!("pole-client {subcommand}")
    } else {
        format!("pole-client {subcommand} {config_path}")
    }
}

fn print_next_step(subcommand: &str, config_path: &Path) {
    println!(
        "next_step={}",
        command_hint(subcommand, config_path.to_string_lossy().as_ref())
    );
}

fn print_usage() {
    let defaults = [
        format!("  config-path: {DEFAULT_CONFIG_PATH}"),
        "  init profile: player".to_string(),
    ];
    print!(
        "{}",
        format_usage_block("pole-client commands:", CLIENT_USAGE_COMMANDS)
    );
    println!();
    print!(
        "{}",
        format_usage_block("defaults:", defaults.iter().map(String::as_str))
    );
}

fn wallet_create_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = args.get(2).map(PathBuf::from).unwrap_or_else(|| {
        default_data_dir_for_config(Path::new(DEFAULT_CONFIG_PATH)).into()
    });
    let password = args.get(3).cloned().unwrap_or_else(|| {
        rpassword::prompt_password("password: ").unwrap_or_default()
    });
    let comment = args.get(4).map(String::clone);

    let mnemonic = pole_protocol_draft::create_wallet(&data_dir, comment, &password)?;
    println!("wallet_created");
    println!("mnemonic={}", mnemonic);
    println!("data_dir={}", data_dir.display());
    Ok(())
}

fn wallet_recover_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = args.get(2).map(PathBuf::from).ok_or("usage: wallet-recover [data-dir] [password] <24-word-mnemonic...>")?;
    let password = args.get(3).cloned().unwrap_or_else(|| {
        rpassword::prompt_password("password: ").unwrap_or_default()
    });
    let words: Vec<String> = args[4..].to_vec();
    if words.len() != 24 {
        return Err("mnemonic must be exactly 24 words".into());
    }

    let address = pole_protocol_draft::recover_wallet(&words[..], &data_dir, None, &password)?;
    println!("wallet_recovered");
    println!("address={}", address);
    println!("data_dir={}", data_dir.display());
    Ok(())
}

fn wallet_address_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = args.get(2).map(PathBuf::from).unwrap_or_else(|| {
        default_data_dir_for_config(Path::new(DEFAULT_CONFIG_PATH)).into()
    });
    let password = args.get(3).cloned().unwrap_or_else(|| {
        rpassword::prompt_password("wallet password: ").unwrap_or_default()
    });
    let address = pole_protocol_draft::show_address_with_password(&data_dir, &password)?;
    println!("{}", address);
    Ok(())
}

fn wallet_set_reward_address_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = PathBuf::from(args.get(2).ok_or("usage: wallet-set-reward-address <config-path> [data-dir] [password]")?);
    let data_dir = args.get(3).map(PathBuf::from).unwrap_or_else(|| {
        default_data_dir_for_config(Path::new(DEFAULT_CONFIG_PATH)).into()
    });
    let password = args.get(4).cloned().unwrap_or_else(|| {
        rpassword::prompt_password("wallet password: ").unwrap_or_default()
    });

    let address = pole_protocol_draft::set_reward_address(&data_dir, &config_path, &password)?;
    println!("reward_address_updated");
    println!("address={}", address);
    println!("config={}", config_path.display());
    Ok(())
}
