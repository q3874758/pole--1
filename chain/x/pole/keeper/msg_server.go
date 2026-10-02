package keeper

import (
	"context"
	"errors"
	"fmt"

	"cosmossdk.io/collections"
	errorsmod "cosmossdk.io/errors"

	sdk "github.com/cosmos/cosmos-sdk/types"
	sdkerrors "github.com/cosmos/cosmos-sdk/types/errors"

	"pole/chain/x/pole/types"
)

var _ types.MsgServer = (*msgServer)(nil)

type msgServer struct {
	keeper *Keeper
}

func NewMsgServerImpl(k *Keeper) types.MsgServer {
	return &msgServer{keeper: k}
}

func (m *msgServer) requireNodeCapability(ctx context.Context, operator string, capability string) (types.NodeRecord, error) {
	node, err := m.keeper.GetNode(ctx, operator)
	if err != nil {
		return types.NodeRecord{}, err
	}
	if !node.Active {
		return types.NodeRecord{}, errorsmod.Wrap(sdkerrors.ErrUnauthorized, "node is not active")
	}
	if node.Capabilities == nil {
		return types.NodeRecord{}, errorsmod.Wrap(sdkerrors.ErrUnauthorized, "node capabilities are not configured")
	}
	allowed := false
	switch capability {
	case "collect":
		allowed = node.Capabilities.Collect
	case "store":
		allowed = node.Capabilities.Store
	case "verify":
		allowed = node.Capabilities.Verify
	case "propose":
		allowed = node.Capabilities.Propose
	}
	if !allowed {
		return types.NodeRecord{}, errorsmod.Wrapf(sdkerrors.ErrUnauthorized, "node missing %s capability", capability)
	}
	if node.BondedTokens < types.RequiredBondedTokensForNode(node) {
		return types.NodeRecord{}, errorsmod.Wrap(sdkerrors.ErrUnauthorized, "node bonded_tokens below required threshold")
	}
	return node, nil
}

func (m *msgServer) UpsertNode(ctx context.Context, msg *types.MsgUpsertNode) (*types.MsgUpsertNodeResponse, error) {
	if msg == nil || msg.Node == nil {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "node is required")
	}
	if msg.Operator == "" || msg.Node.OperatorAddress == "" {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidAddress, "operator address is required")
	}
	if msg.Operator != msg.Node.OperatorAddress {
		return nil, errorsmod.Wrap(sdkerrors.ErrUnauthorized, "operator must match node.operator_address")
	}
	if msg.Node.Capabilities == nil {
		msg.Node.Capabilities = &types.NodeCapabilitySet{}
	}
	node := *msg.Node
	if _, err := sdk.AccAddressFromBech32(node.OperatorAddress); err != nil {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidAddress, err.Error())
	}
	if node.RewardAddress == "" {
		node.RewardAddress = node.OperatorAddress
	}
	if _, err := sdk.AccAddressFromBech32(node.RewardAddress); err != nil {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidAddress, err.Error())
	}
	if node.Role == types.NodeRole_NODE_ROLE_PLAYER && node.Capabilities != nil && (node.Capabilities.Store || node.Capabilities.Verify || node.Capabilities.Propose) {
		return nil, errorsmod.Wrap(sdkerrors.ErrUnauthorized, "player role cannot enable service/coordinator capabilities")
	}
	if node.Role == types.NodeRole_NODE_ROLE_SERVICE && node.Capabilities != nil && node.Capabilities.Propose {
		return nil, errorsmod.Wrap(sdkerrors.ErrUnauthorized, "service role cannot enable propose capability")
	}
	if node.Role != types.NodeRole_NODE_ROLE_COORDINATOR && node.Capabilities != nil && node.Capabilities.Propose {
		return nil, errorsmod.Wrap(sdkerrors.ErrUnauthorized, "only coordinator role can propose")
	}
	requiredBond := types.RequiredBondedTokensForNode(node)
	node.BondedTokens = 0
	if node.ConsensusAddress != "" {
		consAddr, err := sdk.ConsAddressFromBech32(node.ConsensusAddress)
		if err != nil {
			return nil, errorsmod.Wrap(sdkerrors.ErrInvalidAddress, err.Error())
		}
		if m.keeper.stakingKeeper != nil {
			validator, err := m.keeper.stakingKeeper.GetValidatorByConsAddr(ctx, consAddr)
			if err != nil {
				return nil, err
			}
			if !validator.Tokens.IsUint64() {
				return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "validator tokens exceed uint64 range")
			}
			node.BondedTokens = validator.Tokens.Uint64()
		}
	}
	if requiredBond > 0 && node.ConsensusAddress == "" {
		return nil, errorsmod.Wrap(sdkerrors.ErrUnauthorized, "service/coordinator nodes must provide consensus_address")
	}
	if node.BondedTokens < requiredBond {
		return nil, errorsmod.Wrap(sdkerrors.ErrUnauthorized, "node bonded_tokens below required threshold")
	}
	if err := m.keeper.SetNode(ctx, node); err != nil {
		return nil, err
	}
	return &types.MsgUpsertNodeResponse{}, nil
}

