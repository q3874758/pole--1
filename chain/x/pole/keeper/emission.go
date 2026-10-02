package keeper

import (
	"context"
	"encoding/json"
	"fmt"

	sdkmath "cosmossdk.io/math"

	sdk "github.com/cosmos/cosmos-sdk/types"

	"pole/chain/x/pole/types"
)

// annualEmissionState tracks the scheme-A issuance reference curve. The
// 30-day window remains for observability, while issuance itself has no
// monthly or annual hard ceiling. The state is stored as a JSON blob under
// a collections.Item[[]byte] so no proto regeneration is needed, and is
// fully re-derivable from genesis block time (not exported into GenesisState).
type annualEmissionState struct {
	YearIndex       uint64 `json:"year_index"`
	YearStartUnix   int64  `json:"year_start_unix"`
	MonthStartUnix  int64  `json:"month_start_unix"`
	LastMintUnix    int64  `json:"last_mint_unix"`
	MintedThisMonth uint64 `json:"minted_this_month"`
	PrevEpochWeight uint64 `json:"prev_epoch_weight"`
}

func defaultAnnualEmissionState(genesisUnix int64) annualEmissionState {
	return annualEmissionState{
		YearIndex:      1,
		YearStartUnix:  genesisUnix,
		MonthStartUnix: genesisUnix,
		LastMintUnix:   genesisUnix,
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

// BeginBlockAnnualEmission mints the scheme-A activity-linked reference
// rate into the module account proportionally to elapsed block time. The
// adjusted annual value is a rate-control curve, not an issuance ceiling;
// confirmed claims can also mint an exact shortfall on demand. It is the
// on-chain execution of the whitepaper emission curve (4.4) plus the
// activity adjustment anchored on TargetNetworkWeightUnits.
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

	// Advance the protocol year (360 days) and its monthly observation
	// periods (30 days) based on elapsed wall-clock time; period rollover
	// never gates issuance.
	if yearsElapsed := (blockUnix - st.YearStartUnix) / types.SecondsPerYear; yearsElapsed > 0 {
		st.YearIndex += uint64(yearsElapsed)
		st.YearStartUnix += yearsElapsed * types.SecondsPerYear
		st.MonthStartUnix = st.YearStartUnix
		st.MintedThisMonth = 0
	}
	if monthsElapsed := (blockUnix - st.MonthStartUnix) / types.SecondsPerMonth; monthsElapsed > 0 {
		st.MonthStartUnix += monthsElapsed * types.SecondsPerMonth
		st.MintedThisMonth = 0
	}

	// Refresh the activity signal from the latest finalized epoch.
	if latestWeight := k.latestFinalizedEpochWeight(ctx); latestWeight > 0 {
		st.PrevEpochWeight = latestWeight
	}

	yearlyBudget := types.AnnualAdjustedEmission(
		uint32(st.YearIndex),
		params.TargetNetworkWeightUnits,
		st.PrevEpochWeight,
		types.AnnualEmissionCapBps,
	)
	// Time-proportional reference rate. There is deliberately no monthly
	// quota or remaining-budget clamp: the activity-adjusted curve controls
	// unit economics, while confirmed claims remain payable without a hard
	// issuance ceiling. Limit one block's catch-up interval to one month so
	// a long clock jump does not create an accidental burst.
	elapsed := blockUnix - st.LastMintUnix
	if elapsed <= 0 {
		elapsed = 1
	}
	if elapsed > types.SecondsPerMonth {
		elapsed = types.SecondsPerMonth
	}
	mint := yearlyBudget * uint64(elapsed) / uint64(types.SecondsPerYear)

	if mint > 0 {
		coins := sdk.NewCoins(sdk.NewCoin(types.BaseDenom, sdkmath.NewIntFromUint64(mint)))
		if err := k.bankKeeper.MintCoins(ctx, types.ModuleName, coins); err != nil {
			return err
		}
		st.MintedThisMonth += mint
	}
	st.LastMintUnix = blockUnix

	return k.AnnualEmission.Set(ctx, st.marshal())
}
