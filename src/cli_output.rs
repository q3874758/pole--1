use crate::{
    GovernanceArtifactIndex, GovernanceArtifactSummary, GovernanceProposalArtifact,
    GovernanceScheduledParamsArtifact, ProtocolParams, RewardAdjustmentArtifactIndex,
    RewardAdjustmentArtifactSummary, VoteChoice,
};

use std::fmt::Write as _;

pub type CommandHandler = fn(&[String]) -> Result<(), Box<dyn std::error::Error>>;

pub fn parse_vote_choice(input: &str) -> Result<VoteChoice, &'static str> {
    match input {
        "yes" => Ok(VoteChoice::Yes),
        "no" => Ok(VoteChoice::No),
        "abstain" => Ok(VoteChoice::Abstain),
        _ => Err("vote choice must be one of: yes, no, abstain"),
    }
}

pub fn print_protocol_params_summary(params: &ProtocolParams) {
    println!("challenge_window_blocks={}", params.challenge_window_blocks);
    println!("min_retention_epochs={}", params.min_retention_epochs);
    println!(
        "reward_source_is_tokenomics={}",
        params.rewards.reward_source_is_tokenomics
    );
    println!("emission_year={}", params.rewards.emission_year);
    println!(
        "tail_emission_start_year={}",
        params.rewards.tail_emission_start_year
    );
    println!(
        "tail_emission_rate_bps={}",
        params.rewards.tail_emission_rate_bps
    );
    println!("reward_block_secs={}", params.rewards.reward_block_secs);
    println!(
        "effective_player_block_reward={}",
        params.rewards.effective_player_block_reward
    );
    println!(
        "target_network_weight_units={}",
        params.rewards.target_network_weight_units
    );
    println!(
        "reward_adjustment_cap_bps={}",
        params.rewards.reward_adjustment_cap_bps
    );
    println!("tier1_weight_ppm={}", params.rewards.tier1_weight_ppm);
    println!(
        "tier2_weight_min_ppm={}",
        params.rewards.tier2_weight_min_ppm
    );
    println!(
        "tier2_weight_max_ppm={}",
        params.rewards.tier2_weight_max_ppm
    );
    println!(
        "tier3_weight_min_ppm={}",
        params.rewards.tier3_weight_min_ppm
    );
    println!(
        "tier3_weight_max_ppm={}",
        params.rewards.tier3_weight_max_ppm
    );
    println!(
        "app_weight_override_count={}",
        params.rewards.app_weight_overrides.len()
    );
    for override_entry in &params.rewards.app_weight_overrides {
        println!(
            "app_weight_override app_id={} game_coefficient_ppm={}",
            override_entry.app_id, override_entry.game_coefficient_ppm
        );
    }
    println!(
        "player_reward_allocation_bps={}",
        params.rewards.player_reward_allocation_bps
    );
    println!(
        "service_reward_allocation_bps={}",
        params.rewards.service_reward_allocation_bps
    );
    println!("collect_reward_bps={}", params.rewards.collect_reward_bps);
    println!("store_reward_bps={}", params.rewards.store_reward_bps);
    println!("verify_reward_bps={}", params.rewards.verify_reward_bps);
    println!("propose_reward_bps={}", params.rewards.propose_reward_bps);
    println!(
        "params_update_quorum_bps={}",
        params.governance.params_update_quorum_bps
    );
    println!(
        "params_update_approval_bps={}",
        params.governance.params_update_approval_bps
    );
    println!(
        "slow_params_update_bond={}",
        params.governance.slow_params_update_bond
    );
    println!(
        "slow_params_update_quorum_bps={}",
        params.governance.slow_params_update_quorum_bps
    );
    println!(
        "slow_params_update_approval_bps={}",
        params.governance.slow_params_update_approval_bps
    );
}

pub fn print_reward_adjustment_index(index: &RewardAdjustmentArtifactIndex) {
    println!(
        "adjustment_artifact_count={}",
        index.adjustment_artifacts.len()
    );
    println!(
        "adjustment_cycle_artifact_count={}",
        index.adjustment_artifacts.len()
    );
    for entry in &index.adjustment_artifacts {
        println!(
            "adjustment_artifact period_index={} basis_period_index={} adjusted_player_block_reward={} artifact_path={}",
            entry.adjustment_cycle_index,
            entry.basis_cycle_index,
            entry.fixed_player_reward,
            entry.artifact_path
        );
        println!(
            "adjustment_cycle cycle_index={} basis_cycle_index={} fixed_player_reward={} artifact_path={}",
            entry.adjustment_cycle_index,
            entry.basis_cycle_index,
            entry.fixed_player_reward,
            entry.artifact_path
        );
    }
}

pub fn print_reward_adjustment_summary(summary: &RewardAdjustmentArtifactSummary) {
    println!(
        "adjustment_artifact_count={}",
        summary.adjustment_artifact_count
    );
    println!(
        "adjustment_cycle_artifact_count={}",
        summary.adjustment_cycle_artifact_count
    );
    println!("latest_period_index={:?}", summary.latest_period_index);
    println!(
        "latest_adjustment_cycle_index={:?}",
        summary.latest_adjustment_cycle_index
    );
    println!(
        "latest_basis_period_index={:?}",
        summary.latest_basis_period_index
    );
    println!(
        "latest_basis_cycle_index={:?}",
        summary.latest_basis_cycle_index
    );
    println!(
        "latest_adjusted_player_block_reward={:?}",
        summary.latest_adjusted_player_block_reward
    );
    println!(
        "latest_fixed_player_reward={:?}",
        summary.latest_fixed_player_reward
    );
}

