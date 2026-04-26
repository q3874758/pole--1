package types

import "testing"

func TestComputePlayerHourWeight(t *testing.T) {
	weight := ComputePlayerHourWeight(3600, 1_000_000)
	if weight != 3_600_000_000 {
		t.Fatalf("expected full-weight hour to equal 3600000000, got %d", weight)
	}
}

func TestComputePlayerHourReward(t *testing.T) {
	reward := ComputePlayerHourReward(1000, 60, 132)
	if reward != 454 {
		t.Fatalf("expected truncated proportional reward 454, got %d", reward)
	}
}

func TestAdjustedHourlyRewardReturnsBaseWhenTargetOrCurrentIsZero(t *testing.T) {
	adjusted := AdjustedHourlyReward(1000, 0, 25, 2000)
	if adjusted != 1000 {
		t.Fatalf("expected base reward when target weight is zero, got %d", adjusted)
	}

	adjusted = AdjustedHourlyReward(1000, 25, 0, 2000)
	if adjusted != 1000 {
		t.Fatalf("expected base reward when previous weight is zero, got %d", adjusted)
	}
}

func TestAdjustedHourlyRewardClampsUpwardToCap(t *testing.T) {
	adjusted := AdjustedHourlyReward(1000, 400, 100, 2000)
	if adjusted != 1200 {
		t.Fatalf("expected upward adjustment to clamp at 1200, got %d", adjusted)
	}
}

func TestAdjustedHourlyRewardClampsDownwardToCap(t *testing.T) {
	adjusted := AdjustedHourlyReward(1000, 100, 400, 2000)
	if adjusted != 800 {
		t.Fatalf("expected downward adjustment to clamp at 800, got %d", adjusted)
	}
}
