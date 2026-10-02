package app

import (
	"bytes"
	"strings"
	"testing"
	"time"

	"cosmossdk.io/log/v2"

	sdkmath "cosmossdk.io/math"
	cmtabci "github.com/cometbft/cometbft/abci/types"
	storetypes "github.com/cosmos/cosmos-sdk/store/v2/types"
	"github.com/cosmos/cosmos-sdk/testutil"
	sdk "github.com/cosmos/cosmos-sdk/types"
	authtypes "github.com/cosmos/cosmos-sdk/x/auth/types"
	banktypes "github.com/cosmos/cosmos-sdk/x/bank/types"
	epochstypes "github.com/cosmos/cosmos-sdk/x/epochs/types"
	govtypes "github.com/cosmos/cosmos-sdk/x/gov/types"
	slashingtypes "github.com/cosmos/cosmos-sdk/x/slashing/types"
	stakingtypes "github.com/cosmos/cosmos-sdk/x/staking/types"

	"pole/chain/x/pole/types"
)

func TestNewAppInitializesPoleModule(t *testing.T) {
	app, err := NewMem(log.NewNopLogger())
	if err != nil {
		t.Fatalf("new mem app: %v", err)
	}

	if app.ModuleManager == nil {
		t.Fatalf("expected module manager to be initialized")
	}

	ctx := testutil.DefaultContextWithKeys(
		app.KVStoreKeys(),
		map[string]*storetypes.TransientStoreKey{},
		map[string]*storetypes.MemoryStoreKey{},
	)
	if _, err := app.InitChainer(ctx, &cmtabci.RequestInitChain{}); err != nil {
		t.Fatalf("init chainer: %v", err)
	}

	params, err := app.PoleKeeper.GetParams(ctx)
	if err != nil {
		t.Fatalf("get params: %v", err)
	}
	if params.BaseHourlyReward != types.DefaultParams().BaseHourlyReward {
		t.Fatalf("expected default params to be initialized")
	}

	expectedModules := []string{
		authtypes.ModuleName,
		banktypes.ModuleName,
		stakingtypes.ModuleName,
		slashingtypes.ModuleName,
		govtypes.ModuleName,
		epochstypes.ModuleName,
		types.ModuleName,
	}
	for _, moduleName := range expectedModules {
		if _, ok := app.ModuleManager.Modules[moduleName]; !ok {
			t.Fatalf("expected module %s to be registered", moduleName)
		}
	}

	if app.MsgServiceRouter().Handler(&types.MsgUpdateParams{}) == nil {
		t.Fatalf("expected x/pole msg service handler to be registered")
	}
}

func TestClaimRewardMintsTransfersAndMarksClaimed(t *testing.T) {
	app := initTestApp(t)
	ctx := initTestContext(app).WithBlockHeight(100).WithBlockTime(time.Unix(1_700_000_000, 0))

	recipientAddr := sdk.AccAddress(bytes.Repeat([]byte{1}, 20))
	recipient, err := app.AccountKeeper.AddressCodec().BytesToString(recipientAddr)
	if err != nil {
		t.Fatalf("recipient bech32: %v", err)
	}
	app.AccountKeeper.SetAccount(ctx, app.AccountKeeper.NewAccountWithAddress(ctx, recipientAddr))

	if err := app.PoleKeeper.SetEpochCommit(ctx, types.EpochCommit{
		EpochId:                 1,
		ProposerAddress:         recipient,
		ChallengeOpenHeight:     1,
		ChallengeDeadlineHeight: 10,
		Rewards:                 &types.MerkleCommitment{Root: testCommitmentRoot(t, []types.RewardRecord{{EpochId: 1, Recipient: recipient, PlayerReward: 50, NetReward: 50}}), LeafCount: 1},
		Aggregates:              &types.MerkleCommitment{Root: testCommitmentRoot(t, []types.AggregateRecord{{EpochId: 1, AppId: 730, TotalWeightUnits: 50, PlayerCount: 1}}), LeafCount: 1},
		TotalNetworkWeightUnits: 50,
	}); err != nil {
		t.Fatalf("set epoch commit: %v", err)
	}
	if err := app.PoleKeeper.SetRewardRecord(ctx, types.RewardRecord{
		EpochId:      1,
		Recipient:    recipient,
		PlayerReward: 50,
		NetReward:    50,
	}); err != nil {
		t.Fatalf("set reward record: %v", err)
	}
	if err := app.PoleKeeper.SetAggregateRecord(ctx, types.AggregateRecord{EpochId: 1, AppId: 730, TotalWeightUnits: 50, PlayerCount: 1}); err != nil {
		t.Fatalf("set aggregate record: %v", err)
	}

	msgServer := app.MsgServiceRouter().Handler(&types.MsgClaimReward{})
	if msgServer == nil {
		t.Fatalf("expected claim reward handler")
	}
	finalizeHandler := app.MsgServiceRouter().Handler(&types.MsgFinalizeEpoch{})
	if finalizeHandler == nil {
		t.Fatalf("expected finalize epoch handler")
	}
	seedVerificationCoverage(t, app, ctx, 1)
	_, err = finalizeHandler(ctx, &types.MsgFinalizeEpoch{Finalizer: recipient, EpochId: 1})
	if err != nil {
		t.Fatalf("finalize epoch: %v", err)
	}
	// Seed the scheme-A reference curve; claims also mint an exact shortfall
	// when this pool is insufficient.
	if err := app.PoleKeeper.BeginBlockAnnualEmission(ctx); err != nil {
		t.Fatalf("begin block annual emission: %v", err)
	}
	_, err = msgServer(ctx, &types.MsgClaimReward{Claimer: recipient, EpochId: 1, Recipient: recipient})
	if err != nil {
		t.Fatalf("claim reward: %v", err)
	}

	balance := app.BankKeeper.GetBalance(ctx, recipientAddr, types.BaseDenom)
	if !balance.Amount.Equal(sdkmath.NewIntFromUint64(50)) {
		t.Fatalf("expected reward payout balance 50, got %s", balance.Amount.String())
	}
	claim, err := app.PoleKeeper.GetClaimedReward(ctx, 1, recipient)
	if err != nil {
		t.Fatalf("claimed reward record: %v", err)
	}
	if claim.Amount != 50 {
		t.Fatalf("expected claimed reward amount 50, got %d", claim.Amount)
	}
	commit, err := app.PoleKeeper.GetEpochCommit(ctx, 1)
	if err != nil {
		t.Fatalf("epoch commit after claim: %v", err)
	}
	if !commit.Finalized {
		t.Fatalf("expected finalized epoch to stay finalized")
	}
}

