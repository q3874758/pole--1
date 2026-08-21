//! Shared CLI command handlers merged from the `pole-client` / `pole-node`
//! binaries into the library so the two can eventually be unified into a
//! single executable.
//!
//! Commands are moved here incrementally; each batch keeps
//! `cargo test` / `cargo clippy -D warnings` / `cargo fmt` green.

use std::path::{Path, PathBuf};

/// Default client config path, used by client-side command handlers.
pub const DEFAULT_CONFIG_PATH: &str = "./node.json";

/// Infer the running binary label from `argv[0]` so shared handlers keep
/// their `PoLE <client|node|pole>` header consistent whether they run as a
/// standalone `pole-client`/`pole-node` executable or the unified `pole`.
pub fn infer_bin_label() -> &'static str {
    let name = match std::env::args().next() {
        Some(arg0) => {
            let path = Path::new(&arg0);
            match path.file_stem() {
                Some(stem) => stem.to_string_lossy().into_owned(),
                None => String::new(),
            }
        }
        None => String::new(),
    };
    if name.contains("node") {
        "node"
    } else if name.contains("client") {
        "client"
    } else {
        "pole"
    }
}

/// `governance-vote [config-path] <proposal-id-hex> <yes|no|abstain>
/// <voting-power>` — shared by both binaries.
pub fn governance_vote_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    crate::cli_commands::governance_vote(args, infer_bin_label(), DEFAULT_CONFIG_PATH)
}

/// `governance-show-proposal [config-path] <proposal-id-hex>` — shared.
pub fn governance_show_proposal_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    crate::cli_commands::governance_show_proposal(args, infer_bin_label(), DEFAULT_CONFIG_PATH)
}

/// `governance-show-scheduled [config-path] [epoch-id]` — shared.
pub fn governance_show_scheduled_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    crate::cli_commands::governance_show_scheduled(args, infer_bin_label(), DEFAULT_CONFIG_PATH)
}

/// `governance-show-index [config-path]` — shared.
pub fn governance_show_index_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    crate::cli_commands::governance_show_index(args, infer_bin_label(), DEFAULT_CONFIG_PATH)
}

/// `governance-show-summary [config-path]` — shared.
pub fn governance_show_summary_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    crate::cli_commands::governance_show_summary(args, infer_bin_label(), DEFAULT_CONFIG_PATH)
}

/// `reward-adjustment-show-index [config-path]` — shared.
pub fn reward_adjustment_show_index_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    crate::cli_commands::reward_adjustment_show_index(args, infer_bin_label(), DEFAULT_CONFIG_PATH)
}

/// `reward-adjustment-show-summary [config-path]` — shared.
pub fn reward_adjustment_show_summary_cmd(
    args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    crate::cli_commands::reward_adjustment_show_summary(
        args,
        infer_bin_label(),
        DEFAULT_CONFIG_PATH,
    )
}

/// `tokenomics [years]` — print the tokenomics allocation and emission
/// schedule. Title keeps the existing per-binary label so standalone
/// `pole-node` output stays stable.
pub fn tokenomics_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() > 3 {
        return Err("usage: pole tokenomics [years]".into());
    }

    let years = args
        .get(2)
        .map(|value| value.parse::<u32>())
        .transpose()?
        .unwrap_or(10);
    let breakdown = crate::allocation_breakdown();
    let schedule = crate::annual_emission_schedule_with_tail(
        years,
        crate::LONG_TERM_TAIL_START_YEAR,
        crate::LONG_TERM_TAIL_EMISSION_RATE_BPS,
    );

    let title = if infer_bin_label() == "node" {
        "PoLE node tokenomics"
    } else {
        "PoLE tokenomics"
    };
    print!(
        "{}",
        crate::render_tokenomics_schedule(title, &breakdown, &schedule)
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// wallet commands
// ---------------------------------------------------------------------------

/// `wallet-create [data-dir] [password] [comment]` — create a new wallet.
pub fn wallet_create_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = args.get(2).map(PathBuf::from).unwrap_or_else(|| {
        crate::cli_support::default_data_dir_for_config(Path::new(DEFAULT_CONFIG_PATH)).into()
    });
    let password = args
        .get(3)
        .cloned()
        .unwrap_or_else(|| rpassword::prompt_password("password: ").unwrap_or_default());
    let comment = args.get(4).cloned();

    let mnemonic = crate::wallet::create_wallet(&data_dir, comment, &password)?;
    println!("wallet_created");
    println!("mnemonic={}", mnemonic);
    println!("data_dir={}", data_dir.display());
    Ok(())
}

