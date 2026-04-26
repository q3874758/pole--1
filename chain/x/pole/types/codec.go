package types

import (
	"github.com/cosmos/cosmos-sdk/codec"
	"github.com/cosmos/cosmos-sdk/codec/types"
	sdk "github.com/cosmos/cosmos-sdk/types"
	"github.com/cosmos/cosmos-sdk/types/msgservice"
)

func RegisterInterfaces(registry types.InterfaceRegistry) {
	registry.RegisterImplementations(
		(*sdk.Msg)(nil),
		&MsgUpsertNode{},
		&MsgUpsertAggregateRecord{},
		&MsgSubmitBatch{},
		&MsgSubmitReplicaReceipt{},
		&MsgCommitEpoch{},
		&MsgOpenChallenge{},
		&MsgResolveChallenge{},
		&MsgFinalizeEpoch{},
		&MsgClaimReward{},
		&MsgUpsertGameWeight{},
		&MsgUpdateParams{},
	)

	msgservice.RegisterMsgServiceDesc(registry, &_Msg_serviceDesc)
}

func RegisterLegacyAminoCodec(*codec.LegacyAmino) {}
