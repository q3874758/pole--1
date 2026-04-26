package keeper

import (
	"context"
	"errors"
	"fmt"

	"cosmossdk.io/collections"
	"cosmossdk.io/core/store"
	sdkmath "cosmossdk.io/math"

	sdkcodec "github.com/cosmos/cosmos-sdk/codec"
	codectypes "github.com/cosmos/cosmos-sdk/codec/types"
	sdk "github.com/cosmos/cosmos-sdk/types"
	stakingtypes "github.com/cosmos/cosmos-sdk/x/staking/types"

	"pole/chain/x/pole/types"
)

type Keeper struct {
	storeService   store.KVStoreService
	authority      string
	bankKeeper     bankKeeper
	stakingKeeper  stakingKeeper
	slashingKeeper slashingKeeper

	Schema            collections.Schema
	Params            collections.Item[types.Params]
	Nodes             collections.Map[string, types.NodeRecord]
	BatchCommits      collections.Map[collections.Triple[uint64, string, string], types.BatchCommit]
	EpochCommits      collections.Map[uint64, types.EpochCommit]
	RewardRecords     collections.Map[collections.Pair[uint64, string], types.RewardRecord]
	AggregateRecords  collections.Map[collections.Pair[uint64, uint64], types.AggregateRecord]
	Challenges        collections.Map[string, types.Challenge]
	Availability      collections.Map[collections.Triple[uint64, string, string], types.AvailabilityRecord]
	GameWeightEntries collections.Map[collections.Pair[uint64, uint64], types.GameWeightEntry]
	ClaimedRewards    collections.Map[collections.Pair[uint64, string], types.ClaimedReward]
	ReplicaReceipts   collections.Map[collections.Triple[uint64, string, string], types.ReplicaReceipt]
}

type bankKeeper interface {
	MintCoins(ctx context.Context, moduleName string, amounts sdk.Coins) error
	SendCoinsFromModuleToAccount(ctx context.Context, senderModule string, recipientAddr sdk.AccAddress, amt sdk.Coins) error
}

type stakingKeeper interface {
	GetValidatorByConsAddr(ctx context.Context, consAddr sdk.ConsAddress) (stakingtypes.Validator, error)
}

type slashingKeeper interface {
	Slash(ctx context.Context, consAddr sdk.ConsAddress, fraction sdkmath.LegacyDec, power, distributionHeight int64) error
	Jail(ctx context.Context, consAddr sdk.ConsAddress) error
}

