package app

import (
	"testing"
	"time"

	sdkmath "cosmossdk.io/math"

	sdk "github.com/cosmos/cosmos-sdk/types"

	"pole/chain/x/pole/types"
)

// The no-cap issuance model (D2) makes a long-run supply simulation a
// release gate rather than a nice-to-have: issuance follows confirmed
// activity without a hard ceiling, so the only things standing between the
// model and runaway inflation are the decaying reference curve and the burn
// channels. These tests run the real keeper over decades of protocol time
// and pin the two properties that risk depends on:
//
//  1. the reference curve is monotone non-increasing and settles onto a flat
//     tail, so the inflation *rate* falls as supply accumulates;
//  2. burns track issuance, so net supply is emission minus burn and never
//     grows faster than the curve allows.
//
// Neither test asserts a hard ceiling anywhere — that is the point. Issuance
// must keep flowing in year 20 exactly as it does in year 4.

// annualMint advances the chain twelve monthly blocks and returns the
// resulting context plus the coins the reference curve minted over that
// protocol year.
func annualMint(t *testing.T, app *App, ctx sdk.Context) (sdk.Context, sdkmath.Int) {
	t.Helper()
	before := moduleBalance(app, ctx, types.ModuleName)
	for month := 0; month < int(types.PeriodsPerYear); month++ {
		ctx = ctx.WithBlockTime(ctx.BlockTime().Add(
			time.Duration(types.SecondsPerMonth) * time.Second,
		))
		if err := app.PoleKeeper.BeginBlockAnnualEmission(ctx); err != nil {
			t.Fatalf("begin block emission at month %d: %v", month+1, err)
		}
	}
	after := moduleBalance(app, ctx, types.ModuleName)
	return ctx, after.Sub(before)
}

// expectedYearMint is the reference curve's annual issuance as the keeper
// actually delivers it for the twelve monthly steps that open a protocol
// year. The year index rolls over *before* the boundary step mints, so the
// twelfth payment of year N is already charged at year N+1's rate: the
// annual total is eleven months at this year's rate plus one at the next.
// Year five onward the rate is the flat tail, so the bias disappears.
func expectedYearMint(year uint32) sdkmath.Int {
	monthly := types.AnnualEmissionAmount(year) / types.PeriodsPerYear
	nextMonthly := types.AnnualEmissionAmount(year+1) / types.PeriodsPerYear
	return sdkmath.NewIntFromUint64(monthly*(types.PeriodsPerYear-1) + nextMonthly)
}

