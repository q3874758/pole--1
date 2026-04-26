use std::env;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use pole_protocol_draft::{
    aggregate_local_epoch, allocation_breakdown, annual_emission_schedule_with_tail,
    build_epoch_commit_from_local_data, build_inmemory_simulation_network,
    build_libp2p_backend_skeleton, build_real_libp2p_swarm_report,
    collect_configured_activity_source, current_unix_millis, decode_hex32,
    detect_active_game_processes, dispatch_command, effective_collect_interval_secs,
    export_governance_proposal_artifact, export_governance_scheduled_artifact,
    fetch_current_players_live, format_usage_block, governance_index_artifact_path,
    governance_summary_artifact_path, inmemory_simulation_listener_peer_ids,
    inmemory_simulation_retrieval_peer_id, load_status, maybe_write_payload,
    open_local_protocol_state, parse_community_activity_response, parse_current_players_response,
    parse_simulation_topology_args, parse_socket_addr, parse_socket_peer_specs,
    parse_socket_topics, parse_third_party_activity_response, parse_vote_choice,
    prepare_local_epoch, print_batch_summary, print_governance_index,
    print_governance_proposal_artifact, print_governance_scheduled_artifact,
    print_governance_summary, print_reward_adjustment_index, print_reward_adjustment_summary,
    prune_retention, reward_local_epoch, run_collect_tick_with_client,
    run_collect_tick_with_client_and_network, run_libp2p_skeleton_loop, socket_peers_from_config,
    source_kind_label, submit_protocol_params_update_proposal, summarize_collect_loop_with_client,
    summarize_collect_loop_with_client_and_network, verify_local_epoch, ActivitySourceKind,
    BatchBuilder, CollectLoopSummary, CollectTickResult, FilesystemP2pNetwork,
    GovernanceArtifactIndex, GovernanceArtifactSummary, HttpTextClient, InMemoryP2pNetwork,
    LocalNodeProgress, LocalRetentionBook, NodeConfig, P2pNetwork, P2pSimulationConfig, P2pTopic,
    ProtocolStore, ReqwestHttpTextClient, ServiceManager, SocketP2pNetwork, SocketPeerProfile,
    SteamCurrentPlayersSample, INITIAL_EMISSION_RATE_BPS, LONG_TERM_TAIL_EMISSION_RATE_BPS,
    LONG_TERM_TAIL_START_YEAR, TOTAL_SUPPLY,
};
type NodeCommandHandler = pole_protocol_draft::CommandHandler;
const NODE_USAGE_COMMANDS: &[&str] = &[
    "  pole-node init-config <path>",
    "  pole-node build-batch-from-steam-json <config-path> <epoch-id> <slot-id> <appid> <observed-at-millis> <steam-json-path> [payload-out-path]",
    "  pole-node build-batch-from-steam-api <config-path> <epoch-id> <slot-id> <appid> [payload-out-path]",
    "  pole-node build-batch-from-epic-json <config-path> <epoch-id> <slot-id> <appid> <observed-at-millis> <json-path> [payload-out-path]",
    "  pole-node build-batch-from-epic-api <config-path> <epoch-id> <slot-id> <appid> <endpoint-url> [payload-out-path]",
    "  pole-node build-batch-from-ea-json <config-path> <epoch-id> <slot-id> <appid> <observed-at-millis> <json-path> [payload-out-path]",
    "  pole-node build-batch-from-ea-api <config-path> <epoch-id> <slot-id> <appid> <endpoint-url> [payload-out-path]",
    "  pole-node build-batch-from-gog-json <config-path> <epoch-id> <slot-id> <appid> <observed-at-millis> <json-path> [payload-out-path]",
    "  pole-node build-batch-from-gog-api <config-path> <epoch-id> <slot-id> <appid> <endpoint-url> [payload-out-path]",
    "  pole-node build-batch-from-community-json <config-path> <epoch-id> <slot-id> <appid> <observed-at-millis> <json-path> [payload-out-path]",
    "  pole-node build-batch-from-community-inline-json <config-path> <epoch-id> <slot-id> <appid> <observed-at-millis> <inline-json> [payload-out-path]",
    "  pole-node issue-replica-receipt <config-path> <ledger-path> <epoch-id> <payload-file-path>",
    "  pole-node run-once <config-path>",
    "  pole-node run-once-p2p-sim <config-path> [--batch-listeners <count>] [--receipt-listeners <count>] [--dual-listeners <count>]",
    "  pole-node run-once-p2p-fs <config-path> <network-dir>",
    "  pole-node run-loop <config-path> [ticks]",
    "  pole-node run-loop-p2p-sim <config-path> [ticks] [--batch-listeners <count>] [--receipt-listeners <count>] [--dual-listeners <count>]",
    "  pole-node run-loop-p2p-fs <config-path> <network-dir> [ticks]",
    "  pole-node run-once-p2p-socket <config-path> <bind-addr> <peer-specs>",
    "  pole-node run-loop-p2p-socket <config-path> <bind-addr> <peer-specs> [ticks]",
    "    peer-specs format: <peer-id-hex>@<peer-addr>@<topics>[;<peer-id-hex>@<peer-addr>@<topics>]...",
    "  pole-node status <config-path>",
    "  pole-node tokenomics [years]",
    "  pole-node governance-propose-params <config-path> <proposal-id-hex> <effective-epoch> <emission-year> <effective-player-block-reward> [tail-start-year tail-rate-bps]",
    "  pole-node governance-propose-slow-params <config-path> <proposal-id-hex> <effective-epoch> <reward-block-secs> <effective-player-block-reward>",
    "  pole-node governance-propose-retention <config-path> <proposal-id-hex> <effective-epoch> <min-retention-epochs> <challenge-window-blocks>",
    "  pole-node governance-propose-app-weight <config-path> <proposal-id-hex> <effective-epoch> <app-id> <game-coefficient-ppm>",
    "  pole-node governance-propose-tier-weights <config-path> <proposal-id-hex> <effective-epoch> <tier1_weight_ppm> <tier2_min_ppm> <tier2_max_ppm> <tier3_min_ppm> <tier3_max_ppm>",
    "  pole-node governance-propose-reward-tuning <config-path> <proposal-id-hex> <effective-epoch> <target_network_weight_units> <reward_adjustment_cap_bps> <challenge_window_blocks> <effective-player-block-reward>",
    "  pole-node governance-vote <config-path> <proposal-id-hex> <yes|no|abstain> <voting-power>",
    "  pole-node governance-show-proposal <config-path> <proposal-id-hex>",
    "  pole-node governance-show-scheduled <config-path> [epoch-id]",
    "  pole-node governance-show-index <config-path>",
    "  pole-node governance-show-summary <config-path>",
    "  pole-node reward-adjustment-show-index <config-path>",
    "  pole-node reward-adjustment-show-summary <config-path>",
    "  pole-node adjustment-cycle-show-index <config-path>",
    "  pole-node adjustment-cycle-show-summary <config-path>",
    "  pole-node libp2p-diagnose <config-path>",
    "  pole-node libp2p-skeleton <config-path>",
    "  pole-node libp2p-loop <config-path> [ticks]",
    "  pole-node service-run <config-path>",
    "  pole-node service-install <config-path>",
    "  pole-node service-uninstall <config-path>",
    "  pole-node service-start <config-path>",
    "  pole-node service-stop <config-path>",
    "  pole-node service-status <config-path>",
    "  pole-node prune-retention <config-path> <current-epoch>",
    "  pole-node build-epoch-commit <config-path> <epoch-id> <current-height> <challenge-window-blocks>",
    "  pole-node prepare-epoch <config-path> <epoch-id> <current-height> <challenge-window-blocks>",
    "  pole-node aggregate-epoch <config-path> <epoch-id>",
    "  pole-node reward-epoch <config-path> <epoch-id>",
    "  pole-node verify-epoch <config-path> <epoch-id>",
];
const NODE_COMMANDS: &[(&str, NodeCommandHandler)] = &[
    ("init-config", init_config_cmd),
    ("build-batch-from-steam-json", build_batch_from_steam_json),
    ("build-batch-from-steam-api", build_batch_from_steam_api),
    ("build-batch-from-epic-json", build_batch_from_epic_json_cmd),
    ("build-batch-from-epic-api", build_batch_from_epic_api_cmd),
    ("build-batch-from-ea-json", build_batch_from_ea_json_cmd),
    ("build-batch-from-ea-api", build_batch_from_ea_api_cmd),
    ("build-batch-from-gog-json", build_batch_from_gog_json_cmd),
    ("build-batch-from-gog-api", build_batch_from_gog_api_cmd),
    (
        "build-batch-from-community-json",
        build_batch_from_community_json,
    ),
    (
        "build-batch-from-community-inline-json",
        build_batch_from_community_inline_json,
    ),
    ("issue-replica-receipt", issue_replica_receipt),
    ("run-once", run_once),
    ("run-once-p2p-sim", run_once_p2p_sim),
    ("run-once-p2p-fs", run_once_p2p_fs),
    ("run-once-p2p-socket", run_once_p2p_socket),
    ("run-loop", run_loop),
    ("run-loop-p2p-sim", run_loop_p2p_sim),
    ("run-loop-p2p-fs", run_loop_p2p_fs),
    ("run-loop-p2p-socket", run_loop_p2p_socket),
    ("status", status),
    ("tokenomics", tokenomics_cmd),
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
        "governance-propose-app-weight",
        governance_propose_app_weight_cmd,
    ),
    (
        "governance-propose-tier-weights",
        governance_propose_tier_weights_cmd,
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
    ("libp2p-diagnose", libp2p_diagnose_cmd),
    ("libp2p-skeleton", libp2p_skeleton_cmd),
    ("libp2p-loop", libp2p_loop_cmd),
    ("service-run", service_run_cmd),
    ("service-install", service_install_cmd),
    ("service-uninstall", service_uninstall_cmd),
    ("service-start", service_start_cmd),
    ("service-stop", service_stop_cmd),
    ("service-status", service_status_cmd),
    ("prune-retention", prune_retention_cmd),
    ("build-epoch-commit", build_epoch_commit_cmd),
    ("prepare-epoch", prepare_epoch_cmd),
    ("aggregate-epoch", aggregate_epoch_cmd),
    ("reward-epoch", reward_epoch_cmd),
    ("verify-epoch", verify_epoch_cmd),
];