func (m *msgServer) UpsertAggregateRecord(ctx context.Context, msg *types.MsgUpsertAggregateRecord) (*types.MsgUpsertAggregateRecordResponse, error) {
	if msg == nil || msg.AggregateRecord == nil {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "aggregate_record is required")
	}
	if msg.Operator == "" {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidAddress, "operator address is required")
	}
	if _, err := sdk.AccAddressFromBech32(msg.Operator); err != nil {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidAddress, err.Error())
	}
	if _, err := m.requireNodeCapability(ctx, msg.Operator, "verify"); err != nil {
		return nil, err
	}
	if err := m.keeper.SetAggregateRecord(ctx, *msg.AggregateRecord); err != nil {
		return nil, err
	}
	// Keep the epoch's aggregates commitment in sync with the record set:
	// aggregate upserts may arrive during the challenge window, after the
	// proposer committed its roots, and FinalizeEpoch recomputes the
	// aggregates root from the final record set.
	if err := m.keeper.RefreshAggregatesCommitment(ctx, msg.AggregateRecord.EpochId); err != nil {
		return nil, err
	}
	return &types.MsgUpsertAggregateRecordResponse{}, nil
}

func (m *msgServer) SubmitBatch(ctx context.Context, msg *types.MsgSubmitBatch) (*types.MsgSubmitBatchResponse, error) {
	if msg == nil || msg.BatchCommit == nil {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "batch_commit is required")
	}
	if msg.Collector == "" || msg.BatchCommit.CollectorAddress == "" {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidAddress, "collector address is required")
	}
	if msg.Collector != msg.BatchCommit.CollectorAddress {
		return nil, errorsmod.Wrap(sdkerrors.ErrUnauthorized, "collector must match batch commit collector_address")
	}
	if msg.BatchCommit.PayloadCid == "" {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "payload_cid is required")
	}
	if msg.BatchCommit.ObservationCount == 0 {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "observation_count must be greater than 0")
	}
	if msg.BatchCommit.SlotStart > msg.BatchCommit.SlotEnd {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "slot_start must be <= slot_end")
	}
	if _, err := m.requireNodeCapability(ctx, msg.Collector, "collect"); err != nil {
		return nil, err
	}

	batch := *msg.BatchCommit
	batch.SubmittedAtHeight = sdk.UnwrapSDKContext(ctx).BlockHeight()
	if err := m.keeper.SetBatchCommit(ctx, batch); err != nil {
		return nil, err
	}
	return &types.MsgSubmitBatchResponse{}, nil
}

func (m *msgServer) SubmitReplicaReceipt(ctx context.Context, msg *types.MsgSubmitReplicaReceipt) (*types.MsgSubmitReplicaReceiptResponse, error) {
	if msg == nil || msg.ReplicaReceipt == nil {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "replica_receipt is required")
	}
	if msg.Storer == "" || msg.ReplicaReceipt.StorerAddress == "" {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidAddress, "storer address is required")
	}
	if msg.Storer != msg.ReplicaReceipt.StorerAddress {
		return nil, errorsmod.Wrap(sdkerrors.ErrUnauthorized, "storer must match replica receipt storer_address")
	}
	if msg.ReplicaReceipt.PayloadCid == "" {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "payload_cid is required")
	}
	if _, err := m.requireNodeCapability(ctx, msg.Storer, "store"); err != nil {
		return nil, err
	}
	receipt := *msg.ReplicaReceipt
	if err := m.keeper.SetReplicaReceipt(ctx, receipt); err != nil {
		return nil, err
	}
	availability := types.AvailabilityRecord{
		EpochId:             receipt.EpochId,
		OperatorAddress:     receipt.StorerAddress,
		PayloadCid:          receipt.PayloadCid,
		RetentionUntilEpoch: receipt.RetentionUntilEpoch,
		ReceiptHashHex:      receipt.ReceiptHashHex,
	}
	if err := m.keeper.SetAvailabilityRecord(ctx, availability); err != nil {
		return nil, err
	}
	return &types.MsgSubmitReplicaReceiptResponse{}, nil
}

func (m *msgServer) CommitEpoch(ctx context.Context, msg *types.MsgCommitEpoch) (*types.MsgCommitEpochResponse, error) {
	if msg == nil || msg.EpochCommit == nil {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "epoch_commit is required")
	}
	if msg.Proposer == "" || msg.EpochCommit.ProposerAddress == "" {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidAddress, "proposer address is required")
	}
	if msg.Proposer != msg.EpochCommit.ProposerAddress {
		return nil, errorsmod.Wrap(sdkerrors.ErrUnauthorized, "proposer must match epoch commit proposer_address")
	}

	commit := *msg.EpochCommit
	if commit.ChallengeOpenHeight == 0 {
		commit.ChallengeOpenHeight = sdk.UnwrapSDKContext(ctx).BlockHeight()
	}
	if commit.ChallengeDeadlineHeight <= commit.ChallengeOpenHeight {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "challenge deadline must be after challenge open height")
	}
	if _, err := m.requireNodeCapability(ctx, msg.Proposer, "propose"); err != nil {
		return nil, err
	}
	if err := m.keeper.SetEpochCommit(ctx, commit); err != nil {
		return nil, err
	}
	return &types.MsgCommitEpochResponse{}, nil
}