// TestLongRunSupplyFollowsDecayingCurveWithoutExponentialBlowup runs twenty
// protocol years of real emission and checks the curve's shape: issuance is
// exactly the reference amount each year, it never rises, and it lands on a
// flat tail that keeps paying out forever instead of hitting a ceiling.
func TestLongRunSupplyFollowsDecayingCurveWithoutExponentialBlowup(t *testing.T) {
	app := initTestApp(t)
	ctx := initTestContext(app).WithBlockTime(time.Unix(1_700_000_000, 0))
	if err := app.PoleKeeper.InitAnnualEmission(ctx); err != nil {
		t.Fatalf("init annual emission: %v", err)
	}

	const years = 20
	perYear := make([]sdkmath.Int, 0, years)
	total := sdkmath.ZeroInt()
	expectedTotal := sdkmath.ZeroInt()
	for year := uint32(1); year <= years; year++ {
		var minted sdkmath.Int
		ctx, minted = annualMint(t, app, ctx)
		perYear = append(perYear, minted)
		total = total.Add(minted)

		want := expectedYearMint(year)
		expectedTotal = expectedTotal.Add(want)
		if !minted.Equal(want) {
			t.Fatalf("year %d emitted %s, want %s", year, minted.String(), want.String())
		}
	}

	// Reference amounts at the regime boundaries, as whole micro-denom
	// units actually minted. Years one and two share the 20% rate, year
	// three halves to 10%, and year four lands on the flat 2% tail — but
	// because the twelfth monthly step of a year is already charged at the
	// next year's rate, each boundary year emits slightly less than its own
	// nominal amount.
	year1 := perYear[0].Uint64()
	if year1 != 199_999_992 {
		t.Fatalf("year 1 should pay the 20%% reference (199_999_992 after truncation), got %d", year1)
	}
	year3 := perYear[2].Uint64()
	if year3 != 93_333_329 {
		t.Fatalf("year 3 should halve to the 10%% reference (93_333_329), got %d", year3)
	}
	tail := perYear[3].Uint64()
	if tail != 19_999_992 {
		t.Fatalf("year 4 should drop to the 2%% tail, got %d", tail)
	}

	// Monotone non-increasing: the curve must never rise, or a later year
	// would be inflating harder than an earlier one for no reason.
	for i := 1; i < len(perYear); i++ {
		if perYear[i].GT(perYear[i-1]) {
			t.Fatalf(
				"emission rose from year %d (%s) to year %d (%s)",
				i, perYear[i-1].String(), i+1, perYear[i].String(),
			)
		}
	}

	// The tail is flat and non-zero: twenty years in, issuance is still
	// paying the same absolute amount as year 4. Nothing capped it.
	if perYear[years-1].IsZero() {
		t.Fatalf("issuance stopped in year %d", years)
	}
	if !perYear[years-1].Equal(perYear[years-2]) {
		t.Fatalf(
			"tail years should pay a constant amount, got %s then %s",
			perYear[years-2].String(), perYear[years-1].String(),
		)
	}

	// Growth is linear in the tail, never geometric: cumulative issuance
	// over twenty years is exactly the sum of the per-year reference
	// amounts the keeper delivers.
	bound := expectedTotal
	if !total.Equal(bound) {
		t.Fatalf("expected cumulative issuance %s, got %s", bound.String(), total.String())
	}

	// The inflation *rate* is what actually decays: the final year's
	// emission is a shrinking fraction of the supply it sits on.
	rateBps := perYear[years-1].MulRaw(10_000).Quo(total)
	if !rateBps.LT(sdkmath.NewInt(1_300)) {
		t.Fatalf(
			"year-%d emission should be well under 13%% of cumulative supply, got %s bps",
			years, rateBps.String(),
		)
	}

	// And the pool holds exactly what was minted, since nothing claimed it.
	if pool := moduleBalance(app, ctx, types.ModuleName); !pool.Equal(total) {
		t.Fatalf("expected the pool to hold all minted supply %s, got %s", total.String(), pool.String())
	}
}

// TestLongRunNetSupplyStaysBelowEmissionWhenBurnsFollowActivity drives the
// same twenty years, but destroys the reward-burn share of each year's
// issuance as if every minted coin were claimed. This is the mitigation the
// risk register leans on: burns that scale with issuance, so net supply is
// emission minus burn rather than emission alone.
func TestLongRunNetSupplyStaysBelowEmissionWhenBurnsFollowActivity(t *testing.T) {
	app := initTestApp(t)
	ctx := initTestContext(app).WithBlockTime(time.Unix(1_700_000_000, 0))
	if err := app.PoleKeeper.InitAnnualEmission(ctx); err != nil {
		t.Fatalf("init annual emission: %v", err)
	}
	params, err := app.PoleKeeper.GetParams(ctx)
	if err != nil {
		t.Fatalf("params: %v", err)
	}
	if params.RewardBurnBps == 0 {
		t.Fatalf("expected a non-zero reward_burn_bps channel to simulate")
	}

	const years = 20
	cumulativeMinted := sdkmath.ZeroInt()
	cumulativeBurned := sdkmath.ZeroInt()
	supplyBefore := supply(app, ctx)

	for year := uint32(1); year <= years; year++ {
		var minted sdkmath.Int
		ctx, minted = annualMint(t, app, ctx)
		cumulativeMinted = cumulativeMinted.Add(minted)

		// Activity model: the whole year's issuance is claimed, and each
		// claim above the burn threshold destroys its configured share.
		// The pool holds exactly the minted coins, so the burn is bounded
		// by real holdings and can never exceed them.
		burnable := minted.MulRaw(int64(params.RewardBurnBps)).QuoRaw(10_000)
		burned, err := app.PoleKeeper.BurnFromRewardPool(ctx, burnable.Uint64())
		if err != nil {
			t.Fatalf("burn year %d: %v", year, err)
		}
		if burned != burnable.Uint64() {
			t.Fatalf("year %d: expected to burn %s, burned %d", year, burnable.String(), burned)
		}
		cumulativeBurned = cumulativeBurned.Add(burnable)

		// The pool holds everything minted so far minus everything burned
		// so far, because this simulation never pays any of it out. If
		// either channel dropped or double-counted coins, this diverges.
		wantPool := cumulativeMinted.Sub(cumulativeBurned)
		if pool := moduleBalance(app, ctx, types.ModuleName); !pool.Equal(wantPool) {
			t.Fatalf(
				"year %d: pool should hold %s (minted %s minus burned %s), got %s",
				year, wantPool.String(), cumulativeMinted.String(), cumulativeBurned.String(), pool.String(),
			)
		}
	}

	emission := supply(app, ctx).Sub(supplyBefore)
	expectedNet := cumulativeMinted.Sub(cumulativeBurned)
	if !emission.Equal(expectedNet) {
		t.Fatalf(
			"net supply %s should equal emission %s minus burn %s",
			emission.String(), cumulativeMinted.String(), cumulativeBurned.String(),
		)
	}
	if !cumulativeBurned.IsPositive() {
		t.Fatalf("expected burns to accumulate over twenty years")
	}

	// Net supply must stay strictly below raw issuance: this is the only
	// reason a no-cap model is sustainable at all.
	if !emission.LT(cumulativeMinted) {
		t.Fatalf("net supply %s should be below raw emission %s", emission.String(), cumulativeMinted.String())
	}

	// The share removed by burns is bounded above by the configured rate,
	// so the burn channel can never silently become the dominant sink and
	// starve the reward pool.
	burnShare := cumulativeBurned.MulRaw(10_000).Quo(cumulativeMinted)
	if burnShare.GT(sdkmath.NewInt(int64(params.RewardBurnBps))) {
		t.Fatalf(
			"burns removed %s bps of issuance, above the configured %d bps",
			burnShare.String(), params.RewardBurnBps,
		)
	}

	// Over twenty years the burned share is close to the configured rate,
	// i.e. the loop is in the regime the parameter was chosen for.
	if burnShare.LT(sdkmath.NewInt(int64(params.RewardBurnBps)-10)) {
		t.Fatalf(
			"burns removed only %s bps of issuance, expected close to %d bps",
			burnShare.String(), params.RewardBurnBps,
		)
	}

	t.Logf(
		"20-year simulation: emission %s, burn %s (%s bps), net supply %s",
		cumulativeMinted.String(), cumulativeBurned.String(), burnShare.String(), emission.String(),
	)
}