fn main() {
    if let Err(err) = run() {
        eprintln!("pole-node error: {err}");
        std::process::exit(1);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum NetworkMode {
    Disabled,
    InMemorySimulation(P2pSimulationConfig),
    FilesystemBackend(PathBuf),
    SocketBackend {
        bind_addr: SocketAddr,
        peers: Vec<SocketPeerProfile>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct P2pSimulationSummary {
    topology_peer_count: usize,
    batch_listener_count: usize,
    receipt_listener_count: usize,
    dual_listener_count: usize,
    batch_recipients: usize,
    receipt_recipients: usize,
    total_inbox_messages: usize,
    payload_retrieval_ok: bool,
}

#[derive(Debug, Clone)]
struct RunOnceExecution {
    result: CollectTickResult,
    p2p_simulation: Option<P2pSimulationSummary>,
}

#[derive(Debug, Clone)]
struct RunLoopExecution {
    summary: CollectLoopSummary,
    p2p_simulation: Option<P2pSimulationSummary>,
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = env::args().collect::<Vec<_>>();
    dispatch_command(&args, NODE_COMMANDS, print_usage)
}

fn init_config_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let path = args.get(2).ok_or("usage: pole-node init-config <path>")?;
    let config = NodeConfig::default();
    config.save_json(path)?;
    println!("wrote default config to {path}");
    Ok(())
}

fn build_batch_from_epic_json_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    build_batch_from_third_party_json(args, ActivitySourceKind::Epic)
}

fn build_batch_from_epic_api_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    build_batch_from_third_party_api(args, ActivitySourceKind::Epic)
}

fn build_batch_from_ea_json_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    build_batch_from_third_party_json(args, ActivitySourceKind::Ea)
}

fn build_batch_from_ea_api_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    build_batch_from_third_party_api(args, ActivitySourceKind::Ea)
}

fn build_batch_from_gog_json_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    build_batch_from_third_party_json(args, ActivitySourceKind::Gog)
}

fn build_batch_from_gog_api_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    build_batch_from_third_party_api(args, ActivitySourceKind::Gog)
}

fn build_batch_from_steam_json(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 8 && args.len() != 9 {
        return Err(
            "usage: pole-node build-batch-from-steam-json <config-path> <epoch-id> <slot-id> <appid> <observed-at-millis> <steam-json-path> [payload-out-path]"
                .into(),
        );
    }

    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    let epoch_id: u64 = args[3].parse()?;
    let slot_id: u64 = args[4].parse()?;
    let app_id: u32 = args[5].parse()?;
    let observed_at_millis: u64 = args[6].parse()?;
    let raw_body = fs::read_to_string(Path::new(&args[7]))?;

    let sample = parse_current_players_response(app_id, observed_at_millis, &raw_body)?;
    let assembled = build_single_sample_batch(&config, epoch_id, slot_id, sample)?;

    maybe_write_payload(&assembled, args.get(8).map(String::as_str))?;
    print_batch_summary(&config, &assembled);

    Ok(())
}

fn build_batch_from_steam_api(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 6 && args.len() != 7 {
        return Err(
            "usage: pole-node build-batch-from-steam-api <config-path> <epoch-id> <slot-id> <appid> [payload-out-path]"
                .into(),
        );
    }

    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    let epoch_id: u64 = args[3].parse()?;
    let slot_id: u64 = args[4].parse()?;
    let app_id: u32 = args[5].parse()?;

    let client = ReqwestHttpTextClient;
    let sample = fetch_current_players_live(&client, app_id)?;
    let assembled = build_single_sample_batch(&config, epoch_id, slot_id, sample)?;

    maybe_write_payload(&assembled, args.get(6).map(String::as_str))?;
    print_batch_summary(&config, &assembled);

    Ok(())
}

fn build_batch_from_third_party_json(
    args: &[String],
    source_kind: ActivitySourceKind,
) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 8 && args.len() != 9 {
        return Err(format!(
            "usage: pole-node build-batch-from-{}-json <config-path> <epoch-id> <slot-id> <appid> <observed-at-millis> <json-path> [payload-out-path]",
            source_kind_label(source_kind)
        )
        .into());
    }

    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    let epoch_id: u64 = args[3].parse()?;
    let slot_id: u64 = args[4].parse()?;
    let app_id: u32 = args[5].parse()?;
    let observed_at_millis: u64 = args[6].parse()?;
    let raw_body = fs::read_to_string(Path::new(&args[7]))?;

    let sample =
        parse_third_party_activity_response(app_id, observed_at_millis, &raw_body, source_kind)?;
    let assembled = build_single_sample_batch(&config, epoch_id, slot_id, sample)?;
    maybe_write_payload(&assembled, args.get(8).map(String::as_str))?;
    print_batch_summary(&config, &assembled);
    Ok(())
}

fn build_batch_from_third_party_api(
    args: &[String],
    source_kind: ActivitySourceKind,
) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 7 && args.len() != 8 {
        return Err(format!(
            "usage: pole-node build-batch-from-{}-api <config-path> <epoch-id> <slot-id> <appid> <endpoint-url> [payload-out-path]",
            source_kind_label(source_kind)
        )
        .into());
    }

    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    let epoch_id: u64 = args[3].parse()?;
    let slot_id: u64 = args[4].parse()?;
    let app_id: u32 = args[5].parse()?;
    let endpoint_url = &args[6];
    let observed_at_millis = current_unix_millis()?;
    let client = ReqwestHttpTextClient;
    let sample = collect_configured_activity_source(
        &client,
        source_kind,
        app_id,
        observed_at_millis,
        Some(endpoint_url),
        None,
    )?;
    let assembled = build_single_sample_batch(&config, epoch_id, slot_id, sample)?;
    maybe_write_payload(&assembled, args.get(7).map(String::as_str))?;
    print_batch_summary(&config, &assembled);
    Ok(())
}

fn build_batch_from_community_json(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 8 && args.len() != 9 {
        return Err(
            "usage: pole-node build-batch-from-community-json <config-path> <epoch-id> <slot-id> <appid> <observed-at-millis> <json-path> [payload-out-path]"
                .into(),
        );
    }

    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    let epoch_id: u64 = args[3].parse()?;
    let slot_id: u64 = args[4].parse()?;
    let app_id: u32 = args[5].parse()?;
    let observed_at_millis: u64 = args[6].parse()?;
    let raw_body = fs::read_to_string(Path::new(&args[7]))?;

    let sample = parse_community_activity_response(app_id, observed_at_millis, &raw_body)?;
    let assembled = build_single_sample_batch(&config, epoch_id, slot_id, sample)?;
    maybe_write_payload(&assembled, args.get(8).map(String::as_str))?;
    print_batch_summary(&config, &assembled);
    Ok(())
}

fn build_batch_from_community_inline_json(
    args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 8 && args.len() != 9 {
        return Err(
            "usage: pole-node build-batch-from-community-inline-json <config-path> <epoch-id> <slot-id> <appid> <observed-at-millis> <inline-json> [payload-out-path]"
                .into(),
        );
    }

    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    let epoch_id: u64 = args[3].parse()?;
    let slot_id: u64 = args[4].parse()?;
    let app_id: u32 = args[5].parse()?;
    let observed_at_millis: u64 = args[6].parse()?;
    let sample = parse_community_activity_response(app_id, observed_at_millis, &args[7])?;
    let assembled = build_single_sample_batch(&config, epoch_id, slot_id, sample)?;
    maybe_write_payload(&assembled, args.get(8).map(String::as_str))?;
    print_batch_summary(&config, &assembled);
    Ok(())
}

fn run_once(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 3 {
        return Err("usage: pole-node run-once <config-path>".into());
    }

    execute_run_once(&args[2], NetworkMode::Disabled)
}