func TestBeginBlockAnnualEmissionMintsBudgetIntoPool(t *testing.T) {
	app := initTestApp(t)
	genesisTime := time.Unix(1_700_000_000, 0)
	ctx := initTestContext(app).WithBlockTime(genesisTime)
	// Re-seed the emission state at a controlled genesis time so the
	// protocol year arithmetic is deterministic.
	if err := app.PoleKeeper.InitAnnualEmission(ctx); err != nil {
		t.Fatalf("reset annual emission: %v", err)
	}
	moduleAddr := authtypes.NewModuleAddress(types.ModuleName)

	// No finalized epoch yet -> neutral adjustment -> year-1 reference
	// rate (200M annually, one 30-day share = 16,666,666).
	ctx = ctx.WithBlockTime(genesisTime.Add(time.Duration(types.SecondsPerMonth) * time.Second))
	if err := app.PoleKeeper.BeginBlockAnnualEmission(ctx); err != nil {
		t.Fatalf("begin block: %v", err)
	}
	balance := app.BankKeeper.GetBalance(ctx, moduleAddr, types.BaseDenom)
	if !balance.Amount.Equal(sdkmath.NewInt(16_666_666)) {
		t.Fatalf("expected pool 16_666_666, got %s", balance.Amount.String())
	}

	// A finalized epoch far below target raises the reference rate to the
	// +10% activity cap (year 1 base 200M -> 220M). One second into the
	// next observation period mints ~7.
	if err := app.PoleKeeper.SetEpochCommit(ctx, types.EpochCommit{
		EpochId:                 1,
		Finalized:               true,
		TotalNetworkWeightUnits: 50,
	}); err != nil {
		t.Fatalf("set epoch commit: %v", err)
	}
	ctx = ctx.WithBlockTime(genesisTime.Add(time.Duration(types.SecondsPerMonth)*time.Second + time.Second))
	if err := app.PoleKeeper.BeginBlockAnnualEmission(ctx); err != nil {
		t.Fatalf("begin block: %v", err)
	}
	balance = app.BankKeeper.GetBalance(ctx, moduleAddr, types.BaseDenom)
	if !balance.Amount.Equal(sdkmath.NewInt(16_666_673)) {
		t.Fatalf("expected pool 16_666_673, got %s", balance.Amount.String())
	}
}

// TestNoMonthlyQuotaCeiling pins the P3 decision: the scheme-A curve is a
// rate-control reference, not an issuance ceiling. The 30-day counter is
// kept for observability only, so issuance must keep flowing month after
// month and year after year instead of stopping once a budget is spent.
func TestNoMonthlyQuotaCeiling(t *testing.T) {
	app := initTestApp(t)
	genesisTime := time.Unix(1_700_000_000, 0)
	ctx := initTestContext(app).WithBlockTime(genesisTime)
	if err := app.PoleKeeper.InitAnnualEmission(ctx); err != nil {
		t.Fatalf("reset annual emission: %v", err)
	}
	moduleAddr := authtypes.NewModuleAddress(types.ModuleName)

	// One 30-day step: year-1 reference rate (200M annually) pays the
	// monthly share of 16,666,666.
	monthlyShare := sdkmath.NewInt(16_666_666)
	ctx = ctx.WithBlockTime(genesisTime.Add(time.Duration(types.SecondsPerMonth) * time.Second))
	if err := app.PoleKeeper.BeginBlockAnnualEmission(ctx); err != nil {
		t.Fatalf("begin block after one month: %v", err)
	}
	balance := app.BankKeeper.GetBalance(ctx, moduleAddr, types.BaseDenom)
	if !balance.Amount.Equal(monthlyShare) {
		t.Fatalf("expected one monthly share %s, got %s", monthlyShare, balance.Amount.String())
	}

	// A clock jump larger than a month is clamped to a single month's
	// share: catch-up must not create an accidental burst.
	ctx = ctx.WithBlockTime(ctx.BlockTime().Add(
		time.Duration(types.SecondsPerMonth*6) * time.Second,
	))
	if err := app.PoleKeeper.BeginBlockAnnualEmission(ctx); err != nil {
		t.Fatalf("begin block after a long clock jump: %v", err)
	}
	balance = app.BankKeeper.GetBalance(ctx, moduleAddr, types.BaseDenom)
	if !balance.Amount.Equal(monthlyShare.MulRaw(2)) {
		t.Fatalf("expected a single clamped catch-up share, got %s", balance.Amount.String())
	}

	// Cross the protocol year boundary by stepping month by month, then
	// keep going. The years 1-2 rate is 16,666,666 per month and the
	// long-term tail halves from year 3, so the assertion tracks each
	// step individually: issuance must never stop, whatever the
	// year-index arithmetic does.
	for i := 0; i < 18; i++ {
		before := app.BankKeeper.GetBalance(ctx, moduleAddr, types.BaseDenom)
		ctx = ctx.WithBlockTime(ctx.BlockTime().Add(
			time.Duration(types.SecondsPerMonth) * time.Second,
		))
		if err := app.PoleKeeper.BeginBlockAnnualEmission(ctx); err != nil {
			t.Fatalf("begin block at month %d: %v", i+1, err)
		}
		after := app.BankKeeper.GetBalance(ctx, moduleAddr, types.BaseDenom)
		if !after.Amount.GT(before.Amount) {
			t.Fatalf(
				"issuance stopped at month %d (%s -> %s): no quota may cap it",
				i+1, before.Amount.String(), after.Amount.String(),
			)
		}
	}

	// 1 + 1 (clamped catch-up) + 18 monthly steps, all minted: cumulative
	// issuance is well past the whole year-1 reference volume.
	balance = app.BankKeeper.GetBalance(ctx, moduleAddr, types.BaseDenom)
	if balance.Amount.LTE(sdkmath.NewInt(200_000_000)) {
		t.Fatalf("expected cumulative issuance to exceed the year-1 reference, got %s", balance.Amount.String())
	}
}

// TestEmissionFollowsConfirmedSettlements pins the other half of P3: a
// confirmed settlement is payable in full even when the reference curve has
// minted nothing and the module pool is empty. Issuance tracks confirmed
// play, so a single large epoch must not be blocked by a budget.
func TestEmissionFollowsConfirmedSettlements(t *testing.T) {
	app := initTestApp(t)
	genesisTime := time.Unix(1_700_000_000, 0)
	ctx := initTestContext(app).WithBlockHeight(100).WithBlockTime(genesisTime)
	if err := app.PoleKeeper.InitAnnualEmission(ctx); err != nil {
		t.Fatalf("reset annual emission: %v", err)
	}
	moduleAddr := authtypes.NewModuleAddress(types.ModuleName)

	recipientAddr := sdk.AccAddress(bytes.Repeat([]byte{11}, 20))
	recipient, err := app.AccountKeeper.AddressCodec().BytesToString(recipientAddr)
	if err != nil {
		t.Fatalf("recipient bech32: %v", err)
	}
	app.AccountKeeper.SetAccount(ctx, app.AccountKeeper.NewAccountWithAddress(ctx, recipientAddr))

	// One confirmed epoch worth 400M: twice the whole year-1 reference
	// volume, minted from an empty pool without a single BeginBlock.
	const netReward = uint64(400_000_000)
	if err := app.PoleKeeper.SetEpochCommit(ctx, types.EpochCommit{
		EpochId: 1, Finalized: true, TotalNetworkWeightUnits: 400_000_000,
	}); err != nil {
		t.Fatalf("set epoch commit: %v", err)
	}
	if err := app.PoleKeeper.SetRewardRecord(ctx, types.RewardRecord{
		EpochId: 1, Recipient: recipient, PlayerReward: netReward, NetReward: netReward,
	}); err != nil {
		t.Fatalf("set reward record: %v", err)
	}

	// Confirm the pool really is empty before the claim.
	if pool := app.BankKeeper.GetBalance(ctx, moduleAddr, types.BaseDenom); !pool.Amount.IsZero() {
		t.Fatalf("expected an empty module pool before the claim, got %s", pool.Amount.String())
	}

	msgServer := app.MsgServiceRouter().Handler(&types.MsgClaimReward{})
	if _, err := msgServer(ctx, &types.MsgClaimReward{Claimer: recipient, EpochId: 1, Recipient: recipient}); err != nil {
		t.Fatalf("claim a settlement larger than the annual reference: %v", err)
	}

	// burn = (400_000_000 - 10_000) * 1000 / 10_000 = 39_999_000, so the
	// payout is 360_001_000 and the pool ends empty.
	balance := app.BankKeeper.GetBalance(ctx, recipientAddr, types.BaseDenom)
	if !balance.Amount.Equal(sdkmath.NewInt(360_001_000)) {
		t.Fatalf("expected confirmed payout 360_001_000, got %s", balance.Amount.String())
	}
	if pool := app.BankKeeper.GetBalance(ctx, moduleAddr, types.BaseDenom); !pool.Amount.IsZero() {
		t.Fatalf("expected the module pool to end empty, got %s", pool.Amount.String())
	}

	// Total supply must have grown by at least the confirmed payout,
	// which is already past the year-1 reference emission.
	supply := app.BankKeeper.GetSupply(ctx, types.BaseDenom)
	if supply.Amount.LT(sdkmath.NewInt(360_001_000)) {
		t.Fatalf("expected supply to follow the confirmed settlement, got %s", supply.Amount.String())
	}
}

