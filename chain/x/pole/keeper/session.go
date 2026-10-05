package keeper

import (
	"context"
	"errors"
	"fmt"

	"cosmossdk.io/collections"

	sdk "github.com/cosmos/cosmos-sdk/types"

	"pole/chain/x/pole/types"
)

// ---------------------------------------------------------------------------
// Storage accessors
// ---------------------------------------------------------------------------

func (k Keeper) SetPlaySession(ctx context.Context, session types.PlaySession) error {
	return k.PlaySessions.Set(ctx, session.SessionIdHex, session)
}

func (k Keeper) GetPlaySession(ctx context.Context, sessionIDHex string) (types.PlaySession, error) {
	return k.PlaySessions.Get(ctx, sessionIDHex)
}

func (k Keeper) SetPlayHeartbeat(ctx context.Context, heartbeat types.PlayHeartbeat) error {
	return k.PlayHeartbeats.Set(
		ctx,
		collections.Join(heartbeat.SessionIdHex, heartbeat.BucketIndex),
		heartbeat,
	)
}

func (k Keeper) SetWitnessAttestation(ctx context.Context, attestation types.WitnessAttestation) error {
	return k.WitnessAttestations.Set(
		ctx,
		collections.Join(attestation.SessionIdHex, attestation.WitnessAddress),
		attestation,
	)
}

func (k Keeper) GetSessionSettlement(ctx context.Context, sessionIDHex string) (types.SessionSettlement, error) {
	return k.SessionSettlements.Get(ctx, sessionIDHex)
}

// playHeartbeatsForSession returns every stored heartbeat for a session.
func (k Keeper) playHeartbeatsForSession(ctx context.Context, sessionIDHex string) ([]types.PlayHeartbeat, error) {
	iter, err := k.PlayHeartbeats.Iterate(ctx, collections.NewPrefixedPairRange[string, uint64](sessionIDHex))
	if err != nil {
		return nil, err
	}
	defer iter.Close()

	var heartbeats []types.PlayHeartbeat
	for ; iter.Valid(); iter.Next() {
		kv, err := iter.KeyValue()
		if err != nil {
			return nil, err
		}
		if kv.Value.SessionIdHex == sessionIDHex {
			heartbeats = append(heartbeats, kv.Value)
		}
	}
	return heartbeats, nil
}

// witnessAttestationsForSession returns every stored attestation for a session.
func (k Keeper) witnessAttestationsForSession(ctx context.Context, sessionIDHex string) ([]types.WitnessAttestation, error) {
	iter, err := k.WitnessAttestations.Iterate(ctx, collections.NewPrefixedPairRange[string, string](sessionIDHex))
	if err != nil {
		return nil, err
	}
	defer iter.Close()

	var attestations []types.WitnessAttestation
	for ; iter.Valid(); iter.Next() {
		kv, err := iter.KeyValue()
		if err != nil {
			return nil, err
		}
		if kv.Value.SessionIdHex == sessionIDHex {
			attestations = append(attestations, kv.Value)
		}
	}
	return attestations, nil
}

// PlaySessionsForEpoch returns every session submitted for an epoch.
func (k Keeper) PlaySessionsForEpoch(ctx context.Context, epochId uint64) ([]types.PlaySession, error) {
	iter, err := k.PlaySessions.Iterate(ctx, nil)
	if err != nil {
		return nil, err
	}
	defer iter.Close()

	var sessions []types.PlaySession
	for ; iter.Valid(); iter.Next() {
		kv, err := iter.KeyValue()
		if err != nil {
			return nil, err
		}
		if kv.Value.EpochId == epochId {
			sessions = append(sessions, kv.Value)
		}
	}
	return sessions, nil
}