fn run_once_p2p_sim(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() < 3 {
        return Err(
            "usage: pole-node run-once-p2p-sim <config-path> [--batch-listeners <count>] [--receipt-listeners <count>] [--dual-listeners <count>]".into(),
        );
    }

    let (_, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    let topology = parse_simulation_topology_args(
        args,
        3,
        "usage: pole-node run-once-p2p-sim <config-path> [--batch-listeners <count>] [--receipt-listeners <count>] [--dual-listeners <count>]",
        config.runtime.p2p_simulation,
    )?;
    execute_run_once(&args[2], NetworkMode::InMemorySimulation(topology))
}

fn run_loop(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 3 && args.len() != 4 {
        return Err("usage: pole-node run-loop <config-path> [ticks]".into());
    }

    let ticks = args.get(3).map(|value| value.parse()).transpose()?;
    execute_run_loop(&args[2], ticks, NetworkMode::Disabled)
}

fn run_loop_p2p_sim(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() < 3 {
        return Err(
            "usage: pole-node run-loop-p2p-sim <config-path> [ticks] [--batch-listeners <count>] [--receipt-listeners <count>] [--dual-listeners <count>]".into(),
        );
    }

    let (_, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    let (ticks, topology_arg_start) = if let Some(value) = args.get(3) {
        if value.starts_with("--") {
            (None, 3)
        } else {
            (Some(value.parse()?), 4)
        }
    } else {
        (None, 3)
    };
    let topology = parse_simulation_topology_args(
        args,
        topology_arg_start,
        "usage: pole-node run-loop-p2p-sim <config-path> [ticks] [--batch-listeners <count>] [--receipt-listeners <count>] [--dual-listeners <count>]",
        config.runtime.p2p_simulation,
    )?;
    execute_run_loop(&args[2], ticks, NetworkMode::InMemorySimulation(topology))
}

fn run_once_p2p_fs(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 4 {
        return Err("usage: pole-node run-once-p2p-fs <config-path> <network-dir>".into());
    }

    execute_run_once(
        &args[2],
        NetworkMode::FilesystemBackend(PathBuf::from(&args[3])),
    )
}

fn run_loop_p2p_fs(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 4 && args.len() != 5 {
        return Err("usage: pole-node run-loop-p2p-fs <config-path> <network-dir> [ticks]".into());
    }

    let ticks = args.get(4).map(|value| value.parse()).transpose()?;
    execute_run_loop(
        &args[2],
        ticks,
        NetworkMode::FilesystemBackend(PathBuf::from(&args[3])),
    )
}

fn run_once_p2p_socket(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 3 && args.len() != 5 && args.len() != 7 {
        return Err("usage: pole-node run-once-p2p-socket <config-path> [<bind-addr> <peer-specs> | <bind-addr> <peer-id-hex> <peer-addr> <topics>]".into());
    }
    let mode = if args.len() == 3 {
        let (_, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
        socket_network_mode_from_config(&config)?
    } else if args.len() == 5 {
        parse_socket_network_mode_multi(&args[3], &args[4])?
    } else {
        parse_socket_network_mode(&args[3], &args[4], &args[5], &args[6])?
    };
    execute_run_once(&args[2], mode)
}

fn run_loop_p2p_socket(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 3
        && args.len() != 4
        && args.len() != 5
        && args.len() != 6
        && args.len() != 7
        && args.len() != 8
    {
        return Err("usage: pole-node run-loop-p2p-socket <config-path> [<bind-addr> <peer-specs> [ticks] | <bind-addr> <peer-id-hex> <peer-addr> <topics> [ticks]]".into());
    }
    let (mode, ticks) = if args.len() == 5 || args.len() == 6 {
        (
            parse_socket_network_mode_multi(&args[3], &args[4])?,
            args.get(5).map(|value| value.parse()).transpose()?,
        )
    } else if args.len() == 3 || args.len() == 4 {
        let (_, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
        (
            socket_network_mode_from_config(&config)?,
            args.get(3).map(|value| value.parse()).transpose()?,
        )
    } else {
        (
            parse_socket_network_mode(&args[3], &args[4], &args[5], &args[6])?,
            args.get(7).map(|value| value.parse()).transpose()?,
        )
    };
    execute_run_loop(&args[2], ticks, mode)
}

fn status(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 3 {
        return Err("usage: pole-node status <config-path>".into());
    }

    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    let summary = load_status(&config)?;
    let active_game_processes = detect_active_game_processes(&config);
    print_node_status_summary(&summary);
    println!("game_process_names={:?}", config.runtime.game_process_names);
    println!("active_game_processes={:?}", active_game_processes);
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
    println!(
        "effective_poll_interval_secs={}",
        effective_collect_interval_secs(&config, &active_game_processes)
    );
    print_node_storage_and_network_status(&summary);
    Ok(())
}

fn tokenomics_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() > 3 {
        return Err("usage: pole-node tokenomics [years]".into());
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

    println!("PoLE node tokenomics");
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

fn governance_propose_params_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 7 && args.len() != 9 {
        return Err("usage: pole-node governance-propose-params <config-path> <proposal-id-hex> <effective-epoch> <emission-year> <effective-player-block-reward> [tail-start-year tail-rate-bps]".into());
    }
    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    let proposal_id = decode_hex32(&args[3], "proposal_id")?;
    let effective_epoch: u64 = args[4].parse()?;
    let emission_year: u32 = args[5].parse()?;
    let effective_player_block_reward: u128 = args[6].parse()?;
    let tail_policy = if args.len() == 9 {
        Some((args[7].parse::<u32>()?, args[8].parse::<u16>()?))
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

fn governance_propose_reward_tuning_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 9 {
        return Err("usage: pole-node governance-propose-reward-tuning <config-path> <proposal-id-hex> <effective-epoch> <target_network_weight_units> <reward_adjustment_cap_bps> <challenge_window_blocks> <effective-player-block-reward>".into());
    }
    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    let proposal_id = decode_hex32(&args[3], "proposal_id")?;
    let effective_epoch: u64 = args[4].parse()?;
    let target_network_weight_units: u128 = args[5].parse()?;
    let reward_adjustment_cap_bps: u16 = args[6].parse()?;
    let challenge_window_blocks: u32 = args[7].parse()?;
    let effective_player_block_reward: u128 = args[8].parse()?;
    let mut params = open_local_protocol_state(&config, config.runtime.challenge_window_blocks)?
        .1
        .params
        .clone();
    params.rewards.target_network_weight_units = target_network_weight_units;
    params.rewards.reward_adjustment_cap_bps = reward_adjustment_cap_bps;
    params.rewards.effective_player_block_reward = effective_player_block_reward;
    params.challenge_window_blocks = challenge_window_blocks;
    let effects =
        submit_protocol_params_update_proposal(&config, proposal_id, effective_epoch, params)?;

    println!("proposal_id={}", pole_protocol_draft::hex_32(proposal_id));
    println!("effective_epoch={effective_epoch}");
    println!("target_network_weight_units={target_network_weight_units}");
    println!("reward_adjustment_cap_bps={reward_adjustment_cap_bps}");
    println!("challenge_window_blocks={challenge_window_blocks}");
    println!("effective_player_block_reward={effective_player_block_reward}");
    println!("effect_count={}", effects.len());
    Ok(())
}

fn governance_propose_slow_params_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 7 {
        return Err("usage: pole-node governance-propose-slow-params <config-path> <proposal-id-hex> <effective-epoch> <reward-block-secs> <effective-player-block-reward>".into());
    }
    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    let proposal_id = decode_hex32(&args[3], "proposal_id")?;
    let effective_epoch: u64 = args[4].parse()?;
    let reward_block_secs: u64 = args[5].parse()?;
    let effective_player_block_reward: u128 = args[6].parse()?;
    let mut params = open_local_protocol_state(&config, config.runtime.challenge_window_blocks)?
        .1
        .params
        .clone();
    params.rewards.reward_block_secs = reward_block_secs;
    params.rewards.effective_player_block_reward = effective_player_block_reward;
    let effects =
        submit_protocol_params_update_proposal(&config, proposal_id, effective_epoch, params)?;

    println!("proposal_id={}", pole_protocol_draft::hex_32(proposal_id));
    println!("effective_epoch={effective_epoch}");
    println!("reward_block_secs={reward_block_secs}");
    println!("effective_player_block_reward={effective_player_block_reward}");
    println!("effect_count={}", effects.len());
    Ok(())
}

fn governance_propose_retention_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 7 {
        return Err("usage: pole-node governance-propose-retention <config-path> <proposal-id-hex> <effective-epoch> <min-retention-epochs> <challenge-window-blocks>".into());
    }
    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    let proposal_id = decode_hex32(&args[3], "proposal_id")?;
    let effective_epoch: u64 = args[4].parse()?;
    let min_retention_epochs: u32 = args[5].parse()?;
    let challenge_window_blocks: u32 = args[6].parse()?;
    let mut params = open_local_protocol_state(&config, config.runtime.challenge_window_blocks)?
        .1
        .params
        .clone();
    params.min_retention_epochs = min_retention_epochs;
    params.challenge_window_blocks = challenge_window_blocks;
    let effects =
        submit_protocol_params_update_proposal(&config, proposal_id, effective_epoch, params)?;

    println!("proposal_id={}", pole_protocol_draft::hex_32(proposal_id));
    println!("effective_epoch={effective_epoch}");
    println!("min_retention_epochs={min_retention_epochs}");
    println!("challenge_window_blocks={challenge_window_blocks}");
    println!("effect_count={}", effects.len());
    Ok(())
}

fn governance_propose_app_weight_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 7 {
        return Err("usage: pole-node governance-propose-app-weight <config-path> <proposal-id-hex> <effective-epoch> <app-id> <game-coefficient-ppm>".into());
    }
    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    let proposal_id = decode_hex32(&args[3], "proposal_id")?;
    let effective_epoch: u64 = args[4].parse()?;
    let app_id: u32 = args[5].parse()?;
    let game_coefficient_ppm: u32 = args[6].parse()?;
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

    println!("proposal_id={}", pole_protocol_draft::hex_32(proposal_id));
    println!("effective_epoch={effective_epoch}");
    println!("app_id={app_id}");
    println!("game_coefficient_ppm={game_coefficient_ppm}");
    println!("effect_count={}", effects.len());
    Ok(())
}

fn governance_propose_tier_weights_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 10 {
        return Err("usage: pole-node governance-propose-tier-weights <config-path> <proposal-id-hex> <effective-epoch> <tier1_weight_ppm> <tier2_min_ppm> <tier2_max_ppm> <tier3_min_ppm> <tier3_max_ppm>".into());
    }
    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    let proposal_id = decode_hex32(&args[3], "proposal_id")?;
    let effective_epoch: u64 = args[4].parse()?;
    let tier1_weight_ppm: u32 = args[5].parse()?;
    let tier2_weight_min_ppm: u32 = args[6].parse()?;
    let tier2_weight_max_ppm: u32 = args[7].parse()?;
    let tier3_weight_min_ppm: u32 = args[8].parse()?;
    let tier3_weight_max_ppm: u32 = args[9].parse()?;
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

fn governance_vote_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 6 {
        return Err("usage: pole-node governance-vote <config-path> <proposal-id-hex> <yes|no|abstain> <voting-power>".into());
    }
    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    let proposal_id = decode_hex32(&args[3], "proposal_id")?;
    let choice = parse_vote_choice(&args[4])?;
    let voting_power: u128 = args[5].parse()?;

    let (effects, scheduled) =
        pole_protocol_draft::execute_governance_vote(&config, proposal_id, choice, voting_power)?;

    println!("proposal_id={}", pole_protocol_draft::hex_32(proposal_id));
    println!("choice={:?}", choice);
    println!("voting_power={voting_power}");
    println!("effect_count={}", effects.len());
    println!("scheduled_next_epoch={scheduled}");
    Ok(())
}

fn governance_show_proposal_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 4 {
        return Err(
            "usage: pole-node governance-show-proposal <config-path> <proposal-id-hex>".into(),
        );
    }
    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    let proposal_id = decode_hex32(&args[3], "proposal_id")?;
    let (_, state) = open_local_protocol_state(&config, config.runtime.challenge_window_blocks)?;
    let Some((artifact, artifact_path, index_path)) =
        export_governance_proposal_artifact(&config, &state.store, &proposal_id)?
    else {
        return Err("governance params update proposal not found".into());
    };

    print_governance_proposal_artifact(&artifact);
    println!("artifact_path={}", artifact_path.to_string_lossy());
    println!("artifact_index_path={}", index_path.to_string_lossy());
    Ok(())
}

