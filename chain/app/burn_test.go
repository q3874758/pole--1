package app

import (
	"bytes"
	"testing"

	sdkmath "cosmossdk.io/math"
	sdk "github.com/cosmos/cosmos-sdk/types"
	authtypes "github.com/cosmos/cosmos-sdk/x/auth/types"
	govtypes "github.com/cosmos/cosmos-sdk/x/gov/types"
	protov2 "google.golang.org/protobuf/proto"

	"pole/chain/x/pole/types"
)

// feeTxStub is the minimal sdk.FeeTx the ante chain needs. The burn channel
// only reads GetFee, so the signature-related methods stay inert.
type feeTxStub struct {
	fee   sdk.Coins
	gas   uint64
	payer []byte
}

func (feeTxStub) GetMsgs() []sdk.Msg                     { return nil }
func (feeTxStub) GetMsgsV2() ([]protov2.Message, error)  { return nil, nil }
func (feeTxStub) ValidateBasic() error                   { return nil }
func (feeTxStub) GetSigners() []sdk.AccAddress           { return nil }
func (s feeTxStub) GetGas() uint64                       { return s.gas }
func (s feeTxStub) GetFee() sdk.Coins                    { return s.fee }
func (s feeTxStub) FeePayer() []byte                     { return s.payer }
func (feeTxStub) FeeGranter() []byte                     { return nil }

// fundModule mints upole into a module account so a burn channel has
// something to destroy.
func fundModule(t *testing.T, app *App, ctx sdk.Context, module string, amount int64) {
	t.Helper()
	coins := sdk.NewCoins(sdk.NewCoin(types.BaseDenom, sdkmath.NewInt(amount)))
	if err := app.BankKeeper.MintCoins(ctx, types.ModuleName, coins); err != nil {
		t.Fatalf("mint: %v", err)
	}
	if module != types.ModuleName {
		if err := app.BankKeeper.SendCoinsFromModuleToModule(ctx, types.ModuleName, module, coins); err != nil {
			t.Fatalf("fund %s: %v", module, err)
		}
	}
}

// fundAccount gives an address spendable upole and creates its account.
func fundAccount(t *testing.T, app *App, ctx sdk.Context, addr sdk.AccAddress, amount int64) {
	t.Helper()
	app.AccountKeeper.SetAccount(ctx, app.AccountKeeper.NewAccountWithAddress(ctx, addr))
	coins := sdk.NewCoins(sdk.NewCoin(types.BaseDenom, sdkmath.NewInt(amount)))
	if err := app.BankKeeper.MintCoins(ctx, types.ModuleName, coins); err != nil {
		t.Fatalf("mint: %v", err)
	}
	if err := app.BankKeeper.SendCoinsFromModuleToAccount(ctx, types.ModuleName, addr, coins); err != nil {
		t.Fatalf("fund account: %v", err)
	}
}

func moduleBalance(app *App, ctx sdk.Context, module string) sdkmath.Int {
	return app.BankKeeper.GetBalance(ctx, authtypes.NewModuleAddress(module), types.BaseDenom).Amount
}

func supply(app *App, ctx sdk.Context) sdkmath.Int {
	return app.BankKeeper.GetSupply(ctx, types.BaseDenom).Amount
}

// TestFeeBurnShareMatchesConfiguredBps pins the pure share arithmetic.
func TestFeeBurnShareMatchesConfiguredBps(t *testing.T) {
	fee := sdk.NewCoins(sdk.NewCoin(types.BaseDenom, sdkmath.NewInt(4_000)))
	if got := feeBurnShare(fee, 2_500); got != 1_000 {
		t.Fatalf("expected fee burn 1_000 at 2500 bps, got %d", got)
	}
	if got := feeBurnShare(fee, 0); got != 0 {
		t.Fatalf("expected no burn at 0 bps, got %d", got)
	}
	if got := feeBurnShare(fee, 20_000); got != 4_000 {
		t.Fatalf("expected burn clamped to the fee, got %d", got)
	}
	otherDenom := sdk.NewCoins(sdk.NewCoin("uatom", sdkmath.NewInt(4_000)))
	if got := feeBurnShare(otherDenom, 2_500); got != 0 {
		t.Fatalf("expected non-upole fee to be ignored, got %d", got)
	}
}