/// `wallet-recover [data-dir] [password] <24-word-mnemonic...>` — recover
/// a wallet from its mnemonic.
pub fn wallet_recover_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = args
        .get(2)
        .map(PathBuf::from)
        .ok_or("usage: wallet-recover [data-dir] [password] <24-word-mnemonic...>")?;
    let password = args
        .get(3)
        .cloned()
        .unwrap_or_else(|| rpassword::prompt_password("password: ").unwrap_or_default());
    let words: Vec<String> = args[4..].to_vec();
    if words.len() != 24 {
        return Err("mnemonic must be exactly 24 words".into());
    }

    let address = crate::wallet::recover_wallet(&words[..], &data_dir, None, &password)?;
    println!("wallet_recovered");
    println!("address={}", address);
    println!("data_dir={}", data_dir.display());
    Ok(())
}

/// `wallet-address [data-dir] [password]` — print the wallet address.
pub fn wallet_address_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = args.get(2).map(PathBuf::from).unwrap_or_else(|| {
        crate::cli_support::default_data_dir_for_config(Path::new(DEFAULT_CONFIG_PATH)).into()
    });
    let password = args
        .get(3)
        .cloned()
        .unwrap_or_else(|| rpassword::prompt_password("wallet password: ").unwrap_or_default());
    let address = crate::wallet::show_address_with_password(&data_dir, &password)?;
    println!("{}", address);
    Ok(())
}

/// `wallet-set-reward-address <config-path> [data-dir] [password]` — set the
/// node's reward address from the wallet.
pub fn wallet_set_reward_address_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = PathBuf::from(
        args.get(2)
            .ok_or("usage: wallet-set-reward-address <config-path> [data-dir] [password]")?,
    );
    let data_dir = args.get(3).map(PathBuf::from).unwrap_or_else(|| {
        crate::cli_support::default_data_dir_for_config(Path::new(DEFAULT_CONFIG_PATH)).into()
    });
    let password = args
        .get(4)
        .cloned()
        .unwrap_or_else(|| rpassword::prompt_password("wallet password: ").unwrap_or_default());

    let address = crate::wallet::set_reward_address(&data_dir, &config_path, &password)?;
    println!("reward_address_updated");
    println!("address={}", address);
    println!("config={}", config_path.display());
    Ok(())
}

/// `governance-propose-params [config-path] <proposal-id-hex> <effective-epoch>
/// <emission-year> <effective-player-block-reward> [tail-start-year tail-rate-bps]`
/// — propose an emission/reward params update.
pub fn governance_propose_params_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 6 && args.len() != 7 && args.len() != 8 && args.len() != 9 {
        return Err("usage: pole-client governance-propose-params [config-path] <proposal-id-hex> <effective-epoch> <emission-year> <effective-player-block-reward> [tail-start-year tail-rate-bps]".into());
    }
    let (config_path_arg, start_index) = crate::parse_config_path_and_rest_with_known_first_arg(
        args,
        2,
        DEFAULT_CONFIG_PATH,
        crate::looks_like_hex_32_arg,
    );
    if args.len() != start_index + 4 && args.len() != start_index + 6 {
        return Err("usage: pole-client governance-propose-params [config-path] <proposal-id-hex> <effective-epoch> <emission-year> <effective-player-block-reward> [tail-start-year tail-rate-bps]".into());
    }
    let (config_path, config) = crate::NodeConfig::load_json_with_runtime_paths(config_path_arg)?;
    let proposal_id = crate::decode_hex32(&args[start_index], "proposal_id")?;
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

    let mut params =
        crate::open_local_protocol_state(&config, config.runtime.challenge_window_blocks)?
            .1
            .params
            .clone();
    params.rewards.emission_year = emission_year;
    params.rewards.effective_player_block_reward = effective_player_block_reward;
    if let Some((tail_start_year, tail_rate_bps)) = tail_policy {
        params.rewards.tail_emission_start_year = tail_start_year;
        params.rewards.tail_emission_rate_bps = tail_rate_bps;
    }
    let effects = crate::submit_protocol_params_update_proposal(
        &config,
        proposal_id,
        effective_epoch,
        params,
    )?;

    crate::print_command_header("governance-propose-params", &config_path);
    println!("proposal_id={}", crate::hex_32(proposal_id));
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