func NewKeeper(storeService store.KVStoreService, authority string) (Keeper, error) {
	if authority == "" {
		return Keeper{}, fmt.Errorf("authority must not be empty")
	}
	if _, err := sdk.AccAddressFromBech32(authority); err != nil {
		return Keeper{}, fmt.Errorf("invalid authority address: %w", err)
	}

	sb := collections.NewSchemaBuilder(storeService)
	protoCodec := sdkcodec.NewProtoCodec(codectypes.NewInterfaceRegistry())
	k := Keeper{
		storeService: storeService,
		authority:    authority,
		Params: collections.NewItem(
			sb,
			types.ParamsKeyPrefix,
			"params",
			sdkcodec.CollValue[types.Params](protoCodec),
		),
		Nodes: collections.NewMap(
			sb,
			types.NodesKeyPrefix,
			"nodes",
			collections.StringKey,
			sdkcodec.CollValue[types.NodeRecord](protoCodec),
		),
		BatchCommits: collections.NewMap(
			sb,
			types.BatchCommitsKeyPrefix,
			"batch_commits",
			collections.NamedTripleKeyCodec(
				"epoch_id",
				collections.Uint64Key,
				"collector_address",
				collections.StringKey,
				"batch_root_hex",
				collections.StringKey,
			),
			sdkcodec.CollValue[types.BatchCommit](protoCodec),
		),
		EpochCommits: collections.NewMap(
			sb,
			types.EpochCommitsKeyPrefix,
			"epoch_commits",
			collections.Uint64Key,
			sdkcodec.CollValue[types.EpochCommit](protoCodec),
		),
		RewardRecords: collections.NewMap(
			sb,
			types.RewardRecordsKeyPrefix,
			"reward_records",
			collections.NamedPairKeyCodec(
				"epoch_id",
				collections.Uint64Key,
				"recipient",
				collections.StringKey,
			),
			sdkcodec.CollValue[types.RewardRecord](protoCodec),
		),
		AggregateRecords: collections.NewMap(
			sb,
			types.AggregateRecordsKeyPrefix,
			"aggregate_records",
			collections.NamedPairKeyCodec(
				"epoch_id",
				collections.Uint64Key,
				"app_id",
				collections.Uint64Key,
			),
			sdkcodec.CollValue[types.AggregateRecord](protoCodec),
		),
		Challenges: collections.NewMap(
			sb,
			types.ChallengesKeyPrefix,
			"challenges",
			collections.StringKey,
			sdkcodec.CollValue[types.Challenge](protoCodec),
		),
		Availability: collections.NewMap(
			sb,
			types.AvailabilityKeyPrefix,
			"availability",
			collections.NamedTripleKeyCodec(
				"epoch_id",
				collections.Uint64Key,
				"operator_address",
				collections.StringKey,
				"payload_cid",
				collections.StringKey,
			),
			sdkcodec.CollValue[types.AvailabilityRecord](protoCodec),
		),
		GameWeightEntries: collections.NewMap(
			sb,
			types.GameWeightEntriesKeyPrefix,
			"game_weight_entries",
			collections.NamedPairKeyCodec(
				"app_id",
				collections.Uint64Key,
				"effective_from_epoch_id",
				collections.Uint64Key,
			),
			sdkcodec.CollValue[types.GameWeightEntry](protoCodec),
		),
		ClaimedRewards: collections.NewMap(
			sb,
			types.ClaimedRewardsKeyPrefix,
			"claimed_rewards",
			collections.NamedPairKeyCodec(
				"epoch_id",
				collections.Uint64Key,
				"recipient",
				collections.StringKey,
			),
			sdkcodec.CollValue[types.ClaimedReward](protoCodec),
		),
		ReplicaReceipts: collections.NewMap(
			sb,
			types.ReplicaReceiptsKeyPrefix,
			"replica_receipts",
			collections.NamedTripleKeyCodec(
				"epoch_id",
				collections.Uint64Key,
				"storer_address",
				collections.StringKey,
				"payload_cid",
				collections.StringKey,
			),
			sdkcodec.CollValue[types.ReplicaReceipt](protoCodec),
		),
	}

	schema, err := sb.Build()
	if err != nil {
		return Keeper{}, fmt.Errorf("build pole schema: %w", err)
	}
	k.Schema = schema

	return k, nil
}

func (k Keeper) GetAuthority() string {
	return k.authority
}

func (k Keeper) WithBankKeeper(bank bankKeeper) Keeper {
	k.bankKeeper = bank
	return k
}

func (k Keeper) WithStakeSlashKeepers(staking stakingKeeper, slashing slashingKeeper) Keeper {
	k.stakingKeeper = staking
	k.slashingKeeper = slashing
	return k
}

func (k Keeper) ValidateGenesis(genesis *types.GenesisState) error {
	return genesis.Validate()
}

func (k Keeper) GetParams(ctx context.Context) (types.Params, error) {
	params, err := k.Params.Get(ctx)
	if errors.Is(err, collections.ErrNotFound) {
		return types.DefaultParams(), nil
	}
	return params, err
}

func (k Keeper) SetParams(ctx context.Context, params types.Params) error {
	if err := params.Validate(); err != nil {
		return err
	}
	return k.Params.Set(ctx, params)
}

func (k Keeper) SetNode(ctx context.Context, node types.NodeRecord) error {
	return k.Nodes.Set(ctx, node.OperatorAddress, node)
}

func (k Keeper) GetNode(ctx context.Context, operatorAddress string) (types.NodeRecord, error) {
	return k.Nodes.Get(ctx, operatorAddress)
}

func (k Keeper) SetBatchCommit(ctx context.Context, batch types.BatchCommit) error {
	return k.BatchCommits.Set(ctx, batchCommitKey(batch), batch)
}

func (k Keeper) GetBatchCommit(ctx context.Context, epochID uint64, collectorAddress, batchRootHex string) (types.BatchCommit, error) {
	return k.BatchCommits.Get(ctx, collections.Join3(epochID, collectorAddress, batchRootHex))
}

func (k Keeper) SetEpochCommit(ctx context.Context, commit types.EpochCommit) error {
	return k.EpochCommits.Set(ctx, commit.EpochId, commit)
}

