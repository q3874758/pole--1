package app

import (
	"bytes"
	"strings"
	"testing"

	sdk "github.com/cosmos/cosmos-sdk/types"

	"pole/chain/x/pole/types"
)

// bech32Addr builds a distinct valid bech32 address from a repeated byte.
func bech32Addr(t *testing.T, app *App, fill byte) string {
	t.Helper()
	addr := sdk.AccAddress(bytes.Repeat([]byte{fill}, 20))
	encoded, err := app.AccountKeeper.AddressCodec().BytesToString(addr)
	if err != nil {
		t.Fatalf("bech32 for fill %d: %v", fill, err)
	}
	return encoded
}

// registerNode registers an active node with the given capabilities.
func registerNode(t *testing.T, app *App, ctx sdk.Context, address string, caps *types.NodeCapabilitySet) {
	t.Helper()
	if err := app.PoleKeeper.SetNode(ctx, types.NodeRecord{
		OperatorAddress: address,
		Active:          true,
		Capabilities:    caps,
		BondedTokens:    types.MinProposeBondedTokens,
	}); err != nil {
		t.Fatalf("set node %s: %v", address, err)
	}
}

// sessionFixture sets up a player node, two witness nodes and one collected
// batch per witness, so a well-formed session + attestations can be built.
type sessionFixture struct {
	app      *App
	ctx      sdk.Context
	player   string
	witnessA string
	witnessB string
	session  *types.PlaySession
}

func newSessionFixture(t *testing.T, fill byte, playSeconds uint64, observedPlayers uint64) sessionFixture {
	t.Helper()
	app := initTestApp(t)
	ctx := initTestContext(app).WithBlockHeight(50)

	player := bech32Addr(t, app, fill)
	witnessA := bech32Addr(t, app, fill+1)
	witnessB := bech32Addr(t, app, fill+2)

	registerNode(t, app, ctx, player, &types.NodeCapabilitySet{Collect: true})
	registerNode(t, app, ctx, witnessA, &types.NodeCapabilitySet{Collect: true, Verify: true})
	registerNode(t, app, ctx, witnessB, &types.NodeCapabilitySet{Collect: true, Verify: true})

	// Each witness must hold an observation of its own for this epoch; that
	// is the on-chain evidence that it is not merely echoing the claim.
	for i, witness := range []string{witnessA, witnessB} {
		batch := types.BatchCommit{
			EpochId:          1,
			CollectorAddress: witness,
			Batch:            &types.MerkleCommitment{Root: strings.Repeat("ab", 32), LeafCount: 1},
			PayloadCid:       "witness-observation-" + string(rune('a'+i)),
			ObservationCount: 1,
		}
		if err := app.PoleKeeper.SetBatchCommit(ctx, batch); err != nil {
			t.Fatalf("set witness batch: %v", err)
		}
	}

	session := &types.PlaySession{
		SessionIdHex:     "session-1",
		NodeAddress:      player,
		AppId:            730,
		EpochId:          1,
		SlotId:           1,
		PlaySeconds:      playSeconds,
		CollectorAddress: player,
		ObservationCid:   "player-observation",
		ObservedPlayers:  observedPlayers,
		SessionSignature: "deadbeef",
	}
	submit := app.MsgServiceRouter().Handler(&types.MsgSubmitPlaySession{})
	if submit == nil {
		t.Fatalf("expected submit play session handler")
	}
	if _, err := submit(ctx, &types.MsgSubmitPlaySession{NodeAddress: player, Session: session}); err != nil {
		t.Fatalf("submit play session: %v", err)
	}

	return sessionFixture{app: app, ctx: ctx, player: player, witnessA: witnessA, witnessB: witnessB, session: session}
}