fn governance_show_scheduled_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 3 && args.len() != 4 {
        return Err("usage: pole-node governance-show-scheduled <config-path> [epoch-id]".into());
    }
    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    let (_, state) = open_local_protocol_state(&config, config.runtime.challenge_window_blocks)?;
    let epoch_id = args
        .get(3)
        .map(|value| value.parse::<u64>())
        .transpose()?
        .unwrap_or(state.current_epoch.saturating_add(1));
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
    if args.len() != 3 {
        return Err("usage: pole-node governance-show-index <config-path>".into());
    }
    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    let index_path = governance_index_artifact_path(&config);
    let index = GovernanceArtifactIndex::load_or_default_json(&index_path)?;

    println!("artifact_index_path={}", index_path.to_string_lossy());
    print_governance_index(&index);
    Ok(())
}

fn governance_show_summary_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 3 {
        return Err("usage: pole-node governance-show-summary <config-path>".into());
    }
    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    let summary_path = governance_summary_artifact_path(&config);
    let summary = GovernanceArtifactSummary::load_or_default_json(&summary_path)?;

    println!("artifact_summary_path={}", summary_path.to_string_lossy());
    println!("artifact_index_path={}", summary.artifact_index_path);
    print_governance_summary(&summary);
    Ok(())
}

fn reward_adjustment_show_index_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 3 {
        return Err("usage: pole-node reward-adjustment-show-index <config-path>".into());
    }
    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    let index_path = pole_protocol_draft::reward_adjustment_index_path(&config);
    let index =
        pole_protocol_draft::RewardAdjustmentArtifactIndex::load_or_default_json(&index_path)?;

    println!("artifact_index_path={}", index_path.to_string_lossy());
    print_reward_adjustment_index(&index);
    Ok(())
}

fn reward_adjustment_show_summary_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 3 {
        return Err("usage: pole-node reward-adjustment-show-summary <config-path>".into());
    }
    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    let summary_path = pole_protocol_draft::reward_adjustment_summary_path(&config);
    let summary =
        pole_protocol_draft::RewardAdjustmentArtifactSummary::load_or_default_json(&summary_path)?;

    println!("artifact_summary_path={}", summary_path.to_string_lossy());
    println!("artifact_index_path={}", summary.artifact_index_path);
    print_reward_adjustment_summary(&summary);
    Ok(())
}

fn print_node_status_summary(summary: &pole_protocol_draft::NodeStatusSummary) {
    println!("next_epoch_id={}", summary.next_epoch_id);
    println!("next_slot_id={}", summary.next_slot_id);
    println!("ticks_completed={}", summary.ticks_completed);
    println!("reward_block_secs={}", summary.reward_block_secs);
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
        "reward_blocks_completed={}",
        summary.reward_blocks_completed
    );
    println!(
        "current_adjustment_cycle_index={}",
        summary.current_adjustment_cycle_index
    );
    println!(
        "adjustment_cycle_blocks={}",
        summary.adjustment_cycle_blocks
    );
    println!(
        "current_fixed_player_reward={}",
        summary.current_fixed_player_reward
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
        "reward_adjustment_period_blocks={}",
        summary.reward_adjustment_period_blocks
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
    println!("target_app_ids={:?}", summary.target_app_ids);
    println!("low_impact_mode={}", summary.low_impact_mode);
    println!("os_background_priority={}", summary.os_background_priority);
    println!("inline_verify_enabled={}", summary.inline_verify_enabled);
    println!("inline_propose_enabled={}", summary.inline_propose_enabled);
}

fn libp2p_diagnose_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 3 {
        return Err("usage: pole-node libp2p-diagnose <config-path>".into());
    }
    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    let libp2p = &config.runtime.p2p_libp2p;
    println!("libp2p_enabled={}", libp2p.enabled);
    println!("libp2p_listen_addrs={:?}", libp2p.listen_addrs);
    println!(
        "libp2p_bootstrap_peer_count={}",
        libp2p.bootstrap_peers.len()
    );
    println!("libp2p_kademlia={}", libp2p.discovery.kademlia);
    println!("libp2p_mdns={}", libp2p.discovery.mdns);
    println!("libp2p_rendezvous={}", libp2p.discovery.rendezvous);
    Ok(())
}

fn libp2p_skeleton_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 3 {
        return Err("usage: pole-node libp2p-skeleton <config-path>".into());
    }
    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    let skeleton = build_libp2p_backend_skeleton(&config.runtime.p2p_libp2p)?;
    let real_swarm = build_real_libp2p_swarm_report(&config.runtime.p2p_libp2p)?;
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

fn libp2p_loop_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 3 && args.len() != 4 {
        return Err("usage: pole-node libp2p-loop <config-path> [ticks]".into());
    }
    let ticks = args
        .get(3)
        .map(|value| value.parse::<u64>())
        .transpose()?
        .unwrap_or(5);
    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    let skeleton = build_libp2p_backend_skeleton(&config.runtime.p2p_libp2p)?;
    let report = run_libp2p_skeleton_loop(skeleton, ticks, Duration::ZERO);
    println!("ticks_completed={}", report.ticks_completed);
    println!("phase={:?}", report.phase);
    println!("known_peer_count={}", report.known_peer_count);
    println!("connected_peer_count={}", report.connected_peer_count);
    println!("announced_peer_count={}", report.announced_peer_count);
    Ok(())
}

fn service_run_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 3 {
        return Err("usage: pole-node service-run <config-path>".into());
    }
    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    println!("service_mode=true");
    println!("service_name={}", pole_protocol_draft::SYSTEMD_SERVICE_NAME);
    println!("config_path={}", args[2]);
    println!("data_dir={}", config.runtime.data_dir);
    Ok(())
}

fn service_install_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 3 {
        return Err("usage: pole-node service-install <config-path>".into());
    }
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    #[cfg(windows)]
    let _ = &config;
    let exe = std::env::current_exe()?;
    #[cfg(windows)]
    let manager = windows_service_manager(&exe, &config_path);
    #[cfg(not(windows))]
    let manager = linux_service_manager(&exe, &config_path, &config);
    manager.install()?;
    println!("service_name={}", manager.service_name());
    println!("service_install_supported=true");
    println!("config_path={}", config_path.to_string_lossy());
    #[cfg(not(windows))]
    println!(
        "service_unit_path={}",
        linux_service_unit_path(&exe, &config_path, &config).display()
    );
    #[cfg(windows)]
    println!(
        "service_registration_path={}",
        windows_service_registration_path(&exe, &config_path).display()
    );
    Ok(())
}

fn service_uninstall_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 3 {
        return Err("usage: pole-node service-uninstall <config-path>".into());
    }
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    let exe = std::env::current_exe()?;
    #[cfg(windows)]
    let _ = &config;
    #[cfg(windows)]
    let manager = windows_service_manager(&exe, &config_path);
    #[cfg(not(windows))]
    let manager = linux_service_manager(&exe, &config_path, &config);
    manager.uninstall()?;
    println!("service_uninstall_supported=true");
    println!("config_path={}", config_path.to_string_lossy());
    Ok(())
}

fn service_start_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 3 {
        return Err("usage: pole-node service-start <config-path>".into());
    }
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    #[cfg(windows)]
    let _ = &config;
    let exe = std::env::current_exe()?;
    #[cfg(windows)]
    let manager = windows_service_manager(&exe, &config_path);
    #[cfg(not(windows))]
    let manager = linux_service_manager(&exe, &config_path, &config);
    manager.start()?;
    println!("service_start_supported=true");
    println!("config_path={}", config_path.to_string_lossy());
    Ok(())
}

fn service_stop_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 3 {
        return Err("usage: pole-node service-stop <config-path>".into());
    }
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    #[cfg(windows)]
    let _ = &config;
    let exe = std::env::current_exe()?;
    #[cfg(windows)]
    let manager = windows_service_manager(&exe, &config_path);
    #[cfg(not(windows))]
    let manager = linux_service_manager(&exe, &config_path, &config);
    manager.stop()?;
    println!("service_stop_supported=true");
    println!("config_path={}", config_path.to_string_lossy());
    Ok(())
}

fn service_status_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 3 {
        return Err("usage: pole-node service-status <config-path>".into());
    }
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    let exe = std::env::current_exe()?;
    #[cfg(windows)]
    let manager = windows_service_manager(&exe, &config_path);
    #[cfg(not(windows))]
    let manager = linux_service_manager(&exe, &config_path, &config);
    let status = manager.status()?;
    println!("service_name={}", manager.service_name());
    println!("service_status={status:?}");
    println!("data_dir={}", config.runtime.data_dir);
    #[cfg(not(windows))]
    println!(
        "service_unit_path={}",
        linux_service_unit_path(&exe, &config_path, &config).display()
    );
    #[cfg(windows)]
    println!(
        "service_registration_path={}",
        windows_service_registration_path(&exe, &config_path).display()
    );
    Ok(())
}

#[cfg(windows)]
fn windows_service_manager(
    exe: &Path,
    config_path: &Path,
) -> pole_protocol_draft::WindowsServiceManager {
    pole_protocol_draft::WindowsServiceManager::new(windows_service_definition(exe, config_path))
}

#[cfg(windows)]
fn windows_service_definition(
    exe: &Path,
    config_path: &Path,
) -> pole_protocol_draft::WindowsServiceDefinition {
    let definition = pole_protocol_draft::WindowsServiceDefinition::new(exe, config_path);
    let definition = if let Ok(service_root) = env::var("POLE_WINDOWS_SERVICE_ROOT") {
        definition.with_service_root(service_root)
    } else {
        definition
    };
    if let Ok(sc_binary) = env::var("POLE_WINDOWS_SC_BINARY") {
        definition.with_sc_binary(sc_binary)
    } else {
        definition
    }
}

#[cfg(windows)]
fn windows_service_registration_path(exe: &Path, config_path: &Path) -> PathBuf {
    windows_service_definition(exe, config_path).registration_path()
}