func (m *msgServer) OpenChallenge(ctx context.Context, msg *types.MsgOpenChallenge) (*types.MsgOpenChallengeResponse, error) {
	if msg == nil || msg.Challenge == nil {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "challenge is required")
	}
	if msg.Challenger == "" || msg.Challenge.Challenger == "" {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidAddress, "challenger address is required")
	}
	if msg.Challenger != msg.Challenge.Challenger {
		return nil, errorsmod.Wrap(sdkerrors.ErrUnauthorized, "challenger must match challenge.challenger")
	}
	if msg.Challenge.ChallengeIdHex == "" {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "challenge_id_hex is required")
	}

	challenge := *msg.Challenge
	challenge.OpenedAtHeight = sdk.UnwrapSDKContext(ctx).BlockHeight()
	if challenge.State == types.ChallengeState_CHALLENGE_STATE_UNSPECIFIED {
		challenge.State = types.ChallengeStateOpen
	}
	if challenge.DeadlineHeight <= challenge.OpenedAtHeight {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "deadline_height must be after opened_at_height")
	}
	if _, err := m.requireNodeCapability(ctx, msg.Challenger, "verify"); err != nil {
		return nil, err
	}
	// Escrow the bond while the challenge is open. A losing challenge is
	// forfeited along the challenge_bond_burn_bps channel; a zero bond is a
	// no-op so bond-less challenges stay valid.
	if err := m.keeper.EscrowChallengeBond(ctx, msg.Challenger, challenge.BondAmount); err != nil {
		return nil, err
	}
	if err := m.keeper.SetChallenge(ctx, challenge); err != nil {
		return nil, err
	}
	return &types.MsgOpenChallengeResponse{}, nil
}

func (m *msgServer) ResolveChallenge(ctx context.Context, msg *types.MsgResolveChallenge) (*types.MsgResolveChallengeResponse, error) {
	if msg == nil {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "message is required")
	}
	if err := sdk.ValidateAuthority(sdk.UnwrapSDKContext(ctx), m.keeper.GetAuthority(), msg.Resolver); err != nil {
		return nil, err
	}
	challenge, err := m.keeper.GetChallenge(ctx, msg.ChallengeIdHex)
	if err != nil {
		return nil, err
	}
	challenge.SlashAmount = msg.SlashAmount
	challenge.ChallengerReward = msg.ChallengerReward
	challenge.ResolutionSummary = msg.ResolutionSummary
	challenge.State = msg.FinalState
	if challenge.State == types.ChallengeState_CHALLENGE_STATE_UNSPECIFIED {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "final_state must be specified")
	}
	if err := m.validateChallengeEvidence(ctx, challenge); err != nil {
		return nil, err
	}
	if challenge.TargetAddress != "" {
		targetNode, err := m.keeper.GetNode(ctx, challenge.TargetAddress)
		if err != nil {
			return nil, err
		}
		if !targetNode.Active {
			return nil, errorsmod.Wrap(sdkerrors.ErrUnauthorized, "challenge target node is not active")
		}
		switch challenge.Kind {
		case types.ChallengeKindBadBatch:
			if targetNode.Capabilities == nil || !targetNode.Capabilities.Collect {
				return nil, errorsmod.Wrap(sdkerrors.ErrUnauthorized, "challenge target lacks collect capability")
			}
		case types.ChallengeKindBadStorage:
			if targetNode.Capabilities == nil || !targetNode.Capabilities.Store {
				return nil, errorsmod.Wrap(sdkerrors.ErrUnauthorized, "challenge target lacks store capability")
			}
		case types.ChallengeKindBadAggregate:
			if targetNode.Capabilities == nil || !targetNode.Capabilities.Verify {
				return nil, errorsmod.Wrap(sdkerrors.ErrUnauthorized, "challenge target lacks verify capability")
			}
		case types.ChallengeKindBadReward:
			if targetNode.Capabilities == nil || !(targetNode.Capabilities.Propose || targetNode.Capabilities.Verify) {
				return nil, errorsmod.Wrap(sdkerrors.ErrUnauthorized, "challenge target lacks reward-related capability")
			}
		}
	}
	if err := m.keeper.ApplyValidatorSlash(ctx, challenge.TargetConsAddress, msg.SlashFractionBps, msg.JailValidator); err != nil {
		return nil, err
	}
	// Settle the escrowed bond. A rejected challenge forfeits the bond along
	// the challenge_bond_burn_bps channel; an upheld challenge returns it.
	if _, err := m.settleChallengeBond(ctx, &challenge); err != nil {
		return nil, err
	}
	if err := m.applyChallengeRewardEffects(ctx, &challenge); err != nil {
		return nil, err
	}
	if err := m.keeper.RecomputeEpochCommitments(ctx, challenge.EpochId); err != nil {
		return nil, err
	}
	if err := m.keeper.SetChallenge(ctx, challenge); err != nil {
		return nil, err
	}
	return &types.MsgResolveChallengeResponse{}, nil
}

