use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::node_config::NodeConfig;
use crate::node_rewards::latest_activated_protocol_params;
use crate::primitives::SlotId;

const PPM_DENOMINATOR: u64 = 1_000_000;
const TIER1_MIN_PLAYERS: u64 = 1_000;
const TIER1_MIN_COLLECTORS: u32 = 3;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum GvsTier {
    Tier1,
    Tier2,
    Tier3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GvsFactors {
    pub base_glv_microunits: u64,
    pub tier: GvsTier,
    pub tier_weight_ppm: u32,
    pub time_decay_ppm: u32,
    pub coverage_bonus_ppm: u32,
    pub gvs_microunits: u64,
}

pub fn compute_gvs_factors(
    config: &NodeConfig,
    slot_id: SlotId,
    unique_collectors: u32,
    median_players: u64,
) -> GvsFactors {
    let base_glv_microunits = median_players.saturating_mul(PPM_DENOMINATOR);
    let (tier, tier_weight_ppm) = classify_tier(config, unique_collectors, median_players);
    let time_decay_ppm = compute_time_decay_ppm(config, slot_id);
    let coverage_bonus_ppm = compute_coverage_bonus_ppm(unique_collectors);
    let gvs_microunits = compute_gvs_microunits(
        base_glv_microunits,
        tier_weight_ppm,
        time_decay_ppm,
        coverage_bonus_ppm,
    );
    GvsFactors {
        base_glv_microunits,
        tier,
        tier_weight_ppm,
        time_decay_ppm,
        coverage_bonus_ppm,
        gvs_microunits,
    }
}

pub fn classify_tier(
    config: &NodeConfig,
    unique_collectors: u32,
    median_players: u64,
) -> (GvsTier, u32) {
    let activated_params = latest_activated_protocol_params(config);
    let tier1_weight = activated_params
        .as_ref()
        .map(|params| params.rewards.tier1_weight_ppm)
        .unwrap_or(1_000_000);
    let tier2_min = activated_params
        .as_ref()
        .map(|params| params.rewards.tier2_weight_min_ppm)
        .unwrap_or(300_000);
    let tier2_max = activated_params
        .as_ref()
        .map(|params| params.rewards.tier2_weight_max_ppm)
        .unwrap_or(600_000);
    let tier3_min = activated_params
        .as_ref()
        .map(|params| params.rewards.tier3_weight_min_ppm)
        .unwrap_or(50_000);
    let tier3_max = activated_params
        .as_ref()
        .map(|params| params.rewards.tier3_weight_max_ppm)
        .unwrap_or(150_000);

    if unique_collectors >= TIER1_MIN_COLLECTORS && median_players >= TIER1_MIN_PLAYERS {
        return (GvsTier::Tier1, tier1_weight);
    }

    if unique_collectors >= TIER1_MIN_COLLECTORS {
        let collector_bonus = unique_collectors.saturating_sub(TIER1_MIN_COLLECTORS) * 50_000;
        let player_bonus = ((median_players / 500).min(3) as u32) * 50_000;
        return (
            GvsTier::Tier2,
            (tier2_min + collector_bonus + player_bonus).min(tier2_max),
        );
    }

    let collector_bonus = unique_collectors.saturating_sub(1) * 25_000;
    let player_bonus = ((median_players / 500).min(2) as u32) * 25_000;
    (
        GvsTier::Tier3,
        (tier3_min + collector_bonus + player_bonus).min(tier3_max),
    )
}

pub fn compute_time_decay_ppm(config: &NodeConfig, slot_id: SlotId) -> u32 {
    let slots_per_epoch = config.runtime.slots_per_epoch.max(1);
    if slots_per_epoch <= 1 {
        return 1_000_000;
    }

    let slot_index = slot_id.saturating_sub(1).min(slots_per_epoch - 1);
    let ramp = (slot_index * 150_000) / (slots_per_epoch - 1);
    (850_000 + ramp as u32).min(1_000_000)
}

pub fn compute_coverage_bonus_ppm(unique_collectors: u32) -> u32 {
    let extra_collectors = unique_collectors.saturating_sub(1);
    (1_000_000 + extra_collectors.saturating_mul(50_000)).min(1_250_000)
}

pub fn compute_gvs_microunits(
    base_glv_microunits: u64,
    tier_weight_ppm: u32,
    time_decay_ppm: u32,
    coverage_bonus_ppm: u32,
) -> u64 {
    let weighted = u128::from(base_glv_microunits)
        .saturating_mul(u128::from(tier_weight_ppm))
        .saturating_mul(u128::from(time_decay_ppm))
        .saturating_mul(u128::from(coverage_bonus_ppm));
    let divisor = u128::from(PPM_DENOMINATOR).pow(3);
    weighted
        .checked_div(divisor)
        .unwrap_or_default()
        .min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{PersistentStoreStub, ProtocolStore};
    use crate::{
        GovernanceParamsUpdateProposalRecord, GovernanceProposalKind, GovernanceProposalState,
    };

    #[test]
    fn classify_tier_matches_whitepaper_style_thresholds() {
        let config = crate::NodeConfig::default();
        assert_eq!(
            classify_tier(&config, 3, 1_500),
            (GvsTier::Tier1, 1_000_000)
        );
        assert_eq!(classify_tier(&config, 4, 600), (GvsTier::Tier2, 400_000));
        assert_eq!(classify_tier(&config, 2, 200), (GvsTier::Tier3, 75_000));
    }

    #[test]
    fn coverage_bonus_caps_at_policy_limit() {
        assert_eq!(compute_coverage_bonus_ppm(1), 1_000_000);
        assert_eq!(compute_coverage_bonus_ppm(3), 1_100_000);
        assert_eq!(compute_coverage_bonus_ppm(20), 1_250_000);
    }

    #[test]
    fn activated_protocol_tier_weights_override_default_gvs_tier_weights() {
        let temp_dir = std::env::temp_dir().join(format!("pole-gvs-tier-{}", std::process::id()));
        if temp_dir.exists() {
            std::fs::remove_dir_all(&temp_dir).unwrap();
        }
        let mut config = crate::NodeConfig::default();
        config.runtime.data_dir = temp_dir.to_string_lossy().into_owned();
        let store_path = crate::node_daemon::local_chain_store_path(&config);
        let mut store = PersistentStoreStub::open(&store_path).unwrap();

        let params = crate::ProtocolParams {
            slot_seconds: 300,
            epoch_slots: 12,
            committee_size: 21,
            unbonding_blocks: 5,
            min_verify_bond: 100,
            min_propose_bond: 10_000,
            challenge_window_blocks: 20,
            max_emergency_brake_blocks: 100,
            min_retention_epochs: 2,
            fee: crate::FeeParams {
                base_gas_price_nano: 100,
                max_gas_price_nano: 1_000,
                gas_adjustment_ppm: 1_150_000,
                congestion_threshold_ppm: 500_000,
                fee_burn_bps: 2_500,
            },
            rewards: crate::RewardParams {
                reward_source_is_tokenomics: false,
                emission_year: 1,
                reward_block_secs: 3_600,
                initial_emission_rate_bps: 2_000,
                tail_emission_start_year: 4,
                tail_emission_rate_bps: 200,
                player_reward_allocation_bps: 8_000,
                service_reward_allocation_bps: 1_000,
                collect_reward_bps: 5_000,
                store_reward_bps: 2_500,
                verify_reward_bps: 1_500,
                propose_reward_bps: 1_000,
                configured_player_block_reward: 1_000,
                effective_player_block_reward: 1_000,
                target_network_weight_units: 1,
                reward_adjustment_cap_bps: 2_000,
                tier1_weight_ppm: 900_000,
                tier2_weight_min_ppm: 250_000,
                tier2_weight_max_ppm: 500_000,
                tier3_weight_min_ppm: 60_000,
                tier3_weight_max_ppm: 120_000,
                app_weight_overrides: Vec::new(),
                reward_burn_threshold: 10_000,
                reward_burn_bps: 1_000,
                governance_burn_bps: 100,
            },
            governance: crate::GovernanceParams {
                params_update_bond: 10_000,
                params_update_quorum_bps: 2_500,
                params_update_approval_bps: 6_000,
                slow_params_update_bond: 20_000,
                slow_params_update_quorum_bps: 3_300,
                slow_params_update_approval_bps: 7_500,
            },
            slashing: crate::SlashingParams {
                double_sign_bps: 5_000,
                offline_bps: 100,
                medium_deviation_bps: 500,
                severe_deviation_bps: 2_000,
            },
        };
        store.insert_params_update_proposal(
            [0xaa; 32],
            GovernanceParamsUpdateProposalRecord {
                proposal_id: [0xaa; 32],
                proposer: [0x41; 32],
                kind: GovernanceProposalKind::FastParams,
                effective_epoch: 2,
                submitted_height: 1,
                bond_amount: 10_000,
                params_hash: [0xbb; 32],
                params,
                state: GovernanceProposalState::Activated,
            },
        );
        store.flush().unwrap();

        assert_eq!(classify_tier(&config, 3, 1_500), (GvsTier::Tier1, 900_000));
        assert_eq!(classify_tier(&config, 4, 600), (GvsTier::Tier2, 350_000));
        assert_eq!(classify_tier(&config, 2, 200), (GvsTier::Tier3, 85_000));

        std::fs::remove_dir_all(temp_dir).unwrap();
    }
}