#[cfg(not(windows))]
fn linux_service_manager(
    exe: &Path,
    config_path: &Path,
    config: &NodeConfig,
) -> pole_protocol_draft::SystemdServiceManager {
    pole_protocol_draft::SystemdServiceManager::new(linux_service_definition(
        exe,
        config_path,
        config,
    ))
}

#[cfg(not(windows))]
fn linux_service_definition(
    exe: &Path,
    config_path: &Path,
    config: &NodeConfig,
) -> pole_protocol_draft::SystemdUnitDefinition {
    let definition =
        pole_protocol_draft::SystemdUnitDefinition::new(exe, config_path, &config.runtime.data_dir);
    let definition = if let Ok(unit_root) = env::var("POLE_SYSTEMD_UNIT_ROOT") {
        definition.with_unit_root(unit_root)
    } else {
        definition
    };
    if let Ok(systemctl_binary) = env::var("POLE_SYSTEMCTL_BINARY") {
        definition.with_systemctl_binary(systemctl_binary)
    } else {
        definition
    }
}

#[cfg(not(windows))]
fn linux_service_unit_path(exe: &Path, config_path: &Path, config: &NodeConfig) -> PathBuf {
    linux_service_definition(exe, config_path, config).unit_path()
}

fn print_node_storage_and_network_status(summary: &pole_protocol_draft::NodeStatusSummary) {
    println!("stored_payload_count={}", summary.stored_payload_count);
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

fn run_once_with_client(
    config: &NodeConfig,
    progress: &mut LocalNodeProgress,
    client: &dyn HttpTextClient,
    network_mode: NetworkMode,
) -> Result<RunOnceExecution, Box<dyn std::error::Error>> {
    let (result, p2p_simulation) = match network_mode {
        NetworkMode::Disabled => (
            run_collect_tick_with_client(config, progress, client)?,
            None,
        ),
        NetworkMode::InMemorySimulation(topology) => {
            let mut network = build_inmemory_simulation_network(topology);
            let result =
                run_collect_tick_with_client_and_network(config, progress, client, &mut network)?;
            let summary = summarize_tick_p2p_simulation(&mut network, &result, topology)?;
            (result, Some(summary))
        }
        NetworkMode::FilesystemBackend(root) => {
            let mut network = FilesystemP2pNetwork::new(root);
            let result =
                run_collect_tick_with_client_and_network(config, progress, client, &mut network)?;
            let summary = summarize_tick_p2p_backend(&mut network, &result)?;
            (result, Some(summary))
        }
        NetworkMode::SocketBackend { bind_addr, peers } => {
            let mut network = SocketP2pNetwork::bind(config.node_id()?, bind_addr, peers)?;
            network.bootstrap_peer(config.node_id()?, &[P2pTopic::Batches, P2pTopic::Receipts])?;
            let result =
                run_collect_tick_with_client_and_network(config, progress, client, &mut network)?;
            let summary = summarize_tick_p2p_backend(&mut network, &result)?;
            (result, Some(summary))
        }
    };

    Ok(RunOnceExecution {
        result,
        p2p_simulation,
    })
}

fn execute_run_once(
    config_path: &str,
    network_mode: NetworkMode,
) -> Result<(), Box<dyn std::error::Error>> {
    let (_, config) = NodeConfig::load_json_with_runtime_paths(config_path)?;
    let mut progress =
        LocalNodeProgress::load_or_default(pole_protocol_draft::progress_path(&config), &config)?;
    let client = ReqwestHttpTextClient;
    let execution = run_once_with_client(&config, &mut progress, &client, network_mode)?;
    print_run_once_execution(&execution);
    Ok(())
}

fn run_loop_with_client(
    config: &NodeConfig,
    client: &dyn HttpTextClient,
    ticks: Option<u64>,
    network_mode: NetworkMode,
) -> Result<RunLoopExecution, Box<dyn std::error::Error>> {
    let (summary, p2p_simulation) = match network_mode {
        NetworkMode::Disabled => (
            summarize_collect_loop_with_client(config, client, ticks)?,
            None,
        ),
        NetworkMode::InMemorySimulation(topology) => {
            let mut network = build_inmemory_simulation_network(topology);
            let loop_summary = summarize_collect_loop_with_client_and_network(
                config,
                client,
                ticks,
                &mut network,
            )?;
            let p2p_summary = summarize_loop_p2p_simulation(
                &mut network,
                &loop_summary,
                loop_summary.last_result.as_ref(),
                topology,
            )?;
            (loop_summary, Some(p2p_summary))
        }
        NetworkMode::FilesystemBackend(root) => {
            let mut network = FilesystemP2pNetwork::new(root);
            let loop_summary = summarize_collect_loop_with_client_and_network(
                config,
                client,
                ticks,
                &mut network,
            )?;
            let p2p_summary = summarize_loop_p2p_backend(
                &mut network,
                &loop_summary,
                loop_summary.last_result.as_ref(),
            )?;
            (loop_summary, Some(p2p_summary))
        }
        NetworkMode::SocketBackend { bind_addr, peers } => {
            let mut network = SocketP2pNetwork::bind(config.node_id()?, bind_addr, peers)?;
            network.bootstrap_peer(config.node_id()?, &[P2pTopic::Batches, P2pTopic::Receipts])?;
            let loop_summary = summarize_collect_loop_with_client_and_network(
                config,
                client,
                ticks,
                &mut network,
            )?;
            let p2p_summary = summarize_loop_p2p_backend(
                &mut network,
                &loop_summary,
                loop_summary.last_result.as_ref(),
            )?;
            (loop_summary, Some(p2p_summary))
        }
    };

    Ok(RunLoopExecution {
        summary,
        p2p_simulation,
    })
}

fn execute_run_loop(
    config_path: &str,
    ticks: Option<u64>,
    network_mode: NetworkMode,
) -> Result<(), Box<dyn std::error::Error>> {
    let (_, config) = NodeConfig::load_json_with_runtime_paths(config_path)?;
    let client = ReqwestHttpTextClient;
    let execution = run_loop_with_client(&config, &client, ticks, network_mode)?;
    print_run_loop_execution(&execution);
    Ok(())
}

fn summarize_tick_p2p_simulation(
    network: &mut InMemoryP2pNetwork,
    result: &CollectTickResult,
    topology: P2pSimulationConfig,
) -> Result<P2pSimulationSummary, Box<dyn std::error::Error>> {
    let retrieval_peer = inmemory_simulation_retrieval_peer_id(topology);
    let payload_response = network.request_payload(retrieval_peer, &result.artifact.payload_cid);
    let payload_retrieval_ok = matches!(
        payload_response,
        Ok(ref response)
            if response.payload_cid == result.artifact.payload_cid
                && response.payload_bytes == result.outcome.assembled_batch.payload_bytes
    );
    let topology_summary = summarize_inmemory_simulation_topology(network, topology)?;

    Ok(P2pSimulationSummary {
        topology_peer_count: topology_summary.topology_peer_count,
        batch_listener_count: topology_summary.batch_listener_count,
        receipt_listener_count: topology_summary.receipt_listener_count,
        dual_listener_count: topology_summary.dual_listener_count,
        batch_recipients: result.outcome.batch_recipients,
        receipt_recipients: result.outcome.receipt_recipients,
        total_inbox_messages: topology_summary.total_inbox_messages,
        payload_retrieval_ok,
    })
}

fn summarize_loop_p2p_simulation(
    network: &mut InMemoryP2pNetwork,
    loop_summary: &CollectLoopSummary,
    last_result: Option<&CollectTickResult>,
    topology: P2pSimulationConfig,
) -> Result<P2pSimulationSummary, Box<dyn std::error::Error>> {
    let retrieval_peer = inmemory_simulation_retrieval_peer_id(topology);
    let last_result = last_result.ok_or("missing collect loop result")?;
    let payload_retrieval_ok = network
        .request_payload(retrieval_peer, &last_result.artifact.payload_cid)
        .map(|response| {
            response.payload_cid == last_result.artifact.payload_cid
                && response.payload_bytes == last_result.outcome.assembled_batch.payload_bytes
        })
        .unwrap_or(false);
    let topology_summary = summarize_inmemory_simulation_topology(network, topology)?;

    Ok(P2pSimulationSummary {
        topology_peer_count: topology_summary.topology_peer_count,
        batch_listener_count: topology_summary.batch_listener_count,
        receipt_listener_count: topology_summary.receipt_listener_count,
        dual_listener_count: topology_summary.dual_listener_count,
        batch_recipients: loop_summary.total_batch_recipients,
        receipt_recipients: loop_summary.total_receipt_recipients,
        total_inbox_messages: topology_summary.total_inbox_messages,
        payload_retrieval_ok,
    })
}

struct InMemorySimulationTopologySummary {
    topology_peer_count: usize,
    batch_listener_count: usize,
    receipt_listener_count: usize,
    dual_listener_count: usize,
    total_inbox_messages: usize,
}

fn summarize_inmemory_simulation_topology(
    network: &mut InMemoryP2pNetwork,
    topology: P2pSimulationConfig,
) -> Result<InMemorySimulationTopologySummary, Box<dyn std::error::Error>> {
    let mut batch_listener_count = 0;
    let mut receipt_listener_count = 0;
    let mut dual_listener_count = 0;
    let mut total_inbox_messages = 0;

    for peer_id in inmemory_simulation_listener_peer_ids(topology) {
        let subscriptions = network.subscriptions_for(peer_id)?;
        let listens_for_batches = subscriptions.contains(&P2pTopic::Batches);
        let listens_for_receipts = subscriptions.contains(&P2pTopic::Receipts);

        if listens_for_batches {
            batch_listener_count += 1;
        }
        if listens_for_receipts {
            receipt_listener_count += 1;
        }
        if listens_for_batches && listens_for_receipts {
            dual_listener_count += 1;
        }

        total_inbox_messages += network.drain_inbox(peer_id)?.len();
    }

    Ok(InMemorySimulationTopologySummary {
        topology_peer_count: network.known_peers().len(),
        batch_listener_count,
        receipt_listener_count,
        dual_listener_count,
        total_inbox_messages,
    })
}

fn summarize_tick_p2p_backend(
    network: &mut impl P2pNetwork,
    result: &CollectTickResult,
) -> Result<P2pSimulationSummary, Box<dyn std::error::Error>> {
    let topology_summary = summarize_generic_network_topology(network)?;
    let retrieval_peer = topology_summary
        .peer_ids
        .iter()
        .copied()
        .find(|peer_id| peer_id != &result.outcome.assembled_batch.batch_commit.collector_id)
        .ok_or("filesystem backend requires at least one listener peer")?;
    let payload_response = network.request_payload(retrieval_peer, &result.artifact.payload_cid);
    let payload_retrieval_ok = matches!(
        payload_response,
        Ok(ref response)
            if response.payload_cid == result.artifact.payload_cid
                && response.payload_bytes == result.outcome.assembled_batch.payload_bytes
    );

    Ok(P2pSimulationSummary {
        topology_peer_count: topology_summary.topology_peer_count,
        batch_listener_count: topology_summary.batch_listener_count,
        receipt_listener_count: topology_summary.receipt_listener_count,
        dual_listener_count: topology_summary.dual_listener_count,
        batch_recipients: result.outcome.batch_recipients,
        receipt_recipients: result.outcome.receipt_recipients,
        total_inbox_messages: topology_summary.total_inbox_messages,
        payload_retrieval_ok,
    })
}

fn summarize_loop_p2p_backend(
    network: &mut impl P2pNetwork,
    loop_summary: &CollectLoopSummary,
    last_result: Option<&CollectTickResult>,
) -> Result<P2pSimulationSummary, Box<dyn std::error::Error>> {
    let last_result = last_result.ok_or("missing collect loop result")?;
    let topology_summary = summarize_generic_network_topology(network)?;
    let retrieval_peer = topology_summary
        .peer_ids
        .iter()
        .copied()
        .find(|peer_id| {
            *peer_id
                != last_result
                    .outcome
                    .assembled_batch
                    .batch_commit
                    .collector_id
        })
        .ok_or("filesystem backend requires at least one listener peer")?;
    let payload_retrieval_ok = network
        .request_payload(retrieval_peer, &last_result.artifact.payload_cid)
        .map(|response| {
            response.payload_cid == last_result.artifact.payload_cid
                && response.payload_bytes == last_result.outcome.assembled_batch.payload_bytes
        })
        .unwrap_or(false);

    Ok(P2pSimulationSummary {
        topology_peer_count: topology_summary.topology_peer_count,
        batch_listener_count: topology_summary.batch_listener_count,
        receipt_listener_count: topology_summary.receipt_listener_count,
        dual_listener_count: topology_summary.dual_listener_count,
        batch_recipients: loop_summary.total_batch_recipients,
        receipt_recipients: loop_summary.total_receipt_recipients,
        total_inbox_messages: topology_summary.total_inbox_messages,
        payload_retrieval_ok,
    })
}

struct GenericNetworkTopologySummary {
    peer_ids: Vec<[u8; 32]>,
    topology_peer_count: usize,
    batch_listener_count: usize,
    receipt_listener_count: usize,
    dual_listener_count: usize,
    total_inbox_messages: usize,
}

fn summarize_generic_network_topology(
    network: &mut impl P2pNetwork,
) -> Result<GenericNetworkTopologySummary, Box<dyn std::error::Error>> {
    let peer_ids = network.known_peers()?;
    let mut batch_listener_count = 0;
    let mut receipt_listener_count = 0;
    let mut dual_listener_count = 0;
    let mut total_inbox_messages = 0;

    for peer_id in &peer_ids {
        let subscriptions = network.subscriptions_for(*peer_id)?;
        let listens_for_batches = subscriptions.contains(&P2pTopic::Batches);
        let listens_for_receipts = subscriptions.contains(&P2pTopic::Receipts);

        if listens_for_batches {
            batch_listener_count += 1;
        }
        if listens_for_receipts {
            receipt_listener_count += 1;
        }
        if listens_for_batches && listens_for_receipts {
            dual_listener_count += 1;
        }

        total_inbox_messages += network.drain_inbox(*peer_id)?.len();
    }

    Ok(GenericNetworkTopologySummary {
        peer_ids,
        topology_peer_count: network.known_peers()?.len(),
        batch_listener_count,
        receipt_listener_count,
        dual_listener_count,
        total_inbox_messages,
    })
}

fn parse_socket_network_mode(
    bind_addr: &str,
    peer_id_hex: &str,
    peer_addr: &str,
    topics: &str,
) -> Result<NetworkMode, Box<dyn std::error::Error>> {
    let bind_addr = parse_socket_addr(bind_addr, "bind-addr")?;
    let peer_addr = parse_socket_addr(peer_addr, "peer-addr")?;
    let peer_id = decode_hex32(peer_id_hex, "peer-id-hex")?;
    let topics = parse_socket_topics(topics)?;
    Ok(NetworkMode::SocketBackend {
        bind_addr,
        peers: vec![SocketPeerProfile::new(peer_id, peer_addr, topics)],
    })
}

fn parse_socket_network_mode_multi(
    bind_addr: &str,
    peer_specs: &str,
) -> Result<NetworkMode, Box<dyn std::error::Error>> {
    let bind_addr = parse_socket_addr(bind_addr, "bind-addr")?;
    let peers = parse_socket_peer_specs(peer_specs)?;
    Ok(NetworkMode::SocketBackend { bind_addr, peers })
}

fn socket_network_mode_from_config(
    config: &NodeConfig,
) -> Result<NetworkMode, Box<dyn std::error::Error>> {
    let bind_addr = parse_socket_addr(
        &config.runtime.p2p_socket.bind_addr,
        "runtime.p2p_socket.bind_addr",
    )?;
    let peers = if config.runtime.p2p_socket.peers.is_empty() {
        Vec::new()
    } else {
        socket_peers_from_config(&config.runtime.p2p_socket.peers)?
    };
    Ok(NetworkMode::SocketBackend { bind_addr, peers })
}

fn print_run_once_execution(execution: &RunOnceExecution) {
    let result = &execution.result;
    println!("tick_epoch={}", result.artifact.epoch_id);
    println!("tick_slot={}", result.artifact.slot_id);
    println!("payload_cid={}", result.artifact.payload_cid);
    println!("obs_count={}", result.artifact.obs_count);
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
            "epoch_commit_deadline={}",
            artifact.challenge_deadline_height
        );
    }
    print_p2p_simulation_summary(execution.p2p_simulation.as_ref());
}