// TestFeeBurnDecoratorDestroysCollectedShare is the fee_burn_bps channel: the
// fee collector already holds the collected fee, and the ante step destroys the
// configured share of it.
func TestFeeBurnDecoratorDestroysCollectedShare(t *testing.T) {
	app := initTestApp(t)
	ctx := initTestContext(app).WithBlockHeight(10)

	fundModule(t, app, ctx, authtypes.FeeCollectorName, 10_000)
	before := moduleBalance(app, ctx, authtypes.FeeCollectorName)
	supplyBefore := supply(app, ctx)

	decorator := feeBurnDecorator{bankKeeper: app.BankKeeper, poleKeeper: app.PoleKeeper}
	tx := feeTxStub{
		fee:   sdk.NewCoins(sdk.NewCoin(types.BaseDenom, sdkmath.NewInt(4_000))),
		gas:   200_000,
		payer: sdk.AccAddress(bytes.Repeat([]byte{1}, 20)).Bytes(),
	}
	next := func(ctx sdk.Context, _ sdk.Tx, _ bool) (sdk.Context, error) { return ctx, nil }
	if _, err := decorator.anteHandle(ctx, tx, false, next); err != nil {
		t.Fatalf("fee burn ante: %v", err)
	}

	after := moduleBalance(app, ctx, authtypes.FeeCollectorName)
	if !after.Equal(before.Sub(sdkmath.NewInt(1_000))) {
		t.Fatalf("expected collector to drop by 1_000, got %s -> %s", before.String(), after.String())
	}
	if !supply(app, ctx).Equal(supplyBefore.Sub(sdkmath.NewInt(1_000))) {
		t.Fatalf("expected supply to drop by 1_000, got %s -> %s", supplyBefore.String(), supply(app, ctx).String())
	}
}

// TestFeeBurnDecoratorSkipsSimulation keeps query simulation free of state
// mutation.
func TestFeeBurnDecoratorSkipsSimulation(t *testing.T) {
	app := initTestApp(t)
	ctx := initTestContext(app).WithBlockHeight(10)
	fundModule(t, app, ctx, authtypes.FeeCollectorName, 10_000)
	before := moduleBalance(app, ctx, authtypes.FeeCollectorName)

	decorator := feeBurnDecorator{bankKeeper: app.BankKeeper, poleKeeper: app.PoleKeeper}
	tx := feeTxStub{fee: sdk.NewCoins(sdk.NewCoin(types.BaseDenom, sdkmath.NewInt(4_000))), gas: 200_000}
	next := func(ctx sdk.Context, _ sdk.Tx, _ bool) (sdk.Context, error) { return ctx, nil }
	if _, err := decorator.anteHandle(ctx, tx, true, next); err != nil {
		t.Fatalf("fee burn ante: %v", err)
	}
	if !moduleBalance(app, ctx, authtypes.FeeCollectorName).Equal(before) {
		t.Fatalf("expected simulation to leave the collector untouched")
	}
}