// UnsettledSessionsForEpoch returns the sessions of an epoch that have no
// settlement yet. FinalizeEpoch refuses while any remain, so an epoch
// cannot be finalized with unproven play claims still outstanding.
func (k Keeper) UnsettledSessionsForEpoch(ctx context.Context, epochId uint64) ([]types.PlaySession, error) {
	sessions, err := k.PlaySessionsForEpoch(ctx, epochId)
	if err != nil {
		return nil, err
	}
	var unsettled []types.PlaySession
	for _, session := range sessions {
		has, err := k.SessionSettlements.Has(ctx, session.SessionIdHex)
		if err != nil {
			return nil, err
		}
		if !has {
			unsettled = append(unsettled, session)
		}
	}
	return unsettled, nil
}

// ---------------------------------------------------------------------------
// Game weight
// ---------------------------------------------------------------------------

// gameWeightPPMForApp resolves the game weight in effect at `epochId`: the
// highest effective_from_epoch_id that does not exceed the epoch. Missing
// entries fall back to 1.0 (neutral weight), matching the Rust off-chain
// default so both sides compute the same player weight units.
func (k Keeper) gameWeightPPMForApp(ctx context.Context, appId uint32, epochId uint64) (uint32, error) {
	iter, err := k.GameWeightEntries.Iterate(ctx, nil)
	if err != nil {
		return 0, err
	}
	defer iter.Close()

	var bestEpoch uint64
	var bestWeight uint32
	found := false
	for ; iter.Valid(); iter.Next() {
		kv, err := iter.KeyValue()
		if err != nil {
			return 0, err
		}
		entry := kv.Value
		if entry.AppId != appId || entry.EffectiveFromEpochId > epochId {
			continue
		}
		if !found || entry.EffectiveFromEpochId >= bestEpoch {
			bestEpoch = entry.EffectiveFromEpochId
			bestWeight = entry.GameWeightPpm
			found = true
		}
	}
	if !found || bestWeight == 0 {
		return 1_000_000, nil
	}
	return bestWeight, nil
}

// ---------------------------------------------------------------------------
// Witness independence
// ---------------------------------------------------------------------------

// observationDeviationPPM returns the relative deviation between two
// observed player counts, in ppm of the larger value. Both zero counts
// agree (0); a single zero against a non-zero count deviates fully.
func observationDeviationPPM(a, b uint64) uint32 {
	if a == b {
		return 0
	}
	larger := a
	if b > larger {
		larger = b
	}
	if larger == 0 {
		return 0
	}
	var diff uint64
	if a > b {
		diff = a - b
	} else {
		diff = b - a
	}
	deviation := diff * 1_000_000 / larger
	if deviation > 1_000_000 {
		deviation = 1_000_000
	}
	return uint32(deviation)
}

