use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::primitives::Amount;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct SlashingParams {
    pub double_sign_bps: u16,
    pub offline_bps: u16,
    pub medium_deviation_bps: u16,
    pub severe_deviation_bps: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct FeeParams {
    pub base_gas_price_nano: u64,
    pub max_gas_price_nano: u64,
    pub gas_adjustment_ppm: u32,
    pub congestion_threshold_ppm: u32,
    pub fee_burn_bps: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct AppWeightOverride {
    pub app_id: u32,
    pub game_coefficient_ppm: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct RewardParams {
    pub reward_source_is_tokenomics: bool,
    pub emission_year: u32,
    pub reward_block_secs: u64,
    pub initial_emission_rate_bps: u16,
    pub tail_emission_start_year: u32,
    pub tail_emission_rate_bps: u16,
    pub player_reward_allocation_bps: u16,
    pub service_reward_allocation_bps: u16,
    pub collect_reward_bps: u16,
    pub store_reward_bps: u16,
    pub verify_reward_bps: u16,
    pub propose_reward_bps: u16,
    pub configured_player_block_reward: Amount,
    pub effective_player_block_reward: Amount,
    pub target_network_weight_units: Amount,
    pub reward_adjustment_cap_bps: u16,
    pub tier1_weight_ppm: u32,
    pub tier2_weight_min_ppm: u32,
    pub tier2_weight_max_ppm: u32,
    pub tier3_weight_min_ppm: u32,
    pub tier3_weight_max_ppm: u32,
    pub app_weight_overrides: Vec<AppWeightOverride>,
    pub reward_burn_threshold: Amount,
    pub reward_burn_bps: u16,
    pub governance_burn_bps: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct GovernanceParams {
    pub params_update_bond: Amount,
    pub params_update_quorum_bps: u16,
    pub params_update_approval_bps: u16,
    pub slow_params_update_bond: Amount,
    pub slow_params_update_quorum_bps: u16,
    pub slow_params_update_approval_bps: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ProtocolParams {
    pub slot_seconds: u32,
    pub epoch_slots: u32,
    pub committee_size: u16,
    pub unbonding_blocks: u32,
    pub min_verify_bond: Amount,
    pub min_propose_bond: Amount,
    pub challenge_window_blocks: u32,
    pub max_emergency_brake_blocks: u32,
    pub min_retention_epochs: u32,
    pub fee: FeeParams,
    pub rewards: RewardParams,
    pub governance: GovernanceParams,
    pub slashing: SlashingParams,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolParamsError {
    ZeroSlotSeconds,
    ZeroEpochSlots,
    ZeroChallengeWindowBlocks,
    ZeroMinRetentionEpochs,
    ZeroEmissionYear,
    ZeroTailEmissionStartYear,
    ZeroRewardBlockSecs,
    ServiceRewardSplitInvalid { total_bps: u32 },
    AllocationSplitInvalid { total_bps: u32 },
    EffectivePlayerBlockRewardZero,
    InvalidAppWeightOverrides,
    GovernanceThresholdInvalid,
    GovernanceBondZero,
}

impl std::fmt::Display for ProtocolParamsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroSlotSeconds => write!(f, "slot_seconds must be greater than 0"),
            Self::ZeroEpochSlots => write!(f, "epoch_slots must be greater than 0"),
            Self::ZeroChallengeWindowBlocks => {
                write!(f, "challenge_window_blocks must be greater than 0")
            }
            Self::ZeroMinRetentionEpochs => {
                write!(f, "min_retention_epochs must be greater than 0")
            }
            Self::ZeroEmissionYear => write!(f, "rewards.emission_year must be greater than 0"),
            Self::ZeroTailEmissionStartYear => {
                write!(f, "rewards.tail_emission_start_year must be greater than 0")
            }
            Self::ZeroRewardBlockSecs => {
                write!(f, "rewards.reward_block_secs must be greater than 0")
            }
            Self::ServiceRewardSplitInvalid { total_bps } => write!(
                f,
                "service reward split must sum to 10000 bps, got {total_bps}"
            ),
            Self::AllocationSplitInvalid { total_bps } => write!(
                f,
                "player/service allocation split must sum to 9000 bps, got {total_bps}"
            ),
            Self::EffectivePlayerBlockRewardZero => {
                write!(
                    f,
                    "rewards.effective_player_block_reward must be greater than 0"
                )
            }
            Self::InvalidAppWeightOverrides => {
                write!(f, "rewards.app_weight_overrides must be sorted by app_id and use positive coefficients")
            }
            Self::GovernanceThresholdInvalid => {
                write!(
                    f,
                    "governance threshold bps values must be between 1 and 10000"
                )
            }
            Self::GovernanceBondZero => {
                write!(f, "governance params_update_bond must be greater than 0")
            }
        }
    }
}

impl std::error::Error for ProtocolParamsError {}

impl ProtocolParams {
    pub fn validate(&self) -> Result<(), ProtocolParamsError> {
        if self.slot_seconds == 0 {
            return Err(ProtocolParamsError::ZeroSlotSeconds);
        }
        if self.epoch_slots == 0 {
            return Err(ProtocolParamsError::ZeroEpochSlots);
        }
        if self.challenge_window_blocks == 0 {
            return Err(ProtocolParamsError::ZeroChallengeWindowBlocks);
        }
        if self.min_retention_epochs == 0 {
            return Err(ProtocolParamsError::ZeroMinRetentionEpochs);
        }
        if self.rewards.emission_year == 0 {
            return Err(ProtocolParamsError::ZeroEmissionYear);
        }
        if self.rewards.tail_emission_start_year == 0 {
            return Err(ProtocolParamsError::ZeroTailEmissionStartYear);
        }
        if self.rewards.reward_block_secs == 0 {
            return Err(ProtocolParamsError::ZeroRewardBlockSecs);
        }
        let service_split_total = u32::from(self.rewards.collect_reward_bps)
            + u32::from(self.rewards.store_reward_bps)
            + u32::from(self.rewards.verify_reward_bps)
            + u32::from(self.rewards.propose_reward_bps);
        if service_split_total != 10_000 {
            return Err(ProtocolParamsError::ServiceRewardSplitInvalid {
                total_bps: service_split_total,
            });
        }
        let allocation_total = u32::from(self.rewards.player_reward_allocation_bps)
            + u32::from(self.rewards.service_reward_allocation_bps);
        if allocation_total != 9_000 {
            return Err(ProtocolParamsError::AllocationSplitInvalid {
                total_bps: allocation_total,
            });
        }
        if self.rewards.effective_player_block_reward == 0 {
            return Err(ProtocolParamsError::EffectivePlayerBlockRewardZero);
        }
        let mut previous_app_id = None;
        for override_entry in &self.rewards.app_weight_overrides {
            if override_entry.game_coefficient_ppm == 0 {
                return Err(ProtocolParamsError::InvalidAppWeightOverrides);
            }
            if previous_app_id
                .map(|last| override_entry.app_id <= last)
                .unwrap_or(false)
            {
                return Err(ProtocolParamsError::InvalidAppWeightOverrides);
            }
            previous_app_id = Some(override_entry.app_id);
        }
        if self.governance.params_update_quorum_bps == 0
            || self.governance.params_update_quorum_bps > 10_000
            || self.governance.params_update_approval_bps == 0
            || self.governance.params_update_approval_bps > 10_000
        {
            return Err(ProtocolParamsError::GovernanceThresholdInvalid);
        }
        if self.governance.params_update_bond == 0 {
            return Err(ProtocolParamsError::GovernanceBondZero);
        }
        if self.governance.slow_params_update_bond == 0
            || self.governance.slow_params_update_quorum_bps == 0
            || self.governance.slow_params_update_quorum_bps > 10_000
            || self.governance.slow_params_update_approval_bps == 0
            || self.governance.slow_params_update_approval_bps > 10_000
        {
            return Err(ProtocolParamsError::GovernanceThresholdInvalid);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_params() -> ProtocolParams {
        ProtocolParams {
            slot_seconds: 300,
            epoch_slots: 12,
            committee_size: 21,
            unbonding_blocks: 5,
            min_verify_bond: 100,
            min_propose_bond: 10_000,
            challenge_window_blocks: 20,
            max_emergency_brake_blocks: 100,
            min_retention_epochs: 2,
            fee: FeeParams {
                base_gas_price_nano: 100,
                max_gas_price_nano: 1_000,
                gas_adjustment_ppm: 1_150_000,
                congestion_threshold_ppm: 500_000,
                fee_burn_bps: 2_500,
            },
            rewards: RewardParams {
                reward_source_is_tokenomics: true,
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
                effective_player_block_reward: 18_264,
                target_network_weight_units: 150_000_000_000_000,
                reward_adjustment_cap_bps: 2_000,
                tier1_weight_ppm: 1_000_000,
                tier2_weight_min_ppm: 300_000,
                tier2_weight_max_ppm: 600_000,
                tier3_weight_min_ppm: 50_000,
                tier3_weight_max_ppm: 150_000,
                app_weight_overrides: Vec::new(),
                reward_burn_threshold: 10_000,
                reward_burn_bps: 1_000,
                governance_burn_bps: 100,
            },
            governance: GovernanceParams {
                params_update_bond: 10_000,
                params_update_quorum_bps: 2_500,
                params_update_approval_bps: 6_000,
                slow_params_update_bond: 20_000,
                slow_params_update_quorum_bps: 3_300,
                slow_params_update_approval_bps: 7_500,
            },
            slashing: SlashingParams {
                double_sign_bps: 5_000,
                offline_bps: 100,
                medium_deviation_bps: 500,
                severe_deviation_bps: 2_000,
            },
        }
    }

    #[test]
    fn protocol_params_validate_accepts_whitepaper_aligned_reward_settings() {
        assert!(valid_params().validate().is_ok());
    }

    #[test]
    fn protocol_params_validate_rejects_invalid_service_split() {
        let mut params = valid_params();
        params.rewards.collect_reward_bps = 4_000;
        let err = params.validate().unwrap_err();
        assert!(matches!(
            err,
            ProtocolParamsError::ServiceRewardSplitInvalid { total_bps: 9_000 }
        ));
    }
}