// submitHeartbeats records `count` heartbeats for the fixture session.
func (f sessionFixture) submitHeartbeats(t *testing.T, count int) {
	t.Helper()
	handler := f.app.MsgServiceRouter().Handler(&types.MsgSubmitPlayHeartbeat{})
	if handler == nil {
		t.Fatalf("expected submit play heartbeat handler")
	}
	for i := 0; i < count; i++ {
		_, err := handler(f.ctx, &types.MsgSubmitPlayHeartbeat{
			NodeAddress: f.player,
			Heartbeat: &types.PlayHeartbeat{
				SessionIdHex: f.session.SessionIdHex,
				NodeAddress:  f.player,
				BucketIndex:  uint64(i),
			},
		})
		if err != nil {
			t.Fatalf("submit heartbeat %d: %v", i, err)
		}
	}
}

// attest records a witness attestation with its own observation payload.
func (f sessionFixture) attest(t *testing.T, witness, observationCid string, observedPlayers uint64) error {
	t.Helper()
	handler := f.app.MsgServiceRouter().Handler(&types.MsgAttestSession{})
	if handler == nil {
		t.Fatalf("expected attest session handler")
	}
	_, err := handler(f.ctx, &types.MsgAttestSession{
		Witness: witness,
		Attestation: &types.WitnessAttestation{
			SessionIdHex:          f.session.SessionIdHex,
			WitnessAddress:        witness,
			ObservedPlaySeconds:   f.session.PlaySeconds,
			WitnessObservationCid: observationCid,
			ObservedPlayers:       observedPlayers,
		},
	})
	return err
}

func (f sessionFixture) settle(t *testing.T) (types.SessionSettlement, error) {
	t.Helper()
	handler := f.app.MsgServiceRouter().Handler(&types.MsgSettleSession{})
	if handler == nil {
		t.Fatalf("expected settle session handler")
	}
	if _, err := handler(f.ctx, &types.MsgSettleSession{
		Settler:      f.witnessA,
		SessionIdHex: f.session.SessionIdHex,
	}); err != nil {
		return types.SessionSettlement{}, err
	}
	// The router returns a bare sdk.Result, so read the computed verdict
	// back from the keeper.
	return f.app.PoleKeeper.GetSessionSettlement(f.ctx, f.session.SessionIdHex)
}

// TestPlaySessionSettlesWithTwoIndependentWitnesses is the happy path of
// the mutual-proof flow: a node claims play time, two independent nodes
// each corroborate it with their own observation, heartbeats cover the
// window, and the chain accepts the claim.
func TestPlaySessionSettlesWithTwoIndependentWitnesses(t *testing.T) {
	f := newSessionFixture(t, 1, 600, 1_000)
	f.submitHeartbeats(t, 2)

	if err := f.attest(t, f.witnessA, "witness-observation-a", 1_000); err != nil {
		t.Fatalf("attest by witness A: %v", err)
	}
	if err := f.attest(t, f.witnessB, "witness-observation-b", 1_050); err != nil {
		t.Fatalf("attest by witness B: %v", err)
	}

	settlement, err := f.settle(t)
	if err != nil {
		t.Fatalf("settle session: %v", err)
	}
	if !settlement.Valid {
		t.Fatalf("expected valid settlement, got invalid: %s", settlement.InvalidReason)
	}
	if settlement.WitnessCount != 2 {
		t.Fatalf("expected 2 witnesses, got %d", settlement.WitnessCount)
	}
	if settlement.DistinctObservationCount != 2 {
		t.Fatalf("expected 2 distinct observations, got %d", settlement.DistinctObservationCount)
	}
	// play_seconds * game_weight_ppm, default weight 1.0
	if settlement.PlayerWeightUnits != 600*1_000_000 {
		t.Fatalf("expected weight units %d, got %d", 600*1_000_000, settlement.PlayerWeightUnits)
	}
}

// TestWitnessCannotAttestOwnSession: the node claiming play time must not
// be able to corroborate itself.
func TestWitnessCannotAttestOwnSession(t *testing.T) {
	f := newSessionFixture(t, 1, 600, 1_000)
	f.submitHeartbeats(t, 2)

	err := f.attest(t, f.player, "player-observation-2", 1_000)
	if err == nil {
		t.Fatalf("expected self-attestation to be rejected")
	}
	if !strings.Contains(err.Error(), "cannot attest its own play session") {
		t.Fatalf("unexpected error: %v", err)
	}
}

