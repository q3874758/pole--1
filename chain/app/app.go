package app

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"time"

	cmtabci "github.com/cometbft/cometbft/abci/types"
	cmtproto "github.com/cometbft/cometbft/proto/tendermint/types"
	dbm "github.com/cosmos/cosmos-db"

	"cosmossdk.io/log/v2"

	"github.com/cosmos/cosmos-sdk/baseapp"
	"github.com/cosmos/cosmos-sdk/client"
	"github.com/cosmos/cosmos-sdk/codec"
	addresscodec "github.com/cosmos/cosmos-sdk/codec/address"
	codectypes "github.com/cosmos/cosmos-sdk/codec/types"
	"github.com/cosmos/cosmos-sdk/runtime"
	std "github.com/cosmos/cosmos-sdk/std"
	storetypes "github.com/cosmos/cosmos-sdk/store/v2/types"
	sdk "github.com/cosmos/cosmos-sdk/types"
	"github.com/cosmos/cosmos-sdk/types/module"
	authmodule "github.com/cosmos/cosmos-sdk/x/auth"
	authkeeper "github.com/cosmos/cosmos-sdk/x/auth/keeper"
	authsimulation "github.com/cosmos/cosmos-sdk/x/auth/simulation"
	authtx "github.com/cosmos/cosmos-sdk/x/auth/tx"
	authtypes "github.com/cosmos/cosmos-sdk/x/auth/types"
	bankmodule "github.com/cosmos/cosmos-sdk/x/bank"
	bankkeeper "github.com/cosmos/cosmos-sdk/x/bank/keeper"
	banktypes "github.com/cosmos/cosmos-sdk/x/bank/types"
	consensusmodule "github.com/cosmos/cosmos-sdk/x/consensus"
	consensuskeeper "github.com/cosmos/cosmos-sdk/x/consensus/keeper"
	consensustypes "github.com/cosmos/cosmos-sdk/x/consensus/types"
	epochsmodule "github.com/cosmos/cosmos-sdk/x/epochs"
	epochskeeper "github.com/cosmos/cosmos-sdk/x/epochs/keeper"
	epochstypes "github.com/cosmos/cosmos-sdk/x/epochs/types"
	govmodule "github.com/cosmos/cosmos-sdk/x/gov"
	govkeeper "github.com/cosmos/cosmos-sdk/x/gov/keeper"
	govtypes "github.com/cosmos/cosmos-sdk/x/gov/types"
	slashingmodule "github.com/cosmos/cosmos-sdk/x/slashing"
	slashingkeeper "github.com/cosmos/cosmos-sdk/x/slashing/keeper"
	slashingtypes "github.com/cosmos/cosmos-sdk/x/slashing/types"
	stakingmodule "github.com/cosmos/cosmos-sdk/x/staking"
	stakingkeeper "github.com/cosmos/cosmos-sdk/x/staking/keeper"
	stakingtypes "github.com/cosmos/cosmos-sdk/x/staking/types"

	polemodule "pole/chain/x/pole"
	polekeeper "pole/chain/x/pole/keeper"
	poletypes "pole/chain/x/pole/types"
)

const AppName = "pole"

var moduleAccountPermissions = map[string][]string{
	authtypes.FeeCollectorName:     nil,
	govtypes.ModuleName:            {authtypes.Burner},
	poletypes.ModuleName:           {authtypes.Minter, authtypes.Burner},
	stakingtypes.BondedPoolName:    {authtypes.Burner, authtypes.Staking},
	stakingtypes.NotBondedPoolName: {authtypes.Burner, authtypes.Staking},
}

type noopDistributionKeeper struct{}

func (noopDistributionKeeper) FundCommunityPool(context.Context, sdk.Coins, sdk.AccAddress) error {
	return nil
}

type App struct {
	*baseapp.BaseApp

	appCodec          codec.Codec
	interfaceRegistry codectypes.InterfaceRegistry
	keys              map[string]*storetypes.KVStoreKey
	txConfig          client.TxConfig

	AccountKeeper   authkeeper.AccountKeeper
	BankKeeper      bankkeeper.BaseKeeper
	StakingKeeper   *stakingkeeper.Keeper
	SlashingKeeper  slashingkeeper.Keeper
	GovKeeper       *govkeeper.Keeper
	EpochsKeeper    epochskeeper.Keeper
	ConsensusKeeper consensuskeeper.Keeper
	PoleKeeper      polekeeper.Keeper

	ModuleManager      *module.Manager
	BasicModuleManager module.BasicManager
	configurator       module.Configurator
}