/// `governance-propose-service-split [config-path] <proposal-id-hex>
/// <effective-epoch> <collect_bps> <store_bps> <verify_bps> <propose_bps>` —
/// propose a new service reward split.
pub fn governance_propose_service_split_cmd(
    args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 8 && args.len() != 9 {
        return Err("usage: pole-client governance-propose-service-split [config-path] <proposal-id-hex> <effective-epoch> <collect_bps> <store_bps> <verify_bps> <propose_bps>".into());
    }
    let (config_path_arg, start_index) = crate::parse_config_path_and_rest_with_known_first_arg(
        args,
        2,
        DEFAULT_CONFIG_PATH,
        crate::looks_like_hex_32_arg,
    );
    if args.len() != start_index + 6 {
        return Err("usage: pole-client governance-propose-service-split [config-path] <proposal-id-hex> <effective-epoch> <collect_bps> <store_bps> <verify_bps> <propose_bps>".into());
    }
    let (config_path, config) = crate::NodeConfig::load_json_with_runtime_paths(config_path_arg)?;
    let proposal_id = crate::decode_hex32(&args[start_index], "proposal_id")?;
    let effective_epoch: u64 = args[start_index + 1].parse()?;
    let collect_bps: u16 = args[start_index + 2].parse()?;
    let store_bps: u16 = args[start_index + 3].parse()?;
    let verify_bps: u16 = args[start_index + 4].parse()?;
    let propose_bps: u16 = args[start_index + 5].parse()?;

    let mut params =
        crate::open_local_protocol_state(&config, config.runtime.challenge_window_blocks)?
            .1
            .params
            .clone();
    params.rewards.collect_reward_bps = collect_bps;
    params.rewards.store_reward_bps = store_bps;
    params.rewards.verify_reward_bps = verify_bps;
    params.rewards.propose_reward_bps = propose_bps;
    let effects = crate::submit_protocol_params_update_proposal(
        &config,
        proposal_id,
        effective_epoch,
        params,
    )?;

    crate::print_command_header("governance-propose-service-split", &config_path);
    println!("proposal_id={}", crate::hex_32(proposal_id));
    println!("effective_epoch={effective_epoch}");
    println!("collect_reward_bps={collect_bps}");
    println!("store_reward_bps={store_bps}");
    println!("verify_reward_bps={verify_bps}");
    println!("propose_reward_bps={propose_bps}");
    println!("effect_count={}", effects.len());
    Ok(())
}

/// `governance-propose-reward-tuning [config-path] <proposal-id-hex>
/// <effective-epoch> <target_network_weight_units> <reward_adjustment_cap_bps>
/// <challenge_window_blocks> <effective-player-block-reward>` — propose a
/// reward-adjustment tuning update.
pub fn governance_propose_reward_tuning_cmd(
    args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 8 && args.len() != 9 {
        return Err("usage: pole-client governance-propose-reward-tuning [config-path] <proposal-id-hex> <effective-epoch> <target_network_weight_units> <reward_adjustment_cap_bps> <challenge_window_blocks> <effective-player-block-reward>".into());
    }
    let (config_path_arg, start_index) = crate::parse_config_path_and_rest_with_known_first_arg(
        args,
        2,
        DEFAULT_CONFIG_PATH,
        crate::looks_like_hex_32_arg,
    );
    if args.len() != start_index + 6 {
        return Err("usage: pole-client governance-propose-reward-tuning [config-path] <proposal-id-hex> <effective-epoch> <target_network_weight_units> <reward_adjustment_cap_bps> <challenge_window_blocks> <effective-player-block-reward>".into());
    }
    let (config_path, config) = crate::NodeConfig::load_json_with_runtime_paths(config_path_arg)?;
    let proposal_id = crate::decode_hex32(&args[start_index], "proposal_id")?;
    let effective_epoch: u64 = args[start_index + 1].parse()?;
    let target_network_weight_units: u128 = args[start_index + 2].parse()?;
    let reward_adjustment_cap_bps: u16 = args[start_index + 3].parse()?;
    let challenge_window_blocks: u32 = args[start_index + 4].parse()?;
    let effective_player_block_reward: u128 = args[start_index + 5].parse()?;

    let mut params =
        crate::open_local_protocol_state(&config, config.runtime.challenge_window_blocks)?
            .1
            .params
            .clone();
    params.rewards.effective_player_block_reward = effective_player_block_reward;
    params.rewards.target_network_weight_units = target_network_weight_units;
    params.rewards.reward_adjustment_cap_bps = reward_adjustment_cap_bps;
    params.challenge_window_blocks = challenge_window_blocks;
    let effects = crate::submit_protocol_params_update_proposal(
        &config,
        proposal_id,
        effective_epoch,
        params,
    )?;

    crate::print_command_header("governance-propose-reward-tuning", &config_path);
    println!("proposal_id={}", crate::hex_32(proposal_id));
    println!("effective_epoch={effective_epoch}");
    println!("target_network_weight_units={target_network_weight_units}");
    println!("reward_adjustment_cap_bps={reward_adjustment_cap_bps}");
    println!("challenge_window_blocks={challenge_window_blocks}");
    println!("effective_player_block_reward={effective_player_block_reward}");
    println!("effect_count={}", effects.len());
    Ok(())
}