// validateWitnessIndependence enforces that an attestation is a genuine
// independent corroboration rather than a rubber stamp or a copy. Every
// rule here maps to one way the mutual-proof claim could otherwise be
// faked:
//
//  1. witness != player        — no self-attestation.
//  2. witness != collector     — no attesting the batch you collected.
//  3. witness has its own observation for this epoch (it submitted a batch
//     commit and holds the collect capability) — a witness with no
//     observation of its own is just echoing the claim.
//  4. observation CID differs from the session's — no copying one payload
//     and presenting it N times.
//  5. observed player counts agree within tolerance — the witness's own
//     macro-telemetry confirms the game activity environment claimed by the
//     player. Note: In V1, this verifies consistency across external telemetry
//     sources (preventing fabricated telemetry / offline game spoofing), rather
//     than direct cryptographic observation of the player's physical screen/input.
//     Micro-level engagement relies on signed heartbeat bucket coverage and
//     local OS anti-cheat enforcement.
func (k Keeper) validateWitnessIndependence(
	ctx context.Context,
	session types.PlaySession,
	attestation types.WitnessAttestation,
	params types.Params,
) error {
	if attestation.WitnessAddress == session.NodeAddress {
		return fmt.Errorf("witness %s cannot attest its own play session", attestation.WitnessAddress)
	}
	if session.CollectorAddress != "" && attestation.WitnessAddress == session.CollectorAddress {
		return fmt.Errorf("witness %s cannot attest a session it collected", attestation.WitnessAddress)
	}

	witness, err := k.GetNode(ctx, attestation.WitnessAddress)
	if err != nil {
		if errors.Is(err, collections.ErrNotFound) {
			return fmt.Errorf("witness %s is not a registered node", attestation.WitnessAddress)
		}
		return err
	}
	if !witness.Active {
		return fmt.Errorf("witness %s is not active", attestation.WitnessAddress)
	}
	if witness.Capabilities == nil || !witness.Capabilities.Collect {
		return fmt.Errorf("witness %s lacks the collect capability", attestation.WitnessAddress)
	}

	// Witness must satisfy staking requirement
	if !witness.IsPlayer && witness.BondedTokens < types.RequiredBondedTokensForNode(witness) {
		return fmt.Errorf(
			"witness %s has bonded tokens %d below required threshold %d",
			attestation.WitnessAddress, witness.BondedTokens, types.RequiredBondedTokensForNode(witness),
		)
	}

	// Reject sybil collusion where player and witness share the same reward address
	if playerNode, err := k.GetNode(ctx, session.NodeAddress); err == nil {
		if playerNode.RewardAddress != "" && witness.RewardAddress != "" && playerNode.RewardAddress == witness.RewardAddress {
			return fmt.Errorf(
				"witness %s shares reward address %s with player %s (sybil self-attestation rejected)",
				attestation.WitnessAddress, witness.RewardAddress, session.NodeAddress,
			)
		}
	}

	// The witness must have collected something in this epoch: that batch
	// commit is the on-chain evidence that it holds an observation of its
	// own rather than merely repeating the claim.
	batches, err := k.batchCommitsForEpoch(ctx, session.EpochId)
	if err != nil {
		return err
	}
	witnessCollected := false
	for _, batch := range batches {
		if batch.CollectorAddress == attestation.WitnessAddress {
			witnessCollected = true
			break
		}
	}
	if !witnessCollected {
		return fmt.Errorf(
			"witness %s has no observation of its own for epoch %d",
			attestation.WitnessAddress, session.EpochId,
		)
	}

	if attestation.WitnessObservationCid == "" {
		return fmt.Errorf("witness_observation_cid is required")
	}
	if session.ObservationCid != "" && attestation.WitnessObservationCid == session.ObservationCid {
		return fmt.Errorf("witness observation must not reuse the session observation payload")
	}

	deviation := observationDeviationPPM(attestation.ObservedPlayers, session.ObservedPlayers)
	if deviation > params.MinWitnessObservationTolerancePpm {
		return fmt.Errorf(
			"witness observation deviates %d ppm from the claim, tolerance is %d ppm",
			deviation, params.MinWitnessObservationTolerancePpm,
		)
	}
	return nil
}

// ---------------------------------------------------------------------------
// Settlement
// ---------------------------------------------------------------------------

// heartbeatCoverageBPS returns the share (bps) of the declared play window
// covered by distinct heartbeat buckets.
func heartbeatCoverageBPS(heartbeatCount uint64, playSeconds uint64, bucketSeconds uint64) uint32 {
	if playSeconds == 0 || bucketSeconds == 0 {
		return 0
	}
	expectedBuckets := (playSeconds + bucketSeconds - 1) / bucketSeconds
	if expectedBuckets == 0 {
		return 0
	}
	covered := heartbeatCount
	if covered > expectedBuckets {
		covered = expectedBuckets
	}
	coverage := covered * 10_000 / expectedBuckets
	if coverage > 10_000 {
		coverage = 10_000
	}
	return uint32(coverage)
}