func (m *msgServer) FinalizeEpoch(ctx context.Context, msg *types.MsgFinalizeEpoch) (*types.MsgFinalizeEpochResponse, error) {
	if msg == nil {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "message is required")
	}
	if msg.Finalizer == "" {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidAddress, "finalizer address is required")
	}
	if _, err := sdk.AccAddressFromBech32(msg.Finalizer); err != nil {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidAddress, err.Error())
	}
	if err := m.keeper.FinalizeEpoch(ctx, msg.EpochId); err != nil {
		return nil, err
	}
	return &types.MsgFinalizeEpochResponse{}, nil
}

// VerifyBatch records a verifier's attestation for a batch inside the
// challenge window. A verifier may not attest its own batch (no
// self-audit); player collectors may verify without a separate stake.
func (m *msgServer) VerifyBatch(ctx context.Context, msg *types.MsgVerifyBatch) (*types.MsgVerifyBatchResponse, error) {
	if msg == nil || msg.Verifier == "" {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidAddress, "verifier address is required")
	}
	if msg.TargetBatchRootHex == "" {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "target_batch_root_hex is required")
	}
	if msg.TargetCollector == "" {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "target_collector is required")
	}
	if msg.Verifier == msg.TargetCollector {
		return nil, errorsmod.Wrap(sdkerrors.ErrUnauthorized, "verifier cannot attest its own batch")
	}
	node, err := m.keeper.GetNode(ctx, msg.Verifier)
	if err != nil {
		return nil, err
	}
	if !node.Active {
		return nil, errorsmod.Wrap(sdkerrors.ErrUnauthorized, "node is not active")
	}
	if node.Capabilities == nil || !node.Capabilities.Verify {
		return nil, errorsmod.Wrap(sdkerrors.ErrUnauthorized, "node missing verify capability")
	}

	// Dynamic player classification: a verifier is a "player" when it was
	// live in the previous epoch (submitted batches on-chain). A node
	// that was not live is a plain node and must post the verify bond;
	// without a bond it cannot participate in verification.
	var prevEpoch uint64
	if msg.EpochId > 0 {
		prevEpoch = msg.EpochId - 1
	}
	isPlayer, err := m.keeper.isPlayerCollector(ctx, msg.Verifier, prevEpoch)
	if err != nil {
		return nil, err
	}
	if !isPlayer && node.BondedTokens < types.MinVerifyBondedTokens {
		return nil, errorsmod.Wrapf(
			sdkerrors.ErrUnauthorized,
			"non-player verifier %s must post the verify bond (%d upole) to verify",
			msg.Verifier, types.MinVerifyBondedTokens,
		)
	}

	record := types.VerificationRecord{
		EpochId:            msg.EpochId,
		VerifierAddress:    msg.Verifier,
		TargetBatchRootHex: msg.TargetBatchRootHex,
		TargetCollector:    msg.TargetCollector,
		// The chain decides is_player from on-chain activity; the
		// caller-supplied flag is informational only.
		IsPlayer:         isPlayer,
		Verified:         msg.Verified,
		VerifiedAtHeight: sdk.UnwrapSDKContext(ctx).BlockHeight(),
		SignatureHex:     msg.SignatureHex,
	}
	if err := m.keeper.SetVerificationRecord(ctx, record); err != nil {
		return nil, err
	}
	return &types.MsgVerifyBatchResponse{}, nil
}

func (m *msgServer) ClaimReward(ctx context.Context, msg *types.MsgClaimReward) (*types.MsgClaimRewardResponse, error) {
	if msg == nil {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "message is required")
	}
	if msg.Claimer == "" || msg.Recipient == "" {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidAddress, "claimer and recipient are required")
	}
	if msg.Claimer != msg.Recipient {
		return nil, errorsmod.Wrap(sdkerrors.ErrUnauthorized, "claimer must match recipient")
	}
	claimed, err := m.keeper.HasClaimedReward(ctx, msg.EpochId, msg.Recipient)
	if err != nil {
		return nil, err
	}
	if claimed {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "reward already claimed")
	}
	commit, err := m.keeper.GetEpochCommit(ctx, msg.EpochId)
	if err != nil {
		return nil, err
	}
	if !commit.Finalized {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "epoch is not finalized")
	}
	record, err := m.keeper.GetRewardRecord(ctx, msg.EpochId, msg.Recipient)
	if err != nil {
		return nil, err
	}
	if record.NetReward == 0 {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "reward amount is zero")
	}
	claim := types.ClaimedReward{
		EpochId:         msg.EpochId,
		Recipient:       msg.Recipient,
		ClaimedAtHeight: sdk.UnwrapSDKContext(ctx).BlockHeight(),
		Amount:          record.NetReward,
	}
	if err := m.keeper.PayoutClaimedReward(ctx, claim); err != nil {
		return nil, err
	}
	if err := m.keeper.SetClaimedReward(ctx, claim); err != nil {
		return nil, err
	}
	return &types.MsgClaimRewardResponse{RewardRecord: &record}, nil
}

