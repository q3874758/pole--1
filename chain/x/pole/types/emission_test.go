package types

import "testing"

func TestAnnualEmissionRateBpsMatchesReferenceCurve(t *testing.T) {
	cases := []struct {
		year uint32
		want uint16
	}{
		{1, 2000},
		{2, 2000},
		{3, 1000},
		{4, 200},
		{5, 200},
		{30, 200},
	}
	for _, c := range cases {
		if got := AnnualEmissionRateBps(c.year); got != c.want {
			t.Fatalf("AnnualEmissionRateBps(%d) = %d, want %d", c.year, got, c.want)
		}
	}
}

func TestAnnualEmissionAmountMatchesReferenceCurve(t *testing.T) {
	cases := []struct {
		year uint32
		want uint64
	}{
		{1, 200_000_000},
		{3, 100_000_000},
		{4, 20_000_000},
	}
	for _, c := range cases {
		if got := AnnualEmissionAmount(c.year); got != c.want {
			t.Fatalf("AnnualEmissionAmount(%d) = %d, want %d", c.year, got, c.want)
		}
	}
}

func TestAnnualAdjustedEmissionIsNeutralWhenWeightsMissing(t *testing.T) {
	if got := AnnualAdjustedEmission(1, 0, 25, 1000); got != 200_000_000 {
		t.Fatalf("target 0: got %d, want 200_000_000", got)
	}
	if got := AnnualAdjustedEmission(1, 25, 0, 1000); got != 200_000_000 {
		t.Fatalf("current 0: got %d, want 200_000_000", got)
	}
}

func TestAnnualAdjustedEmissionIsNeutralWhenWeightsEqual(t *testing.T) {
	if got := AnnualAdjustedEmission(1, 100_000, 100_000, 1000); got != 200_000_000 {
		t.Fatalf("equal weights: got %d, want 200_000_000", got)
	}
}

func TestAnnualAdjustedEmissionClampsToCap(t *testing.T) {
	// current far below target -> capped at +10% (220M).
	if got := AnnualAdjustedEmission(1, 100_000, 25_000, 1000); got != 220_000_000 {
		t.Fatalf("under-active: got %d, want 220_000_000", got)
	}
	// current far above target -> capped at -10% (180M).
	if got := AnnualAdjustedEmission(1, 25_000, 100_000, 1000); got != 180_000_000 {
		t.Fatalf("over-active: got %d, want 180_000_000", got)
	}
	// cap 0 -> no adjustment.
	if got := AnnualAdjustedEmission(1, 100_000, 25_000, 0); got != 200_000_000 {
		t.Fatalf("cap 0: got %d, want 200_000_000", got)
	}
}

func TestAnnualAdjustedEmissionTailYearAdjusts(t *testing.T) {
	// Year 4 tail base is 20M; -10% cap -> 18M.
	if got := AnnualAdjustedEmission(4, 25_000, 100_000, 1000); got != 18_000_000 {
		t.Fatalf("tail year: got %d, want 18_000_000", got)
	}
}
