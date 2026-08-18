//! Shared CLI command helpers used by multiple binaries (`pole-client`,
//! `pole-node`, …) so that identical command-handler logic is not
//! copy-pasted across bin crates.
//!
//! The `governance-show-*` / `reward-adjustment-show-*` command bodies
//! are unified: both binaries accept an optional leading `[config-path]`
//! and print a `PoLE <bin> <command>` header line, so the printed fields
//! are identical except for the bin label passed in.

use std::fmt::Write as _;
use std::path::PathBuf;

use crate::node_config::NodeConfig;
use crate::store::ProtocolStore;

/// Print the merkle roots of a built epoch commit artifact, one per
/// line.  Shared verbatim by `pole-client` and `pole-node`.
pub fn print_epoch_commit_artifact_roots(
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

/// Render the tokenomics report body shared between binaries.
///
/// Both `pole-client tokenomics` and `pole-node tokenomics` print the
/// same fixed allocation table plus the per-year emission schedule;
/// only the leading title line differs and is passed in as
/// `title_line`.
pub fn render_tokenomics_schedule(
    title_line: &str,
    breakdown: &crate::tokenomics::AllocationBreakdown,
    schedule: &[crate::tokenomics::AnnualEmissionSchedule],
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{title_line}");
    let _ = writeln!(
        out,
        "{}",
        format_args!("total_supply={}", crate::tokenomics::TOTAL_SUPPLY)
    );
    let _ = writeln!(
        out,
        "initial_emission_rate_bps={}",
        crate::tokenomics::INITIAL_EMISSION_RATE_BPS
    );
    let _ = writeln!(
        out,
        "tail_emission_start_year={}",
        crate::tokenomics::LONG_TERM_TAIL_START_YEAR
    );
    let _ = writeln!(
        out,
        "tail_emission_rate_bps={}",
        crate::tokenomics::LONG_TERM_TAIL_EMISSION_RATE_BPS
    );
    let _ = writeln!(
        out,
        "player_rewards_allocation={}",
        breakdown.player_rewards
    );
    let _ = writeln!(
        out,
        "service_rewards_allocation={}",
        breakdown.service_rewards
    );
    let _ = writeln!(out, "treasury_allocation={}", breakdown.treasury);
    let _ = writeln!(out, "team_allocation={}", breakdown.team);
    let _ = writeln!(
        out,
        "early_supporters_allocation={}",
        breakdown.early_supporters
    );
    for row in schedule {
        let _ = writeln!(
            out,
            "year={} nominal_rate_bps={} annual_emission={} cumulative_emission={}",
            row.year, row.nominal_rate_bps, row.annual_emission, row.cumulative_emission
        );
    }
    out
}

/// Unified config-path resolution for bin command handlers.
///
/// Accepts an optional leading `[config-path]` (default
/// `default_config_path`) and returns the resolved absolute path, the
/// loaded config, and the next argument index where business
/// parameters begin. Prints the `PoLE <bin> <command>` header.
pub fn resolve_config_and_header(
    args: &[String],
    command_name: &str,
    bin_name: &str,
    default_config_path: &str,
) -> Result<(PathBuf, NodeConfig, usize), Box<dyn std::error::Error>> {
    use crate::cli_support::parse_config_path_and_rest;
    let (config_path_arg, start_index) = parse_config_path_and_rest(args, 2, default_config_path);
    let (config_path, config) =
        NodeConfig::load_json_with_runtime_paths(config_path_arg).map_err(|e| e.to_string())?;
    crate::cli_support::print_command_header_for(bin_name, command_name, &config_path);
    Ok((config_path, config, start_index))
}

/// `governance-show-index [config-path]` — print the governance
/// artifact index. Shared by both binaries.
pub fn governance_show_index(
    args: &[String],
    bin_name: &str,
    default_config_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let (config_path, config, start_index) =
        resolve_config_and_header(args, "governance-show-index", bin_name, default_config_path)?;
    let index_path = crate::node_daemon::governance_index_artifact_path(&config);
    let index = crate::node_settlement::GovernanceArtifactIndex::load_or_default_json(&index_path)?;
    println!("artifact_index_path={}", index_path.to_string_lossy());
    crate::cli_output::print_governance_index(&index);
    let _ = (config_path, start_index);
    Ok(())
}

/// `governance-show-summary [config-path]` — print the governance
/// artifact summary. Shared by both binaries.
pub fn governance_show_summary(
    args: &[String],
    bin_name: &str,
    default_config_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let (config_path, config, start_index) = resolve_config_and_header(
        args,
        "governance-show-summary",
        bin_name,
        default_config_path,
    )?;
    let summary_path = crate::node_daemon::governance_summary_artifact_path(&config);
    let summary =
        crate::node_settlement::GovernanceArtifactSummary::load_or_default_json(&summary_path)?;
    println!("artifact_summary_path={}", summary_path.to_string_lossy());
    println!("artifact_index_path={}", summary.artifact_index_path);
    crate::cli_output::print_governance_summary(&summary);
    let _ = (config_path, start_index);
    Ok(())
}

/// `governance-show-scheduled [config-path] [epoch-id]` — print a
/// scheduled protocol-params artifact. Shared by both binaries.
pub fn governance_show_scheduled(
    args: &[String],
    bin_name: &str,
    default_config_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let (config_path, config, start_index) = resolve_config_and_header(
        args,
        "governance-show-scheduled",
        bin_name,
        default_config_path,
    )?;
    let (_, state) = crate::node_settlement::open_local_protocol_state(
        &config,
        config.runtime.challenge_window_blocks,
    )?;
    let epoch_id = crate::cli_support::parse_optional_u64_arg(args, start_index)?
        .unwrap_or(state.current_epoch.saturating_add(1));
    let scheduled_params = state.store.scheduled_protocol_params(&epoch_id);
    let (artifact, artifact_path, index_path) =
        crate::node_settlement::export_governance_scheduled_artifact(
            &config,
            state.current_epoch,
            epoch_id,
            scheduled_params,
        )?;
    crate::cli_output::print_governance_scheduled_artifact(&artifact);
    println!("artifact_path={}", artifact_path.to_string_lossy());
    println!("artifact_index_path={}", index_path.to_string_lossy());
    let _ = (config_path, start_index);
    Ok(())
}

/// `reward-adjustment-show-index [config-path]` — print the reward
/// adjustment artifact index. Shared by both binaries.
pub fn reward_adjustment_show_index(
    args: &[String],
    bin_name: &str,
    default_config_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let (config_path, config, start_index) = resolve_config_and_header(
        args,
        "reward-adjustment-show-index",
        bin_name,
        default_config_path,
    )?;
    let index_path = crate::node_daemon::reward_adjustment_index_path(&config);
    let index =
        crate::node_daemon::RewardAdjustmentArtifactIndex::load_or_default_json(&index_path)?;
    println!("artifact_index_path={}", index_path.to_string_lossy());
    crate::cli_output::print_reward_adjustment_index(&index);
    let _ = (config_path, start_index);
    Ok(())
}

/// `reward-adjustment-show-summary [config-path]` — print the reward
/// adjustment artifact summary. Shared by both binaries.
pub fn reward_adjustment_show_summary(
    args: &[String],
    bin_name: &str,
    default_config_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let (config_path, config, start_index) = resolve_config_and_header(
        args,
        "reward-adjustment-show-summary",
        bin_name,
        default_config_path,
    )?;
    let summary_path = crate::node_daemon::reward_adjustment_summary_path(&config);
    let summary =
        crate::node_daemon::RewardAdjustmentArtifactSummary::load_or_default_json(&summary_path)?;
    println!("artifact_summary_path={}", summary_path.to_string_lossy());
    println!("artifact_index_path={}", summary.artifact_index_path);
    crate::cli_output::print_reward_adjustment_summary(&summary);
    let _ = (config_path, start_index);
    Ok(())
}