// TestWitnessCannotAttestSessionItCollected: a collector cannot vouch for a
// session it collected itself.
func TestWitnessCannotAttestSessionItCollected(t *testing.T) {
	f := newSessionFixture(t, 1, 600, 1_000)
	f.submitHeartbeats(t, 2)

	// Re-point the session at witness A as its collector.
	session := *f.session
	session.CollectorAddress = f.witnessA
	if err := f.app.PoleKeeper.SetPlaySession(f.ctx, session); err != nil {
		t.Fatalf("repoint session collector: %v", err)
	}

	err := f.attest(t, f.witnessA, "witness-observation-a", 1_000)
	if err == nil {
		t.Fatalf("expected collector self-attestation to be rejected")
	}
	if !strings.Contains(err.Error(), "cannot attest a session it collected") {
		t.Fatalf("unexpected error: %v", err)
	}
}

// TestWitnessWithoutOwnObservationIsRejected: a node with no observation of
// its own is just echoing the claim, which is exactly the "rubber stamp"
// the mutual-proof requirement exists to prevent.
func TestWitnessWithoutOwnObservationIsRejected(t *testing.T) {
	f := newSessionFixture(t, 1, 600, 1_000)
	f.submitHeartbeats(t, 2)

	// A node with the collect capability but no batch commit this epoch.
	silent := bech32Addr(t, f.app, 40)
	registerNode(t, f.app, f.ctx, silent, &types.NodeCapabilitySet{Collect: true, Verify: true})

	err := f.attest(t, silent, "silent-observation", 1_000)
	if err == nil {
		t.Fatalf("expected a witness with no observation to be rejected")
	}
	if !strings.Contains(err.Error(), "no observation of its own") {
		t.Fatalf("unexpected error: %v", err)
	}
}

// TestWitnessReusingPlayerObservationIsRejected: N witnesses all forwarding
// the player's own payload must not count as N independent corroborations.
func TestWitnessReusingPlayerObservationIsRejected(t *testing.T) {
	f := newSessionFixture(t, 1, 600, 1_000)
	f.submitHeartbeats(t, 2)

	err := f.attest(t, f.witnessA, f.session.ObservationCid, 1_000)
	if err == nil {
		t.Fatalf("expected observation reuse to be rejected")
	}
	if !strings.Contains(err.Error(), "must not reuse the session observation payload") {
		t.Fatalf("unexpected error: %v", err)
	}
}

// TestWitnessObservationOutsideToleranceIsRejected: a witness whose own
// data contradicts the claim is not corroborating it.
func TestWitnessObservationOutsideToleranceIsRejected(t *testing.T) {
	f := newSessionFixture(t, 1, 600, 1_000)
	f.submitHeartbeats(t, 2)

	// Claim says 1000 players; witness observed 100 -> 90% deviation,
	// far beyond the default 50% tolerance.
	err := f.attest(t, f.witnessA, "witness-observation-a", 100)
	if err == nil {
		t.Fatalf("expected out-of-tolerance observation to be rejected")
	}
	if !strings.Contains(err.Error(), "deviates") {
		t.Fatalf("unexpected error: %v", err)
	}
}

// TestSessionWithoutHeartbeatsIsInvalid: the heartbeats are what make
// "I played for N seconds" checkable rather than merely asserted.
func TestSessionWithoutHeartbeatsIsInvalid(t *testing.T) {
	f := newSessionFixture(t, 1, 600, 1_000)
	// No heartbeats submitted.
	if err := f.attest(t, f.witnessA, "witness-observation-a", 1_000); err != nil {
		t.Fatalf("attest by witness A: %v", err)
	}
	if err := f.attest(t, f.witnessB, "witness-observation-b", 1_000); err != nil {
		t.Fatalf("attest by witness B: %v", err)
	}

	settlement, err := f.settle(t)
	if err != nil {
		t.Fatalf("settle session: %v", err)
	}
	if settlement.Valid {
		t.Fatalf("expected invalid settlement without heartbeats")
	}
	if !strings.Contains(settlement.InvalidReason, "heartbeats") {
		t.Fatalf("expected heartbeat reason, got: %s", settlement.InvalidReason)
	}
	if settlement.PlayerWeightUnits != 0 {
		t.Fatalf("expected zero weight units for invalid session, got %d", settlement.PlayerWeightUnits)
	}
}