/// `governance-propose-thresholds [config-path] <proposal-id-hex>
/// <effective-epoch> <quorum_bps> <approval_bps>` — propose new governance
/// params-update thresholds.
pub fn governance_propose_thresholds_cmd(
    args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 6 && args.len() != 7 {
        return Err("usage: pole-client governance-propose-thresholds [config-path] <proposal-id-hex> <effective-epoch> <quorum_bps> <approval_bps>".into());
    }
    let (config_path_arg, start_index) = crate::parse_config_path_and_rest_with_known_first_arg(
        args,
        2,
        DEFAULT_CONFIG_PATH,
        crate::looks_like_hex_32_arg,
    );
    if args.len() != start_index + 4 {
        return Err("usage: pole-client governance-propose-thresholds [config-path] <proposal-id-hex> <effective-epoch> <quorum_bps> <approval_bps>".into());
    }
    let (config_path, config) = crate::NodeConfig::load_json_with_runtime_paths(config_path_arg)?;
    let proposal_id = crate::decode_hex32(&args[start_index], "proposal_id")?;
    let effective_epoch: u64 = args[start_index + 1].parse()?;
    let quorum_bps: u16 = args[start_index + 2].parse()?;
    let approval_bps: u16 = args[start_index + 3].parse()?;

    let mut params =
        crate::open_local_protocol_state(&config, config.runtime.challenge_window_blocks)?
            .1
            .params
            .clone();
    params.governance.params_update_quorum_bps = quorum_bps;
    params.governance.params_update_approval_bps = approval_bps;
    let effects = crate::submit_protocol_params_update_proposal(
        &config,
        proposal_id,
        effective_epoch,
        params,
    )?;

    crate::print_command_header("governance-propose-thresholds", &config_path);
    println!("proposal_id={}", crate::hex_32(proposal_id));
    println!("effective_epoch={effective_epoch}");
    println!("params_update_quorum_bps={quorum_bps}");
    println!("params_update_approval_bps={approval_bps}");
    println!("effect_count={}", effects.len());
    Ok(())
}