func TestClaimRewardMintsConfirmedShortfall(t *testing.T) {
	app := initTestApp(t)
	genesisTime := time.Unix(1_700_000_000, 0)
	ctx := initTestContext(app).WithBlockHeight(100).WithBlockTime(genesisTime)
	if err := app.PoleKeeper.InitAnnualEmission(ctx); err != nil {
		t.Fatalf("reset annual emission: %v", err)
	}

	recipientAddr := sdk.AccAddress(bytes.Repeat([]byte{8}, 20))
	recipient, err := app.AccountKeeper.AddressCodec().BytesToString(recipientAddr)
	if err != nil {
		t.Fatalf("recipient bech32: %v", err)
	}
	app.AccountKeeper.SetAccount(ctx, app.AccountKeeper.NewAccountWithAddress(ctx, recipientAddr))
	if err := app.PoleKeeper.SetEpochCommit(ctx, types.EpochCommit{
		EpochId: 1, Finalized: true, TotalNetworkWeightUnits: 50,
	}); err != nil {
		t.Fatalf("set epoch commit: %v", err)
	}
	if err := app.PoleKeeper.SetRewardRecord(ctx, types.RewardRecord{
		EpochId: 1, Recipient: recipient, PlayerReward: 50_000, NetReward: 50_000,
	}); err != nil {
		t.Fatalf("set reward record: %v", err)
	}

	// Do not run BeginBlockAnnualEmission: the module account starts empty.
	// A confirmed claim must mint only the exact payout shortfall.
	msgServer := app.MsgServiceRouter().Handler(&types.MsgClaimReward{})
	if _, err := msgServer(ctx, &types.MsgClaimReward{Claimer: recipient, EpochId: 1, Recipient: recipient}); err != nil {
		t.Fatalf("claim reward from empty pool: %v", err)
	}
	balance := app.BankKeeper.GetBalance(ctx, recipientAddr, types.BaseDenom)
	// Default reward burn is 10% of the 40_000 excess above the 10_000
	// threshold, so the exact 50_000 claim mints 50_000, pays 46_000,
	// and burns 4_000.
	if !balance.Amount.Equal(sdkmath.NewInt(46_000)) {
		t.Fatalf("expected on-demand payout 46_000, got %s", balance.Amount.String())
	}
	moduleAddr := authtypes.NewModuleAddress(types.ModuleName)
	moduleBalance := app.BankKeeper.GetBalance(ctx, moduleAddr, types.BaseDenom)
	if !moduleBalance.Amount.IsZero() {
		t.Fatalf("expected empty module pool after payout and burn, got %s", moduleBalance.Amount.String())
	}
}

func TestClaimRewardBurnsExcessAboveThreshold(t *testing.T) {
	app := initTestApp(t)
	genesisTime := time.Unix(1_700_000_000, 0)
	ctx := initTestContext(app).WithBlockHeight(100).WithBlockTime(genesisTime)
	if err := app.PoleKeeper.InitAnnualEmission(ctx); err != nil {
		t.Fatalf("reset annual emission: %v", err)
	}

	recipientAddr := sdk.AccAddress(bytes.Repeat([]byte{9}, 20))
	recipient, err := app.AccountKeeper.AddressCodec().BytesToString(recipientAddr)
	if err != nil {
		t.Fatalf("recipient bech32: %v", err)
	}
	app.AccountKeeper.SetAccount(ctx, app.AccountKeeper.NewAccountWithAddress(ctx, recipientAddr))

	if err := app.PoleKeeper.SetEpochCommit(ctx, types.EpochCommit{
		EpochId:                 1,
		Finalized:               true,
		TotalNetworkWeightUnits: 50,
	}); err != nil {
		t.Fatalf("set epoch commit: %v", err)
	}
	// Net reward 50_000, above the burn threshold (10_000) -> excess
	// 40_000 burned at 10% -> 4_000 burned, 46_000 paid.
	if err := app.PoleKeeper.SetRewardRecord(ctx, types.RewardRecord{
		EpochId:      1,
		Recipient:    recipient,
		NetReward:    50_000,
		PlayerReward: 50_000,
	}); err != nil {
		t.Fatalf("set reward record: %v", err)
	}

	ctx = ctx.WithBlockTime(genesisTime.Add(time.Duration(types.SecondsPerMonth) * time.Second))
	if err := app.PoleKeeper.BeginBlockAnnualEmission(ctx); err != nil {
		t.Fatalf("begin block: %v", err)
	}
	moduleAddr := authtypes.NewModuleAddress(types.ModuleName)
	poolBefore := app.BankKeeper.GetBalance(ctx, moduleAddr, types.BaseDenom)
	if !poolBefore.Amount.Equal(sdkmath.NewInt(18_333_333)) {
		t.Fatalf("expected pool 18_333_333 before claim, got %s", poolBefore.Amount.String())
	}

	msgServer := app.MsgServiceRouter().Handler(&types.MsgClaimReward{})
	_, err = msgServer(ctx, &types.MsgClaimReward{Claimer: recipient, EpochId: 1, Recipient: recipient})
	if err != nil {
		t.Fatalf("claim reward: %v", err)
	}

	balance := app.BankKeeper.GetBalance(ctx, recipientAddr, types.BaseDenom)
	if !balance.Amount.Equal(sdkmath.NewInt(46_000)) {
		t.Fatalf("expected payout 46_000, got %s", balance.Amount.String())
	}
	poolAfter := app.BankKeeper.GetBalance(ctx, moduleAddr, types.BaseDenom)
	wantPool := sdkmath.NewInt(18_333_333 - 50_000)
	if !poolAfter.Amount.Equal(wantPool) {
		t.Fatalf("expected pool %s after claim, got %s", wantPool.String(), poolAfter.Amount.String())
	}
}

func TestUpsertNodeStoresNodeRecord(t *testing.T) {
	app := initTestApp(t)
	ctx := initTestContext(app)

	operatorAddr := sdk.AccAddress(bytes.Repeat([]byte{5}, 20))
	operator, err := app.AccountKeeper.AddressCodec().BytesToString(operatorAddr)
	if err != nil {
		t.Fatalf("operator bech32: %v", err)
	}

	handler := app.MsgServiceRouter().Handler(&types.MsgUpsertNode{})
	if handler == nil {
		t.Fatalf("expected upsert node handler")
	}
	_, err = handler(ctx, &types.MsgUpsertNode{
		Operator: operator,
		Node: &types.NodeRecord{
			OperatorAddress: operator,
			RewardAddress:   operator,
			Role:            types.NodeRole_NODE_ROLE_PLAYER,
			Capabilities:    &types.NodeCapabilitySet{Collect: true},
			Active:          true,
		},
	})
	if err != nil {
		t.Fatalf("upsert node: %v", err)
	}

	node, err := app.PoleKeeper.GetNode(ctx, operator)
	if err != nil {
		t.Fatalf("get node: %v", err)
	}
	if !node.Active || !node.Capabilities.Collect {
		t.Fatalf("expected node state to persist")
	}
}