// TestSessionWithInsufficientWitnessesIsInvalid: one witness is not enough
// for a mutually-proven claim.
func TestSessionWithInsufficientWitnessesIsInvalid(t *testing.T) {
	f := newSessionFixture(t, 1, 600, 1_000)
	f.submitHeartbeats(t, 2)
	if err := f.attest(t, f.witnessA, "witness-observation-a", 1_000); err != nil {
		t.Fatalf("attest by witness A: %v", err)
	}

	settlement, err := f.settle(t)
	if err != nil {
		t.Fatalf("settle session: %v", err)
	}
	if settlement.Valid {
		t.Fatalf("expected invalid settlement with a single witness")
	}
	if !strings.Contains(settlement.InvalidReason, "insufficient witnesses") {
		t.Fatalf("expected witness reason, got: %s", settlement.InvalidReason)
	}
}

// TestDuplicateWitnessObservationDoesNotCountTwice: two witnesses submitting
// the SAME observation payload count as one independent observation, so the
// distinct-observation gate still fails.
func TestDuplicateWitnessObservationDoesNotCountTwice(t *testing.T) {
	f := newSessionFixture(t, 1, 600, 1_000)
	f.submitHeartbeats(t, 2)

	// Both witnesses present the same payload CID. Neither reuses the
	// player's CID, so both attestations are individually accepted — but
	// they only amount to one independent observation.
	if err := f.attest(t, f.witnessA, "shared-observation", 1_000); err != nil {
		t.Fatalf("attest by witness A: %v", err)
	}
	if err := f.attest(t, f.witnessB, "shared-observation", 1_000); err != nil {
		t.Fatalf("attest by witness B: %v", err)
	}

	settlement, err := f.settle(t)
	if err != nil {
		t.Fatalf("settle session: %v", err)
	}
	if settlement.WitnessCount != 2 {
		t.Fatalf("expected 2 witnesses, got %d", settlement.WitnessCount)
	}
	if settlement.DistinctObservationCount != 1 {
		t.Fatalf("expected 1 distinct observation, got %d", settlement.DistinctObservationCount)
	}
	if settlement.Valid {
		t.Fatalf("expected invalid settlement: one observation is not independent corroboration")
	}
	if !strings.Contains(settlement.InvalidReason, "distinct witness observations") {
		t.Fatalf("expected distinct-observation reason, got: %s", settlement.InvalidReason)
	}
}

// TestPlaySecondsCannotExceedSlotDuration: a claim cannot cover more time
// than the slot it belongs to.
func TestPlaySecondsCannotExceedSlotDuration(t *testing.T) {
	f := newSessionFixture(t, 1, 600, 1_000)

	handler := f.app.MsgServiceRouter().Handler(&types.MsgSubmitPlaySession{})
	overlong := &types.PlaySession{
		SessionIdHex:    "session-too-long",
		NodeAddress:     f.player,
		AppId:           730,
		EpochId:         1,
		SlotId:          2,
		PlaySeconds:     99_999,
		ObservationCid:  "player-observation-2",
		ObservedPlayers: 1_000,
	}
	_, err := handler(f.ctx, &types.MsgSubmitPlaySession{NodeAddress: f.player, Session: overlong})
	if err == nil {
		t.Fatalf("expected play_seconds beyond the slot duration to be rejected")
	}
	if !strings.Contains(err.Error(), "exceeds slot duration") {
		t.Fatalf("unexpected error: %v", err)
	}
}

