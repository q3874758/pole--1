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