func New(logger log.Logger, db dbm.DB, baseAppOptions ...func(*baseapp.BaseApp)) (*App, error) {
	interfaceRegistry := codectypes.NewInterfaceRegistry()
	std.RegisterInterfaces(interfaceRegistry)

	appCodec := codec.NewProtoCodec(interfaceRegistry)
	txConfig := authtx.NewTxConfig(appCodec, authtx.DefaultSignModes)
	keys := storetypes.NewKVStoreKeys(
		authtypes.StoreKey,
		banktypes.StoreKey,
		stakingtypes.StoreKey,
		slashingtypes.StoreKey,
		govtypes.StoreKey,
		epochstypes.StoreKey,
		consensustypes.StoreKey,
		poletypes.StoreKey,
	)
	legacyAmino := codec.NewLegacyAmino()
	std.RegisterLegacyAminoCodec(legacyAmino)
	accAddrCodec := addresscodec.NewBech32Codec("cosmos")
	valAddrCodec := addresscodec.NewBech32Codec("cosmosvaloper")
	consAddrCodec := addresscodec.NewBech32Codec("cosmosvalcons")
	authority, err := accAddrCodec.BytesToString(authtypes.NewModuleAddress(govtypes.ModuleName))
	if err != nil {
		return nil, fmt.Errorf("encode gov authority address: %w", err)
	}

	bApp := baseapp.NewBaseApp(AppName, logger, db, txConfig.TxDecoder(), append(baseAppOptions, baseapp.SetChainID(AppName))...)
	bApp.SetInterfaceRegistry(interfaceRegistry)
	bApp.SetTxEncoder(txConfig.TxEncoder())
	bApp.MsgServiceRouter().SetInterfaceRegistry(interfaceRegistry)
	bApp.MountKVStores(keys)

	accountKeeper := authkeeper.NewAccountKeeper(
		appCodec,
		runtime.NewKVStoreService(keys[authtypes.StoreKey]),
		authtypes.ProtoBaseAccount,
		moduleAccountPermissions,
		accAddrCodec,
		"cosmos",
		authority,
	)

	blockedAddrs := map[string]bool{}
	for _, permission := range accountKeeper.GetModulePermissions() {
		blockedAddrs[permission.GetAddress().String()] = true
	}
	bankKeeper := bankkeeper.NewBaseKeeper(
		appCodec,
		runtime.NewKVStoreService(keys[banktypes.StoreKey]),
		accountKeeper,
		blockedAddrs,
		authority,
		logger,
	)
	stakingKeeper := stakingkeeper.NewKeeper(
		appCodec,
		runtime.NewKVStoreService(keys[stakingtypes.StoreKey]),
		accountKeeper,
		bankKeeper,
		authority,
		valAddrCodec,
		consAddrCodec,
	)
	slashingKeeper := slashingkeeper.NewKeeper(
		appCodec,
		legacyAmino,
		runtime.NewKVStoreService(keys[slashingtypes.StoreKey]),
		*stakingKeeper,
		authority,
	)
	stakingKeeper.SetHooks(slashingKeeper.Hooks())
	epochsKeeper := epochskeeper.NewKeeper(runtime.NewKVStoreService(keys[epochstypes.StoreKey]), appCodec)
	consensusKeeper := consensuskeeper.NewKeeper(
		appCodec,
		runtime.NewKVStoreService(keys[consensustypes.StoreKey]),
		authority,
		runtime.EventService{},
	)
	bApp.SetParamStore(consensusKeeper.ParamsStore)
	govKeeper := govkeeper.NewKeeper(
		appCodec,
		runtime.NewKVStoreService(keys[govtypes.StoreKey]),
		accountKeeper,
		bankKeeper,
		noopDistributionKeeper{},
		bApp.MsgServiceRouter(),
		govtypes.DefaultConfig(),
		authority,
		govkeeper.NewDefaultCalculateVoteResultsAndVotingPower(stakingKeeper),
	)
	poleKeeper, err := polekeeper.NewKeeper(runtime.NewKVStoreService(keys[poletypes.StoreKey]), authority)
	if err != nil {
		return nil, err
	}
	poleKeeper = poleKeeper.WithBankKeeper(bankKeeper)
	poleKeeper = poleKeeper.WithStakeSlashKeepers(stakingKeeper, slashingKeeper)

	authAppModule := authmodule.NewAppModule(appCodec, accountKeeper, authsimulation.RandomGenesisAccounts, nil)
	bankAppModule := bankmodule.NewAppModule(appCodec, bankKeeper, accountKeeper, nil)
	stakingAppModule := stakingmodule.NewAppModule(appCodec, stakingKeeper, accountKeeper, bankKeeper, nil)
	slashingAppModule := slashingmodule.NewAppModule(appCodec, slashingKeeper, accountKeeper, bankKeeper, *stakingKeeper, nil, interfaceRegistry)
	govAppModule := govmodule.NewAppModule(appCodec, govKeeper, accountKeeper, bankKeeper, nil)
	epochsAppModule := epochsmodule.NewAppModule(&epochsKeeper)
	consensusAppModule := consensusmodule.NewAppModule(appCodec, consensusKeeper)
	poleAppModule := polemodule.NewAppModule(poleKeeper)

	moduleManager := module.NewManager(
		authAppModule,
		bankAppModule,
		stakingAppModule,
		slashingAppModule,
		govAppModule,
		epochsAppModule,
		consensusAppModule,
		poleAppModule,
	)
	moduleManager.SetOrderInitGenesis(
		authtypes.ModuleName,
		banktypes.ModuleName,
		stakingtypes.ModuleName,
		slashingtypes.ModuleName,
		govtypes.ModuleName,
		epochstypes.ModuleName,
		consensustypes.ModuleName,
		poletypes.ModuleName,
	)
	moduleManager.SetOrderExportGenesis(
		authtypes.ModuleName,
		banktypes.ModuleName,
		stakingtypes.ModuleName,
		slashingtypes.ModuleName,
		govtypes.ModuleName,
		epochstypes.ModuleName,
		consensustypes.ModuleName,
		poletypes.ModuleName,
	)
	moduleManager.SetOrderBeginBlockers(
		slashingtypes.ModuleName,
		epochstypes.ModuleName,
		stakingtypes.ModuleName,
		poletypes.ModuleName,
	)
	moduleManager.SetOrderEndBlockers(
		govtypes.ModuleName,
		banktypes.ModuleName,
		stakingtypes.ModuleName,
		slashingtypes.ModuleName,
		poletypes.ModuleName,
	)

	basicModuleManager := module.NewBasicManager(
		authAppModule,
		bankAppModule,
		stakingAppModule,
		slashingAppModule,
		govAppModule,
		epochsAppModule,
		consensusAppModule,
		poleAppModule,
	)
	basicModuleManager.RegisterInterfaces(interfaceRegistry)

	configurator := module.NewConfigurator(appCodec, bApp.MsgServiceRouter(), bApp.GRPCQueryRouter())
	if err := moduleManager.RegisterServices(configurator); err != nil {
		return nil, err
	}

	app := &App{
		BaseApp:            bApp,
		appCodec:           appCodec,
		interfaceRegistry:  interfaceRegistry,
		keys:               keys,
		txConfig:           txConfig,
		AccountKeeper:      accountKeeper,
		BankKeeper:         bankKeeper,
		StakingKeeper:      stakingKeeper,
		SlashingKeeper:     slashingKeeper,
		GovKeeper:          govKeeper,
		EpochsKeeper:       epochsKeeper,
		ConsensusKeeper:    consensusKeeper,
		PoleKeeper:         poleKeeper,
		ModuleManager:      moduleManager,
		BasicModuleManager: basicModuleManager,
		configurator:       configurator,
	}

	bApp.SetInitChainer(app.InitChainer)
	bApp.SetBeginBlocker(app.BeginBlocker)
	bApp.SetEndBlocker(app.EndBlocker)

	if err := bApp.LoadLatestVersion(); err != nil {
		return nil, fmt.Errorf("load latest version: %w", err)
	}

	return app, nil
}

