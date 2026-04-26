use crate::primitives::Amount;

pub const TOTAL_SUPPLY: Amount = 1_000_000_000;
pub const INITIAL_EMISSION_RATE_BPS: u16 = 2_000;
pub const LONG_TERM_TAIL_START_YEAR: u32 = 4;
pub const LONG_TERM_TAIL_EMISSION_RATE_BPS: u16 = 200;

pub const PLAYER_REWARD_ALLOCATION_BPS: u16 = 8_000;
pub const SERVICE_REWARD_ALLOCATION_BPS: u16 = 1_000;
pub const TREASURY_ALLOCATION_BPS: u16 = 500;
pub const TEAM_ALLOCATION_BPS: u16 = 300;
pub const EARLY_SUPPORTER_ALLOCATION_BPS: u16 = 200;
pub const HOURS_PER_YEAR: u64 = 24 * 365;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AllocationBreakdown {
    pub player_rewards: Amount,
    pub service_rewards: Amount,
    pub treasury: Amount,
    pub team: Amount,
    pub early_supporters: Amount,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnnualEmissionSchedule {
    pub year: u32,
    pub nominal_rate_bps: u16,
    pub annual_emission: Amount,
    pub cumulative_emission: Amount,
}

pub fn allocation_breakdown() -> AllocationBreakdown {
    AllocationBreakdown {
        player_rewards: proportional_amount(TOTAL_SUPPLY, PLAYER_REWARD_ALLOCATION_BPS),
        service_rewards: proportional_amount(TOTAL_SUPPLY, SERVICE_REWARD_ALLOCATION_BPS),
        treasury: proportional_amount(TOTAL_SUPPLY, TREASURY_ALLOCATION_BPS),
        team: proportional_amount(TOTAL_SUPPLY, TEAM_ALLOCATION_BPS),
        early_supporters: proportional_amount(TOTAL_SUPPLY, EARLY_SUPPORTER_ALLOCATION_BPS),
    }
}

pub fn annual_emission_rate_bps(year: u32) -> u16 {
    annual_emission_rate_bps_with_tail(
        year,
        LONG_TERM_TAIL_START_YEAR,
        LONG_TERM_TAIL_EMISSION_RATE_BPS,
    )
}

pub fn annual_emission_rate_bps_with_tail(
    year: u32,
    tail_start_year: u32,
    tail_emission_rate_bps: u16,
) -> u16 {
    if year >= tail_start_year.max(1) {
        return tail_emission_rate_bps;
    }
    let period_index = year.saturating_sub(1) / 2;
    let mut rate = u32::from(INITIAL_EMISSION_RATE_BPS);
    for _ in 0..period_index {
        rate /= 2;
        if rate == 0 {
            break;
        }
    }
    rate as u16
}

pub fn annual_emission_amount(year: u32) -> Amount {
    proportional_amount(TOTAL_SUPPLY, annual_emission_rate_bps(year))
}

pub fn annual_emission_amount_with_tail(
    year: u32,
    tail_start_year: u32,
    tail_emission_rate_bps: u16,
) -> Amount {
    proportional_amount(
        TOTAL_SUPPLY,
        annual_emission_rate_bps_with_tail(year, tail_start_year, tail_emission_rate_bps),
    )
}

pub fn cumulative_emission_amount(years: u32) -> Amount {
    (1..=years).map(annual_emission_amount).sum()
}

pub fn annual_emission_schedule(years: u32) -> Vec<AnnualEmissionSchedule> {
    annual_emission_schedule_with_tail(
        years,
        LONG_TERM_TAIL_START_YEAR,
        LONG_TERM_TAIL_EMISSION_RATE_BPS,
    )
}

pub fn annual_emission_schedule_with_tail(
    years: u32,
    tail_start_year: u32,
    tail_emission_rate_bps: u16,
) -> Vec<AnnualEmissionSchedule> {
    let mut cumulative = 0;
    let mut out = Vec::with_capacity(years as usize);
    for year in 1..=years {
        let annual_emission =
            annual_emission_amount_with_tail(year, tail_start_year, tail_emission_rate_bps);
        cumulative += annual_emission;
        out.push(AnnualEmissionSchedule {
            year,
            nominal_rate_bps: annual_emission_rate_bps_with_tail(
                year,
                tail_start_year,
                tail_emission_rate_bps,
            ),
            annual_emission,
            cumulative_emission: cumulative,
        });
    }
    out
}

pub fn annual_player_rewards_emission(year: u32) -> Amount {
    proportional_amount(annual_emission_amount(year), PLAYER_REWARD_ALLOCATION_BPS)
}

pub fn annual_player_rewards_emission_with_tail(
    year: u32,
    tail_start_year: u32,
    tail_emission_rate_bps: u16,
) -> Amount {
    proportional_amount(
        annual_emission_amount_with_tail(year, tail_start_year, tail_emission_rate_bps),
        PLAYER_REWARD_ALLOCATION_BPS,
    )
}

pub fn annual_service_rewards_emission(year: u32) -> Amount {
    proportional_amount(annual_emission_amount(year), SERVICE_REWARD_ALLOCATION_BPS)
}

pub fn annual_service_rewards_emission_with_tail(
    year: u32,
    tail_start_year: u32,
    tail_emission_rate_bps: u16,
) -> Amount {
    proportional_amount(
        annual_emission_amount_with_tail(year, tail_start_year, tail_emission_rate_bps),
        SERVICE_REWARD_ALLOCATION_BPS,
    )
}

pub fn base_player_reward_per_block(year: u32, reward_block_secs: u64) -> Amount {
    base_player_reward_per_block_with_tail(
        year,
        reward_block_secs,
        LONG_TERM_TAIL_START_YEAR,
        LONG_TERM_TAIL_EMISSION_RATE_BPS,
    )
}

pub fn base_player_reward_per_block_with_tail(
    year: u32,
    reward_block_secs: u64,
    tail_start_year: u32,
    tail_emission_rate_bps: u16,
) -> Amount {
    if reward_block_secs == 0 {
        return 0;
    }
    let annual_player_budget =
        annual_player_rewards_emission_with_tail(year, tail_start_year, tail_emission_rate_bps);
    let blocks_per_year = (u128::from(HOURS_PER_YEAR) * 3600) / u128::from(reward_block_secs);
    if blocks_per_year == 0 {
        return 0;
    }
    annual_player_budget / blocks_per_year
}

pub fn base_service_reward_per_block(year: u32, reward_block_secs: u64) -> Amount {
    base_service_reward_per_block_with_tail(
        year,
        reward_block_secs,
        LONG_TERM_TAIL_START_YEAR,
        LONG_TERM_TAIL_EMISSION_RATE_BPS,
    )
}

pub fn base_service_reward_per_block_with_tail(
    year: u32,
    reward_block_secs: u64,
    tail_start_year: u32,
    tail_emission_rate_bps: u16,
) -> Amount {
    if reward_block_secs == 0 {
        return 0;
    }
    let annual_service_budget =
        annual_service_rewards_emission_with_tail(year, tail_start_year, tail_emission_rate_bps);
    let blocks_per_year = (u128::from(HOURS_PER_YEAR) * 3600) / u128::from(reward_block_secs);
    if blocks_per_year == 0 {
        return 0;
    }
    annual_service_budget / blocks_per_year
}

fn proportional_amount(total: Amount, bps: u16) -> Amount {
    total.saturating_mul(Amount::from(bps)) / 10_000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocations_cover_total_supply() {
        let breakdown = allocation_breakdown();
        let total = breakdown.player_rewards
            + breakdown.service_rewards
            + breakdown.treasury
            + breakdown.team
            + breakdown.early_supporters;
        assert_eq!(total, TOTAL_SUPPLY);
    }

    #[test]
    fn emission_rate_halves_every_two_years() {
        assert_eq!(annual_emission_rate_bps(1), 2_000);
        assert_eq!(annual_emission_rate_bps(2), 2_000);
        assert_eq!(annual_emission_rate_bps(3), 1_000);
        assert_eq!(
            annual_emission_rate_bps(4),
            LONG_TERM_TAIL_EMISSION_RATE_BPS
        );
    }

    #[test]
    fn emission_rate_enters_non_zero_tail_floor_from_year_four() {
        assert_eq!(
            annual_emission_rate_bps(4),
            LONG_TERM_TAIL_EMISSION_RATE_BPS
        );
        assert_eq!(
            annual_emission_rate_bps(5),
            LONG_TERM_TAIL_EMISSION_RATE_BPS
        );
        assert_eq!(
            annual_emission_rate_bps(30),
            LONG_TERM_TAIL_EMISSION_RATE_BPS
        );
    }

    #[test]
    fn annual_schedule_tracks_cumulative_emissions() {
        let schedule = annual_emission_schedule(4);
        assert_eq!(schedule.len(), 4);
        assert_eq!(schedule[0].annual_emission, 200_000_000);
        assert_eq!(schedule[1].cumulative_emission, 400_000_000);
        assert_eq!(schedule[2].annual_emission, 100_000_000);
        assert_eq!(schedule[3].annual_emission, 20_000_000);
        assert_eq!(schedule[3].cumulative_emission, 520_000_000);
    }

    #[test]
    fn annual_schedule_includes_tail_emission_floor() {
        let schedule = annual_emission_schedule(5);
        assert_eq!(schedule.len(), 5);
        assert_eq!(schedule[3].year, 4);
        assert_eq!(
            schedule[3].nominal_rate_bps,
            LONG_TERM_TAIL_EMISSION_RATE_BPS
        );
        assert_eq!(schedule[3].annual_emission, 20_000_000);
        assert_eq!(schedule[4].cumulative_emission, 540_000_000);
    }

    #[test]
    fn player_reward_budget_and_hourly_block_reward_match_reference_curve() {
        assert_eq!(annual_player_rewards_emission(1), 160_000_000);
        assert_eq!(annual_player_rewards_emission(3), 80_000_000);
        assert_eq!(annual_player_rewards_emission(4), 16_000_000);
        assert_eq!(base_player_reward_per_block(1, 3_600), 18_264);
        assert_eq!(base_player_reward_per_block(3, 3_600), 9_132);
        assert_eq!(base_player_reward_per_block(4, 3_600), 1_826);
    }

    #[test]
    fn service_reward_budget_and_hourly_block_reward_match_reference_curve() {
        assert_eq!(annual_service_rewards_emission(1), 20_000_000);
        assert_eq!(annual_service_rewards_emission(3), 10_000_000);
        assert_eq!(annual_service_rewards_emission(4), 2_000_000);
        assert_eq!(base_service_reward_per_block(1, 3_600), 2_283);
        assert_eq!(base_service_reward_per_block(3, 3_600), 1_141);
        assert_eq!(base_service_reward_per_block(4, 3_600), 228);
    }

    #[test]
    fn configurable_tail_policy_supports_runtime_tuning() {
        assert_eq!(annual_emission_rate_bps_with_tail(4, 4, 180), 180);
        assert_eq!(annual_emission_rate_bps_with_tail(6, 6, 220), 220);
        assert_eq!(
            base_player_reward_per_block_with_tail(4, 3_600, 4, 180),
            1_643
        );
    }
}