func (m *msgServer) UpsertGameWeight(ctx context.Context, msg *types.MsgUpsertGameWeight) (*types.MsgUpsertGameWeightResponse, error) {
	if msg == nil || msg.Entry == nil {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "entry is required")
	}
	if err := sdk.ValidateAuthority(sdk.UnwrapSDKContext(ctx), m.keeper.GetAuthority(), msg.Authority); err != nil {
		return nil, err
	}
	if msg.Entry.GameWeightPpm == 0 {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "game_weight_ppm must be greater than 0")
	}
	if err := m.keeper.SetGameWeightEntry(ctx, *msg.Entry); err != nil {
		return nil, err
	}
	return &types.MsgUpsertGameWeightResponse{}, nil
}

func (m *msgServer) UpdateParams(ctx context.Context, msg *types.MsgUpdateParams) (*types.MsgUpdateParamsResponse, error) {
	if msg == nil || msg.Params == nil {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "params are required")
	}
	if err := sdk.ValidateAuthority(sdk.UnwrapSDKContext(ctx), m.keeper.GetAuthority(), msg.Authority); err != nil {
		return nil, err
	}
	if err := m.keeper.SetParams(ctx, *msg.Params); err != nil {
		return nil, err
	}
	return &types.MsgUpdateParamsResponse{}, nil
}

func (m *msgServer) String() string {
	return fmt.Sprintf("pole-msg-server<authority=%s>", m.keeper.GetAuthority())
}

// SubmitPlaySession records a node's signed claim that it played a mapped
// game for N seconds in one slot. The chain validates the claim's internal
// consistency and identity binding; whether the claim is *true* is decided
// later by witness attestations and heartbeats (see AttestSession and
// SettleSession), not by the claim itself.
func (m *msgServer) SubmitPlaySession(ctx context.Context, msg *types.MsgSubmitPlaySession) (*types.MsgSubmitPlaySessionResponse, error) {
	if msg == nil || msg.Session == nil {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "session is required")
	}
	if msg.NodeAddress == "" || msg.Session.NodeAddress == "" {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidAddress, "node_address is required")
	}
	// The signer must be the node making the claim: a node cannot submit
	// play time on another node's behalf.
	if msg.NodeAddress != msg.Session.NodeAddress {
		return nil, errorsmod.Wrap(sdkerrors.ErrUnauthorized, "node_address must match session.node_address")
	}
	if msg.Session.SessionIdHex == "" {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "session_id_hex is required")
	}
	if msg.Session.PlaySeconds == 0 {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "play_seconds must be greater than 0")
	}

	node, err := m.keeper.GetNode(ctx, msg.NodeAddress)
	if err != nil {
		return nil, err
	}
	if !node.Active {
		return nil, errorsmod.Wrap(sdkerrors.ErrUnauthorized, "node is not active")
	}

	// A claim cannot exceed the slot it belongs to.
	params, err := m.keeper.GetParams(ctx)
	if err != nil {
		return nil, err
	}
	slotSeconds := params.RewardBlockDurationSeconds
	if slotSeconds > 0 && msg.Session.PlaySeconds > slotSeconds {
		return nil, errorsmod.Wrapf(
			sdkerrors.ErrInvalidRequest,
			"play_seconds %d exceeds slot duration %d",
			msg.Session.PlaySeconds, slotSeconds,
		)
	}

	session := *msg.Session
	session.SubmittedAtHeight = sdk.UnwrapSDKContext(ctx).BlockHeight()
	if err := m.keeper.SetPlaySession(ctx, session); err != nil {
		return nil, err
	}
	return &types.MsgSubmitPlaySessionResponse{}, nil
}

// SubmitPlayHeartbeat records one signed liveness proof for a session.
// Heartbeats are what make "I played for N seconds" checkable: settlement
// requires them to cover the declared window.
func (m *msgServer) SubmitPlayHeartbeat(ctx context.Context, msg *types.MsgSubmitPlayHeartbeat) (*types.MsgSubmitPlayHeartbeatResponse, error) {
	if msg == nil || msg.Heartbeat == nil {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "heartbeat is required")
	}
	if msg.NodeAddress == "" || msg.Heartbeat.NodeAddress == "" {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidAddress, "node_address is required")
	}
	if msg.NodeAddress != msg.Heartbeat.NodeAddress {
		return nil, errorsmod.Wrap(sdkerrors.ErrUnauthorized, "node_address must match heartbeat.node_address")
	}
	if msg.Heartbeat.SessionIdHex == "" {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "session_id_hex is required")
	}

	// A heartbeat must belong to a session the same node declared, and a
	// session already settled cannot accumulate more evidence.
	session, err := m.keeper.GetPlaySession(ctx, msg.Heartbeat.SessionIdHex)
	if err != nil {
		if errors.Is(err, collections.ErrNotFound) {
			return nil, errorsmod.Wrap(sdkerrors.ErrNotFound, "play session not found")
		}
		return nil, err
	}
	if session.NodeAddress != msg.NodeAddress {
		return nil, errorsmod.Wrap(sdkerrors.ErrUnauthorized, "heartbeat node does not own the session")
	}
	if settled, err := m.keeper.SessionSettlements.Has(ctx, msg.Heartbeat.SessionIdHex); err != nil {
		return nil, err
	} else if settled {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "session is already settled")
	}

	heartbeat := *msg.Heartbeat
	if err := m.keeper.SetPlayHeartbeat(ctx, heartbeat); err != nil {
		return nil, err
	}
	return &types.MsgSubmitPlayHeartbeatResponse{}, nil
}