func NewMem(logger log.Logger) (*App, error) {
	return New(logger, dbm.NewMemDB())
}

func (a *App) DefaultGenesis() map[string]json.RawMessage {
	return a.BasicModuleManager.DefaultGenesis(a.appCodec)
}

func (a *App) ValidateGenesisState(genesisState map[string]json.RawMessage) error {
	txConfig := authtx.NewTxConfig(a.appCodec, authtx.DefaultSignModes)
	return a.BasicModuleManager.ValidateGenesis(a.appCodec, txConfig, genesisState)
}

func (a *App) InitFromGenesisJSON(genesisJSON []byte) error {
	_, err := a.InitChain(&cmtabci.RequestInitChain{
		ChainId:       AppName,
		InitialHeight: 1,
		AppStateBytes: genesisJSON,
	})
	if err != nil {
		return err
	}
	_, err = a.Commit()
	return err
}

func (a *App) ExecuteLocalMsg(msg sdk.Msg) error {
	_, err := a.BroadcastLocalMsgs(msg)
	return err
}

func (a *App) BroadcastLocalMsgs(msgs ...sdk.Msg) (string, error) {
	builder := a.txConfig.NewTxBuilder()
	if err := builder.SetMsgs(msgs...); err != nil {
		return "", err
	}
	txBytes, err := a.txConfig.TxEncoder()(builder.GetTx())
	if err != nil {
		return "", err
	}
	hash := sha256.Sum256(txBytes)
	ctx := a.NewNextBlockContext(cmtproto.Header{
		ChainID: AppName,
		Height:  a.LastBlockHeight() + 1,
		Time:    time.Now(),
	})
	for _, msg := range msgs {
		handler := a.MsgServiceRouter().Handler(msg)
		if handler == nil {
			return "", fmt.Errorf("no handler registered for %s", sdk.MsgTypeURL(msg))
		}
		if _, err := handler(ctx, msg); err != nil {
			return "", err
		}
	}
	_, err = a.Commit()
	if err != nil {
		return "", err
	}
	return hex.EncodeToString(hash[:]), nil
}