pub fn print_governance_index(index: &GovernanceArtifactIndex) {
    println!("proposal_artifact_count={}", index.proposal_artifacts.len());
    println!(
        "scheduled_artifact_count={}",
        index.scheduled_artifacts.len()
    );
    for entry in &index.proposal_artifacts {
        println!(
            "proposal_artifact proposal_id={} state={} effective_epoch={} artifact_path={}",
            entry.proposal_id_hex, entry.proposal_state, entry.effective_epoch, entry.artifact_path
        );
    }
    for entry in &index.scheduled_artifacts {
        println!(
            "scheduled_artifact epoch_id={} scheduled={} artifact_path={}",
            entry.epoch_id, entry.scheduled, entry.artifact_path
        );
    }
}

pub fn print_governance_summary(summary: &GovernanceArtifactSummary) {
    println!("pending_proposal_count={}", summary.pending_proposal_count);
    println!(
        "scheduled_proposal_count={}",
        summary.scheduled_proposal_count
    );
    println!(
        "activated_proposal_count={}",
        summary.activated_proposal_count
    );
    println!("expired_proposal_count={}", summary.expired_proposal_count);
    println!(
        "proposal_artifact_count={}",
        summary.proposal_artifact_count
    );
    println!(
        "scheduled_artifact_count={}",
        summary.scheduled_artifact_count
    );
    println!(
        "latest_effective_epoch={:?}",
        summary.latest_effective_epoch
    );
}

pub fn print_governance_proposal_artifact(artifact: &GovernanceProposalArtifact) {
    println!("proposal_id={}", artifact.proposal_id_hex);
    println!("proposer={}", artifact.proposer_hex);
    println!("effective_epoch={}", artifact.effective_epoch);
    println!("submitted_height={}", artifact.submitted_height);
    println!("bond_amount={}", artifact.bond_amount);
    println!("params_hash={}", artifact.params_hash_hex);
    println!("proposal_state={}", artifact.proposal_state);
    println!("vote_record_count={}", artifact.vote_record_count);
    println!("yes_voting_power={}", artifact.yes_voting_power);
    println!("no_voting_power={}", artifact.no_voting_power);
    println!("abstain_voting_power={}", artifact.abstain_voting_power);
    print_protocol_params_summary(&artifact.params);
}

pub fn print_governance_scheduled_artifact(artifact: &GovernanceScheduledParamsArtifact) {
    println!("epoch_id={}", artifact.epoch_id);
    println!("current_epoch={}", artifact.current_epoch);
    println!("scheduled={}", artifact.scheduled);
    if let Some(params) = artifact.params.as_ref() {
        print_protocol_params_summary(params);
    }
}

pub fn format_usage_block<I, S>(title: &str, lines: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut output = String::new();
    let _ = writeln!(&mut output, "{title}");
    for line in lines {
        let _ = writeln!(&mut output, "{}", line.as_ref());
    }
    output
}

pub fn dispatch_command<F>(
    args: &[String],
    commands: &[(&str, CommandHandler)],
    on_missing: F,
) -> Result<(), Box<dyn std::error::Error>>
where
    F: FnOnce(),
{
    if let Some(command_name) = args.get(1).map(String::as_str) {
        if let Some((_, handler)) = commands.iter().find(|(name, _)| *name == command_name) {
            return handler(args);
        }
    }
    on_missing();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{dispatch_command, format_usage_block, parse_vote_choice, CommandHandler};
    use crate::VoteChoice;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static HANDLER_CALLS: AtomicUsize = AtomicUsize::new(0);
    static FALLBACK_CALLS: AtomicUsize = AtomicUsize::new(0);

    fn test_handler(_: &[String]) -> Result<(), Box<dyn std::error::Error>> {
        HANDLER_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    #[test]
    fn parse_vote_choice_accepts_supported_values() {
        assert_eq!(parse_vote_choice("yes"), Ok(VoteChoice::Yes));
        assert_eq!(parse_vote_choice("no"), Ok(VoteChoice::No));
        assert_eq!(parse_vote_choice("abstain"), Ok(VoteChoice::Abstain));
    }

    #[test]
    fn parse_vote_choice_rejects_unknown_values() {
        assert_eq!(
            parse_vote_choice("maybe"),
            Err("vote choice must be one of: yes, no, abstain")
        );
    }

    #[test]
    fn format_usage_block_renders_title_and_lines() {
        let output = format_usage_block("commands:", ["  first", "  second"]);
        assert_eq!(output, "commands:\n  first\n  second\n");
    }

    #[test]
    fn dispatch_command_invokes_matching_handler() {
        HANDLER_CALLS.store(0, Ordering::SeqCst);
        FALLBACK_CALLS.store(0, Ordering::SeqCst);
        let args = vec!["bin".to_string(), "known".to_string()];
        let commands: &[(&str, CommandHandler)] = &[("known", test_handler)];

        dispatch_command(&args, commands, || {
            FALLBACK_CALLS.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();

        assert_eq!(HANDLER_CALLS.load(Ordering::SeqCst), 1);
        assert_eq!(FALLBACK_CALLS.load(Ordering::SeqCst), 0);
    }
}