// TestChallengeBondEscrowAndReturn covers the two challenge_bond_burn_bps
// outcomes that keep the bond: escrow on open and full return on an upheld
// challenge.
func TestChallengeBondEscrowAndReturn(t *testing.T) {
	app := initTestApp(t)
	ctx := initTestContext(app).WithBlockHeight(50)

	challengerAddr := sdk.AccAddress(bytes.Repeat([]byte{5}, 20))
	challenger, err := app.AccountKeeper.AddressCodec().BytesToString(challengerAddr)
	if err != nil {
		t.Fatalf("challenger bech32: %v", err)
	}
	fundAccount(t, app, ctx, challengerAddr, 10_000)
	registerNode(t, app, ctx, challenger, &types.NodeCapabilitySet{Verify: true})
	if err := app.PoleKeeper.SetEpochCommit(ctx, types.EpochCommit{EpochId: 1}); err != nil {
		t.Fatalf("set epoch commit: %v", err)
	}

	openHandler := app.MsgServiceRouter().Handler(&types.MsgOpenChallenge{})
	if _, err := openHandler(ctx, &types.MsgOpenChallenge{
		Challenger: challenger,
		Challenge: &types.Challenge{
			ChallengeIdHex: "bond-1",
			Kind:           types.ChallengeKindBadAggregate,
			EpochId:        1,
			TargetAddress:  challenger,
			Challenger:     challenger,
			BondAmount:     1_000,
			DeadlineHeight: 100,
		},
	}); err != nil {
		t.Fatalf("open challenge: %v", err)
	}

	challengerBalance := app.BankKeeper.GetBalance(ctx, challengerAddr, types.BaseDenom).Amount
	if !challengerBalance.Equal(sdkmath.NewInt(9_000)) {
		t.Fatalf("expected challenger to lock 1_000, got %s", challengerBalance.String())
	}
	if !moduleBalance(app, ctx, types.ModuleName).Equal(sdkmath.NewInt(1_000)) {
		t.Fatalf("expected pool to hold the escrowed bond, got %s", moduleBalance(app, ctx, types.ModuleName).String())
	}

	govAuthority, err := app.AccountKeeper.AddressCodec().BytesToString(authtypes.NewModuleAddress(govtypes.ModuleName))
	if err != nil {
		t.Fatalf("gov authority: %v", err)
	}
	resolveHandler := app.MsgServiceRouter().Handler(&types.MsgResolveChallenge{})
	if _, err := resolveHandler(ctx, &types.MsgResolveChallenge{
		Resolver:          govAuthority,
		ChallengeIdHex:    "bond-1",
		ResolutionSummary: "challenge upheld",
		FinalState:        types.ChallengeStateResolved,
	}); err != nil {
		t.Fatalf("resolve challenge: %v", err)
	}

	if got := app.BankKeeper.GetBalance(ctx, challengerAddr, types.BaseDenom).Amount; !got.Equal(sdkmath.NewInt(10_000)) {
		t.Fatalf("expected upheld bond to return, got %s", got.String())
	}
	if !moduleBalance(app, ctx, types.ModuleName).IsZero() {
		t.Fatalf("expected pool to be emptied by the refund, got %s", moduleBalance(app, ctx, types.ModuleName).String())
	}
}

// TestChallengeBondForfeitBurnsConfiguredShare is the challenge_bond_burn_bps
// channel: a rejected challenge forfeits the bond and destroys its configured
// share, while the remainder stays with the protocol.
func TestChallengeBondForfeitBurnsConfiguredShare(t *testing.T) {
	app := initTestApp(t)
	ctx := initTestContext(app).WithBlockHeight(50)

	challengerAddr := sdk.AccAddress(bytes.Repeat([]byte{6}, 20))
	challenger, err := app.AccountKeeper.AddressCodec().BytesToString(challengerAddr)
	if err != nil {
		t.Fatalf("challenger bech32: %v", err)
	}
	fundAccount(t, app, ctx, challengerAddr, 10_000)
	registerNode(t, app, ctx, challenger, &types.NodeCapabilitySet{Verify: true})
	if err := app.PoleKeeper.SetEpochCommit(ctx, types.EpochCommit{EpochId: 1}); err != nil {
		t.Fatalf("set epoch commit: %v", err)
	}

	openHandler := app.MsgServiceRouter().Handler(&types.MsgOpenChallenge{})
	if _, err := openHandler(ctx, &types.MsgOpenChallenge{
		Challenger: challenger,
		Challenge: &types.Challenge{
			ChallengeIdHex: "bond-2",
			Kind:           types.ChallengeKindBadAggregate,
			EpochId:        1,
			TargetAddress:  challenger,
			Challenger:     challenger,
			BondAmount:     1_000,
			DeadlineHeight: 100,
		},
	}); err != nil {
		t.Fatalf("open challenge: %v", err)
	}

	supplyBefore := supply(app, ctx)
	govAuthority, err := app.AccountKeeper.AddressCodec().BytesToString(authtypes.NewModuleAddress(govtypes.ModuleName))
	if err != nil {
		t.Fatalf("gov authority: %v", err)
	}
	resolveHandler := app.MsgServiceRouter().Handler(&types.MsgResolveChallenge{})
	if _, err := resolveHandler(ctx, &types.MsgResolveChallenge{
		Resolver:          govAuthority,
		ChallengeIdHex:    "bond-2",
		ResolutionSummary: "challenge rejected",
		FinalState:        types.ChallengeStateRejected,
	}); err != nil {
		t.Fatalf("resolve challenge: %v", err)
	}

	if got := app.BankKeeper.GetBalance(ctx, challengerAddr, types.BaseDenom).Amount; !got.Equal(sdkmath.NewInt(9_000)) {
		t.Fatalf("expected forfeited bond to stay with the protocol, got %s", got.String())
	}
	// Default challenge_bond_burn_bps is 2500, so 250 of the 1_000 bond is
	// destroyed and 750 stays in the reward pool.
	if got := moduleBalance(app, ctx, types.ModuleName); !got.Equal(sdkmath.NewInt(750)) {
		t.Fatalf("expected pool to keep 750 after forfeit, got %s", got.String())
	}
	if got := supply(app, ctx); !got.Equal(supplyBefore.Sub(sdkmath.NewInt(250))) {
		t.Fatalf("expected supply to drop by 250, got %s -> %s", supplyBefore.String(), got.String())
	}
}