func (a *App) ExecuteLocalWrite(write func(ctx sdk.Context) error) error {
	ctx := a.NewNextBlockContext(cmtproto.Header{
		ChainID: AppName,
		Height:  a.LastBlockHeight() + 1,
		Time:    time.Now(),
	})
	if err := write(ctx); err != nil {
		return err
	}
	_, err := a.Commit()
	return err
}

func (a *App) InitChainer(ctx sdk.Context, req *cmtabci.RequestInitChain) (*cmtabci.ResponseInitChain, error) {
	genesisState := a.DefaultGenesis()
	if len(req.AppStateBytes) > 0 {
		if err := json.Unmarshal(req.AppStateBytes, &genesisState); err != nil {
			return nil, fmt.Errorf("unmarshal app state: %w", err)
		}
	}

	for _, moduleName := range a.ModuleManager.OrderInitGenesis {
		mod := a.ModuleManager.Modules[moduleName]
		state := genesisState[moduleName]
		legacyModule, ok := mod.(module.HasGenesis)
		if !ok {
			continue
		}
		if len(state) == 0 {
			state = legacyModule.DefaultGenesis(a.appCodec)
		}
		legacyModule.InitGenesis(ctx, a.appCodec, state)

		if moduleName == authtypes.ModuleName {
			a.ensureModuleAccounts(ctx)
		}
	}

	return &cmtabci.ResponseInitChain{}, nil
}

func (a *App) BeginBlocker(ctx sdk.Context) (sdk.BeginBlock, error) {
	return a.ModuleManager.BeginBlock(ctx)
}

func (a *App) EndBlocker(ctx sdk.Context) (sdk.EndBlock, error) {
	return a.ModuleManager.EndBlock(ctx)
}

func (a *App) ensureModuleAccounts(ctx sdk.Context) {
	for name, perms := range moduleAccountPermissions {
		if a.AccountKeeper.GetModuleAccount(ctx, name) != nil {
			continue
		}
		a.AccountKeeper.SetModuleAccount(ctx, authtypes.NewEmptyModuleAccount(name, perms...))
	}
}

func (a *App) AppCodec() codec.Codec {
	return a.appCodec
}

func (a *App) InterfaceRegistry() codectypes.InterfaceRegistry {
	return a.interfaceRegistry
}

func (a *App) KVStoreKeys() map[string]*storetypes.KVStoreKey {
	return a.keys
}