func (k Keeper) GetEpochCommit(ctx context.Context, epochID uint64) (types.EpochCommit, error) {
	return k.EpochCommits.Get(ctx, epochID)
}

func (k Keeper) SetRewardRecord(ctx context.Context, record types.RewardRecord) error {
	return k.RewardRecords.Set(ctx, rewardRecordKey(record), record)
}

func (k Keeper) GetRewardRecord(ctx context.Context, epochID uint64, recipient string) (types.RewardRecord, error) {
	return k.RewardRecords.Get(ctx, collections.Join(epochID, recipient))
}

func (k Keeper) SetAggregateRecord(ctx context.Context, record types.AggregateRecord) error {
	return k.AggregateRecords.Set(ctx, aggregateRecordKey(record), record)
}

func (k Keeper) GetAggregateRecord(ctx context.Context, epochID uint64, appID uint32) (types.AggregateRecord, error) {
	return k.AggregateRecords.Get(ctx, collections.Join(epochID, uint64(appID)))
}

func (k Keeper) SetChallenge(ctx context.Context, challenge types.Challenge) error {
	return k.Challenges.Set(ctx, challenge.ChallengeIdHex, challenge)
}

func (k Keeper) GetChallenge(ctx context.Context, challengeIDHex string) (types.Challenge, error) {
	return k.Challenges.Get(ctx, challengeIDHex)
}

func (k Keeper) SetAvailabilityRecord(ctx context.Context, record types.AvailabilityRecord) error {
	return k.Availability.Set(ctx, availabilityKey(record), record)
}

func (k Keeper) SetGameWeightEntry(ctx context.Context, entry types.GameWeightEntry) error {
	return k.GameWeightEntries.Set(ctx, gameWeightKey(entry), entry)
}

func (k Keeper) GetGameWeightEntry(ctx context.Context, appId uint32, effectiveFromEpochId uint64) (types.GameWeightEntry, error) {
	return k.GameWeightEntries.Get(ctx, collections.Join(uint64(appId), effectiveFromEpochId))
}

func (k Keeper) SetClaimedReward(ctx context.Context, claim types.ClaimedReward) error {
	return k.ClaimedRewards.Set(ctx, claimedRewardKey(claim), claim)
}

func (k Keeper) GetClaimedReward(ctx context.Context, epochId uint64, recipient string) (types.ClaimedReward, error) {
	return k.ClaimedRewards.Get(ctx, collections.Join(epochId, recipient))
}

func (k Keeper) HasClaimedReward(ctx context.Context, epochId uint64, recipient string) (bool, error) {
	return k.ClaimedRewards.Has(ctx, collections.Join(epochId, recipient))
}

func (k Keeper) SetReplicaReceipt(ctx context.Context, receipt types.ReplicaReceipt) error {
	return k.ReplicaReceipts.Set(ctx, replicaReceiptKey(receipt), receipt)
}

func (k Keeper) GetReplicaReceipt(ctx context.Context, epochId uint64, storerAddress, payloadCid string) (types.ReplicaReceipt, error) {
	return k.ReplicaReceipts.Get(ctx, collections.Join3(epochId, storerAddress, payloadCid))
}