fn print_run_loop_execution(execution: &RunLoopExecution) {
    println!("ticks_completed={}", execution.summary.ticks_completed);
    if let Some(last) = execution.summary.last_result.as_ref() {
        println!("last_epoch_id={}", last.artifact.epoch_id);
        println!("last_slot_id={}", last.artifact.slot_id);
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
    print_p2p_simulation_summary(execution.p2p_simulation.as_ref());
}

fn print_p2p_simulation_summary(summary: Option<&P2pSimulationSummary>) {
    println!("p2p_simulation_enabled={}", summary.is_some());
    if let Some(summary) = summary {
        println!("p2p_topology_peers={}", summary.topology_peer_count);
        println!("p2p_batch_listeners={}", summary.batch_listener_count);
        println!("p2p_receipt_listeners={}", summary.receipt_listener_count);
        println!("p2p_dual_listeners={}", summary.dual_listener_count);
        println!("p2p_batch_recipients={}", summary.batch_recipients);
        println!("p2p_receipt_recipients={}", summary.receipt_recipients);
        println!("p2p_total_inbox_messages={}", summary.total_inbox_messages);
        println!("p2p_payload_retrieval_ok={}", summary.payload_retrieval_ok);
    }
}

fn prune_retention_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 4 {
        return Err("usage: pole-node prune-retention <config-path> <current-epoch>".into());
    }

    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    let current_epoch: u64 = args[3].parse()?;
    let outcome = prune_retention(&config, current_epoch)?;
    println!("current_epoch={}", outcome.current_epoch);
    println!("removed_payloads={}", outcome.removed_payloads.len());
    for payload in outcome.removed_payloads {
        println!("removed={payload}");
    }
    Ok(())
}

fn build_epoch_commit_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 6 {
        return Err(
            "usage: pole-node build-epoch-commit <config-path> <epoch-id> <current-height> <challenge-window-blocks>"
                .into(),
        );
    }

    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    let epoch_id: u64 = args[3].parse()?;
    let current_height: u64 = args[4].parse()?;
    let challenge_window_blocks: u32 = args[5].parse()?;

    let (epoch_commit, artifact) = build_epoch_commit_from_local_data(
        &config,
        epoch_id,
        current_height,
        challenge_window_blocks,
        [0u8; 32],
        [0u8; 32],
    )?;
    println!("epoch_id={}", epoch_commit.epoch_id);
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
    if args.len() != 6 {
        return Err(
            "usage: pole-node prepare-epoch <config-path> <epoch-id> <current-height> <challenge-window-blocks>"
                .into(),
        );
    }

    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    let epoch_id: u64 = args[3].parse()?;
    let current_height: u64 = args[4].parse()?;
    let challenge_window_blocks: u32 = args[5].parse()?;

    let artifact = prepare_local_epoch(&config, epoch_id, current_height, challenge_window_blocks)?;
    println!("epoch_id={}", artifact.epoch_id);
    println!("batch_count={}", artifact.batch_count);
    println!("payload_count={}", artifact.payload_count);
    println!(
        "verification_batch_count={}",
        artifact.verification_batch_count
    );
    println!("stored_payload_count={}", artifact.stored_payload_count);
    println!("aggregate_count={}", artifact.aggregate_count);
    println!("reward_count={}", artifact.reward_count);
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

fn aggregate_epoch_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 4 {
        return Err("usage: pole-node aggregate-epoch <config-path> <epoch-id>".into());
    }

    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    let epoch_id: u64 = args[3].parse()?;
    let artifact = aggregate_local_epoch(&config, epoch_id)?;

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