/// `governance-propose-slow-params [config-path] <proposal-id-hex>
/// <effective-epoch> <reward-block-secs> <effective-player-block-reward>` —
/// propose a slow (block-level) params update.
pub fn governance_propose_slow_params_cmd(
    args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 6 && args.len() != 7 {
        return Err("usage: pole-client governance-propose-slow-params [config-path] <proposal-id-hex> <effective-epoch> <reward-block-secs> <effective-player-block-reward>".into());
    }
    let (config_path_arg, start_index) = crate::parse_config_path_and_rest_with_known_first_arg(
        args,
        2,
        DEFAULT_CONFIG_PATH,
        crate::looks_like_hex_32_arg,
    );
    if args.len() != start_index + 4 {
        return Err("usage: pole-client governance-propose-slow-params [config-path] <proposal-id-hex> <effective-epoch> <reward-block-secs> <effective-player-block-reward>".into());
    }
    let (config_path, config) = crate::NodeConfig::load_json_with_runtime_paths(config_path_arg)?;
    let proposal_id = crate::decode_hex32(&args[start_index], "proposal_id")?;
    let effective_epoch: u64 = args[start_index + 1].parse()?;
    let reward_block_secs: u64 = args[start_index + 2].parse()?;
    let effective_player_block_reward: u128 = args[start_index + 3].parse()?;

    let mut params =
        crate::open_local_protocol_state(&config, config.runtime.challenge_window_blocks)?
            .1
            .params
            .clone();
    params.rewards.reward_block_secs = reward_block_secs;
    params.rewards.effective_player_block_reward = effective_player_block_reward;
    let effects = crate::submit_protocol_params_update_proposal(
        &config,
        proposal_id,
        effective_epoch,
        params,
    )?;

    crate::print_command_header("governance-propose-slow-params", &config_path);
    println!("proposal_id={}", crate::hex_32(proposal_id));
    println!("effective_epoch={effective_epoch}");
    println!("reward_block_secs={reward_block_secs}");
    println!("effective_player_block_reward={effective_player_block_reward}");
    println!("effect_count={}", effects.len());
    Ok(())
}

/// `governance-propose-retention [config-path] <proposal-id-hex>
/// <effective-epoch> <min-retention-epochs> <challenge-window-blocks>` —
/// propose a retention/backfill params update.
pub fn governance_propose_retention_cmd(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 6 && args.len() != 7 {
        return Err("usage: pole-client governance-propose-retention [config-path] <proposal-id-hex> <effective-epoch> <min-retention-epochs> <challenge-window-blocks>".into());
    }
    let (config_path_arg, start_index) = crate::parse_config_path_and_rest_with_known_first_arg(
        args,
        2,
        DEFAULT_CONFIG_PATH,
        crate::looks_like_hex_32_arg,
    );
    if args.len() != start_index + 4 {
        return Err("usage: pole-client governance-propose-retention [config-path] <proposal-id-hex> <effective-epoch> <min-retention-epochs> <challenge-window-blocks>".into());
    }
    let (config_path, config) = crate::NodeConfig::load_json_with_runtime_paths(config_path_arg)?;
    let proposal_id = crate::decode_hex32(&args[start_index], "proposal_id")?;
    let effective_epoch: u64 = args[start_index + 1].parse()?;
    let min_retention_epochs: u32 = args[start_index + 2].parse()?;
    let challenge_window_blocks: u32 = args[start_index + 3].parse()?;

    let mut params =
        crate::open_local_protocol_state(&config, config.runtime.challenge_window_blocks)?
            .1
            .params
            .clone();
    params.min_retention_epochs = min_retention_epochs;
    params.challenge_window_blocks = challenge_window_blocks;
    let effects = crate::submit_protocol_params_update_proposal(
        &config,
        proposal_id,
        effective_epoch,
        params,
    )?;

    crate::print_command_header("governance-propose-retention", &config_path);
    println!("proposal_id={}", crate::hex_32(proposal_id));
    println!("effective_epoch={effective_epoch}");
    println!("min_retention_epochs={min_retention_epochs}");
    println!("challenge_window_blocks={challenge_window_blocks}");
    println!("effect_count={}", effects.len());
    Ok(())
}