func TestUpsertNodeRejectsServiceNodeBelowMinimumBond(t *testing.T) {
	app := initTestApp(t)
	ctx := initTestContext(app)

	operatorAddr := sdk.AccAddress(bytes.Repeat([]byte{8}, 20))
	operator, err := app.AccountKeeper.AddressCodec().BytesToString(operatorAddr)
	if err != nil {
		t.Fatalf("operator bech32: %v", err)
	}
	handler := app.MsgServiceRouter().Handler(&types.MsgUpsertNode{})
	if handler == nil {
		t.Fatalf("expected upsert node handler")
	}
	_, err = handler(ctx, &types.MsgUpsertNode{
		Operator: operator,
		Node: &types.NodeRecord{
			OperatorAddress: operator,
			RewardAddress:   operator,
			Role:            types.NodeRole_NODE_ROLE_SERVICE,
			Capabilities:    &types.NodeCapabilitySet{Store: true},
			Active:          true,
			BondedTokens:    1,
		},
	})
	if err == nil {
		t.Fatalf("expected service node below minimum bond to be rejected")
	}
}

func TestSubmitBatchRequiresRegisteredCollectCapability(t *testing.T) {
	app := initTestApp(t)
	ctx := initTestContext(app).WithBlockHeight(5)

	collectorAddr := sdk.AccAddress(bytes.Repeat([]byte{7}, 20))
	collector, err := app.AccountKeeper.AddressCodec().BytesToString(collectorAddr)
	if err != nil {
		t.Fatalf("collector bech32: %v", err)
	}
	handler := app.MsgServiceRouter().Handler(&types.MsgSubmitBatch{})
	if handler == nil {
		t.Fatalf("expected submit batch handler")
	}
	_, err = handler(ctx, &types.MsgSubmitBatch{
		Collector: collector,
		BatchCommit: &types.BatchCommit{
			EpochId:          1,
			CollectorAddress: collector,
			SlotStart:        1,
			SlotEnd:          1,
			Batch:            &types.MerkleCommitment{Root: "batch-root", LeafCount: 1},
			PayloadCid:       "cid://batch",
			ObservationCount: 1,
		},
	})
	if err == nil {
		t.Fatalf("expected unregistered collector to be rejected")
	}

	nodeHandler := app.MsgServiceRouter().Handler(&types.MsgUpsertNode{})
	if nodeHandler == nil {
		t.Fatalf("expected upsert node handler")
	}
	_, err = nodeHandler(ctx, &types.MsgUpsertNode{
		Operator: collector,
		Node: &types.NodeRecord{
			OperatorAddress: collector,
			RewardAddress:   collector,
			Role:            types.NodeRole_NODE_ROLE_PLAYER,
			Capabilities:    &types.NodeCapabilitySet{Collect: true},
			Active:          true,
		},
	})
	if err != nil {
		t.Fatalf("upsert collector node: %v", err)
	}
	_, err = handler(ctx, &types.MsgSubmitBatch{
		Collector: collector,
		BatchCommit: &types.BatchCommit{
			EpochId:          1,
			CollectorAddress: collector,
			SlotStart:        1,
			SlotEnd:          1,
			Batch:            &types.MerkleCommitment{Root: "batch-root", LeafCount: 1},
			PayloadCid:       "cid://batch",
			ObservationCount: 1,
		},
	})
	if err != nil {
		t.Fatalf("registered collector should be allowed: %v", err)
	}
}

func TestFinalizeEpochValidatesRoots(t *testing.T) {
	app := initTestApp(t)
	ctx := initTestContext(app).WithBlockHeight(25)

	recipientAddr := sdk.AccAddress(bytes.Repeat([]byte{6}, 20))
	recipient, err := app.AccountKeeper.AddressCodec().BytesToString(recipientAddr)
	if err != nil {
		t.Fatalf("recipient bech32: %v", err)
	}
	reward := types.RewardRecord{EpochId: 9, Recipient: recipient, NetReward: 77}
	aggregate := types.AggregateRecord{EpochId: 9, AppId: 730, TotalWeightUnits: 88, PlayerCount: 2}
	if err := app.PoleKeeper.SetRewardRecord(ctx, reward); err != nil {
		t.Fatalf("set reward record: %v", err)
	}
	if err := app.PoleKeeper.SetAggregateRecord(ctx, aggregate); err != nil {
		t.Fatalf("set aggregate record: %v", err)
	}
	rewardRoot := testCommitmentRoot(t, []types.RewardRecord{reward})
	aggregateRoot := testCommitmentRoot(t, []types.AggregateRecord{aggregate})
	if err := app.PoleKeeper.SetEpochCommit(ctx, types.EpochCommit{
		EpochId:                 9,
		ProposerAddress:         reward.Recipient,
		ChallengeOpenHeight:     1,
		ChallengeDeadlineHeight: 10,
		Rewards:                 &types.MerkleCommitment{Root: rewardRoot, LeafCount: 1},
		Aggregates:              &types.MerkleCommitment{Root: aggregateRoot, LeafCount: 1},
		TotalNetworkWeightUnits: 88,
	}); err != nil {
		t.Fatalf("set epoch commit: %v", err)
	}

	finalize := app.MsgServiceRouter().Handler(&types.MsgFinalizeEpoch{})
	if finalize == nil {
		t.Fatalf("expected finalize epoch handler")
	}
	seedVerificationCoverage(t, app, ctx, 9)
	_, err = finalize(ctx, &types.MsgFinalizeEpoch{Finalizer: reward.Recipient, EpochId: 9})
	if err != nil {
		t.Fatalf("finalize epoch with valid roots: %v", err)
	}

	if err := app.PoleKeeper.SetEpochCommit(ctx, types.EpochCommit{
		EpochId:                 10,
		ProposerAddress:         reward.Recipient,
		ChallengeOpenHeight:     1,
		ChallengeDeadlineHeight: 10,
		Rewards:                 &types.MerkleCommitment{Root: "bad-root", LeafCount: 1},
		Aggregates:              &types.MerkleCommitment{Root: aggregateRoot, LeafCount: 1},
		TotalNetworkWeightUnits: 88,
	}); err != nil {
		t.Fatalf("set invalid epoch commit: %v", err)
	}
	if err := app.PoleKeeper.SetRewardRecord(ctx, types.RewardRecord{EpochId: 10, Recipient: reward.Recipient, NetReward: 77}); err != nil {
		t.Fatalf("set reward record for invalid finalize: %v", err)
	}
	if err := app.PoleKeeper.SetAggregateRecord(ctx, types.AggregateRecord{EpochId: 10, AppId: 730, TotalWeightUnits: 88, PlayerCount: 2}); err != nil {
		t.Fatalf("set aggregate record for invalid finalize: %v", err)
	}
	_, err = finalize(ctx, &types.MsgFinalizeEpoch{Finalizer: reward.Recipient, EpochId: 10})
	if err == nil {
		t.Fatalf("expected finalize epoch to fail on invalid reward root")
	}
}