// TestLongRunSupplyInflationRateFallsMonotonically is the invariant the
// whole no-cap model rests on: because the tail pays a constant absolute
// amount while supply accumulates, the issuance *rate* falls every year. A
// model whose rate refused to fall would not be a decaying emission curve.
func TestLongRunSupplyInflationRateFallsMonotonically(t *testing.T) {
	app := initTestApp(t)
	ctx := initTestContext(app).WithBlockTime(time.Unix(1_700_000_000, 0))
	if err := app.PoleKeeper.InitAnnualEmission(ctx); err != nil {
		t.Fatalf("init annual emission: %v", err)
	}

	const years = 30
	var cumulative sdkmath.Int = sdkmath.ZeroInt()
	rates := make([]sdkmath.Int, 0, years)
	for year := uint32(1); year <= years; year++ {
		var minted sdkmath.Int
		ctx, minted = annualMint(t, app, ctx)
		cumulative = cumulative.Add(minted)
		// Issuance rate = this year's emission as bps of the supply it
		// lands on (the supply at the start of the year).
		startOfYear := cumulative.Sub(minted)
		if startOfYear.IsZero() {
			continue
		}
		rates = append(rates, minted.MulRaw(10_000).Quo(startOfYear))
	}

	if len(rates) < 2 {
		t.Fatalf("expected several measurable years, got %d", len(rates))
	}
	for i := 1; i < len(rates); i++ {
		if !rates[i].LT(rates[i-1]) {
			t.Fatalf(
				"issuance rate should fall every year: rate %d was %s bps, rate %d was %s bps",
				i, rates[i-1].String(), i+1, rates[i].String(),
			)
		}
	}

	// After thirty years the tail rate has decayed toward the ratio of the
	// flat tail payment to accumulated supply.
	last := rates[len(rates)-1]
	t.Logf("issuance rate fell from %s bps to %s bps over %d years", rates[0].String(), last.String(), years)
	if !last.LT(sdkmath.NewInt(1_200)) {
		t.Fatalf("expected the tail issuance rate to sit below 12%% after %d years, got %s bps", years, last.String())
	}

	// Sanity: the pool holds every minted coin, so the accounting above
	// is not missing a sink.
	if pool := moduleBalance(app, ctx, types.ModuleName); !pool.Equal(cumulative) {
		t.Fatalf("pool %s should equal cumulative minted %s", pool.String(), cumulative.String())
	}
}