func (k Keeper) InitGenesis(ctx context.Context, genesis *types.GenesisState) error {
	if genesis == nil {
		genesis = types.DefaultGenesis()
	}
	if err := genesis.Validate(); err != nil {
		return err
	}

	if genesis.Params == nil {
		defaultParams := types.DefaultParams()
		genesis.Params = &defaultParams
	}
	if err := k.SetParams(ctx, *genesis.Params); err != nil {
		return err
	}
	for _, node := range genesis.Nodes {
		if node == nil {
			continue
		}
		if err := k.SetNode(ctx, *node); err != nil {
			return err
		}
	}
	for _, batch := range genesis.BatchCommits {
		if batch == nil {
			continue
		}
		if err := k.SetBatchCommit(ctx, *batch); err != nil {
			return err
		}
	}
	for _, commit := range genesis.EpochCommits {
		if commit == nil {
			continue
		}
		if err := k.SetEpochCommit(ctx, *commit); err != nil {
			return err
		}
	}
	for _, record := range genesis.RewardRecords {
		if record == nil {
			continue
		}
		if err := k.SetRewardRecord(ctx, *record); err != nil {
			return err
		}
	}
	for _, record := range genesis.AggregateRecords {
		if record == nil {
			continue
		}
		if err := k.SetAggregateRecord(ctx, *record); err != nil {
			return err
		}
	}
	for _, challenge := range genesis.Challenges {
		if challenge == nil {
			continue
		}
		if err := k.SetChallenge(ctx, *challenge); err != nil {
			return err
		}
	}
	for _, record := range genesis.Availability {
		if record == nil {
			continue
		}
		if err := k.SetAvailabilityRecord(ctx, *record); err != nil {
			return err
		}
	}
	for _, entry := range genesis.GameWeightEntries {
		if entry == nil {
			continue
		}
		if err := k.SetGameWeightEntry(ctx, *entry); err != nil {
			return err
		}
	}
	for _, claim := range genesis.ClaimedRewards {
		if claim == nil {
			continue
		}
		if err := k.SetClaimedReward(ctx, *claim); err != nil {
			return err
		}
	}
	for _, receipt := range genesis.ReplicaReceipts {
		if receipt == nil {
			continue
		}
		if err := k.SetReplicaReceipt(ctx, *receipt); err != nil {
			return err
		}
	}

	return nil
}

func (k Keeper) ExportGenesis(ctx context.Context) (*types.GenesisState, error) {
	params, err := k.GetParams(ctx)
	if err != nil {
		return nil, err
	}
	nodes, err := valuesFromMap(k.Nodes, ctx)
	if err != nil {
		return nil, err
	}

	batchCommits, err := valuesFromMap(k.BatchCommits, ctx)
	if err != nil {
		return nil, err
	}
	epochCommits, err := valuesFromMap(k.EpochCommits, ctx)
	if err != nil {
		return nil, err
	}
	rewardRecords, err := valuesFromMap(k.RewardRecords, ctx)
	if err != nil {
		return nil, err
	}
	aggregateRecords, err := valuesFromMap(k.AggregateRecords, ctx)
	if err != nil {
		return nil, err
	}
	challenges, err := valuesFromMap(k.Challenges, ctx)
	if err != nil {
		return nil, err
	}
	availability, err := valuesFromMap(k.Availability, ctx)
	if err != nil {
		return nil, err
	}
	gameWeightEntries, err := valuesFromMap(k.GameWeightEntries, ctx)
	if err != nil {
		return nil, err
	}
	claimedRewards, err := valuesFromMap(k.ClaimedRewards, ctx)
	if err != nil {
		return nil, err
	}
	replicaReceipts, err := valuesFromMap(k.ReplicaReceipts, ctx)
	if err != nil {
		return nil, err
	}

	return &types.GenesisState{
		Params:            &params,
		Nodes:             pointerSlice(nodes),
		BatchCommits:      pointerSlice(batchCommits),
		EpochCommits:      pointerSlice(epochCommits),
		RewardRecords:     pointerSlice(rewardRecords),
		AggregateRecords:  pointerSlice(aggregateRecords),
		Challenges:        pointerSlice(challenges),
		Availability:      pointerSlice(availability),
		GameWeightEntries: pointerSlice(gameWeightEntries),
		ClaimedRewards:    pointerSlice(claimedRewards),
		ReplicaReceipts:   pointerSlice(replicaReceipts),
	}, nil
}

func (k Keeper) ComputePlayerReward(hourlyRewardPool uint64, playerWeight uint64, totalWeight uint64) uint64 {
	return types.ComputePlayerHourReward(hourlyRewardPool, playerWeight, totalWeight)
}

func (k Keeper) ComputeAdjustedHourlyReward(baseReward uint64, targetWeight uint64, previousWeight uint64, capBps uint32) uint64 {
	return types.AdjustedHourlyReward(baseReward, targetWeight, previousWeight, capBps)
}

func (k Keeper) HasOpenChallengesForEpoch(ctx context.Context, epochId uint64) (bool, error) {
	iter, err := k.Challenges.Iterate(ctx, nil)
	if err != nil {
		return false, err
	}
	defer iter.Close()

	for ; iter.Valid(); iter.Next() {
		kv, err := iter.KeyValue()
		if err != nil {
			return false, err
		}
		if kv.Value.EpochId == epochId && kv.Value.State == types.ChallengeStateOpen {
			return true, nil
		}
	}

	return false, nil
}