// TestFinalizeEpochRejectsMissingRewardRecords covers the other half of the
// unbacked-reward gap: a proposer that omits the rewards commitment entirely
// must not be able to close the epoch. Before the unconditional rewards
// check, a nil commitment was read as "no rewards to verify" and the epoch
// finalized with no payout whatsoever.
func TestFinalizeEpochRejectsMissingRewardRecords(t *testing.T) {
	app := initTestApp(t)
	ctx := initTestContext(app).WithBlockHeight(25)

	recipientAddr := sdk.AccAddress(bytes.Repeat([]byte{7}, 20))
	recipient, err := app.AccountKeeper.AddressCodec().BytesToString(recipientAddr)
	if err != nil {
		t.Fatalf("recipient bech32: %v", err)
	}

	aggregate := types.AggregateRecord{EpochId: 30, AppId: 730, TotalWeightUnits: 88, PlayerCount: 2}
	if err := app.PoleKeeper.SetAggregateRecord(ctx, aggregate); err != nil {
		t.Fatalf("set aggregate record: %v", err)
	}

	// Everything else about the commit is well formed; only Rewards is nil.
	if err := app.PoleKeeper.SetEpochCommit(ctx, types.EpochCommit{
		EpochId:                 30,
		ProposerAddress:         recipient,
		ChallengeOpenHeight:     1,
		ChallengeDeadlineHeight: 10,
		Aggregates:              &types.MerkleCommitment{Root: testCommitmentRoot(t, []types.AggregateRecord{aggregate}), LeafCount: 1},
		TotalNetworkWeightUnits: 88,
	}); err != nil {
		t.Fatalf("set epoch commit: %v", err)
	}

	finalize := app.MsgServiceRouter().Handler(&types.MsgFinalizeEpoch{})
	if finalize == nil {
		t.Fatalf("expected finalize epoch handler")
	}
	seedVerificationCoverage(t, app, ctx, 30)

	_, err = finalize(ctx, &types.MsgFinalizeEpoch{Finalizer: recipient, EpochId: 30})
	if err == nil {
		t.Fatalf("expected finalize to reject an epoch with no rewards commitment")
	}
	if !strings.Contains(err.Error(), "missing rewards commitment") {
		t.Fatalf("unexpected error: %v", err)
	}

	commit, err := app.PoleKeeper.GetEpochCommit(ctx, 30)
	if err != nil {
		t.Fatalf("epoch commit after rejected finalize: %v", err)
	}
	if commit.Finalized {
		t.Fatalf("epoch must not be marked finalized when the rewards commitment is missing")
	}
}

// TestFinalizeEpochRejectsUnbackedRewardRoot covers the gap that made
// claims unpayable: reward records used to have no live submission path,
// so a proposer committed a rewards root over an off-chain reward set
// that never landed on-chain, FinalizeEpoch waved it through, and every
// ClaimReward against that epoch then failed with "reward record not
// found". The chain now requires the committed rewards root to be backed
// by records it actually holds.
func TestFinalizeEpochRejectsUnbackedRewardRoot(t *testing.T) {
	app := initTestApp(t)
	ctx := initTestContext(app).WithBlockHeight(25)

	recipientAddr := sdk.AccAddress(bytes.Repeat([]byte{7}, 20))
	recipient, err := app.AccountKeeper.AddressCodec().BytesToString(recipientAddr)
	if err != nil {
		t.Fatalf("recipient bech32: %v", err)
	}
	aggregate := types.AggregateRecord{EpochId: 20, AppId: 730, TotalWeightUnits: 88, PlayerCount: 2}
	if err := app.PoleKeeper.SetAggregateRecord(ctx, aggregate); err != nil {
		t.Fatalf("set aggregate record: %v", err)
	}
	// Proposer commits a rewards root over a set that never lands on-chain.
	offchainReward := types.RewardRecord{EpochId: 20, Recipient: recipient, PlayerReward: 50, NetReward: 50}
	aggregateRoot := testCommitmentRoot(t, []types.AggregateRecord{aggregate})
	if err := app.PoleKeeper.SetEpochCommit(ctx, types.EpochCommit{
		EpochId:                 20,
		ProposerAddress:         recipient,
		ChallengeOpenHeight:     1,
		ChallengeDeadlineHeight: 10,
		Rewards:                 &types.MerkleCommitment{Root: testCommitmentRoot(t, []types.RewardRecord{offchainReward}), LeafCount: 1},
		Aggregates:              &types.MerkleCommitment{Root: aggregateRoot, LeafCount: 1},
		TotalNetworkWeightUnits: 88,
	}); err != nil {
		t.Fatalf("set epoch commit: %v", err)
	}

	finalize := app.MsgServiceRouter().Handler(&types.MsgFinalizeEpoch{})
	if finalize == nil {
		t.Fatalf("expected finalize epoch handler")
	}
	seedVerificationCoverage(t, app, ctx, 20)
	_, err = finalize(ctx, &types.MsgFinalizeEpoch{Finalizer: recipient, EpochId: 20})
	if err == nil {
		t.Fatalf("expected finalize to reject a rewards root with no backing records")
	}
	if !strings.Contains(err.Error(), "reward root mismatch") {
		t.Fatalf("expected reward root mismatch, got: %v", err)
	}

	// Submitting the records that back the committed root makes it finalizable,
	// and — the point of the fix — makes the reward claimable.
	submit := app.MsgServiceRouter().Handler(&types.MsgSubmitRewardRecords{})
	if submit == nil {
		t.Fatalf("expected submit reward records handler")
	}
	if err := app.PoleKeeper.SetNode(ctx, types.NodeRecord{
		OperatorAddress: recipient,
		Active:          true,
		Capabilities:    &types.NodeCapabilitySet{Propose: true},
		BondedTokens:    types.MinProposeBondedTokens,
	}); err != nil {
		t.Fatalf("set proposer node: %v", err)
	}
	resp, err := submit(ctx, &types.MsgSubmitRewardRecords{
		Proposer: recipient,
		EpochId:  20,
		Records:  []*types.RewardRecord{&offchainReward},
	})
	if err != nil {
		t.Fatalf("submit reward records: %v", err)
	}
	if resp == nil {
		t.Fatalf("expected submit response")
	}
	seedVerificationCoverage(t, app, ctx, 20)
	if _, err := finalize(ctx, &types.MsgFinalizeEpoch{Finalizer: recipient, EpochId: 20}); err != nil {
		t.Fatalf("finalize epoch after submitting backing records: %v", err)
	}

	// The record is now on-chain, so the claim finds it.
	stored, err := app.PoleKeeper.GetRewardRecord(ctx, 20, recipient)
	if err != nil {
		t.Fatalf("reward record after submit: %v", err)
	}
	if stored.NetReward != 50 {
		t.Fatalf("expected stored net reward 50, got %d", stored.NetReward)
	}
}

// TestSubmitRewardRecordsRejectsRootMismatch: a proposer must not be able
// to commit one rewards root and later submit a different record set.
func TestSubmitRewardRecordsRejectsRootMismatch(t *testing.T) {
	app := initTestApp(t)
	ctx := initTestContext(app).WithBlockHeight(25)

	proposerAddr := sdk.AccAddress(bytes.Repeat([]byte{6}, 20))
	proposer, err := app.AccountKeeper.AddressCodec().BytesToString(proposerAddr)
	if err != nil {
		t.Fatalf("proposer bech32: %v", err)
	}
	if err := app.PoleKeeper.SetNode(ctx, types.NodeRecord{
		OperatorAddress: proposer,
		Active:          true,
		Capabilities:    &types.NodeCapabilitySet{Propose: true},
		BondedTokens:    types.MinProposeBondedTokens,
	}); err != nil {
		t.Fatalf("set proposer node: %v", err)
	}

	committed := types.RewardRecord{EpochId: 30, Recipient: proposer, NetReward: 100}
	if err := app.PoleKeeper.SetEpochCommit(ctx, types.EpochCommit{
		EpochId:                 30,
		ProposerAddress:         proposer,
		ChallengeOpenHeight:     1,
		ChallengeDeadlineHeight: 10,
		Rewards:                 &types.MerkleCommitment{Root: testCommitmentRoot(t, []types.RewardRecord{committed}), LeafCount: 1},
	}); err != nil {
		t.Fatalf("set epoch commit: %v", err)
	}

	// A different amount yields a different root: must be rejected.
	swapped := types.RewardRecord{EpochId: 30, Recipient: proposer, NetReward: 999}
	submit := app.MsgServiceRouter().Handler(&types.MsgSubmitRewardRecords{})
	if _, err := submit(ctx, &types.MsgSubmitRewardRecords{
		Proposer: proposer,
		EpochId:  30,
		Records:  []*types.RewardRecord{&swapped},
	}); err == nil {
		t.Fatalf("expected submit to reject a record set that does not match the committed root")
	}
}