// AttestSession records an independent node's corroboration of a play
// session. This is the "mutual proof" step: the witness must hold its own
// observation of the same app/slot and its data must agree with the claim
// within tolerance. A witness cannot attest its own session, a session it
// collected, or one it has no observation of.
func (m *msgServer) AttestSession(ctx context.Context, msg *types.MsgAttestSession) (*types.MsgAttestSessionResponse, error) {
	if msg == nil || msg.Attestation == nil {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "attestation is required")
	}
	if msg.Witness == "" || msg.Attestation.WitnessAddress == "" {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidAddress, "witness address is required")
	}
	if msg.Witness != msg.Attestation.WitnessAddress {
		return nil, errorsmod.Wrap(sdkerrors.ErrUnauthorized, "witness must match attestation.witness_address")
	}
	if msg.Attestation.SessionIdHex == "" {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "session_id_hex is required")
	}

	session, err := m.keeper.GetPlaySession(ctx, msg.Attestation.SessionIdHex)
	if err != nil {
		if errors.Is(err, collections.ErrNotFound) {
			return nil, errorsmod.Wrap(sdkerrors.ErrNotFound, "play session not found")
		}
		return nil, err
	}
	if settled, err := m.keeper.SessionSettlements.Has(ctx, msg.Attestation.SessionIdHex); err != nil {
		return nil, err
	} else if settled {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "session is already settled")
	}

	params, err := m.keeper.GetParams(ctx)
	if err != nil {
		return nil, err
	}
	if err := m.keeper.validateWitnessIndependence(ctx, session, *msg.Attestation, params); err != nil {
		return nil, errorsmod.Wrap(sdkerrors.ErrUnauthorized, err.Error())
	}

	attestation := *msg.Attestation
	attestation.AttestedAtHeight = sdk.UnwrapSDKContext(ctx).BlockHeight()
	if err := m.keeper.SetWitnessAttestation(ctx, attestation); err != nil {
		return nil, err
	}
	return &types.MsgAttestSessionResponse{}, nil
}

// SettleSession asks the chain to evaluate a play session. The settlement
// is computed from stored evidence; the caller supplies only the id, so a
// proposer cannot assert a session valid that the evidence contradicts.
func (m *msgServer) SettleSession(ctx context.Context, msg *types.MsgSettleSession) (*types.MsgSettleSessionResponse, error) {
	if msg == nil || msg.SessionIdHex == "" {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "session_id_hex is required")
	}
	if msg.Settler == "" {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidAddress, "settler address is required")
	}
	if _, err := m.requireNodeCapability(ctx, msg.Settler, "verify"); err != nil {
		return nil, err
	}
	settlement, err := m.keeper.SettlePlaySession(ctx, msg.SessionIdHex)
	if err != nil {
		return nil, err
	}
	return &types.MsgSettleSessionResponse{Settlement: &settlement}, nil
}

// SubmitRewardRecords is the live reward-record submission path. Without
// it, reward records only ever entered the store via genesis or challenge
// resolution, so a claim against a proposer-committed reward root could
// never find its record and always failed. The chain recomputes the
// rewards root from the submitted records and requires it to match the
// committed root, which turns "the proposer promised these rewards" into
// "the chain holds the records backing that promise".
func (m *msgServer) SubmitRewardRecords(ctx context.Context, msg *types.MsgSubmitRewardRecords) (*types.MsgSubmitRewardRecordsResponse, error) {
	if msg == nil {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "message is required")
	}
	if msg.Proposer == "" {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidAddress, "proposer address is required")
	}
	if len(msg.Records) == 0 {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "records must not be empty")
	}
	if _, err := m.requireNodeCapability(ctx, msg.Proposer, "propose"); err != nil {
		return nil, err
	}

	commit, err := m.keeper.GetEpochCommit(ctx, msg.EpochId)
	if err != nil {
		return nil, err
	}
	if commit.Finalized {
		return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "epoch is already finalized")
	}

	records := make([]types.RewardRecord, 0, len(msg.Records))
	for _, record := range msg.Records {
		if record == nil {
			return nil, errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "reward record must not be nil")
		}
		if record.EpochId != msg.EpochId {
			return nil, errorsmod.Wrapf(
				sdkerrors.ErrInvalidRequest,
				"reward record epoch %d does not match message epoch %d",
				record.EpochId, msg.EpochId,
			)
		}
		if record.Recipient == "" {
			return nil, errorsmod.Wrap(sdkerrors.ErrInvalidAddress, "reward record recipient is required")
		}
		records = append(records, *record)
	}

	// Bind the submitted set to the proposer's commitment: the root the
	// chain derives from these records must equal the committed root. This
	// is what stops a proposer from committing one root and later
	// submitting a different record set.
	root, leafCount, err := types.MerkleRootHexForRecords(records)
	if err != nil {
		return nil, err
	}
	if commit.Rewards != nil && (commit.Rewards.Root != root || commit.Rewards.LeafCount != leafCount) {
		return nil, errorsmod.Wrapf(
			sdkerrors.ErrInvalidRequest,
			"reward records root %s (%d leaves) does not match committed root %s (%d leaves)",
			root, leafCount, commit.Rewards.Root, commit.Rewards.LeafCount,
		)
	}

	for _, record := range records {
		if err := m.keeper.SetRewardRecord(ctx, record); err != nil {
			return nil, err
		}
	}
	return &types.MsgSubmitRewardRecordsResponse{RewardRootHex: root, LeafCount: leafCount}, nil
}