// TestSessionSlashBurnsRewardOnBadRewardChallenge is the session_slash_bps
// channel: a bad-reward challenge invalidates the play session, so the player
// reward behind it is slashed and destroyed.
func TestSessionSlashBurnsRewardOnBadRewardChallenge(t *testing.T) {
	app := initTestApp(t)
	ctx := initTestContext(app).WithBlockHeight(50)

	targetAddr := sdk.AccAddress(bytes.Repeat([]byte{7}, 20))
	target, err := app.AccountKeeper.AddressCodec().BytesToString(targetAddr)
	if err != nil {
		t.Fatalf("target bech32: %v", err)
	}
	challengerAddr := sdk.AccAddress(bytes.Repeat([]byte{8}, 20))
	challenger, err := app.AccountKeeper.AddressCodec().BytesToString(challengerAddr)
	if err != nil {
		t.Fatalf("challenger bech32: %v", err)
	}
	registerNode(t, app, ctx, target, &types.NodeCapabilitySet{Verify: true})
	registerNode(t, app, ctx, challenger, &types.NodeCapabilitySet{Verify: true})

	if err := app.PoleKeeper.SetRewardRecord(ctx, types.RewardRecord{
		EpochId: 3, Recipient: target, PlayerReward: 100_000, NetReward: 100_000,
	}); err != nil {
		t.Fatalf("set target reward: %v", err)
	}
	if err := app.PoleKeeper.SetEpochCommit(ctx, types.EpochCommit{EpochId: 3}); err != nil {
		t.Fatalf("set epoch commit: %v", err)
	}
	// The slash debits the reward pool, so the pool must hold real coins;
	// otherwise the bounded burn would be a no-op by design.
	fundModule(t, app, ctx, types.ModuleName, 50_000)
	if err := app.PoleKeeper.SetChallenge(ctx, types.Challenge{
		ChallengeIdHex: "slash-1",
		Kind:           types.ChallengeKindBadReward,
		EpochId:        3,
		TargetAddress:  target,
		Challenger:     challenger,
		State:          types.ChallengeStateOpen,
	}); err != nil {
		t.Fatalf("set challenge: %v", err)
	}

	moduleBefore := moduleBalance(app, ctx, types.ModuleName)
	supplyBefore := supply(app, ctx)
	govAuthority, err := app.AccountKeeper.AddressCodec().BytesToString(authtypes.NewModuleAddress(govtypes.ModuleName))
	if err != nil {
		t.Fatalf("gov authority: %v", err)
	}
	handler := app.MsgServiceRouter().Handler(&types.MsgResolveChallenge{})
	if _, err := handler(ctx, &types.MsgResolveChallenge{
		Resolver:          govAuthority,
		ChallengeIdHex:    "slash-1",
		SlashAmount:       10_000,
		ResolutionSummary: "fabricated play session",
		FinalState:        types.ChallengeStateResolved,
	}); err != nil {
		t.Fatalf("resolve challenge: %v", err)
	}

	record, err := app.PoleKeeper.GetRewardRecord(ctx, 3, target)
	if err != nil {
		t.Fatalf("target reward: %v", err)
	}
	// Governance burn (100 bps of 10_000 = 100) plus session slash (5000 bps
	// of the remaining 90_000 = 45_000), both destroyed from the pool.
	wantSlash := uint64(10_000 + 45_000)
	if record.SlashDebit != wantSlash {
		t.Fatalf("expected slash debit %d, got %d", wantSlash, record.SlashDebit)
	}
	if record.NetReward != 100_000-wantSlash {
		t.Fatalf("expected net reward %d, got %d", 100_000-wantSlash, record.NetReward)
	}
	burned := uint64(100 + 45_000)
	if got := moduleBalance(app, ctx, types.ModuleName); !got.Equal(moduleBefore.Sub(sdkmath.NewIntFromUint64(burned))) {
		t.Fatalf("expected pool to drop by %d, got %s -> %s", burned, moduleBefore.String(), got.String())
	}
	if got := supply(app, ctx); !got.Equal(supplyBefore.Sub(sdkmath.NewIntFromUint64(burned))) {
		t.Fatalf("expected supply to drop by %d, got %s -> %s", burned, supplyBefore.String(), got.String())
	}
}