// TestUpsertAggregateRefreshesEpochCommitment covers the challenge-window
// flow: a proposer commits aggregates root R over the record set known at
// commit time; a verifier then upserts an additional aggregate record. The
// stored epoch commit must track the on-chain record set (aggregates root
// + total weight refreshed, rewards root untouched) so FinalizeEpoch
// succeeds against the final record set.
func TestUpsertAggregateRefreshesEpochCommitment(t *testing.T) {
	app := initTestApp(t)
	ctx := initTestContext(app).WithBlockHeight(25)

	recipientAddr := sdk.AccAddress(bytes.Repeat([]byte{8}, 20))
	recipient, err := app.AccountKeeper.AddressCodec().BytesToString(recipientAddr)
	if err != nil {
		t.Fatalf("recipient bech32: %v", err)
	}
	operatorAddr := sdk.AccAddress(bytes.Repeat([]byte{9}, 20))
	operator, err := app.AccountKeeper.AddressCodec().BytesToString(operatorAddr)
	if err != nil {
		t.Fatalf("operator bech32: %v", err)
	}
	if err := app.PoleKeeper.SetNode(ctx, types.NodeRecord{
		OperatorAddress: operator,
		Active:          true,
		Capabilities:    &types.NodeCapabilitySet{Verify: true},
		BondedTokens:    types.MinVerifyBondedTokens,
	}); err != nil {
		t.Fatalf("set verifier node: %v", err)
	}

	aggA := types.AggregateRecord{EpochId: 21, AppId: 730, TotalWeightUnits: 88, PlayerCount: 2}
	if err := app.PoleKeeper.SetAggregateRecord(ctx, aggA); err != nil {
		t.Fatalf("set aggregate record A: %v", err)
	}
	rewardRoot := testCommitmentRoot(t, []types.RewardRecord{{EpochId: 21, Recipient: recipient, NetReward: 77}})
	if err := app.PoleKeeper.SetEpochCommit(ctx, types.EpochCommit{
		EpochId:                 21,
		ProposerAddress:         recipient,
		ChallengeOpenHeight:     1,
		ChallengeDeadlineHeight: 10,
		Rewards:                 &types.MerkleCommitment{Root: rewardRoot, LeafCount: 1},
		Aggregates:              &types.MerkleCommitment{Root: testCommitmentRoot(t, []types.AggregateRecord{aggA}), LeafCount: 1},
		TotalNetworkWeightUnits: 88,
	}); err != nil {
		t.Fatalf("set epoch commit: %v", err)
	}

	// Verifier upserts a second aggregate during the challenge window.
	aggB := types.AggregateRecord{EpochId: 21, AppId: 42, TotalWeightUnits: 100, PlayerCount: 1}
	upsert := app.MsgServiceRouter().Handler(&types.MsgUpsertAggregateRecord{})
	if upsert == nil {
		t.Fatalf("expected upsert aggregate handler")
	}
	_, err = upsert(ctx, &types.MsgUpsertAggregateRecord{Operator: operator, AggregateRecord: &aggB})
	if err != nil {
		t.Fatalf("upsert aggregate record B: %v", err)
	}

	commit, err := app.PoleKeeper.GetEpochCommit(ctx, 21)
	if err != nil {
		t.Fatalf("epoch commit after upsert: %v", err)
	}
	// Aggregates commitment refreshed to the union in key order (app 42, 730).
	wantAggRoot := testCommitmentRoot(t, []types.AggregateRecord{aggB, aggA})
	if commit.Aggregates == nil || commit.Aggregates.Root != wantAggRoot || commit.Aggregates.LeafCount != 2 {
		t.Fatalf("expected refreshed aggregates commitment %s (2 leaves), got %+v", wantAggRoot, commit.Aggregates)
	}
	if commit.TotalNetworkWeightUnits != 188 {
		t.Fatalf("expected refreshed total weight 188, got %d", commit.TotalNetworkWeightUnits)
	}
	// Rewards commitment must be untouched by the aggregate upsert
	// (proposer-side; the reward records arrive via MsgSubmitRewardRecords).
	if commit.Rewards == nil || commit.Rewards.Root != rewardRoot {
		t.Fatalf("expected rewards commitment to stay untouched, got %+v", commit.Rewards)
	}

	// Back the committed rewards root with actual records, otherwise the
	// now-unconditional rewards root check rejects the finalize.
	rewardRecord := types.RewardRecord{EpochId: 21, Recipient: recipient, NetReward: 77}
	if err := app.PoleKeeper.SetRewardRecord(ctx, rewardRecord); err != nil {
		t.Fatalf("set reward record: %v", err)
	}

	finalize := app.MsgServiceRouter().Handler(&types.MsgFinalizeEpoch{})
	if finalize == nil {
		t.Fatalf("expected finalize epoch handler")
	}
	seedVerificationCoverage(t, app, ctx, 21)
	_, err = finalize(ctx, &types.MsgFinalizeEpoch{Finalizer: recipient, EpochId: 21})
	if err != nil {
		t.Fatalf("finalize epoch after window upsert: %v", err)
	}
}

func TestVerifyBatchEnforcesRulesAndStoresRecord(t *testing.T) {
	app := initTestApp(t)
	ctx := initTestContext(app).WithBlockHeight(100)

	verifier := "pole1verify11111111111111111111111111111111"
	collector := "pole1collector222222222222222222222222222222"
	other := "pole1other33333333333333333333333333333333"
	if err := app.PoleKeeper.SetNode(ctx, types.NodeRecord{
		OperatorAddress: verifier,
		Active:          true,
		Capabilities:    &types.NodeCapabilitySet{Verify: true, Collect: true},
		BondedTokens:    types.MinVerifyBondedTokens,
	}); err != nil {
		t.Fatalf("set verifier node: %v", err)
	}
	if err := app.PoleKeeper.SetNode(ctx, types.NodeRecord{
		OperatorAddress: other,
		Active:          true,
		Capabilities:    &types.NodeCapabilitySet{Collect: true},
	}); err != nil {
		t.Fatalf("set plain collector node: %v", err)
	}

	handler := app.MsgServiceRouter().Handler(&types.MsgVerifyBatch{})
	if handler == nil {
		t.Fatalf("expected verify batch handler")
	}

	// A verifier may not attest its own batch.
	_, err := handler(ctx, &types.MsgVerifyBatch{
		Verifier: verifier, EpochId: 1, TargetBatchRootHex: "ab", TargetCollector: verifier,
	})
	if err == nil {
		t.Fatalf("expected self-audit rejection")
	}

	// A node without the verify capability may not attest.
	_, err = handler(ctx, &types.MsgVerifyBatch{
		Verifier: other, EpochId: 1, TargetBatchRootHex: "ab", TargetCollector: collector,
	})
	if err == nil {
		t.Fatalf("expected non-verify rejection")
	}

	// A bonded non-player may attest: prev epoch had no batch activity,
	// so the chain classifies it as a plain node, but its bond satisfies
	// the verify requirement.
	_, err = handler(ctx, &types.MsgVerifyBatch{
		Verifier: verifier, EpochId: 1, TargetBatchRootHex: "ab", TargetCollector: collector,
		IsPlayer: true, Verified: true, // self-reported flag is informational
	})
	if err != nil {
		t.Fatalf("verify batch: %v", err)
	}
	record, err := app.PoleKeeper.GetVerificationRecord(ctx, 1, verifier, "ab")
	if err != nil {
		t.Fatalf("get verification record: %v", err)
	}
	if record.IsPlayer || !record.Verified {
		t.Fatalf("chain must classify bonded non-player as IsPlayer=false, got %+v", record)
	}
}