func (m *msgServer) validateChallengeEvidence(ctx context.Context, challenge types.Challenge) error {
	if challenge.Evidence == nil {
		return nil
	}
	commit, err := m.keeper.GetEpochCommit(ctx, challenge.EpochId)
	if err != nil {
		return err
	}
	switch challenge.Kind {
	case types.ChallengeKindBadBatch:
		if challenge.TargetAddress == "" || challenge.Evidence.BatchRootHex == "" || len(challenge.Evidence.MerkleProofHex) == 0 {
			return errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "bad batch challenge requires target_address, batch_root_hex, and merkle_proof_hex")
		}
		if commit.AcceptedBatches == nil {
			return errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "epoch missing accepted_batches commitment")
		}
		records, err := m.keeper.batchCommitsForEpoch(ctx, challenge.EpochId)
		if err != nil {
			return err
		}
		index := -1
		var target types.BatchCommit
		for i, record := range records {
			if record.CollectorAddress == challenge.TargetAddress && record.Batch != nil && record.Batch.Root == challenge.Evidence.BatchRootHex {
				index = i
				target = record
				break
			}
		}
		if index < 0 {
			return errorsmod.Wrap(sdkerrors.ErrNotFound, "batch record referenced by challenge not found")
		}
		leaf, err := types.MerkleLeafFromRecord(target)
		if err != nil {
			return err
		}
		if commit.AcceptedBatches == nil || !types.VerifyMerkleProofHex(leaf, challenge.Evidence.MerkleProofHex, index, commit.AcceptedBatches.Root) {
			return errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "batch merkle proof verification failed")
		}
	case types.ChallengeKindBadReward:
		if challenge.TargetAddress == "" || challenge.Evidence.RewardRootHex == "" || len(challenge.Evidence.MerkleProofHex) == 0 {
			return errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "bad reward challenge requires target_address, reward_root_hex, and merkle_proof_hex")
		}
		if commit.Rewards == nil || commit.Rewards.Root != challenge.Evidence.RewardRootHex {
			return errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "reward_root_hex does not match committed reward root")
		}
		records, err := m.keeper.rewardRecordsForEpoch(ctx, challenge.EpochId)
		if err != nil {
			return err
		}
		index := -1
		var target types.RewardRecord
		for i, record := range records {
			if record.Recipient == challenge.TargetAddress {
				index = i
				target = record
				break
			}
		}
		if index < 0 {
			return errorsmod.Wrap(sdkerrors.ErrNotFound, "reward record referenced by challenge not found")
		}
		leaf, err := types.MerkleLeafFromRecord(target)
		if err != nil {
			return err
		}
		if !types.VerifyMerkleProofHex(leaf, challenge.Evidence.MerkleProofHex, index, challenge.Evidence.RewardRootHex) {
			return errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "reward merkle proof verification failed")
		}
	case types.ChallengeKindBadAggregate:
		if challenge.Evidence.AggregateRootHex == "" || len(challenge.Evidence.MerkleProofHex) == 0 {
			return errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "bad aggregate challenge requires aggregate_root_hex and merkle_proof_hex")
		}
		if commit.Aggregates == nil || commit.Aggregates.Root != challenge.Evidence.AggregateRootHex {
			return errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "aggregate_root_hex does not match committed aggregate root")
		}
		records, err := m.keeper.aggregateRecordsForEpoch(ctx, challenge.EpochId)
		if err != nil {
			return err
		}
		index := -1
		var target types.AggregateRecord
		for i, record := range records {
			if record.AppId == challenge.Evidence.AggregateAppId {
				index = i
				target = record
				break
			}
		}
		if index < 0 {
			return errorsmod.Wrap(sdkerrors.ErrNotFound, "aggregate record referenced by challenge not found")
		}
		leaf, err := types.MerkleLeafFromRecord(target)
		if err != nil {
			return err
		}
		if !types.VerifyMerkleProofHex(leaf, challenge.Evidence.MerkleProofHex, index, challenge.Evidence.AggregateRootHex) {
			return errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "aggregate merkle proof verification failed")
		}
	case types.ChallengeKindBadStorage:
		if challenge.TargetAddress == "" || challenge.Evidence.PayloadCid == "" || len(challenge.Evidence.MerkleProofHex) == 0 {
			return errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "bad storage challenge requires target_address, payload_cid, and merkle_proof_hex")
		}
		if commit.Availability == nil {
			return errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "epoch missing availability commitment")
		}
		records, err := m.keeper.availabilityRecordsForEpoch(ctx, challenge.EpochId)
		if err != nil {
			return err
		}
		index := -1
		var target types.AvailabilityRecord
		for i, record := range records {
			if record.OperatorAddress == challenge.TargetAddress && record.PayloadCid == challenge.Evidence.PayloadCid {
				index = i
				target = record
				break
			}
		}
		if index < 0 {
			return errorsmod.Wrap(sdkerrors.ErrNotFound, "availability record referenced by challenge not found")
		}
		leaf, err := types.MerkleLeafFromRecord(target)
		if err != nil {
			return err
		}
		if !types.VerifyMerkleProofHex(leaf, challenge.Evidence.MerkleProofHex, index, commit.Availability.Root) {
			return errorsmod.Wrap(sdkerrors.ErrInvalidRequest, "availability merkle proof verification failed")
		}
	}
	return nil
}