// TestNetSupplyChangeEqualsEmissionMinusBurn closes the model: across a block
// that both mints the reference emission and destroys fee share, the observed
// supply change equals emission minus burn.
func TestNetSupplyChangeEqualsEmissionMinusBurn(t *testing.T) {
	app := initTestApp(t)
	ctx := initTestContext(app).WithBlockHeight(20)
	if err := app.PoleKeeper.InitAnnualEmission(ctx); err != nil {
		t.Fatalf("reset annual emission: %v", err)
	}

	supplyBefore := supply(app, ctx)

	fundModule(t, app, ctx, authtypes.FeeCollectorName, 10_000)
	supplyAfterFunding := supply(app, ctx)

	// Burn the fee share first, then run the reference emission for the block.
	decorator := feeBurnDecorator{bankKeeper: app.BankKeeper, poleKeeper: app.PoleKeeper}
	tx := feeTxStub{fee: sdk.NewCoins(sdk.NewCoin(types.BaseDenom, sdkmath.NewInt(40_000))), gas: 200_000}
	next := func(ctx sdk.Context, _ sdk.Tx, _ bool) (sdk.Context, error) { return ctx, nil }
	if _, err := decorator.anteHandle(ctx, tx, false, next); err != nil {
		t.Fatalf("fee burn ante: %v", err)
	}
	// The collector only holds 10_000, so the bounded channel destroys the
	// whole balance rather than the nominal 10_000 share of 40_000.
	feeBurn := sdkmath.NewInt(10_000)

	emissionCtx := ctx.WithBlockTime(ctx.BlockTime().Add(3600 * 1e9))
	if err := app.PoleKeeper.BeginBlockAnnualEmission(emissionCtx); err != nil {
		t.Fatalf("begin block emission: %v", err)
	}
	emission := moduleBalance(app, emissionCtx, types.ModuleName)

	supplyAfter := supply(app, emissionCtx)
	observed := supplyAfter.Sub(supplyAfterFunding)
	expected := emission.Sub(feeBurn)
	if !observed.Equal(expected) {
		t.Fatalf("expected net supply change %s (emission %s - burn %s), got %s",
			expected.String(), emission.String(), feeBurn.String(), observed.String())
	}
	_ = supplyBefore
}
