package app

import (
	"bytes"
	"testing"

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
	ctx := initTestContext(app).WithBlockHeight(100)

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
	_, err = finalizeHandler(ctx, &types.MsgFinalizeEpoch{Finalizer: recipient, EpochId: 1})
	if err != nil {
		t.Fatalf("finalize epoch: %v", err)
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