// SettlePlaySession computes the chain's verdict on a play session. The
// settlement is derived entirely from stored sessions, heartbeats and
// attestations — never from caller-supplied values — so a proposer cannot
// assert a session valid that the evidence does not support.
func (k Keeper) SettlePlaySession(ctx context.Context, sessionIDHex string) (types.SessionSettlement, error) {
	session, err := k.GetPlaySession(ctx, sessionIDHex)
	if err != nil {
		if errors.Is(err, collections.ErrNotFound) {
			return types.SessionSettlement{}, fmt.Errorf("play session %s not found", sessionIDHex)
		}
		return types.SessionSettlement{}, err
	}
	params, err := k.GetParams(ctx)
	if err != nil {
		return types.SessionSettlement{}, err
	}

	attestations, err := k.witnessAttestationsForSession(ctx, sessionIDHex)
	if err != nil {
		return types.SessionSettlement{}, err
	}
	heartbeats, err := k.playHeartbeatsForSession(ctx, sessionIDHex)
	if err != nil {
		return types.SessionSettlement{}, err
	}

	// Count distinct witnesses and distinct observation payloads. The
	// Count distinct witnesses, distinct reward addresses, and distinct observation payloads.
	// The distinct-reward-address and distinct-payload counts defeat sybil rings and replay.
	witnesses := map[string]struct{}{}
	observations := map[string]struct{}{}
	witnessRewardAddrs := map[string]struct{}{}
	for _, attestation := range attestations {
		witnesses[attestation.WitnessAddress] = struct{}{}
		if attestation.WitnessObservationCid != "" {
			observations[attestation.WitnessObservationCid] = struct{}{}
		}
		if wNode, err := k.GetNode(ctx, attestation.WitnessAddress); err == nil && wNode.RewardAddress != "" {
			witnessRewardAddrs[wNode.RewardAddress] = struct{}{}
		} else {
			witnessRewardAddrs[attestation.WitnessAddress] = struct{}{}
		}
	}
	witnessCount := uint64(len(witnesses))
	distinctObservations := uint64(len(observations))
	distinctRewardAddrs := uint64(len(witnessRewardAddrs))

	bucketSeconds := params.HeartbeatBucketSeconds
	if bucketSeconds == 0 {
		bucketSeconds = 1
	}
	heartbeatCount := uint64(len(heartbeats))
	coverageBPS := heartbeatCoverageBPS(heartbeatCount, session.PlaySeconds, bucketSeconds)

	gameWeightPPM, err := k.gameWeightPPMForApp(ctx, session.AppId, session.EpochId)
	if err != nil {
		return types.SessionSettlement{}, err
	}

	effectivePlaySeconds := session.PlaySeconds
	maxCoveredSeconds := heartbeatCount * bucketSeconds
	if maxCoveredSeconds < effectivePlaySeconds {
		effectivePlaySeconds = maxCoveredSeconds
	}
	playerWeightUnits := types.ComputePlayerHourWeight(effectivePlaySeconds, gameWeightPPM)

	settlement := types.SessionSettlement{
		SessionIdHex:             session.SessionIdHex,
		EpochId:                  session.EpochId,
		AppId:                    session.AppId,
		NodeAddress:              session.NodeAddress,
		PlaySeconds:              effectivePlaySeconds,
		GameWeightPpm:            gameWeightPPM,
		PlayerWeightUnits:        playerWeightUnits,
		WitnessCount:             witnessCount,
		DistinctObservationCount: distinctObservations,
		HeartbeatCount:           heartbeatCount,
		HeartbeatCoverageBps:     coverageBPS,
		SettledAtHeight:          sdk.UnwrapSDKContext(ctx).BlockHeight(),
	}

	// Gate the claim on independent corroboration and liveness. Each check
	// corresponds to a way the claim could be false.
	switch {
	case witnessCount < params.MinWitnessCount:
		settlement.InvalidReason = fmt.Sprintf(
			"insufficient witnesses (%d < %d)", witnessCount, params.MinWitnessCount,
		)
	case distinctRewardAddrs < params.MinWitnessCount:
		settlement.InvalidReason = fmt.Sprintf(
			"insufficient distinct witness reward addresses (%d < %d)",
			distinctRewardAddrs, params.MinWitnessCount,
		)
	case distinctObservations < params.MinDistinctObservations:
		settlement.InvalidReason = fmt.Sprintf(
			"insufficient distinct witness observations (%d < %d)",
			distinctObservations, params.MinDistinctObservations,
		)
	case heartbeatCount < params.MinHeartbeatCount:
		settlement.InvalidReason = fmt.Sprintf(
			"insufficient play heartbeats (%d < %d)", heartbeatCount, params.MinHeartbeatCount,
		)
	case coverageBPS < params.MinHeartbeatCoverageBps:
		settlement.InvalidReason = fmt.Sprintf(
			"heartbeat coverage %d bps < %d bps", coverageBPS, params.MinHeartbeatCoverageBps,
		)
	case session.PlaySeconds == 0:
		settlement.InvalidReason = "play_seconds must be greater than 0"
	default:
		settlement.Valid = true
	}
	if !settlement.Valid {
		settlement.PlayerWeightUnits = 0
	}

	if err := k.SessionSettlements.Set(ctx, settlement.SessionIdHex, settlement); err != nil {
		return types.SessionSettlement{}, err
	}
	return settlement, nil
}

