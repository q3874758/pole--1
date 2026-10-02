package types

import "testing"

func TestAllocateWitnessRewardsSplitsByCreditShare(t *testing.T) {
	allocation := AllocateWitnessRewards(map[string]uint64{
		"pole1witnessaaaaaaaaaaaaaaaaaaaaaaaaaaaaa1": 3,
		"pole1witnessbbbbbbbbbbbbbbbbbbbbbbbbbbbbb2": 1,
	}, 4000)

	if allocation["pole1witnessaaaaaaaaaaaaaaaaaaaaaaaaaaaaa1"] != 3000 {
		t.Fatalf("expected 3000 for the 3-credit witness, got %d", allocation["pole1witnessaaaaaaaaaaaaaaaaaaaaaaaaaaaaa1"])
	}
	if allocation["pole1witnessbbbbbbbbbbbbbbbbbbbbbbbbbbbbb2"] != 1000 {
		t.Fatalf("expected 1000 for the 1-credit witness, got %d", allocation["pole1witnessbbbbbbbbbbbbbbbbbbbbbbbbbbbbb2"])
	}
}

func TestAllocateWitnessRewardsIsExactAndDeterministic(t *testing.T) {
	credits := map[string]uint64{
		"pole1witnessaaaaaaaaaaaaaaaaaaaaaaaaaaaaa1": 2,
		"pole1witnessbbbbbbbbbbbbbbbbbbbbbbbbbbbbb2": 1,
		"pole1witnesscccccccccccccccccccccccccccc3": 1,
	}

	// 1001 over 4 credits does not divide evenly; the dust must still be
	// handed out so the pool is exhausted to the unit.
	allocation := AllocateWitnessRewards(credits, 1001)
	var total uint64
	for _, amount := range allocation {
		total += amount
	}
	if total != 1001 {
		t.Fatalf("expected the allocation to sum to the pool 1001, got %d", total)
	}
	// The largest remainder (the 2-credit witness) takes the extra unit.
	if allocation["pole1witnessaaaaaaaaaaaaaaaaaaaaaaaaaaaaa1"] != 501 {
		t.Fatalf("expected 501 for the 2-credit witness, got %d", allocation["pole1witnessaaaaaaaaaaaaaaaaaaaaaaaaaaaaa1"])
	}
	if allocation["pole1witnessbbbbbbbbbbbbbbbbbbbbbbbbbbbbb2"] != 250 ||
		allocation["pole1witnesscccccccccccccccccccccccccccc3"] != 250 {
		t.Fatalf("expected 250 each for the 1-credit witnesses, got %+v", allocation)
	}

	// Every node recomputing the split from the same credits must derive the
	// same amounts, otherwise the consensus-critical result would fork.
	for i := 0; i < 10; i++ {
		again := AllocateWitnessRewards(credits, 1001)
		for witness, amount := range allocation {
			if again[witness] != amount {
				t.Fatalf("allocation is not deterministic: %s saw %d then %d", witness, amount, again[witness])
			}
		}
	}
}

func TestAllocateWitnessRewardsHandlesDegenerateInputs(t *testing.T) {
	for _, pool := range AllocateWitnessRewards(map[string]uint64{"pole1nobodyaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa1": 4}, 0) {
		if pool != 0 {
			t.Fatalf("expected zero allocation for a zero pool, got %d", pool)
		}
	}
	for _, amount := range AllocateWitnessRewards(map[string]uint64{"pole1nobodyaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa1": 0}, 500) {
		if amount != 0 {
			t.Fatalf("expected zero allocation when no credits were earned, got %d", amount)
		}
	}
	if got := AllocateWitnessRewards(nil, 500); len(got) != 0 {
		t.Fatalf("expected an empty allocation for empty credits, got %+v", got)
	}
}