// TestFinalizeEpochRejectsUnsettledPlaySessions: an epoch cannot close with
// play claims still unproven.
func TestFinalizeEpochRejectsUnsettledPlaySessions(t *testing.T) {
	f := newSessionFixture(t, 1, 600, 1_000)

	// Minimal epoch commit; finalize should fail on the unsettled session
	// before it ever reaches the root checks.
	if err := f.app.PoleKeeper.SetEpochCommit(f.ctx, types.EpochCommit{
		EpochId:                 1,
		ProposerAddress:         f.player,
		ChallengeOpenHeight:     1,
		ChallengeDeadlineHeight: 10,
	}); err != nil {
		t.Fatalf("set epoch commit: %v", err)
	}

	finalize := f.app.MsgServiceRouter().Handler(&types.MsgFinalizeEpoch{})
	_, err := finalize(f.ctx, &types.MsgFinalizeEpoch{Finalizer: f.player, EpochId: 1})
	if err == nil {
		t.Fatalf("expected finalize to reject an epoch with unsettled play sessions")
	}
	if !strings.Contains(err.Error(), "unsettled play session") {
		t.Fatalf("unexpected error: %v", err)
	}
}

// TestWitnessCreditsAndWeightUnitsFeedRewardSplit verifies the reward inputs
// the mutual-proof flow produces: valid sessions yield player weight units,
// and corroborating witnesses accrue credits.
func TestWitnessCreditsAndWeightUnitsFeedRewardSplit(t *testing.T) {
	f := newSessionFixture(t, 1, 600, 1_000)
	f.submitHeartbeats(t, 2)
	if err := f.attest(t, f.witnessA, "witness-observation-a", 1_000); err != nil {
		t.Fatalf("attest by witness A: %v", err)
	}
	if err := f.attest(t, f.witnessB, "witness-observation-b", 1_000); err != nil {
		t.Fatalf("attest by witness B: %v", err)
	}
	if _, err := f.settle(t); err != nil {
		t.Fatalf("settle session: %v", err)
	}

	totalWeight, err := f.app.PoleKeeper.ValidSessionWeightUnitsForEpoch(f.ctx, 1)
	if err != nil {
		t.Fatalf("valid session weight units: %v", err)
	}
	if totalWeight != 600*1_000_000 {
		t.Fatalf("expected total weight units %d, got %d", 600*1_000_000, totalWeight)
	}

	credits, err := f.app.PoleKeeper.WitnessCreditsForEpoch(f.ctx, 1)
	if err != nil {
		t.Fatalf("witness credits: %v", err)
	}
	if credits[f.witnessA] != 1 || credits[f.witnessB] != 1 {
		t.Fatalf("expected one credit per witness, got %+v", credits)
	}
}