fn reward_epoch_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 4 {
        return Err("usage: pole-node reward-epoch <config-path> <epoch-id>".into());
    }

    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    let epoch_id: u64 = args[3].parse()?;
    let artifact = reward_local_epoch(&config, epoch_id)?;

    println!("epoch_id={}", artifact.epoch_id);
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
            "node_id={} collect_score={} storage_score={} collect_reward={} store_reward={} verify_reward={} propose_reward={} slash_debit={} net_reward={}",
            entry.node_id_hex,
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

fn verify_epoch_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 4 {
        return Err("usage: pole-node verify-epoch <config-path> <epoch-id>".into());
    }

    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    let epoch_id: u64 = args[3].parse()?;
    let report = verify_local_epoch(&config, epoch_id)?;

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

fn issue_replica_receipt(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 6 {
        return Err(
            "usage: pole-node issue-replica-receipt <config-path> <ledger-path> <epoch-id> <payload-file-path>"
                .into(),
        );
    }

    let (_config_path, config) = NodeConfig::load_json_with_runtime_paths(&args[2])?;
    let ledger_path = &args[3];
    let epoch_id: u64 = args[4].parse()?;
    let payload_bytes = fs::read(&args[5])?;

    let storer_id = config.node_id()?;
    let mut ledger =
        LocalRetentionBook::load_or_default_json(ledger_path, config.storage.quota_gb)?;
    let record = ledger.record_batch_payload(
        storer_id,
        epoch_id,
        config.storage.retention_epochs,
        &payload_bytes,
    )?;
    ledger.save_json(ledger_path)?;

    println!("storer_id={}", config.node_id_hex);
    println!("payload_cid={}", record.payload_cid);
    println!(
        "payload_hash={}",
        pole_protocol_draft::hex_32(record.payload_hash)
    );
    println!("size_bytes={}", record.size_bytes);
    println!("retention_until_epoch={}", record.retention_until_epoch);
    println!(
        "receipt_signature={}",
        pole_protocol_draft::hex_32(
            record
                .receipt
                .receipt_signature
                .as_slice()
                .try_into()
                .unwrap_or([0u8; 32])
        )
    );

    Ok(())
}

fn build_single_sample_batch(
    config: &NodeConfig,
    epoch_id: u64,
    slot_id: u64,
    sample: SteamCurrentPlayersSample,
) -> Result<pole_protocol_draft::AssembledBatch, Box<dyn std::error::Error>> {
    let collector_id = config.node_id()?;
    let signature = development_signature_placeholder(
        epoch_id,
        slot_id,
        sample.app_id,
        sample.observed_players,
    );
    let observation = sample.into_observation(epoch_id, slot_id, collector_id, signature)?;

    let mut builder = BatchBuilder::new(epoch_id, collector_id);
    builder.push(observation)?;
    Ok(builder.finalize(0)?)
}

fn development_signature_placeholder(
    epoch_id: u64,
    slot_id: u64,
    app_id: u32,
    observed_players: u64,
) -> Vec<u8> {
    let seed = format!("dev-sig:{epoch_id}:{slot_id}:{app_id}:{observed_players}");
    pole_protocol_draft::stable_hash32(seed.as_bytes()).to_vec()
}

