package keeper

import (
	"context"
	"encoding/json"
	"fmt"

	sdkmath "cosmossdk.io/math"

	sdk "github.com/cosmos/cosmos-sdk/types"

	"pole/chain/x/pole/types"
)

// annualEmissionState tracks the scheme-A yearly issuance budget. It is
// stored as a JSON blob under a collections.Item[[]byte] so no proto
// regeneration is needed. The state is fully re-derivable from genesis
// block time, so it is not exported into GenesisState.
type annualEmissionState struct {
	YearIndex      uint64 `json:"year_index"`
	YearStartUnix  int64  `json:"year_start_unix"`
	LastMintUnix   int64  `json:"last_mint_unix"`
	MintedThisYear uint64 `json:"minted_this_year"`
	PrevEpochWeight uint64 `json:"prev_epoch_weight"`
}

func defaultAnnualEmissionState(genesisUnix int64) annualEmissionState {
	return annualEmissionState{
		YearIndex:     1,
		YearStartUnix: genesisUnix,
		LastMintUnix:  genesisUnix,
	}
}

func (s annualEmissionState) marshal() []byte {
	bz, err := json.Marshal(s)
	if err != nil {
		panic(fmt.Sprintf("marshal annual emission state: %v", err))
	}
	return bz
}

func unmarshalAnnualEmissionState(bz []byte) (annualEmissionState, error) {
	var st annualEmissionState
	if err := json.Unmarshal(bz, &st); err != nil {
		return st, err
	}
	return st, nil
}

// InitAnnualEmission seeds the emission state at genesis block time.
func (k Keeper) InitAnnualEmission(ctx context.Context) error {
	genesisUnix := sdk.UnwrapSDKContext(ctx).BlockTime().Unix()
	return k.AnnualEmission.Set(ctx, defaultAnnualEmissionState(genesisUnix).marshal())
}

func (k Keeper) loadAnnualEmission(ctx context.Context) (annualEmissionState, error) {
	bz, err := k.AnnualEmission.Get(ctx)
	if err != nil {
		return annualEmissionState{}, err
	}
	return unmarshalAnnualEmissionState(bz)
}

// latestFinalizedEpochWeight returns the total network weight units of
// the most recently finalized epoch, or 0 when none has been finalized.
// This is the on-chain activity signal for the scheme-A adjustment.
func (k Keeper) latestFinalizedEpochWeight(ctx context.Context) uint64 {
	var bestEpoch uint64
	var bestWeight uint64
	iter, err := k.EpochCommits.Iterate(ctx, nil)
	if err != nil {
		return 0
	}
	defer iter.Close()
	for ; iter.Valid(); iter.Next() {
		kv, err := iter.KeyValue()
		if err != nil {
			continue
		}
		if kv.Value.Finalized && kv.Value.EpochId >= bestEpoch {
			bestEpoch = kv.Value.EpochId
			bestWeight = kv.Value.TotalNetworkWeightUnits
		}
	}
	return bestWeight
}

// BeginBlockAnnualEmission mints the scheme-A activity-linked annual
// budget into the module account proportionally to elapsed block time,
// honoring the per-year cap. It is the on-chain execution of the
// whitepaper emission curve (4.4) plus the activity adjustment anchored
// on TargetNetworkWeightUnits with a ±capBps bound.
func (k Keeper) BeginBlockAnnualEmission(ctx context.Context) error {
	if k.bankKeeper == nil {
		return fmt.Errorf("bank keeper is not configured")
	}
	sdkCtx := sdk.UnwrapSDKContext(ctx)
	blockUnix := sdkCtx.BlockTime().Unix()

	st, err := k.loadAnnualEmission(ctx)
	if err != nil {
		return err
	}
	params, err := k.GetParams(ctx)
	if err != nil {
		return err
	}

	// Advance the protocol year based on elapsed wall-clock time.
	if yearsElapsed := (blockUnix - st.YearStartUnix) / types.SecondsPerYear; yearsElapsed > 0 {
		st.YearIndex += uint64(yearsElapsed)
		st.YearStartUnix += yearsElapsed * types.SecondsPerYear
		st.MintedThisYear = 0
	}

	// Refresh the activity signal from the latest finalized epoch.
	if latestWeight := k.latestFinalizedEpochWeight(ctx); latestWeight > 0 {
		st.PrevEpochWeight = latestWeight
	}

	budget := types.AnnualAdjustedEmission(
		uint32(st.YearIndex),
		params.TargetNetworkWeightUnits,
		st.PrevEpochWeight,
		types.AnnualEmissionCapBps,
	)

	// Time-proportional share of the year's budget, never exceeding the
	// remaining yearly quota. A clock jump mints at most one year's share.
	elapsed := blockUnix - st.LastMintUnix
	if elapsed <= 0 {
		elapsed = 1
	}
	if elapsed > types.SecondsPerYear {
		elapsed = types.SecondsPerYear
	}
	var mint uint64
	if st.MintedThisYear >= budget {
		mint = 0
	} else {
		mint = budget * uint64(elapsed) / uint64(types.SecondsPerYear)
	}
	if remaining := budget - st.MintedThisYear; mint > remaining {
		mint = remaining
	}

	if mint > 0 {
		coins := sdk.NewCoins(sdk.NewCoin(types.BaseDenom, sdkmath.NewIntFromUint64(mint)))
		if err := k.bankKeeper.MintCoins(ctx, types.ModuleName, coins); err != nil {
			return err
		}
		st.MintedThisYear += mint
	}
	st.LastMintUnix = blockUnix

	return k.AnnualEmission.Set(ctx, st.marshal())
}