func (m *msgServer) applyChallengeRewardEffects(ctx context.Context, challenge *types.Challenge) error {
	if challenge == nil {
		return nil
	}
	params, err := m.keeper.GetParams(ctx)
	if err != nil {
		return err
	}

	if challenge.TargetAddress != "" && challenge.SlashAmount > 0 {
		targetRecord, err := m.keeper.GetRewardRecord(ctx, challenge.EpochId, challenge.TargetAddress)
		if err != nil {
			if !errors.Is(err, collections.ErrNotFound) {
				return err
			}
			targetRecord = types.RewardRecord{EpochId: challenge.EpochId, Recipient: challenge.TargetAddress}
		}
		slashApplied := challenge.SlashAmount
		if slashApplied > targetRecord.NetReward {
			slashApplied = targetRecord.NetReward
		}
		targetRecord.SlashDebit += slashApplied
		targetRecord.NetReward -= slashApplied

		// Governance penalty channel: a share of the actually-applied slash is
		// destroyed rather than recycled into the pool. Bounded by the pool
		// balance, so the channel can never over-burn.
		if params.GovernanceBurnBps > 0 {
			burn := slashApplied * uint64(params.GovernanceBurnBps) / 10_000
			if _, err := m.keeper.BurnFromRewardPool(ctx, burn); err != nil {
				return err
			}
		}

		// False-play channel: when a challenge invalidates a play session
		// (bad reward / fabricated engagement), the session's player reward is
		// slashed and destroyed. This is what makes a fake claim cost more than
		// it earns.
		if challenge.Kind == types.ChallengeKindBadReward && params.SessionSlashBps > 0 {
			slashBase := targetRecord.PlayerReward
			if slashBase > targetRecord.NetReward {
				slashBase = targetRecord.NetReward
			}
			slashed := slashBase * uint64(params.SessionSlashBps) / 10_000
			if slashed > 0 {
				targetRecord.SlashDebit += slashed
				targetRecord.NetReward -= slashed
				if _, err := m.keeper.BurnFromRewardPool(ctx, slashed); err != nil {
					return err
				}
			}
		}

		if err := m.keeper.SetRewardRecord(ctx, targetRecord); err != nil {
			return err
		}
	}

	if challenge.Challenger != "" && challenge.ChallengerReward > 0 {
		challengerRecord, err := m.keeper.GetRewardRecord(ctx, challenge.EpochId, challenge.Challenger)
		if err != nil {
			if !errors.Is(err, collections.ErrNotFound) {
				return err
			}
			challengerRecord = types.RewardRecord{EpochId: challenge.EpochId, Recipient: challenge.Challenger}
		}
		challengerRecord.VerifyReward += challenge.ChallengerReward
		challengerRecord.NetReward += challenge.ChallengerReward
		if err := m.keeper.SetRewardRecord(ctx, challengerRecord); err != nil {
			return err
		}
	}

	return nil
}

// settleChallengeBond resolves the escrowed challenge bond along the
// challenge_bond_burn_bps channel. A REJECTED challenge means the challenger
// was wrong, so the bond is forfeited and the configured share destroyed; any
// other final state returns the bond intact.
func (m *msgServer) settleChallengeBond(ctx context.Context, challenge *types.Challenge) (uint64, error) {
	if challenge == nil || challenge.BondAmount == 0 || challenge.Challenger == "" {
		return 0, nil
	}
	params, err := m.keeper.GetParams(ctx)
	if err != nil {
		return 0, err
	}
	forfeit := challenge.State == types.ChallengeStateRejected
	return m.keeper.SettleChallengeBond(ctx, challenge.Challenger, challenge.BondAmount, params.ChallengeBondBurnBps, forfeit)
}