// ValidSessionWeightUnitsForEpoch returns the total player weight units of
// the epoch's *valid* settled sessions. This is the amount the epoch's
// player reward pool is divided across, and the basis for the emission
// that funds it under the no-cap model.
func (k Keeper) ValidSessionWeightUnitsForEpoch(ctx context.Context, epochId uint64) (uint64, error) {
	sessions, err := k.PlaySessionsForEpoch(ctx, epochId)
	if err != nil {
		return 0, err
	}
	var total uint64
	for _, session := range sessions {
		settlement, err := k.GetSessionSettlement(ctx, session.SessionIdHex)
		if err != nil {
			if errors.Is(err, collections.ErrNotFound) {
				continue
			}
			return 0, err
		}
		if settlement.Valid {
			total += settlement.PlayerWeightUnits
		}
	}
	return total, nil
}

// WitnessCreditsForEpoch returns, per witness address, how many settled
// *valid* sessions that witness corroborated. This is the basis for the
// witness reward split, replacing the old behaviour of handing the whole
// verify pool to whichever node happened to run with verify enabled.
func (k Keeper) WitnessCreditsForEpoch(ctx context.Context, epochId uint64) (map[string]uint64, error) {
	sessions, err := k.PlaySessionsForEpoch(ctx, epochId)
	if err != nil {
		return nil, err
	}
	credits := map[string]uint64{}
	for _, session := range sessions {
		settlement, err := k.GetSessionSettlement(ctx, session.SessionIdHex)
		if err != nil {
			if errors.Is(err, collections.ErrNotFound) {
				continue
			}
			return nil, err
		}
		if !settlement.Valid {
			continue
		}
		attestations, err := k.witnessAttestationsForSession(ctx, session.SessionIdHex)
		if err != nil {
			return nil, err
		}
		for _, attestation := range attestations {
			credits[attestation.WitnessAddress]++
		}
	}
	return credits, nil
}

// WitnessRewardAllocationForEpoch splits a witness reward pool for an epoch
// across the witnesses that corroborated settled, valid sessions, in
// proportion to the number of attestations each one had adopted.
//
// This is the on-chain half of the verify-reward rule: a node earns verify
// reward for the corroboration the chain actually recorded, not for
// operating with the verify capability enabled. The split is exact and
// deterministic, so a proposer recomputing it off-chain derives the same
// per-witness amounts.
func (k Keeper) WitnessRewardAllocationForEpoch(ctx context.Context, epochId uint64, verifyPool uint64) (map[string]uint64, error) {
	credits, err := k.WitnessCreditsForEpoch(ctx, epochId)
	if err != nil {
		return nil, err
	}
	return types.AllocateWitnessRewards(credits, verifyPool), nil
}