fn print_usage() {
    print!(
        "{}",
        format_usage_block("pole-node commands:", NODE_USAGE_COMMANDS)
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use pole_protocol_draft::SteamCollectorError;

    struct FixedHttpClient;

    impl HttpTextClient for FixedHttpClient {
        fn get_text(&self, url: &str) -> Result<String, SteamCollectorError> {
            let app_id = url.split("appid=").nth(1).unwrap_or("0");
            let player_count = match app_id {
                "730" => 500_000,
                "570" => 300_000,
                _ => 1_000,
            };
            Ok(format!(
                "{{\"response\":{{\"player_count\":{player_count},\"result\":1}}}}"
            ))
        }
    }

    fn temp_data_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("pole-node-bin-{name}-{}", std::process::id()))
    }

    fn test_config(name: &str) -> NodeConfig {
        NodeConfig {
            chain_id: "pole-local".into(),
            node_id_hex: pole_protocol_draft::hex_32([0x31; 32]),
            reward_address_hex: pole_protocol_draft::hex_32([0x41; 32]),
            capabilities: pole_protocol_draft::CapabilityConfig {
                collect: true,
                store: true,
                verify: true,
                propose: true,
                archive: false,
            },
            collect: pole_protocol_draft::CollectConfig {
                enabled: true,
                default_epoch_id: 1,
                default_slot_id: 1,
            },
            runtime: pole_protocol_draft::RuntimeConfig {
                data_dir: temp_data_dir(name).to_string_lossy().into_owned(),
                poll_interval_secs: 0,
                slots_per_epoch: 2,
                challenge_window_blocks: 20,
                low_impact_mode: false,
                os_background_priority: false,
                game_active_poll_interval_secs: 0,
                game_process_names: Vec::new(),
                target_app_ids: vec![730, 570],
                p2p_simulation: pole_protocol_draft::P2pSimulationConfig::default(),
                p2p_socket: pole_protocol_draft::P2pSocketConfig::default(),
                p2p_libp2p: pole_protocol_draft::P2pLibp2pConfig::default(),
                activity_sources: Vec::new(),
            },
            storage: pole_protocol_draft::StorageConfig {
                quota_gb: 1,
                retention_epochs: 2,
            },
            reward: pole_protocol_draft::RewardConfig::default(),
        }
    }

    #[test]
    fn run_once_simulates_p2p_propagation() {
        let config = test_config("run-once-p2p");
        let data_dir = Path::new(&config.runtime.data_dir);
        if data_dir.exists() {
            std::fs::remove_dir_all(data_dir).unwrap();
        }

        let client = FixedHttpClient;
        let mut progress = LocalNodeProgress::load_or_default(
            pole_protocol_draft::progress_path(&config),
            &config,
        )
        .unwrap();
        let execution = run_once_with_client(
            &config,
            &mut progress,
            &client,
            NetworkMode::InMemorySimulation(pole_protocol_draft::P2pSimulationConfig::default()),
        )
        .unwrap();

        let simulation = execution.p2p_simulation.unwrap();
        assert_eq!(simulation.topology_peer_count, 4);
        assert_eq!(simulation.batch_listener_count, 2);
        assert_eq!(simulation.receipt_listener_count, 2);
        assert_eq!(simulation.dual_listener_count, 1);
        assert_eq!(simulation.batch_recipients, 2);
        assert_eq!(simulation.receipt_recipients, 2);
        assert_eq!(simulation.total_inbox_messages, 4);
        assert!(simulation.payload_retrieval_ok);

        std::fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn parse_simulation_topology_args_supports_defaults_and_flags() {
        let default_args = vec!["pole-node".to_string(), "run-once-p2p-sim".to_string()];
        let default_topology = parse_simulation_topology_args(
            &default_args,
            2,
            "usage",
            pole_protocol_draft::P2pSimulationConfig {
                batch_listener_count: 2,
                receipt_listener_count: 1,
                dual_listener_count: 3,
            },
        )
        .unwrap();
        assert_eq!(default_topology.batch_listener_count, 2);
        assert_eq!(default_topology.receipt_listener_count, 1);
        assert_eq!(default_topology.dual_listener_count, 3);

        let fallback_topology = parse_simulation_topology_args(
            &default_args,
            2,
            "usage",
            pole_protocol_draft::P2pSimulationConfig::default(),
        )
        .unwrap();
        assert_eq!(fallback_topology.batch_listener_count, 1);
        assert_eq!(fallback_topology.receipt_listener_count, 1);
        assert_eq!(fallback_topology.dual_listener_count, 1);

        let custom_args = vec![
            "pole-node".to_string(),
            "run-loop-p2p-sim".to_string(),
            "2".to_string(),
            "--batch-listeners".to_string(),
            "2".to_string(),
            "--receipt-listeners".to_string(),
            "1".to_string(),
            "--dual-listeners".to_string(),
            "2".to_string(),
        ];
        let custom_topology = parse_simulation_topology_args(
            &custom_args,
            3,
            "usage",
            pole_protocol_draft::P2pSimulationConfig::default(),
        )
        .unwrap();
        assert_eq!(custom_topology.batch_listener_count, 2);
        assert_eq!(custom_topology.receipt_listener_count, 1);
        assert_eq!(custom_topology.dual_listener_count, 2);
    }

    #[test]
    fn run_once_simulates_custom_p2p_topology() {
        let config = test_config("run-once-custom-p2p");
        let data_dir = Path::new(&config.runtime.data_dir);
        if data_dir.exists() {
            std::fs::remove_dir_all(data_dir).unwrap();
        }

        let client = FixedHttpClient;
        let mut progress = LocalNodeProgress::load_or_default(
            pole_protocol_draft::progress_path(&config),
            &config,
        )
        .unwrap();
        let execution = run_once_with_client(
            &config,
            &mut progress,
            &client,
            NetworkMode::InMemorySimulation(pole_protocol_draft::P2pSimulationConfig {
                batch_listener_count: 2,
                receipt_listener_count: 1,
                dual_listener_count: 2,
            }),
        )
        .unwrap();

        let simulation = execution.p2p_simulation.unwrap();
        assert_eq!(simulation.topology_peer_count, 6);
        assert_eq!(simulation.batch_listener_count, 4);
        assert_eq!(simulation.receipt_listener_count, 3);
        assert_eq!(simulation.dual_listener_count, 2);
        assert_eq!(simulation.batch_recipients, 4);
        assert_eq!(simulation.receipt_recipients, 3);
        assert_eq!(simulation.total_inbox_messages, 7);
        assert!(simulation.payload_retrieval_ok);

        std::fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn run_once_uses_config_backed_simulation_topology_defaults() {
        let mut config = test_config("run-once-config-topology");
        config.runtime.p2p_simulation = pole_protocol_draft::P2pSimulationConfig {
            batch_listener_count: 2,
            receipt_listener_count: 1,
            dual_listener_count: 2,
        };
        let data_dir = Path::new(&config.runtime.data_dir);
        if data_dir.exists() {
            std::fs::remove_dir_all(data_dir).unwrap();
        }

        let client = FixedHttpClient;
        let mut progress = LocalNodeProgress::load_or_default(
            pole_protocol_draft::progress_path(&config),
            &config,
        )
        .unwrap();
        let execution = run_once_with_client(
            &config,
            &mut progress,
            &client,
            NetworkMode::InMemorySimulation(config.runtime.p2p_simulation),
        )
        .unwrap();

        let simulation = execution.p2p_simulation.unwrap();
        assert_eq!(simulation.topology_peer_count, 6);
        assert_eq!(simulation.batch_listener_count, 4);
        assert_eq!(simulation.receipt_listener_count, 3);
        assert_eq!(simulation.dual_listener_count, 2);

        std::fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn run_loop_simulates_p2p_propagation_across_ticks() {
        let config = test_config("run-loop-p2p");
        let data_dir = Path::new(&config.runtime.data_dir);
        if data_dir.exists() {
            std::fs::remove_dir_all(data_dir).unwrap();
        }

        let client = FixedHttpClient;
        let execution = run_loop_with_client(
            &config,
            &client,
            Some(2),
            NetworkMode::InMemorySimulation(pole_protocol_draft::P2pSimulationConfig::default()),
        )
        .unwrap();

        let simulation = execution.p2p_simulation.unwrap();
        assert_eq!(execution.summary.ticks_completed, 2);
        assert_eq!(simulation.topology_peer_count, 4);
        assert_eq!(simulation.batch_listener_count, 2);
        assert_eq!(simulation.receipt_listener_count, 2);
        assert_eq!(simulation.dual_listener_count, 1);
        assert_eq!(simulation.batch_recipients, 4);
        assert_eq!(simulation.receipt_recipients, 4);
        assert_eq!(simulation.total_inbox_messages, 8);
        assert!(simulation.payload_retrieval_ok);

        std::fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn run_once_uses_filesystem_p2p_backend() {
        let config = test_config("run-once-fs-p2p");
        let data_dir = Path::new(&config.runtime.data_dir);
        let network_dir = temp_data_dir("run-once-fs-network");
        if data_dir.exists() {
            std::fs::remove_dir_all(data_dir).unwrap();
        }
        if network_dir.exists() {
            std::fs::remove_dir_all(&network_dir).unwrap();
        }

        let client = FixedHttpClient;
        let mut progress = LocalNodeProgress::load_or_default(
            pole_protocol_draft::progress_path(&config),
            &config,
        )
        .unwrap();
        let mut seed_network = FilesystemP2pNetwork::new(&network_dir);
        seed_network
            .bootstrap_peer([0x91; 32], &[P2pTopic::Batches, P2pTopic::Receipts])
            .unwrap();
        let execution = run_once_with_client(
            &config,
            &mut progress,
            &client,
            NetworkMode::FilesystemBackend(network_dir.clone()),
        )
        .unwrap();

        let simulation = execution.p2p_simulation.unwrap();
        assert_eq!(simulation.topology_peer_count, 2);
        assert_eq!(simulation.batch_listener_count, 1);
        assert_eq!(simulation.receipt_listener_count, 1);
        assert_eq!(simulation.dual_listener_count, 1);
        assert_eq!(simulation.batch_recipients, 1);
        assert_eq!(simulation.receipt_recipients, 1);
        assert_eq!(simulation.total_inbox_messages, 2);
        assert!(simulation.payload_retrieval_ok);

        std::fs::remove_dir_all(data_dir).unwrap();
        std::fs::remove_dir_all(network_dir).unwrap();
    }

    #[test]
    fn run_loop_uses_filesystem_p2p_backend() {
        let config = test_config("run-loop-fs-p2p");
        let data_dir = Path::new(&config.runtime.data_dir);
        let network_dir = temp_data_dir("run-loop-fs-network");
        if data_dir.exists() {
            std::fs::remove_dir_all(data_dir).unwrap();
        }
        if network_dir.exists() {
            std::fs::remove_dir_all(&network_dir).unwrap();
        }

        let client = FixedHttpClient;
        let mut seed_network = FilesystemP2pNetwork::new(&network_dir);
        seed_network
            .bootstrap_peer([0x92; 32], &[P2pTopic::Batches, P2pTopic::Receipts])
            .unwrap();
        let execution = run_loop_with_client(
            &config,
            &client,
            Some(2),
            NetworkMode::FilesystemBackend(network_dir.clone()),
        )
        .unwrap();

        let simulation = execution.p2p_simulation.unwrap();
        assert_eq!(execution.summary.ticks_completed, 2);
        assert_eq!(simulation.topology_peer_count, 2);
        assert_eq!(simulation.batch_listener_count, 1);
        assert_eq!(simulation.receipt_listener_count, 1);
        assert_eq!(simulation.dual_listener_count, 1);
        assert_eq!(simulation.batch_recipients, 2);
        assert_eq!(simulation.receipt_recipients, 2);
        assert_eq!(simulation.total_inbox_messages, 4);
        assert!(simulation.payload_retrieval_ok);

        std::fs::remove_dir_all(data_dir).unwrap();
        std::fs::remove_dir_all(network_dir).unwrap();
    }

    #[test]
    fn run_once_uses_socket_p2p_backend() {
        let config = test_config("run-once-socket-p2p");
        let data_dir = Path::new(&config.runtime.data_dir);
        if data_dir.exists() {
            std::fs::remove_dir_all(data_dir).unwrap();
        }

        let client = FixedHttpClient;
        let sink = [0x68; 32];
        let mut sink_network =
            SocketP2pNetwork::bind(sink, "127.0.0.1:0".parse().unwrap(), vec![]).unwrap();
        sink_network
            .bootstrap_peer(sink, &[P2pTopic::Batches, P2pTopic::Receipts])
            .unwrap();
        let sink_addr = sink_network.local_addr().unwrap();

        let mut progress = LocalNodeProgress::load_or_default(
            pole_protocol_draft::progress_path(&config),
            &config,
        )
        .unwrap();
        let execution = run_once_with_client(
            &config,
            &mut progress,
            &client,
            NetworkMode::SocketBackend {
                bind_addr: "127.0.0.1:0".parse().unwrap(),
                peers: vec![SocketPeerProfile::new(
                    sink,
                    sink_addr,
                    [P2pTopic::Batches, P2pTopic::Receipts],
                )],
            },
        )
        .unwrap();

        let simulation = execution.p2p_simulation.unwrap();
        assert_eq!(simulation.topology_peer_count, 2);
        assert_eq!(simulation.batch_recipients, 1);
        assert_eq!(simulation.receipt_recipients, 1);
        assert!(simulation.payload_retrieval_ok);

        std::fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn run_loop_uses_socket_p2p_backend() {
        let config = test_config("run-loop-socket-p2p");
        let data_dir = Path::new(&config.runtime.data_dir);
        if data_dir.exists() {
            std::fs::remove_dir_all(data_dir).unwrap();
        }

        let client = FixedHttpClient;
        let sink = [0x69; 32];
        let mut sink_network =
            SocketP2pNetwork::bind(sink, "127.0.0.1:0".parse().unwrap(), vec![]).unwrap();
        sink_network
            .bootstrap_peer(sink, &[P2pTopic::Batches, P2pTopic::Receipts])
            .unwrap();
        let sink_addr = sink_network.local_addr().unwrap();

        let execution = run_loop_with_client(
            &config,
            &client,
            Some(2),
            NetworkMode::SocketBackend {
                bind_addr: "127.0.0.1:0".parse().unwrap(),
                peers: vec![SocketPeerProfile::new(
                    sink,
                    sink_addr,
                    [P2pTopic::Batches, P2pTopic::Receipts],
                )],
            },
        )
        .unwrap();

        let simulation = execution.p2p_simulation.unwrap();
        assert_eq!(execution.summary.ticks_completed, 2);
        assert_eq!(simulation.topology_peer_count, 2);
        assert_eq!(simulation.batch_recipients, 2);
        assert_eq!(simulation.receipt_recipients, 2);
        assert!(simulation.payload_retrieval_ok);

        std::fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn run_once_uses_multi_peer_socket_p2p_backend() {
        let config = test_config("run-once-multi-socket-p2p");
        let data_dir = Path::new(&config.runtime.data_dir);
        if data_dir.exists() {
            std::fs::remove_dir_all(data_dir).unwrap();
        }

        let client = FixedHttpClient;
        let sink_a = [0x6a; 32];
        let sink_b = [0x6b; 32];
        let mut sink_network_a =
            SocketP2pNetwork::bind(sink_a, "127.0.0.1:0".parse().unwrap(), vec![]).unwrap();
        sink_network_a
            .bootstrap_peer(sink_a, &[P2pTopic::Batches, P2pTopic::Receipts])
            .unwrap();
        let sink_addr_a = sink_network_a.local_addr().unwrap();
        let mut sink_network_b =
            SocketP2pNetwork::bind(sink_b, "127.0.0.1:0".parse().unwrap(), vec![]).unwrap();
        sink_network_b
            .bootstrap_peer(sink_b, &[P2pTopic::Batches, P2pTopic::Receipts])
            .unwrap();
        let sink_addr_b = sink_network_b.local_addr().unwrap();

        let mut progress = LocalNodeProgress::load_or_default(
            pole_protocol_draft::progress_path(&config),
            &config,
        )
        .unwrap();
        let execution = run_once_with_client(
            &config,
            &mut progress,
            &client,
            NetworkMode::SocketBackend {
                bind_addr: "127.0.0.1:0".parse().unwrap(),
                peers: vec![
                    SocketPeerProfile::new(
                        sink_a,
                        sink_addr_a,
                        [P2pTopic::Batches, P2pTopic::Receipts],
                    ),
                    SocketPeerProfile::new(
                        sink_b,
                        sink_addr_b,
                        [P2pTopic::Batches, P2pTopic::Receipts],
                    ),
                ],
            },
        )
        .unwrap();

        let simulation = execution.p2p_simulation.unwrap();
        assert_eq!(simulation.topology_peer_count, 3);
        assert_eq!(simulation.batch_recipients, 2);
        assert_eq!(simulation.receipt_recipients, 2);
        assert!(simulation.payload_retrieval_ok);

        std::fs::remove_dir_all(data_dir).unwrap();
    }
}
