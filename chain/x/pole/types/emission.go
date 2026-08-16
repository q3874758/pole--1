package types

// Scheme A: activity-linked annual emission.
//
// Annual nominal issuance follows the same reference curve as the Rust
// tokenomics layer (20% initial, halving every two years, entering a
// low non-zero tail from year 4). The actual annual budget is then scaled
// by the network-activity factor sqrt(target / current), clamped to
// +/- capBps — the exact adjustment already implemented by
// AdjustedHourlyReward. Keeping the constants and math here identical to
// `src/tokenomics.rs` is what makes the Rust↔Go cross-language fixtures
// (3.4) hold.

const (
	// TotalSupplyAmount is 1,000,000,000 tokens (micro-denom units),
	// mirroring Rust `TOTAL_SUPPLY`.
	TotalSupplyAmount uint64 = 1_000_000_000
	// InitialEmissionRateBps is the year 1-2 nominal emission rate (20%).
	InitialEmissionRateBps uint16 = 2_000
	// TailStartYear is the first year of the tail regime.
	TailStartYear uint32 = 4
	// TailEmissionRateBps is the long-term tail rate (2%).
	TailEmissionRateBps uint16 = 200
	// SecondsPerMonth is the scheme-A budget settlement period (30 days).
	SecondsPerMonth int64 = 30 * 24 * 3600
	// PeriodsPerYear is the number of monthly periods per protocol year.
	PeriodsPerYear uint64 = 12
	// SecondsPerYear is a 360-day protocol year (12 × 30-day periods), the
	// year the nominal emission curve is indexed on.
	SecondsPerYear int64 = SecondsPerMonth * int64(PeriodsPerYear)
	// AnnualEmissionCapBps is the scheme-A yearly issuance adjustment cap
	// (10%), a protocol constant per the confirmed parameters — anchored on
	// the existing TargetNetworkWeightUnits governance parameter only.
	AnnualEmissionCapBps uint32 = 1_000
)

// AnnualEmissionRateBps returns the nominal emission rate (bps of total
// supply) for a protocol year: 20% for years 1-2, halving every two
// years, then the tail rate from TailStartYear. Identical to Rust
// `annual_emission_rate_bps_with_tail`.
func AnnualEmissionRateBps(year uint32) uint16 {
	if year >= TailStartYear {
		return TailEmissionRateBps
	}
	periodIndex := (year - 1) / 2
	rate := uint32(InitialEmissionRateBps)
	for i := uint32(0); i < periodIndex; i++ {
		rate /= 2
		if rate == 0 {
			break
		}
	}
	return uint16(rate)
}

// AnnualEmissionAmount returns the nominal annual issuance
// (TotalSupplyAmount × rate / 10000). Identical to Rust
// `annual_emission_amount`.
func AnnualEmissionAmount(year uint32) uint64 {
	return mulDiv(TotalSupplyAmount, uint64(AnnualEmissionRateBps(year)), basisPointsDivisor)
}

// AnnualAdjustedEmission returns the scheme-A activity-linked annual
// budget: nominal annual issuance scaled by sqrt(target/current) and
// clamped to ±capBps. It reuses AdjustedHourlyReward — the same
// adjustment the reward path uses — so the chain has exactly one
// adjustment implementation and ComputeAdjustedHourlyReward gains a real
// call site. Identical to Rust `annual_emission`.
func AnnualAdjustedEmission(year uint32, targetWeight, currentWeight uint64, capBps uint32) uint64 {
	return AdjustedHourlyReward(AnnualEmissionAmount(year), targetWeight, currentWeight, capBps)
}
