package keeper

import (
	"context"
	"errors"

	"cosmossdk.io/collections"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/status"

	"pole/chain/x/pole/types"
)

var _ types.QueryServer = (*queryServer)(nil)

type queryServer struct {
	keeper *Keeper
}

func NewQueryServer(k *Keeper) types.QueryServer {
	return &queryServer{keeper: k}
}

func (q *queryServer) Params(ctx context.Context, _ *types.QueryParamsRequest) (*types.QueryParamsResponse, error) {
	params, err := q.keeper.GetParams(ctx)
	if err != nil {
		return nil, status.Error(codes.Internal, err.Error())
	}
	return &types.QueryParamsResponse{Params: &params}, nil
}

func (q *queryServer) Node(ctx context.Context, req *types.QueryNodeRequest) (*types.QueryNodeResponse, error) {
	record, err := q.keeper.GetNode(ctx, req.OperatorAddress)
	if err != nil {
		return nil, grpcError(err)
	}
	return &types.QueryNodeResponse{Node: &record}, nil
}

func (q *queryServer) BatchCommit(ctx context.Context, req *types.QueryBatchCommitRequest) (*types.QueryBatchCommitResponse, error) {
	record, err := q.keeper.GetBatchCommit(ctx, req.EpochId, req.CollectorAddress, req.BatchRootHex)
	if err != nil {
		return nil, grpcError(err)
	}
	return &types.QueryBatchCommitResponse{BatchCommit: &record}, nil
}

func (q *queryServer) AggregateRecord(ctx context.Context, req *types.QueryAggregateRecordRequest) (*types.QueryAggregateRecordResponse, error) {
	record, err := q.keeper.GetAggregateRecord(ctx, req.EpochId, uint32(req.AppId))
	if err != nil {
		return nil, grpcError(err)
	}
	return &types.QueryAggregateRecordResponse{AggregateRecord: &record}, nil
}

func (q *queryServer) ReplicaReceipt(ctx context.Context, req *types.QueryReplicaReceiptRequest) (*types.QueryReplicaReceiptResponse, error) {
	record, err := q.keeper.GetReplicaReceipt(ctx, req.EpochId, req.StorerAddress, req.PayloadCid)
	if err != nil {
		return nil, grpcError(err)
	}
	return &types.QueryReplicaReceiptResponse{ReplicaReceipt: &record}, nil
}

func (q *queryServer) EpochCommit(ctx context.Context, req *types.QueryEpochCommitRequest) (*types.QueryEpochCommitResponse, error) {
	record, err := q.keeper.GetEpochCommit(ctx, req.EpochId)
	if err != nil {
		return nil, grpcError(err)
	}
	return &types.QueryEpochCommitResponse{EpochCommit: &record}, nil
}

func (q *queryServer) RewardRecord(ctx context.Context, req *types.QueryRewardRecordRequest) (*types.QueryRewardRecordResponse, error) {
	record, err := q.keeper.GetRewardRecord(ctx, req.EpochId, req.Recipient)
	if err != nil {
		return nil, grpcError(err)
	}
	return &types.QueryRewardRecordResponse{RewardRecord: &record}, nil
}

func (q *queryServer) ClaimedReward(ctx context.Context, req *types.QueryClaimedRewardRequest) (*types.QueryClaimedRewardResponse, error) {
	record, err := q.keeper.GetClaimedReward(ctx, req.EpochId, req.Recipient)
	if err != nil {
		return nil, grpcError(err)
	}
	return &types.QueryClaimedRewardResponse{ClaimedReward: &record}, nil
}

func (q *queryServer) Challenge(ctx context.Context, req *types.QueryChallengeRequest) (*types.QueryChallengeResponse, error) {
	record, err := q.keeper.GetChallenge(ctx, req.ChallengeIdHex)
	if err != nil {
		return nil, grpcError(err)
	}
	return &types.QueryChallengeResponse{Challenge: &record}, nil
}

func (q *queryServer) GameWeight(ctx context.Context, req *types.QueryGameWeightRequest) (*types.QueryGameWeightResponse, error) {
	record, err := q.keeper.GetGameWeightEntry(ctx, uint32(req.AppId), req.EffectiveFromEpochId)
	if err != nil {
		return nil, grpcError(err)
	}
	return &types.QueryGameWeightResponse{Entry: &record}, nil
}

func grpcError(err error) error {
	if errors.Is(err, collections.ErrNotFound) {
		return status.Error(codes.NotFound, err.Error())
	}
	return status.Error(codes.Internal, err.Error())
}
