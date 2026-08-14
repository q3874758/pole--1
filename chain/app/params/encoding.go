package params

import (
	"github.com/cosmos/cosmos-sdk/client"
	"github.com/cosmos/cosmos-sdk/codec"
	codectypes "github.com/cosmos/cosmos-sdk/codec/types"
	"github.com/cosmos/cosmos-sdk/std"
	authtx "github.com/cosmos/cosmos-sdk/x/auth/tx"

	"pole/chain/app"
	poletypes "pole/chain/x/pole/types"
)

// EncodingConfig specifies the concrete encoding types to use for a given app.
// This is provided for compatibility between protobuf and amino implementations.
type EncodingConfig struct {
	InterfaceRegistry codectypes.InterfaceRegistry
	Codec             codec.Codec
	TxConfig          client.TxConfig
	Amino             *codec.LegacyAmino
}

// MakeEncodingConfig creates an EncodingConfig for the PoLE chain.
func MakeEncodingConfig() EncodingConfig {
	interfaceRegistry, err := app.NewInterfaceRegistry()
	if err != nil {
		panic(err)
	}
	std.RegisterInterfaces(interfaceRegistry)
	app.ModuleBasics.RegisterInterfaces(interfaceRegistry)

	cdc := codec.NewProtoCodec(interfaceRegistry)
	amino := codec.NewLegacyAmino()
	std.RegisterLegacyAminoCodec(amino)
	poletypes.RegisterLegacyAminoCodec(amino)

	return EncodingConfig{
		InterfaceRegistry: interfaceRegistry,
		Codec:             cdc,
		TxConfig:          authtx.NewTxConfig(cdc, authtx.DefaultSignModes),
		Amino:             amino,
	}
}
