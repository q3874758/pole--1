package types

import (
	"math/big"
	"sort"
)

// AllocateWitnessRewards splits a witness reward pool across witnesses in
// proportion to the number of independent attestations each one had adopted
// for the epoch.
//
// Credits come from the keeper's WitnessCreditsForEpoch, which counts only
// attestations attached to settled *valid* sessions. A node therefore earns
// verify reward for the corroboration it actually delivered, not for running
// with the verify capability enabled.
//
// The split is exact: the returned amounts sum to pool whenever pool > 0 and
// the credits are non-empty. Fractional shares are resolved by the largest
// remainder method, with the witness address as the deterministic tie-break,
// so every node derives an identical allocation from identical credits — the
// property a consensus-critical split requires.
func AllocateWitnessRewards(credits map[string]uint64, pool uint64) map[string]uint64 {
	allocation := make(map[string]uint64, len(credits))
	if pool == 0 {
		for witness := range credits {
			allocation[witness] = 0
		}
		return allocation
	}

	var totalCredits uint64
	for _, credit := range credits {
		totalCredits += credit
	}
	if totalCredits == 0 {
		for witness := range credits {
			allocation[witness] = 0
		}
		return allocation
	}

	witnesses := make([]string, 0, len(credits))
	for witness, credit := range credits {
		if credit == 0 {
			allocation[witness] = 0
			continue
		}
		witnesses = append(witnesses, witness)
	}
	sort.Strings(witnesses)

	type remainder struct {
		witness string
		value   *big.Int
	}
	remainders := make([]remainder, 0, len(witnesses))
	poolBig := new(big.Int).SetUint64(pool)
	totalBig := new(big.Int).SetUint64(totalCredits)

	var distributed uint64
	for _, witness := range witnesses {
		product := new(big.Int).Mul(poolBig, new(big.Int).SetUint64(credits[witness]))
		share := new(big.Int).Div(product, totalBig)
		whole := share.Uint64()
		allocation[witness] = whole
		distributed += whole

		rest := new(big.Int).Sub(product, new(big.Int).Mul(share, totalBig))
		remainders = append(remainders, remainder{witness: witness, value: rest})
	}

	// The floor above loses less than one unit per witness, so the dust is
	// always smaller than the number of witnesses: handing one unit each to
	// the largest remainders both exhausts the pool exactly and never runs
	// off the end of the slice.
	sort.SliceStable(remainders, func(i, j int) bool {
		if cmp := remainders[i].value.Cmp(remainders[j].value); cmp != 0 {
			return cmp > 0
		}
		return remainders[i].witness < remainders[j].witness
	})
	for i := 0; distributed < pool && i < len(remainders); i++ {
		allocation[remainders[i].witness]++
		distributed++
	}
	return allocation
}
