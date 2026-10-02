package app

import (
	sdk "github.com/cosmos/cosmos-sdk/types"
	authkeeper "github.com/cosmos/cosmos-sdk/x/auth/keeper"
	"github.com/cosmos/cosmos-sdk/x/auth/ante"
	bankkeeper "github.com/cosmos/cosmos-sdk/x/bank/keeper"
	txsigning "github.com/cosmos/cosmos-sdk/x/tx/signing"

	polekeeper "pole/chain/x/pole/keeper"
	poletypes "pole/chain/x/pole/types"
)

// feeBurnShare reports the quantity of a collected fee that the fee_burn_bps
// channel destroys. It only ever considers the chain's base denom and never
// returns more than the fee actually paid.
func feeBurnShare(fee sdk.Coins, bps uint32) uint64 {
	if bps == 0 {
		return 0
	}
	if bps > 10_000 {
		bps = 10_000
	}
	amount := fee.AmountOf(poletypes.BaseDenom)
	if amount.IsNil() || !amount.IsPositive() {
		return 0
	}
	return amount.Uint64() * uint64(bps) / 10_000
}

// feeBurnDecorator is the ante-side half of the fee_burn_bps channel: the SDK's
// DeductFeeDecorator moves the whole fee into the fee collector module account
// and has no burn hook, so the configured share is destroyed here, after
// collection has already succeeded. The collector therefore needs Burner
// permission (see moduleAccountPermissions in app.go).
//
// The step is a no-op during simulation, when fee_burn_bps is zero, when the tx
// pays no upole, and when the collector holds less than the computed share.
type feeBurnDecorator struct {
	bankKeeper bankkeeper.BaseKeeper
	poleKeeper polekeeper.Keeper
}

func (d feeBurnDecorator) anteHandle(
	ctx sdk.Context, tx sdk.Tx, simulate bool, next sdk.AnteHandler,
) (sdk.Context, error) {
	newCtx, err := next(ctx, tx, simulate)
	if err != nil {
		return newCtx, err
	}
	if simulate {
		return newCtx, nil
	}
	params, err := d.poleKeeper.GetParams(newCtx)
	if err != nil {
		return newCtx, err
	}
	if params.FeeBurnBps == 0 {
		return newCtx, nil
	}
	feeTx, ok := tx.(sdk.FeeTx)
	if !ok {
		return newCtx, nil
	}
	burn := feeBurnShare(feeTx.GetFee(), params.FeeBurnBps)
	if burn == 0 {
		return newCtx, nil
	}
	if _, err := d.poleKeeper.BurnFeeCoins(newCtx, burn); err != nil {
		return newCtx, err
	}
	return newCtx, nil
}

// NewAppAnteHandler builds the ante chain used by the node: the stock SDK chain
// (validate, verify signatures, deduct fees, bump sequences) wrapped by the fee
// destruction step that implements fee_burn_bps.
func NewAppAnteHandler(
	accountKeeper authkeeper.AccountKeeper,
	bankKeeper bankkeeper.BaseKeeper,
	poleKeeper polekeeper.Keeper,
	signModeHandler *txsigning.HandlerMap,
) (sdk.AnteHandler, error) {
	chainAnte, err := ante.NewAnteHandler(ante.HandlerOptions{
		AccountKeeper:   accountKeeper,
		BankKeeper:      bankKeeper,
		SignModeHandler: signModeHandler,
	})
	if err != nil {
		return nil, err
	}

	burner := feeBurnDecorator{bankKeeper: bankKeeper, poleKeeper: poleKeeper}
	return func(ctx sdk.Context, tx sdk.Tx, simulate bool) (sdk.Context, error) {
		return burner.anteHandle(ctx, tx, simulate, chainAnte)
	}, nil
}