func (k Keeper) FinalizeEpoch(ctx context.Context, epochId uint64) error {
	commit, err := k.GetEpochCommit(ctx, epochId)
	if err != nil {
		return err
	}
	if commit.Finalized {
		return nil
	}

	currentHeight := sdk.UnwrapSDKContext(ctx).BlockHeight()
	if currentHeight <= commit.ChallengeDeadlineHeight {
		return fmt.Errorf("epoch %d challenge window still open", epochId)
	}
	hasOpenChallenges, err := k.HasOpenChallengesForEpoch(ctx, epochId)
	if err != nil {
		return err
	}
	if hasOpenChallenges {
		return fmt.Errorf("epoch %d still has open challenges", epochId)
	}
	if err := k.ValidateEpochRoots(ctx, epochId, commit); err != nil {
		return err
	}

	commit.Finalized = true
	return k.SetEpochCommit(ctx, commit)
}

func (k Keeper) ValidateEpochRoots(ctx context.Context, epochId uint64, commit types.EpochCommit) error {
	rewardRecords, err := k.rewardRecordsForEpoch(ctx, epochId)
	if err != nil {
		return err
	}
	aggregateRecords, err := k.aggregateRecordsForEpoch(ctx, epochId)
	if err != nil {
		return err
	}

	rewardRoot, rewardLeafCount, err := types.MerkleRootHexForRecords(rewardRecords)
	if err != nil {
		return err
	}
	aggregateRoot, aggregateLeafCount, err := types.MerkleRootHexForRecords(aggregateRecords)
	if err != nil {
		return err
	}

	if commit.Rewards == nil {
		return fmt.Errorf("epoch %d missing rewards commitment", epochId)
	}
	if commit.Rewards.Root != rewardRoot || commit.Rewards.LeafCount != rewardLeafCount {
		return fmt.Errorf("epoch %d reward root mismatch", epochId)
	}
	if commit.Aggregates == nil {
		return fmt.Errorf("epoch %d missing aggregates commitment", epochId)
	}
	if commit.Aggregates.Root != aggregateRoot || commit.Aggregates.LeafCount != aggregateLeafCount {
		return fmt.Errorf("epoch %d aggregate root mismatch", epochId)
	}

	var totalNetworkWeightUnits uint64
	for _, record := range aggregateRecords {
		totalNetworkWeightUnits += record.TotalWeightUnits
	}
	if commit.TotalNetworkWeightUnits != totalNetworkWeightUnits {
		return fmt.Errorf("epoch %d total_network_weight_units mismatch", epochId)
	}

	return nil
}

func (k Keeper) ApplyValidatorSlash(ctx context.Context, consAddress string, slashFractionBps uint32, jail bool) error {
	if consAddress == "" {
		return nil
	}
	if k.stakingKeeper == nil || k.slashingKeeper == nil {
		return fmt.Errorf("staking/slashing keepers are not configured")
	}
	if slashFractionBps > 10_000 {
		return fmt.Errorf("slash_fraction_bps must be <= 10000")
	}

	consAddr, err := sdk.ConsAddressFromBech32(consAddress)
	if err != nil {
		return err
	}
	validator, err := k.stakingKeeper.GetValidatorByConsAddr(ctx, consAddr)
	if err != nil {
		return err
	}
	if jail {
		if err := k.slashingKeeper.Jail(ctx, consAddr); err != nil {
			return err
		}
	}
	if slashFractionBps == 0 {
		return nil
	}
	fraction := sdkmath.LegacyNewDec(int64(slashFractionBps)).Quo(sdkmath.LegacyNewDec(10_000))
	power := validator.GetConsensusPower(sdk.DefaultPowerReduction)
	distributionHeight := sdk.UnwrapSDKContext(ctx).BlockHeight()
	return k.slashingKeeper.Slash(ctx, consAddr, fraction, power, distributionHeight)
}