// TestWitnessRewardDistributedByAttestationCount checks the split the
// verify pool is actually divided by. A witness that corroborated more
// sessions earns a proportionally larger share, and the shares are computed
// from on-chain evidence only: neither witness can influence the weight by
// anything it submits alongside its attestation.
func TestWitnessRewardDistributedByAttestationCount(t *testing.T) {
	f := newSessionFixture(t, 1, 600, 1_000)
	f.submitHeartbeats(t, 2)

	// A third independent witness. The fixture only wires two, but the
	// split must generalise to any number of corroborators.
	witnessC := bech32Addr(t, f.app, 51)
	registerNode(t, f.app, f.ctx, witnessC, &types.NodeCapabilitySet{Collect: true, Verify: true})
	if err := f.app.PoleKeeper.SetBatchCommit(f.ctx, types.BatchCommit{
		EpochId:          1,
		CollectorAddress: witnessC,
		Batch:            &types.MerkleCommitment{Root: strings.Repeat("ab", 32), LeafCount: 1},
		PayloadCid:       "witness-observation-c",
		ObservationCount: 1,
	}); err != nil {
		t.Fatalf("set witness C batch: %v", err)
	}

	// A second session in the same epoch. The player collects it too, so
	// no witness is its collector and all remain eligible.
	second := &types.PlaySession{
		SessionIdHex:     "session-2",
		NodeAddress:      f.player,
		AppId:            730,
		EpochId:          1,
		SlotId:           2,
		PlaySeconds:      600,
		CollectorAddress: f.player,
		ObservationCid:   "player-observation-2",
		ObservedPlayers:  1_000,
		SessionSignature: "deadbeef",
	}
	submitSession := f.app.MsgServiceRouter().Handler(&types.MsgSubmitPlaySession{})
	if _, err := submitSession(f.ctx, &types.MsgSubmitPlaySession{NodeAddress: f.player, Session: second}); err != nil {
		t.Fatalf("submit second play session: %v", err)
	}
	submitHeartbeat := f.app.MsgServiceRouter().Handler(&types.MsgSubmitPlayHeartbeat{})
	for i := 0; i < 2; i++ {
		if _, err := submitHeartbeat(f.ctx, &types.MsgSubmitPlayHeartbeat{
			NodeAddress: f.player,
			Heartbeat: &types.PlayHeartbeat{
				SessionIdHex: second.SessionIdHex,
				NodeAddress:  f.player,
				BucketIndex:  uint64(i),
			},
		}); err != nil {
			t.Fatalf("submit second heartbeat %d: %v", i, err)
		}
	}

	attest := func(witness, sessionIDHex, observationCid string) {
		t.Helper()
		handler := f.app.MsgServiceRouter().Handler(&types.MsgAttestSession{})
		if _, err := handler(f.ctx, &types.MsgAttestSession{
			Witness: witness,
			Attestation: &types.WitnessAttestation{
				SessionIdHex:          sessionIDHex,
				WitnessAddress:        witness,
				ObservedPlaySeconds:   600,
				WitnessObservationCid: observationCid,
				ObservedPlayers:       1_000,
			},
		}); err != nil {
			t.Fatalf("attest %s by %s: %v", sessionIDHex, witness, err)
		}
	}

	// Session 1 is corroborated by A and B, session 2 by A and C: witness
	// A delivered two independent corroborations, B and C one each.
	attest(f.witnessA, "session-1", "witness-observation-a")
	attest(f.witnessB, "session-1", "witness-observation-b")
	attest(f.witnessA, "session-2", "witness-observation-a2")
	attest(witnessC, "session-2", "witness-observation-c2")

	settle := f.app.MsgServiceRouter().Handler(&types.MsgSettleSession{})
	for _, sessionIDHex := range []string{"session-1", "session-2"} {
		if _, err := settle(f.ctx, &types.MsgSettleSession{Settler: f.witnessA, SessionIdHex: sessionIDHex}); err != nil {
			t.Fatalf("settle %s: %v", sessionIDHex, err)
		}
		settlement, err := f.app.PoleKeeper.GetSessionSettlement(f.ctx, sessionIDHex)
		if err != nil {
			t.Fatalf("read settlement %s: %v", sessionIDHex, err)
		}
		if !settlement.Valid {
			t.Fatalf("expected %s to settle valid, got: %s", sessionIDHex, settlement.InvalidReason)
		}
	}

	credits, err := f.app.PoleKeeper.WitnessCreditsForEpoch(f.ctx, 1)
	if err != nil {
		t.Fatalf("witness credits: %v", err)
	}
	if credits[f.witnessA] != 2 || credits[f.witnessB] != 1 || credits[witnessC] != 1 {
		t.Fatalf("expected 2/1/1 credits, got %+v", credits)
	}

	// The verify pool must follow the credits, not be handed whole to one
	// node: 4000 over 4 credits is 2000 / 1000 / 1000.
	allocation, err := f.app.PoleKeeper.WitnessRewardAllocationForEpoch(f.ctx, 1, 4000)
	if err != nil {
		t.Fatalf("witness reward allocation: %v", err)
	}
	if allocation[f.witnessA] != 2000 {
		t.Fatalf("expected witness A to receive 2000, got %d", allocation[f.witnessA])
	}
	if allocation[f.witnessB] != 1000 {
		t.Fatalf("expected witness B to receive 1000, got %d", allocation[f.witnessB])
	}
	if allocation[witnessC] != 1000 {
		t.Fatalf("expected witness C to receive 1000, got %d", allocation[witnessC])
	}
	if allocation[f.player] != 0 {
		t.Fatalf("the player collected but did not corroborate, so it must earn no verify reward; got %d", allocation[f.player])
	}

	// A witness with no adopted attestation must not be paid for merely
	// holding the verify capability.
	idle := bech32Addr(t, f.app, 50)
	registerNode(t, f.app, f.ctx, idle, &types.NodeCapabilitySet{Collect: true, Verify: true})
	allocation, err = f.app.PoleKeeper.WitnessRewardAllocationForEpoch(f.ctx, 1, 4000)
	if err != nil {
		t.Fatalf("witness reward allocation after registering an idle verifier: %v", err)
	}
	if allocation[idle] != 0 {
		t.Fatalf("expected no verify reward for a witness with zero credits, got %d", allocation[idle])
	}
	if allocation[f.witnessA] != 2000 || allocation[f.witnessB] != 1000 || allocation[witnessC] != 1000 {
		t.Fatalf("registering an idle verifier must not dilute earned credits, got %+v", allocation)
	}
}

