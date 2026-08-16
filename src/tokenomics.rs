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

/// 方案 A：年度发行活跃度调节的默认 cap（10%）。
pub const ANNUAL_EMISSION_ADJUSTMENT_CAP_BPS: u16 = 1_000;

/// Integer square root (floor), Newton's method. Shared by the tokenomics
/// activity adjustment and the network-weight reward adjustment.
pub(crate) fn integer_sqrt(value: Amount) -> Amount {
    if value < 2 {
        return value;
    }
    let mut x0 = value;
    let mut x1 = (x0 + value / x0) / 2;
    while x1 < x0 {
        x0 = x1;
        x1 = (x0 + value / x0) / 2;
    }
    x0
}

/// 方案 A 活跃度调节因子（ppm，`1_000_000` = 1.0）：
/// `sqrt(锚点 / 实际活跃度)`，截断到 `[1 - cap, 1 + cap]`。
///
/// 与链上 `AdjustedHourlyReward` / 链下 `adjusted_player_block_reward`
/// 同一公式方向：网络实际活跃度低于锚点时上调发行（激励补足），
/// 高于锚点时下调，单向最大偏差受 `cap_bps` 约束（默认 10%）。
/// 锚点或实际权重为 0 时返回 1.0（不调节）。
pub fn annual_emission_activity_factor(
    target_network_weight_units: Amount,
    current_network_weight_units: Amount,
    cap_bps: u16,
) -> Amount {
    if target_network_weight_units == 0 || current_network_weight_units == 0 {
        return 1_000_000;
    }
    let cap_ppm = Amount::from(cap_bps.min(10_000)) * 100;
    let lower = 1_000_000u128.saturating_sub(cap_ppm);
    let upper = 1_000_000u128.saturating_add(cap_ppm);
    let scaled_ratio = target_network_weight_units.saturating_mul(1_000_000_000_000u128)
        / current_network_weight_units;
    integer_sqrt(scaled_ratio).clamp(lower, upper)
}

/// 方案 A：年度发行 = 基准名义发行 × 活跃度调节因子（sqrt + cap）。
pub fn annual_emission(
    year: u32,
    target_network_weight_units: Amount,
    current_network_weight_units: Amount,
    cap_bps: u16,
) -> Amount {
    let base = annual_emission_amount(year);
    let factor = annual_emission_activity_factor(
        target_network_weight_units,
        current_network_weight_units,
        cap_bps,
    );
    base.saturating_mul(factor) / 1_000_000
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

    #[test]
    fn annual_emission_activity_factor_is_neutral_when_weights_missing() {
        assert_eq!(annual_emission_activity_factor(0, 100, 1_000), 1_000_000);
        assert_eq!(annual_emission_activity_factor(100, 0, 1_000), 1_000_000);
        assert_eq!(annual_emission_activity_factor(0, 0, 1_000), 1_000_000);
    }

    #[test]
    fn annual_emission_activity_factor_is_one_when_weights_equal() {
        assert_eq!(
            annual_emission_activity_factor(1_000, 1_000, 1_000),
            1_000_000
        );
        assert_eq!(
            annual_emission_activity_factor(7_777, 7_777, 500),
            1_000_000
        );
    }

    #[test]
    fn annual_emission_activity_factor_clamps_to_cap() {
        // current 远低于 target → 上调被 cap 截断（10%）。
        assert_eq!(
            annual_emission_activity_factor(100_000, 100, 1_000),
            1_100_000
        );
        // current 远高于 target → 下调被 cap 截断。
        assert_eq!(
            annual_emission_activity_factor(100, 100_000, 1_000),
            900_000
        );
        // cap = 0 → 恒 1.0。
        assert_eq!(annual_emission_activity_factor(100_000, 100, 0), 1_000_000);
    }

    #[test]
    fn annual_emission_scales_with_activity_and_respects_cap() {
        // 基准：第 1 年 200_000_000；权重相等 → 无调节。
        assert_eq!(annual_emission(1, 100_000, 100_000, 1_000), 200_000_000);
        // 活跃不足（current = target/4）→ 上调至 cap 上限：220M。
        assert_eq!(annual_emission(1, 100_000, 25_000, 1_000), 220_000_000);
        // 活跃过剩（current = 4×target）→ 下调至 cap 下限：180M。
        assert_eq!(annual_emission(1, 25_000, 100_000, 1_000), 180_000_000);
        // 第 4 年 tail：基准 20M，cap 下调 10% → 18M。
        assert_eq!(annual_emission(4, 25_000, 100_000, 1_000), 18_000_000);
    }

    #[test]
    fn annual_emission_matches_integer_sqrt_reference() {
        // current = target/4 → factor = sqrt(4e12) = 2_000_000，clamp 1_100_000。
        assert_eq!(
            annual_emission_activity_factor(4_000, 1_000, 10_000),
            2_000_000
        );
        // current = target → factor 1.0。
        assert_eq!(
            annual_emission_activity_factor(4_000, 4_000, 10_000),
            1_000_000
        );
    }

    /// Cross-language fixtures shared with
    /// `chain/x/pole/types/emission_cross_language_test.go`: the same
    /// (year, target, current, cap) rows must produce identical annual
    /// emissions on both sides (scheme A, 3.4).
    #[test]
    fn annual_emission_matches_chain_go_fixtures() {
        let fixtures = [
            (1u32, 100_000u128, 100_000u128, 1_000u16, 200_000_000u128),
            (1, 100_000, 25_000, 1_000, 220_000_000),
            (1, 25_000, 100_000, 1_000, 180_000_000),
            (2, 150_000_000_000_000, 50, 1_000, 220_000_000),
            (3, 100_000, 100_000, 0, 100_000_000),
            (4, 25_000, 100_000, 1_000, 18_000_000),
            (5, 100_000, 25_000, 1_000, 22_000_000),
            (10, 400, 100, 1_000, 22_000_000),
        ];
        for (year, target, current, cap, want) in fixtures {
            let got = annual_emission(year, target, current, cap);
            assert_eq!(
                got, want,
                "year={year} target={target} current={current} cap={cap}"
            );
        }
    }
}