func (k Keeper) PayoutClaimedReward(ctx context.Context, claim types.ClaimedReward) error {
	if k.bankKeeper == nil {
		return fmt.Errorf("bank keeper is not configured")
	}
	if claim.Amount == 0 {
		return fmt.Errorf("claimed reward amount must be greater than 0")
	}
	recipient, err := sdk.AccAddressFromBech32(claim.Recipient)
	if err != nil {
		return err
	}
	coins := sdk.NewCoins(sdk.NewCoin(types.BaseDenom, sdkmath.NewIntFromUint64(claim.Amount)))
	if err := k.bankKeeper.MintCoins(ctx, types.ModuleName, coins); err != nil {
		return err
	}
	return k.bankKeeper.SendCoinsFromModuleToAccount(ctx, types.ModuleName, recipient, coins)
}

func (k Keeper) rewardRecordsForEpoch(ctx context.Context, epochId uint64) ([]types.RewardRecord, error) {
	iter, err := k.RewardRecords.Iterate(ctx, nil)
	if err != nil {
		return nil, err
	}
	defer iter.Close()

	var records []types.RewardRecord
	for ; iter.Valid(); iter.Next() {
		kv, err := iter.KeyValue()
		if err != nil {
			return nil, err
		}
		if kv.Value.EpochId == epochId {
			records = append(records, kv.Value)
		}
	}
	return records, nil
}

func (k Keeper) RewardRecordsForEpoch(ctx context.Context, epochId uint64) ([]types.RewardRecord, error) {
	return k.rewardRecordsForEpoch(ctx, epochId)
}

func (k Keeper) batchCommitsForEpoch(ctx context.Context, epochId uint64) ([]types.BatchCommit, error) {
	iter, err := k.BatchCommits.Iterate(ctx, nil)
	if err != nil {
		return nil, err
	}
	defer iter.Close()

	var records []types.BatchCommit
	for ; iter.Valid(); iter.Next() {
		kv, err := iter.KeyValue()
		if err != nil {
			return nil, err
		}
		if kv.Value.EpochId == epochId {
			records = append(records, kv.Value)
		}
	}
	return records, nil
}

func (k Keeper) availabilityRecordsForEpoch(ctx context.Context, epochId uint64) ([]types.AvailabilityRecord, error) {
	iter, err := k.Availability.Iterate(ctx, nil)
	if err != nil {
		return nil, err
	}
	defer iter.Close()

	var records []types.AvailabilityRecord
	for ; iter.Valid(); iter.Next() {
		kv, err := iter.KeyValue()
		if err != nil {
			return nil, err
		}
		if kv.Value.EpochId == epochId {
			records = append(records, kv.Value)
		}
	}
	return records, nil
}

func (k Keeper) aggregateRecordsForEpoch(ctx context.Context, epochId uint64) ([]types.AggregateRecord, error) {
	iter, err := k.AggregateRecords.Iterate(ctx, nil)
	if err != nil {
		return nil, err
	}
	defer iter.Close()

	var records []types.AggregateRecord
	for ; iter.Valid(); iter.Next() {
		kv, err := iter.KeyValue()
		if err != nil {
			return nil, err
		}
		if kv.Value.EpochId == epochId {
			records = append(records, kv.Value)
		}
	}
	return records, nil
}

func (k Keeper) AggregateRecordsForEpoch(ctx context.Context, epochId uint64) ([]types.AggregateRecord, error) {
	return k.aggregateRecordsForEpoch(ctx, epochId)
}

func (k Keeper) ComputeEpochCommitments(ctx context.Context, epochId uint64) (rewardRoot string, rewardLeaves uint32, aggregateRoot string, aggregateLeaves uint32, totalWeight uint64, err error) {
	rewardRecords, err := k.rewardRecordsForEpoch(ctx, epochId)
	if err != nil {
		return "", 0, "", 0, 0, err
	}
	aggregateRecords, err := k.aggregateRecordsForEpoch(ctx, epochId)
	if err != nil {
		return "", 0, "", 0, 0, err
	}
	rewardRoot, rewardLeaves, err = types.MerkleRootHexForRecords(rewardRecords)
	if err != nil {
		return "", 0, "", 0, 0, err
	}
	aggregateRoot, aggregateLeaves, err = types.MerkleRootHexForRecords(aggregateRecords)
	if err != nil {
		return "", 0, "", 0, 0, err
	}
	for _, record := range aggregateRecords {
		totalWeight += record.TotalWeightUnits
	}
	return rewardRoot, rewardLeaves, aggregateRoot, aggregateLeaves, totalWeight, nil
}

