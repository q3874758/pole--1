package keeper

import (
	"context"
	"fmt"

	sdkmath "cosmossdk.io/math"
	sdk "github.com/cosmos/cosmos-sdk/types"
	authtypes "github.com/cosmos/cosmos-sdk/x/auth/types"

	"pole/chain/x/pole/types"
)

// The burn channels are what keep a no-cap issuance model from inflating
// without bound: Net Supply = Emission - Burn. Every channel funnels through
// burnFromModule so the destroyed quantity is always bounded by coins the
// protocol actually holds, and a channel can never destroy more than it
// collected.

// BurnFromRewardPool destroys up to amount from the pole reward pool. It never
// fails on a short pool; it returns the quantity actually destroyed so callers
// can account for it.
func (k Keeper) BurnFromRewardPool(ctx context.Context, amount uint64) (uint64, error) {
	return k.burnFromModule(ctx, types.ModuleName, amount)
}

// BurnFeeCoins destroys up to amount from the fee collector module account. It
// is the sink for the fee_burn_bps channel.
func (k Keeper) BurnFeeCoins(ctx context.Context, amount uint64) (uint64, error) {
	return k.burnFromModule(ctx, authtypes.FeeCollectorName, amount)
}

func (k Keeper) burnFromModule(ctx context.Context, moduleName string, amount uint64) (uint64, error) {
	if k.bankKeeper == nil {
		return 0, fmt.Errorf("bank keeper is not configured")
	}
	if amount == 0 {
		return 0, nil
	}
	moduleAddr := authtypes.NewModuleAddress(moduleName)
	balance := k.bankKeeper.GetBalance(ctx, moduleAddr, types.BaseDenom)
	if !balance.Amount.IsPositive() {
		return 0, nil
	}
	burnable := balance.Amount.Uint64()
	if burnable > amount {
		burnable = amount
	}
	if burnable == 0 {
		return 0, nil
	}
	coins := sdk.NewCoins(sdk.NewCoin(types.BaseDenom, sdkmath.NewIntFromUint64(burnable)))
	if err := k.bankKeeper.BurnCoins(ctx, moduleName, coins); err != nil {
		return 0, err
	}
	return burnable, nil
}

// EscrowChallengeBond moves a challenger's bond into the pole module account so
// a losing challenge can be forfeited. A zero bond is a no-op, which keeps
// bond-less challenges valid.
func (k Keeper) EscrowChallengeBond(ctx context.Context, challenger string, amount uint64) error {
	if amount == 0 {
		return nil
	}
	if k.bankKeeper == nil {
		return fmt.Errorf("bank keeper is not configured")
	}
	addr, err := sdk.AccAddressFromBech32(challenger)
	if err != nil {
		return err
	}
	coins := sdk.NewCoins(sdk.NewCoin(types.BaseDenom, sdkmath.NewIntFromUint64(amount)))
	return k.bankKeeper.SendCoinsFromAccountToModule(ctx, addr, types.ModuleName, coins)
}

// SettleChallengeBond resolves an escrowed bond. When the challenge held up the
// full bond returns to the challenger. When it was rejected (forfeit) the
// challenge_bond_burn_bps share is destroyed and the remainder stays in the
// reward pool for honest participants. It returns the destroyed quantity.
func (k Keeper) SettleChallengeBond(ctx context.Context, challenger string, bond uint64, burnBps uint32, forfeit bool) (uint64, error) {
	if bond == 0 {
		return 0, nil
	}
	if k.bankKeeper == nil {
		return 0, fmt.Errorf("bank keeper is not configured")
	}
	if !forfeit {
		addr, err := sdk.AccAddressFromBech32(challenger)
		if err != nil {
			return 0, err
		}
		coins := sdk.NewCoins(sdk.NewCoin(types.BaseDenom, sdkmath.NewIntFromUint64(bond)))
		if err := k.bankKeeper.SendCoinsFromModuleToAccount(ctx, types.ModuleName, addr, coins); err != nil {
			return 0, err
		}
		return 0, nil
	}
	if burnBps > 10_000 {
		burnBps = 10_000
	}
	return k.BurnFromRewardPool(ctx, bond*uint64(burnBps)/10_000)
}