/// `governance-propose-tier-weights [config-path] <proposal-id-hex>
/// <effective-epoch> <tier1_weight_ppm> <tier2_min_ppm> <tier2_max_ppm>
/// <tier3_min_ppm> <tier3_max_ppm>` — propose new tier weight bounds.
pub fn governance_propose_tier_weights_cmd(
    args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 9 && args.len() != 10 {
        return Err("usage: pole-client governance-propose-tier-weights [config-path] <proposal-id-hex> <effective-epoch> <tier1_weight_ppm> <tier2_min_ppm> <tier2_max_ppm> <tier3_min_ppm> <tier3_max_ppm>".into());
    }
    let (config_path_arg, start_index) = crate::parse_config_path_and_rest_with_known_first_arg(
        args,
        2,
        DEFAULT_CONFIG_PATH,
        crate::looks_like_hex_32_arg,
    );
    if args.len() != start_index + 7 {
        return Err("usage: pole-client governance-propose-tier-weights [config-path] <proposal-id-hex> <effective-epoch> <tier1_weight_ppm> <tier2_min_ppm> <tier2_max_ppm> <tier3_min_ppm> <tier3_max_ppm>".into());
    }
    let (config_path, config) = crate::NodeConfig::load_json_with_runtime_paths(config_path_arg)?;
    let proposal_id = crate::decode_hex32(&args[start_index], "proposal_id")?;
    let effective_epoch: u64 = args[start_index + 1].parse()?;
    let tier1_weight_ppm: u32 = args[start_index + 2].parse()?;
    let tier2_weight_min_ppm: u32 = args[start_index + 3].parse()?;
    let tier2_weight_max_ppm: u32 = args[start_index + 4].parse()?;
    let tier3_weight_min_ppm: u32 = args[start_index + 5].parse()?;
    let tier3_weight_max_ppm: u32 = args[start_index + 6].parse()?;

    let mut params =
        crate::open_local_protocol_state(&config, config.runtime.challenge_window_blocks)?
            .1
            .params
            .clone();
    params.rewards.tier1_weight_ppm = tier1_weight_ppm;
    params.rewards.tier2_weight_min_ppm = tier2_weight_min_ppm;
    params.rewards.tier2_weight_max_ppm = tier2_weight_max_ppm;
    params.rewards.tier3_weight_min_ppm = tier3_weight_min_ppm;
    params.rewards.tier3_weight_max_ppm = tier3_weight_max_ppm;
    let effects = crate::submit_protocol_params_update_proposal(
        &config,
        proposal_id,
        effective_epoch,
        params,
    )?;

    crate::print_command_header("governance-propose-tier-weights", &config_path);
    println!("proposal_id={}", crate::hex_32(proposal_id));
    println!("effective_epoch={effective_epoch}");
    println!("tier1_weight_ppm={tier1_weight_ppm}");
    println!("tier2_weight_min_ppm={tier2_weight_min_ppm}");
    println!("tier2_weight_max_ppm={tier2_weight_max_ppm}");
    println!("tier3_weight_min_ppm={tier3_weight_min_ppm}");
    println!("tier3_weight_max_ppm={tier3_weight_max_ppm}");
    println!("effect_count={}", effects.len());
    Ok(())
}

/// `governance-propose-app-weight [config-path] <proposal-id-hex>
/// <effective-epoch> <app-id> <game-coefficient-ppm>` — propose an
/// app-specific reward weight override.
pub fn governance_propose_app_weight_cmd(
    args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 6 && args.len() != 7 {
        return Err("usage: pole-client governance-propose-app-weight [config-path] <proposal-id-hex> <effective-epoch> <app-id> <game-coefficient-ppm>".into());
    }
    let (config_path_arg, start_index) = crate::parse_config_path_and_rest_with_known_first_arg(
        args,
        2,
        DEFAULT_CONFIG_PATH,
        crate::looks_like_hex_32_arg,
    );
    if args.len() != start_index + 4 {
        return Err("usage: pole-client governance-propose-app-weight [config-path] <proposal-id-hex> <effective-epoch> <app-id> <game-coefficient-ppm>".into());
    }
    let (config_path, config) = crate::NodeConfig::load_json_with_runtime_paths(config_path_arg)?;
    let proposal_id = crate::decode_hex32(&args[start_index], "proposal_id")?;
    let effective_epoch: u64 = args[start_index + 1].parse()?;
    let app_id: u32 = args[start_index + 2].parse()?;
    let game_coefficient_ppm: u32 = args[start_index + 3].parse()?;

    let mut params =
        crate::open_local_protocol_state(&config, config.runtime.challenge_window_blocks)?
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
        .push(crate::AppWeightOverride {
            app_id,
            game_coefficient_ppm,
        });
    params
        .rewards
        .app_weight_overrides
        .sort_by_key(|entry| entry.app_id);
    let effects = crate::submit_protocol_params_update_proposal(
        &config,
        proposal_id,
        effective_epoch,
        params,
    )?;

    crate::print_command_header("governance-propose-app-weight", &config_path);
    println!("proposal_id={}", crate::hex_32(proposal_id));
    println!("effective_epoch={effective_epoch}");
    println!("app_id={app_id}");
    println!("game_coefficient_ppm={game_coefficient_ppm}");
    println!("effect_count={}", effects.len());
    Ok(())
}