func (k Keeper) RecomputeEpochCommitments(ctx context.Context, epochId uint64) error {
	commit, err := k.GetEpochCommit(ctx, epochId)
	if err != nil {
		return err
	}
	rewardRoot, rewardLeaves, aggregateRoot, aggregateLeaves, totalWeight, err := k.ComputeEpochCommitments(ctx, epochId)
	if err != nil {
		return err
	}
	if commit.Rewards == nil {
		commit.Rewards = &types.MerkleCommitment{}
	}
	commit.Rewards.Root = rewardRoot
	commit.Rewards.LeafCount = rewardLeaves
	if commit.Aggregates == nil {
		commit.Aggregates = &types.MerkleCommitment{}
	}
	commit.Aggregates.Root = aggregateRoot
	commit.Aggregates.LeafCount = aggregateLeaves
	commit.TotalNetworkWeightUnits = totalWeight
	return k.SetEpochCommit(ctx, commit)
}

func (k Keeper) RewardProof(ctx context.Context, epochId uint64, recipient string) (root string, proof []string, record types.RewardRecord, index int, err error) {
	records, err := k.rewardRecordsForEpoch(ctx, epochId)
	if err != nil {
		return "", nil, types.RewardRecord{}, 0, err
	}
	for i, candidate := range records {
		if candidate.Recipient == recipient {
			proof, err = types.MerkleProofHexForRecords(records, i)
			if err != nil {
				return "", nil, types.RewardRecord{}, 0, err
			}
			root, _, err = types.MerkleRootHexForRecords(records)
			return root, proof, candidate, i, err
		}
	}
	return "", nil, types.RewardRecord{}, 0, fmt.Errorf("reward record not found for epoch %d recipient %s", epochId, recipient)
}

func (k Keeper) AggregateProof(ctx context.Context, epochId uint64, appId uint32) (root string, proof []string, record types.AggregateRecord, index int, err error) {
	records, err := k.aggregateRecordsForEpoch(ctx, epochId)
	if err != nil {
		return "", nil, types.AggregateRecord{}, 0, err
	}
	for i, candidate := range records {
		if candidate.AppId == appId {
			proof, err = types.MerkleProofHexForRecords(records, i)
			if err != nil {
				return "", nil, types.AggregateRecord{}, 0, err
			}
			root, _, err = types.MerkleRootHexForRecords(records)
			return root, proof, candidate, i, err
		}
	}
	return "", nil, types.AggregateRecord{}, 0, fmt.Errorf("aggregate record not found for epoch %d app %d", epochId, appId)
}
func batchCommitKey(batch types.BatchCommit) collections.Triple[uint64, string, string] {
	batchRoot := ""
	if batch.Batch != nil {
		batchRoot = batch.Batch.Root
	}
	return collections.Join3(batch.EpochId, batch.CollectorAddress, batchRoot)
}

func rewardRecordKey(record types.RewardRecord) collections.Pair[uint64, string] {
	return collections.Join(record.EpochId, record.Recipient)
}

func aggregateRecordKey(record types.AggregateRecord) collections.Pair[uint64, uint64] {
	return collections.Join(record.EpochId, uint64(record.AppId))
}

func availabilityKey(record types.AvailabilityRecord) collections.Triple[uint64, string, string] {
	return collections.Join3(record.EpochId, record.OperatorAddress, record.PayloadCid)
}

func gameWeightKey(entry types.GameWeightEntry) collections.Pair[uint64, uint64] {
	return collections.Join(uint64(entry.AppId), entry.EffectiveFromEpochId)
}

func claimedRewardKey(claim types.ClaimedReward) collections.Pair[uint64, string] {
	return collections.Join(claim.EpochId, claim.Recipient)
}

func replicaReceiptKey(receipt types.ReplicaReceipt) collections.Triple[uint64, string, string] {
	return collections.Join3(receipt.EpochId, receipt.StorerAddress, receipt.PayloadCid)
}

func valuesFromMap[K, V any](m collections.Map[K, V], ctx context.Context) ([]V, error) {
	iter, err := m.Iterate(ctx, nil)
	if err != nil {
		return nil, err
	}
	return iter.Values()
}

func pointerSlice[T any](values []T) []*T {
	result := make([]*T, 0, len(values))
	for i := range values {
		value := values[i]
		result = append(result, &value)
	}
	return result
}
