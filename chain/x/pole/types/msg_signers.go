package types

import (
	"fmt"

	sdk "github.com/cosmos/cosmos-sdk/types"
)

func (m *MsgSubmitBatch) GetSigners() []sdk.AccAddress {
	return mustAccSigners(m.Collector)
}

func (m *MsgUpsertNode) GetSigners() []sdk.AccAddress {
	return mustAccSigners(m.Operator)
}

func (m *MsgUpsertAggregateRecord) GetSigners() []sdk.AccAddress {
	return mustAccSigners(m.Operator)
}

func (m *MsgSubmitReplicaReceipt) GetSigners() []sdk.AccAddress {
	return mustAccSigners(m.Storer)
}

func (m *MsgCommitEpoch) GetSigners() []sdk.AccAddress {
	return mustAccSigners(m.Proposer)
}

func (m *MsgOpenChallenge) GetSigners() []sdk.AccAddress {
	return mustAccSigners(m.Challenger)
}

func (m *MsgResolveChallenge) GetSigners() []sdk.AccAddress {
	return mustAccSigners(m.Resolver)
}

func (m *MsgFinalizeEpoch) GetSigners() []sdk.AccAddress {
	return mustAccSigners(m.Finalizer)
}

func (m *MsgClaimReward) GetSigners() []sdk.AccAddress {
	return mustAccSigners(m.Claimer)
}

func (m *MsgUpsertGameWeight) GetSigners() []sdk.AccAddress {
	return mustAccSigners(m.Authority)
}

func (m *MsgUpdateParams) GetSigners() []sdk.AccAddress {
	return mustAccSigners(m.Authority)
}

func mustAccSigners(bech32 string) []sdk.AccAddress {
	addr, err := sdk.AccAddressFromBech32(bech32)
	if err != nil {
		panic(fmt.Errorf("invalid signer address %q: %w", bech32, err))
	}
	return []sdk.AccAddress{addr}
}
