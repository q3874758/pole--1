package types

import "cosmossdk.io/collections"

const (
	ModuleName = "pole"
	StoreKey   = ModuleName
)

var (
	ParamsKeyPrefix            = collections.NewPrefix(0)
	BatchCommitsKeyPrefix      = collections.NewPrefix(1)
	EpochCommitsKeyPrefix      = collections.NewPrefix(2)
	RewardRecordsKeyPrefix     = collections.NewPrefix(3)
	AggregateRecordsKeyPrefix  = collections.NewPrefix(4)
	ChallengesKeyPrefix        = collections.NewPrefix(5)
	AvailabilityKeyPrefix      = collections.NewPrefix(6)
	GameWeightEntriesKeyPrefix = collections.NewPrefix(7)
	ClaimedRewardsKeyPrefix    = collections.NewPrefix(8)
	ReplicaReceiptsKeyPrefix   = collections.NewPrefix(9)
	NodesKeyPrefix             = collections.NewPrefix(10)
)
