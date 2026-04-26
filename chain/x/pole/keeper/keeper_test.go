package keeper

import (
	"bytes"
	"context"
	"testing"

	sdkmath "cosmossdk.io/math"
	"github.com/cosmos/cosmos-sdk/runtime"
	storetypes "github.com/cosmos/cosmos-sdk/store/v2/types"
	"github.com/cosmos/cosmos-sdk/testutil"
	sdk "github.com/cosmos/cosmos-sdk/types"
	authtypes "github.com/cosmos/cosmos-sdk/x/auth/types"
	stakingtypes "github.com/cosmos/cosmos-sdk/x/staking/types"

	"pole/chain/x/pole/types"
)

func TestKeeperInitAndExportGenesis(t *testing.T) {
	storeKey := storetypes.NewKVStoreKey(types.StoreKey)
	transientKey := storetypes.NewTransientStoreKey("transient-test")
	ctx := testutil.DefaultContext(storeKey, transientKey)

	k, err := NewKeeper(runtime.NewKVStoreService(storeKey), authtypes.NewModuleAddress("gov").String())
	if err != nil {
		t.Fatalf("new keeper: %v", err)
	}

	genesis := &types.GenesisState{
		Params: func() *types.Params {
			params := types.DefaultParams()
			return &params
		}(),
		BatchCommits: []*types.BatchCommit{{
			EpochId:          1,
			CollectorAddress: "cosmos1collector0000000000000000000000000000000",
			SlotStart:        1,
			SlotEnd:          1,
			Batch: &types.MerkleCommitment{
				Root:      "batch-root",
				LeafCount: 1,
			},
			PayloadCid:       "cid://batch-1",
			ObservationCount: 1,
		}},
		EpochCommits: []*types.EpochCommit{{
			EpochId:                 1,
			ProposerAddress:         "cosmos1proposer000000000000000000000000000000",
			ChallengeOpenHeight:     10,
			ChallengeDeadlineHeight: 20,
		}},
		RewardRecords: []*types.RewardRecord{{
			EpochId:      1,
			Recipient:    "cosmos1reward000000000000000000000000000000000",
			NetReward:    123,
			PlayerReward: 123,
		}},
		Challenges: []*types.Challenge{{
			ChallengeIdHex: "challenge-1",
			EpochId:        1,
			DeadlineHeight: 20,
			State:          types.ChallengeStateOpen,
		}},
		Availability: []*types.AvailabilityRecord{{
			EpochId:             1,
			OperatorAddress:     "cosmos1operator000000000000000000000000000000",
			PayloadCid:          "cid://batch-1",
			RetentionUntilEpoch: 3,
		}},
		GameWeightEntries: []*types.GameWeightEntry{{
			AppId:                730,
			GameWeightPpm:        1_000_000,
			Tier:                 "tier1",
			EffectiveFromEpochId: 1,
		}},
	}

	if err := k.InitGenesis(ctx, genesis); err != nil {
		t.Fatalf("init genesis: %v", err)
	}

	exported, err := k.ExportGenesis(ctx)
	if err != nil {
		t.Fatalf("export genesis: %v", err)
	}

	if exported.Params.BaseHourlyReward != genesis.Params.BaseHourlyReward {
		t.Fatalf("expected params to round-trip, got %d", exported.Params.BaseHourlyReward)
	}
	if len(exported.BatchCommits) != 1 || exported.BatchCommits[0].PayloadCid != "cid://batch-1" {
		t.Fatalf("expected batch commit to round-trip")
	}
	if len(exported.EpochCommits) != 1 || exported.EpochCommits[0].EpochId != 1 {
		t.Fatalf("expected epoch commit to round-trip")
	}
	if len(exported.RewardRecords) != 1 || exported.RewardRecords[0].NetReward != 123 {
		t.Fatalf("expected reward record to round-trip")
	}
	if len(exported.Challenges) != 1 || exported.Challenges[0].ChallengeIdHex != "challenge-1" {
		t.Fatalf("expected challenge to round-trip")
	}
	if len(exported.Availability) != 1 || exported.Availability[0].PayloadCid != "cid://batch-1" {
		t.Fatalf("expected availability record to round-trip")
	}
	if len(exported.GameWeightEntries) != 1 || exported.GameWeightEntries[0].AppId != 730 {
		t.Fatalf("expected game weight entry to round-trip")
	}
}

func TestApplyValidatorSlashUsesConfiguredKeepers(t *testing.T) {
	storeKey := storetypes.NewKVStoreKey(types.StoreKey)
	transientKey := storetypes.NewTransientStoreKey("transient-test")
	ctx := testutil.DefaultContext(storeKey, transientKey)

	k, err := NewKeeper(runtime.NewKVStoreService(storeKey), authtypes.NewModuleAddress("gov").String())
	if err != nil {
		t.Fatalf("new keeper: %v", err)
	}
	consAddr := sdk.ConsAddress(bytes.Repeat([]byte{9}, 20))
	fakeSlash := &fakeSlashingKeeper{}
	fakeStake := fakeStakingKeeper{validator: stakingtypes.Validator{Tokens: sdkmath.NewInt(5_000_000), Status: stakingtypes.Bonded}}
	k = k.WithStakeSlashKeepers(fakeStake, fakeSlash)

	err = k.ApplyValidatorSlash(ctx, consAddr.String(), 250, true)
	if err != nil {
		t.Fatalf("apply validator slash: %v", err)
	}
	if !fakeSlash.jailed {
		t.Fatalf("expected validator to be jailed")
	}
	if fakeSlash.power <= 0 {
		t.Fatalf("expected positive validator power")
	}
	if fakeSlash.consAddr.String() != consAddr.String() {
		t.Fatalf("expected slash to target %s, got %s", consAddr.String(), fakeSlash.consAddr.String())
	}
	if !fakeSlash.fraction.Equal(sdkmath.LegacyNewDec(250).Quo(sdkmath.LegacyNewDec(10_000))) {
		t.Fatalf("unexpected slash fraction %s", fakeSlash.fraction.String())
	}
}

type fakeStakingKeeper struct {
	validator stakingtypes.Validator
}

func (f fakeStakingKeeper) GetValidatorByConsAddr(context.Context, sdk.ConsAddress) (stakingtypes.Validator, error) {
	return f.validator, nil
}

type fakeSlashingKeeper struct {
	consAddr sdk.ConsAddress
	fraction sdkmath.LegacyDec
	power    int64
	jailed   bool
}

func (f *fakeSlashingKeeper) Slash(_ context.Context, consAddr sdk.ConsAddress, fraction sdkmath.LegacyDec, power, _ int64) error {
	f.consAddr = consAddr
	f.fraction = fraction
	f.power = power
	return nil
}

func (f *fakeSlashingKeeper) Jail(_ context.Context, consAddr sdk.ConsAddress) error {
	f.consAddr = consAddr
	f.jailed = true
	return nil
}
