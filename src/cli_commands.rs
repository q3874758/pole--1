//! Shared CLI command helpers used by multiple binaries (`pole-client`,
//! `pole-node`, …) so that identical command-handler logic is not
//! copy-pasted across bin crates.  Command names, arguments, and
//! printed output above must stay byte-for-byte identical to what the
//! individual binaries previously produced.

use std::fmt::Write as _;

/// Print the merkle roots of a built epoch commit artifact, one per
/// line.  Shared verbatim by `pole-client` and `pole-node` (previously
/// duplicated in both binaries byte-for-byte).
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
/// only the leading title line ("PoLE tokenomics" vs "PoLE node
/// tokenomics") differs and is passed in as `title_line`.
pub fn render_tokenomics_schedule(
    title_line: &str,
    breakdown: &crate::tokenomics::AllocationBreakdown,
    schedule: &[crate::tokenomics::AnnualEmissionSchedule],
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{title_line}");
    let _ = writeln!(out, "{}", format_args!("total_supply={}", crate::tokenomics::TOTAL_SUPPLY));
    let _ = writeln!(out, "initial_emission_rate_bps={}", crate::tokenomics::INITIAL_EMISSION_RATE_BPS);
    let _ = writeln!(out, "tail_emission_start_year={}", crate::tokenomics::LONG_TERM_TAIL_START_YEAR);
    let _ = writeln!(out, "tail_emission_rate_bps={}", crate::tokenomics::LONG_TERM_TAIL_EMISSION_RATE_BPS);
    let _ = writeln!(out, "player_rewards_allocation={}", breakdown.player_rewards);
    let _ = writeln!(out, "service_rewards_allocation={}", breakdown.service_rewards);
    let _ = writeln!(out, "treasury_allocation={}", breakdown.treasury);
    let _ = writeln!(out, "team_allocation={}", breakdown.team);
    let _ = writeln!(out, "early_supporters_allocation={}", breakdown.early_supporters);
    for row in schedule {
        let _ = writeln!(
            out,
            "year={} nominal_rate_bps={} annual_emission={} cumulative_emission={}",
            row.year, row.nominal_rate_bps, row.annual_emission, row.cumulative_emission
        );
    }
    out
}