// settledSessionWithTwoWitnesses drives the fixture to a valid settlement so
// witnesses A and B each hold one adopted attestation in epoch 1.
func settledSessionWithTwoWitnesses(t *testing.T) sessionFixture {
	t.Helper()
	f := newSessionFixture(t, 1, 600, 1_000)
	f.submitHeartbeats(t, 2)
	if err := f.attest(t, f.witnessA, "witness-observation-a", 1_000); err != nil {
		t.Fatalf("attest by witness A: %v", err)
	}
	if err := f.attest(t, f.witnessB, "witness-observation-b", 1_000); err != nil {
		t.Fatalf("attest by witness B: %v", err)
	}
	if _, err := f.settle(t); err != nil {
		t.Fatalf("settle session: %v", err)
	}
	return f
}

// witnessProposer registers a fresh node allowed to submit reward records.
func witnessProposer(t *testing.T, f sessionFixture, fill byte) string {
	t.Helper()
	proposer := bech32Addr(t, f.app, fill)
	registerNode(t, f.app, f.ctx, proposer, &types.NodeCapabilitySet{Propose: true})
	return proposer
}

// submitRewardsWithCommit commits the rewards root derived from `records` and
// submits the very same set, returning the handler's error.
func submitRewardsWithCommit(t *testing.T, f sessionFixture, proposer string, records []types.RewardRecord) error {
	t.Helper()
	if err := f.app.PoleKeeper.SetEpochCommit(f.ctx, types.EpochCommit{
		EpochId:                 1,
		ProposerAddress:         proposer,
		ChallengeOpenHeight:     1,
		ChallengeDeadlineHeight: 100,
		Rewards: &types.MerkleCommitment{
			Root:      testCommitmentRoot(t, records),
			LeafCount: uint32(len(records)),
		},
	}); err != nil {
		t.Fatalf("set epoch commit: %v", err)
	}

	handler := f.app.MsgServiceRouter().Handler(&types.MsgSubmitRewardRecords{})
	if handler == nil {
		t.Fatalf("expected submit reward records handler")
	}
	msgRecords := make([]*types.RewardRecord, 0, len(records))
	for i := range records {
		msgRecords = append(msgRecords, &records[i])
	}
	_, err := handler(f.ctx, &types.MsgSubmitRewardRecords{
		Proposer: proposer,
		EpochId:  1,
		Records:  msgRecords,
	})
	return err
}