// A verifier that was live (submitted batches) in the previous epoch is
// classified as a player by the chain and may verify without a stake.
func TestVerifyBatchClassifiesPlayerByPreviousEpochActivity(t *testing.T) {
	app := initTestApp(t)
	ctx := initTestContext(app).WithBlockHeight(100)

	player := "pole1player55555555555555555555555555555555"
	collector := "pole1collector222222222222222222222222222222"
	if err := app.PoleKeeper.SetNode(ctx, types.NodeRecord{
		OperatorAddress: player,
		Active:          true,
		Capabilities:    &types.NodeCapabilitySet{Verify: true, Collect: true},
		// No bond: player collectors verify without a stake.
		BondedTokens: 0,
	}); err != nil {
		t.Fatalf("set player node: %v", err)
	}
	// The player was live in epoch 0 (submitted a batch there).
	if err := app.PoleKeeper.SetBatchCommit(ctx, types.BatchCommit{
		EpochId: 0, CollectorAddress: player, PayloadCid: "cid", ObservationCount: 1,
	}); err != nil {
		t.Fatalf("set batch commit: %v", err)
	}

	handler := app.MsgServiceRouter().Handler(&types.MsgVerifyBatch{})
	_, err := handler(ctx, &types.MsgVerifyBatch{
		Verifier: player, EpochId: 1, TargetBatchRootHex: "cd", TargetCollector: collector,
		Verified: true,
	})
	if err != nil {
		t.Fatalf("player verify batch: %v", err)
	}
	record, err := app.PoleKeeper.GetVerificationRecord(ctx, 1, player, "cd")
	if err != nil {
		t.Fatalf("get verification record: %v", err)
	}
	if !record.IsPlayer {
		t.Fatalf("chain must classify live collector as IsPlayer=true, got %+v", record)
	}
}

