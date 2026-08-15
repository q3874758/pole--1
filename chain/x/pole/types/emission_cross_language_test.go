package types

import "testing"

// Cross-language annual-emission fixtures (scheme A, 3.4).
//
// The exact same (year, target_weight, current_weight, cap_bps) rows are
// evaluated by the Rust off-chain layer
// (src/tokenomics.rs::tests::annual_emission_matches_chain_go_fixtures)
// and by the chain here via AnnualAdjustedEmission. Any drift in the
// nominal curve, the sqrt adjustment, the cap clamping, or integer
// rounding breaks both tests together.

func TestAnnualAdjustedEmissionCrossLanguageFixture(t *testing.T) {
	fixtures := []struct {
		year           uint32
		target, current uint64
		capBps         uint32
		want           uint64
	}{
		{1, 100_000, 100_000, 1_000, 200_000_000},
		{1, 100_000, 25_000, 1_000, 220_000_000},
		{1, 25_000, 100_000, 1_000, 180_000_000},
		{2, 150_000_000_000_000, 50, 1_000, 220_000_000},
		{3, 100_000, 100_000, 0, 100_000_000},
		{4, 25_000, 100_000, 1_000, 18_000_000},
		{5, 100_000, 25_000, 1_000, 22_000_000},
		{10, 400, 100, 1_000, 22_000_000},
	}
	for _, f := range fixtures {
		got := AnnualAdjustedEmission(f.year, f.target, f.current, f.capBps)
		if got != f.want {
			t.Fatalf(
				"year=%d target=%d current=%d cap=%d: got %d, want %d",
				f.year, f.target, f.current, f.capBps, got, f.want,
			)
		}
	}
}