// TestSubmitRewardRecordsEnforcesAttestationProportionalSplit: verify reward
// is paid for recorded corroboration, not for holding the verify capability.
// The split among credited witnesses must match their adopted-attestation
// counts exactly.
func TestSubmitRewardRecordsEnforcesAttestationProportionalSplit(t *testing.T) {
	f := settledSessionWithTwoWitnesses(t)
	proposer := witnessProposer(t, f, 60)

	// A 4000 pool over two equal credits splits 2000/2000. Paying the whole
	// pool to one witness is the self-dealing case the rule must catch.
	records := []types.RewardRecord{
		{EpochId: 1, Recipient: f.player, PlayerReward: 5_000, NetReward: 5_000},
		{EpochId: 1, Recipient: f.witnessA, VerifyReward: 4_000, NetReward: 4_000},
		{EpochId: 1, Recipient: f.witnessB, VerifyReward: 0, NetReward: 0},
	}
	err := submitRewardsWithCommit(t, f, proposer, records)
	if err == nil {
		t.Fatalf("expected a concentrated verify split to be rejected")
	}
	if !strings.Contains(err.Error(), "attestation-proportional split") {
		t.Fatalf("expected a proportional-split error, got: %v", err)
	}

	// The proportional split is accepted.
	records = []types.RewardRecord{
		{EpochId: 1, Recipient: f.player, PlayerReward: 5_000, NetReward: 5_000},
		{EpochId: 1, Recipient: f.witnessA, VerifyReward: 2_000, NetReward: 2_000},
		{EpochId: 1, Recipient: f.witnessB, VerifyReward: 2_000, NetReward: 2_000},
	}
	if err := submitRewardsWithCommit(t, f, proposer, records); err != nil {
		t.Fatalf("expected the proportional split to be accepted, got: %v", err)
	}
}

// TestSubmitRewardRecordsRejectsOmittedWitness closes the omission hole: a
// proposer must not drop the other credited witnesses and keep the pool,
// because a set holding only one witness is trivially proportional.
func TestSubmitRewardRecordsRejectsOmittedWitness(t *testing.T) {
	f := settledSessionWithTwoWitnesses(t)
	proposer := witnessProposer(t, f, 62)

	records := []types.RewardRecord{
		{EpochId: 1, Recipient: f.witnessA, VerifyReward: 4_000, NetReward: 4_000},
	}
	err := submitRewardsWithCommit(t, f, proposer, records)
	if err == nil {
		t.Fatalf("expected omitting a credited witness to be rejected")
	}
	if !strings.Contains(err.Error(), "missing witness") {
		t.Fatalf("expected a missing-witness error, got: %v", err)
	}
}

// TestSubmitRewardRecordsRejectsVerifyRewardWithoutCredits: a node that
// corroborated nothing must receive no verify reward, even though it is a
// registered verifier.
func TestSubmitRewardRecordsRejectsVerifyRewardWithoutCredits(t *testing.T) {
	f := settledSessionWithTwoWitnesses(t)
	proposer := witnessProposer(t, f, 63)

	idle := bech32Addr(t, f.app, 64)
	registerNode(t, f.app, f.ctx, idle, &types.NodeCapabilitySet{Collect: true, Verify: true})

	records := []types.RewardRecord{
		{EpochId: 1, Recipient: f.witnessA, VerifyReward: 2_000, NetReward: 2_000},
		{EpochId: 1, Recipient: f.witnessB, VerifyReward: 2_000, NetReward: 2_000},
		{EpochId: 1, Recipient: idle, VerifyReward: 100, NetReward: 100},
	}
	err := submitRewardsWithCommit(t, f, proposer, records)
	if err == nil {
		t.Fatalf("expected a verify reward without credits to be rejected")
	}
	if !strings.Contains(err.Error(), "corroborated no settled session") {
		t.Fatalf("expected a no-credits error, got: %v", err)
	}
}

// TestSubmitRewardRecordsSkipsSplitCheckWithoutWitnessCredits: epochs with no
// mutual-proof evidence keep the previous behaviour untouched, so historical
// and challenge-adjusted epochs validate as before.
func TestSubmitRewardRecordsSkipsSplitCheckWithoutWitnessCredits(t *testing.T) {
	f := newSessionFixture(t, 1, 600, 1_000) // no attestations, so no credits
	proposer := witnessProposer(t, f, 65)

	records := []types.RewardRecord{
		{EpochId: 1, Recipient: proposer, VerifyReward: 9_999, NetReward: 9_999},
	}
	if err := submitRewardsWithCommit(t, f, proposer, records); err != nil {
		t.Fatalf("expected an epoch without witness credits to keep validating, got: %v", err)
	}
}