// A node that was not live in the previous epoch and holds no verify bond
// is rejected: no stake, no verification.
func TestVerifyBatchRejectsUnbondedNonPlayer(t *testing.T) {
	app := initTestApp(t)
	ctx := initTestContext(app).WithBlockHeight(100)

	idle := "pole1idle66666666666666666666666666666666"
	collector := "pole1collector222222222222222222222222222222"
	if err := app.PoleKeeper.SetNode(ctx, types.NodeRecord{
		OperatorAddress: idle,
		Active:          true,
		Capabilities:    &types.NodeCapabilitySet{Verify: true},
		BondedTokens:    0, // no bond
	}); err != nil {
		t.Fatalf("set idle node: %v", err)
	}

	handler := app.MsgServiceRouter().Handler(&types.MsgVerifyBatch{})
	_, err := handler(ctx, &types.MsgVerifyBatch{
		Verifier: idle, EpochId: 1, TargetBatchRootHex: "ef", TargetCollector: collector,
	})
	if err == nil {
		t.Fatalf("expected unbonded non-player verification rejection")
	}
	if !strings.Contains(err.Error(), "must post the verify bond") {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestFinalizeEpochRejectsInsufficientVerifications(t *testing.T) {
	app := initTestApp(t)
	ctx := initTestContext(app).WithBlockHeight(100)

	recipientAddr := sdk.AccAddress(bytes.Repeat([]byte{9}, 20))
	recipient, err := app.AccountKeeper.AddressCodec().BytesToString(recipientAddr)
	if err != nil {
		t.Fatalf("recipient bech32: %v", err)
	}
	reward := types.RewardRecord{EpochId: 11, Recipient: recipient, NetReward: 9}
	aggregate := types.AggregateRecord{EpochId: 11, AppId: 730, TotalWeightUnits: 9, PlayerCount: 1}
	if err := app.PoleKeeper.SetRewardRecord(ctx, reward); err != nil {
		t.Fatalf("set reward record: %v", err)
	}
	if err := app.PoleKeeper.SetAggregateRecord(ctx, aggregate); err != nil {
		t.Fatalf("set aggregate record: %v", err)
	}
	if err := app.PoleKeeper.SetEpochCommit(ctx, types.EpochCommit{
		EpochId:                 11,
		ProposerAddress:         recipient,
		ChallengeOpenHeight:     1,
		ChallengeDeadlineHeight: 10,
		Rewards:                 &types.MerkleCommitment{Root: testCommitmentRoot(t, []types.RewardRecord{reward}), LeafCount: 1},
		Aggregates:              &types.MerkleCommitment{Root: testCommitmentRoot(t, []types.AggregateRecord{aggregate}), LeafCount: 1},
		TotalNetworkWeightUnits: 9,
	}); err != nil {
		t.Fatalf("set epoch commit: %v", err)
	}

	// Only two credentials: below the default min of 3.
	for i := 0; i < 2; i++ {
		verifier := "pole1verify55555555555555555555555555555" + string(rune('a'+i))
		if err := app.PoleKeeper.SetVerificationRecord(ctx, types.VerificationRecord{
			EpochId: 11, VerifierAddress: verifier, TargetBatchRootHex: "ab", TargetCollector: recipient,
			IsPlayer: true, Verified: true, VerifiedAtHeight: 5,
		}); err != nil {
			t.Fatalf("seed verification: %v", err)
		}
	}

	finalize := app.MsgServiceRouter().Handler(&types.MsgFinalizeEpoch{})
	_, err = finalize(ctx, &types.MsgFinalizeEpoch{Finalizer: recipient, EpochId: 11})
	if err == nil {
		t.Fatalf("expected insufficient-verifications rejection")
	}
	if !strings.Contains(err.Error(), "insufficient verifications") {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestResolveChallengeAdjustsRewardRecords(t *testing.T) {
	app := initTestApp(t)
	ctx := initTestContext(app).WithBlockHeight(50)

	targetAddr := sdk.AccAddress(bytes.Repeat([]byte{2}, 20))
	target, err := app.AccountKeeper.AddressCodec().BytesToString(targetAddr)
	if err != nil {
		t.Fatalf("target bech32: %v", err)
	}
	challengerAddr := sdk.AccAddress(bytes.Repeat([]byte{3}, 20))
	challenger, err := app.AccountKeeper.AddressCodec().BytesToString(challengerAddr)
	if err != nil {
		t.Fatalf("challenger bech32: %v", err)
	}
	govAuthority, err := app.AccountKeeper.AddressCodec().BytesToString(authtypes.NewModuleAddress(govtypes.ModuleName))
	if err != nil {
		t.Fatalf("gov authority: %v", err)
	}

	if err := app.PoleKeeper.SetRewardRecord(ctx, types.RewardRecord{EpochId: 7, Recipient: target, NetReward: 100}); err != nil {
		t.Fatalf("set target reward: %v", err)
	}
	if err := app.PoleKeeper.SetAggregateRecord(ctx, types.AggregateRecord{EpochId: 7, AppId: 730, TotalWeightUnits: 100, PlayerCount: 1}); err != nil {
		t.Fatalf("set aggregate record: %v", err)
	}
	initialRewardRoot := testCommitmentRoot(t, []types.RewardRecord{{EpochId: 7, Recipient: target, NetReward: 100}})
	aggregateRoot := testCommitmentRoot(t, []types.AggregateRecord{{EpochId: 7, AppId: 730, TotalWeightUnits: 100, PlayerCount: 1}})
	if err := app.PoleKeeper.SetEpochCommit(ctx, types.EpochCommit{
		EpochId:                 7,
		ProposerAddress:         challenger,
		ChallengeOpenHeight:     1,
		ChallengeDeadlineHeight: 10,
		Rewards:                 &types.MerkleCommitment{Root: initialRewardRoot, LeafCount: 1},
		Aggregates:              &types.MerkleCommitment{Root: aggregateRoot, LeafCount: 1},
		TotalNetworkWeightUnits: 100,
	}); err != nil {
		t.Fatalf("set epoch commit: %v", err)
	}
	if err := app.PoleKeeper.SetNode(ctx, types.NodeRecord{
		OperatorAddress: target,
		RewardAddress:   target,
		Role:            types.NodeRole_NODE_ROLE_SERVICE,
		Capabilities:    &types.NodeCapabilitySet{Verify: true},
		Active:          true,
		BondedTokens:    types.MinVerifyBondedTokens,
	}); err != nil {
		t.Fatalf("set target node: %v", err)
	}
	if err := app.PoleKeeper.SetNode(ctx, types.NodeRecord{
		OperatorAddress: challenger,
		RewardAddress:   challenger,
		Role:            types.NodeRole_NODE_ROLE_SERVICE,
		Capabilities:    &types.NodeCapabilitySet{Verify: true},
		Active:          true,
		BondedTokens:    types.MinVerifyBondedTokens,
	}); err != nil {
		t.Fatalf("set challenger node: %v", err)
	}
	if err := app.PoleKeeper.SetChallenge(ctx, types.Challenge{
		ChallengeIdHex: "challenge-7",
		EpochId:        7,
		TargetAddress:  target,
		Challenger:     challenger,
		State:          types.ChallengeStateOpen,
	}); err != nil {
		t.Fatalf("set challenge: %v", err)
	}

	msgServer := app.MsgServiceRouter().Handler(&types.MsgResolveChallenge{})
	if msgServer == nil {
		t.Fatalf("expected resolve challenge handler")
	}
	_, err = msgServer(ctx, &types.MsgResolveChallenge{
		Resolver:          govAuthority,
		ChallengeIdHex:    "challenge-7",
		SlashAmount:       30,
		ChallengerReward:  12,
		ResolutionSummary: "bad reward root corrected",
		FinalState:        types.ChallengeStateResolved,
	})
	if err != nil {
		t.Fatalf("resolve challenge: %v", err)
	}

	targetReward, err := app.PoleKeeper.GetRewardRecord(ctx, 7, target)
	if err != nil {
		t.Fatalf("target reward after resolution: %v", err)
	}
	if targetReward.NetReward != 70 || targetReward.SlashDebit != 30 {
		t.Fatalf("expected target reward to become net=70 slash=30, got net=%d slash=%d", targetReward.NetReward, targetReward.SlashDebit)
	}
	challengerReward, err := app.PoleKeeper.GetRewardRecord(ctx, 7, challenger)
	if err != nil {
		t.Fatalf("challenger reward after resolution: %v", err)
	}
	if challengerReward.NetReward != 12 || challengerReward.VerifyReward != 12 {
		t.Fatalf("expected challenger reward net=12 verify=12, got net=%d verify=%d", challengerReward.NetReward, challengerReward.VerifyReward)
	}
	epochCommit, err := app.PoleKeeper.GetEpochCommit(ctx, 7)
	if err != nil {
		t.Fatalf("epoch commit after challenge: %v", err)
	}
	if epochCommit.Rewards == nil || epochCommit.Rewards.Root == initialRewardRoot {
		t.Fatalf("expected reward root to be recomputed after challenge resolution")
	}
}

func TestSubmitReplicaReceiptCreatesAvailabilityRecord(t *testing.T) {
	app := initTestApp(t)
	ctx := initTestContext(app).WithBlockHeight(12)

	storerAddr := sdk.AccAddress(bytes.Repeat([]byte{4}, 20))
	storer, err := app.AccountKeeper.AddressCodec().BytesToString(storerAddr)
	if err != nil {
		t.Fatalf("storer bech32: %v", err)
	}
	if err := app.PoleKeeper.SetNode(ctx, types.NodeRecord{
		OperatorAddress: storer,
		RewardAddress:   storer,
		Role:            types.NodeRole_NODE_ROLE_SERVICE,
		Capabilities:    &types.NodeCapabilitySet{Store: true},
		Active:          true,
		BondedTokens:    types.MinServiceBondedTokens,
	}); err != nil {
		t.Fatalf("set storer node: %v", err)
	}

	handler := app.MsgServiceRouter().Handler(&types.MsgSubmitReplicaReceipt{})
	if handler == nil {
		t.Fatalf("expected submit replica receipt handler")
	}
	_, err = handler(ctx, &types.MsgSubmitReplicaReceipt{
		Storer: storer,
		ReplicaReceipt: &types.ReplicaReceipt{
			EpochId:             2,
			PayloadCid:          "cid://payload-2",
			StorerAddress:       storer,
			RetentionUntilEpoch: 5,
			ReceiptSignature:    "sig-1",
			ReceiptHashHex:      "hash-1",
		},
	})
	if err != nil {
		t.Fatalf("submit replica receipt: %v", err)
	}

	receipt, err := app.PoleKeeper.GetReplicaReceipt(ctx, 2, storer, "cid://payload-2")
	if err != nil {
		t.Fatalf("stored replica receipt: %v", err)
	}
	if receipt.ReceiptHashHex != "hash-1" {
		t.Fatalf("expected receipt hash to persist")
	}
	availabilityIter, err := app.PoleKeeper.Availability.Iterate(ctx, nil)
	if err != nil {
		t.Fatalf("availability iterate: %v", err)
	}
	availability, err := availabilityIter.Values()
	if err != nil {
		t.Fatalf("availability values: %v", err)
	}
	if len(availability) != 1 || availability[0].ReceiptHashHex != "hash-1" {
		t.Fatalf("expected availability record to be derived from replica receipt")
	}
}

// seedVerificationCoverage injects 3 independent verification
// credentials for an epoch (2 players, 1 non-player) so the finalize
// verification gate (>= 3 credentials, >= 50% players) passes.
func seedVerificationCoverage(t *testing.T, app *App, ctx sdk.Context, epochId uint64) {
	t.Helper()
	for i, verifier := range []string{
		"pole1playerverifyaaaaaaaaaaaaaaaaaaaaaa1",
		"pole1playerverifybbbbbbbbbbbbbbbbbbbbbb2",
		"pole1nodeverifycccccccccccccccccccccccc3",
	} {
		if err := app.PoleKeeper.SetVerificationRecord(ctx, types.VerificationRecord{
			EpochId:            epochId,
			VerifierAddress:    verifier,
			TargetBatchRootHex: "aabbccdd",
			TargetCollector:    "pole1collectordddddddddddddddddddddddddd4",
			IsPlayer:           i < 2,
			Verified:           true,
			VerifiedAtHeight:   5,
		}); err != nil {
			t.Fatalf("seed verification record: %v", err)
		}
	}
}

func initTestApp(t *testing.T) *App {
	t.Helper()
	app, err := NewMem(log.NewNopLogger())
	if err != nil {
		t.Fatalf("new mem app: %v", err)
	}
	return app
}

func initTestContext(app *App) sdk.Context {
	ctx := testutil.DefaultContextWithKeys(
		app.KVStoreKeys(),
		map[string]*storetypes.TransientStoreKey{},
		map[string]*storetypes.MemoryStoreKey{},
	)
	_, err := app.InitChainer(ctx, &cmtabci.RequestInitChain{})
	if err != nil {
		panic(err)
	}
	return ctx
}

func testCommitmentRoot[T any](t *testing.T, records []T) string {
	t.Helper()
	root, _, err := types.MerkleRootHexForRecords(records)
	if err != nil {
		t.Fatalf("compute merkle root: %v", err)
	}
	return root
}
