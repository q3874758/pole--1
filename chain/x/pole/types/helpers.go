package types

import "fmt"

const BaseDenom = "upole"

const (
	MinServiceBondedTokens     uint64 = 1_000_000
	MinCoordinatorBondedTokens uint64 = 10_000_000
	MinVerifyBondedTokens      uint64 = 2_500_000
	MinProposeBondedTokens     uint64 = 10_000_000
)

const (
	ChallengeKindBadBatch     = ChallengeKind_CHALLENGE_KIND_BAD_BATCH
	ChallengeKindOmission     = ChallengeKind_CHALLENGE_KIND_OMISSION
	ChallengeKindBadAggregate = ChallengeKind_CHALLENGE_KIND_BAD_AGGREGATE
	ChallengeKindBadReward    = ChallengeKind_CHALLENGE_KIND_BAD_REWARD
	ChallengeKindBadStorage   = ChallengeKind_CHALLENGE_KIND_BAD_STORAGE

	ChallengeStateOpen     = ChallengeState_CHALLENGE_STATE_OPEN
	ChallengeStateResolved = ChallengeState_CHALLENGE_STATE_RESOLVED
	ChallengeStateRejected = ChallengeState_CHALLENGE_STATE_REJECTED
)

func DefaultParams() Params {
	return Params{
		RewardBlockDurationSeconds: 3600,
		BaseHourlyReward:           1000,
		TargetNetworkWeightUnits:   150_000_000_000_000,
		RewardAdjustmentCapBps:     2000,
		ChallengeWindowBlocks:      20,
		MinRetentionEpochs:         2,
		PlayerRewardAllocationBps:  8000,
		ServiceRewardAllocationBps: 1000,
		CollectRewardBps:           5000,
		StoreRewardBps:             2500,
		VerifyRewardBps:            1500,
		ProposeRewardBps:           1000,
		Tier1WeightPpm:             1_000_000,
		Tier2WeightMinPpm:          300_000,
		Tier2WeightMaxPpm:          600_000,
		Tier3WeightMinPpm:          50_000,
		Tier3WeightMaxPpm:          150_000,
		FeeBurnBps:                 2500,
		RewardBurnThreshold:        10_000,
		RewardBurnBps:              1000,
		GovernanceBurnBps:          100,
	}
}

func (p Params) Validate() error {
	if p.RewardBlockDurationSeconds == 0 {
		return fmt.Errorf("reward_block_duration_seconds must be greater than 0")
	}
	if p.BaseHourlyReward == 0 {
		return fmt.Errorf("base_hourly_reward must be greater than 0")
	}
	if p.TargetNetworkWeightUnits == 0 {
		return fmt.Errorf("target_network_weight_units must be greater than 0")
	}
	if p.RewardAdjustmentCapBps > 10_000 {
		return fmt.Errorf("reward_adjustment_cap_bps must be <= 10000")
	}
	if p.ChallengeWindowBlocks == 0 {
		return fmt.Errorf("challenge_window_blocks must be greater than 0")
	}
	if p.MinRetentionEpochs == 0 {
		return fmt.Errorf("min_retention_epochs must be greater than 0")
	}

	allocationTotal := p.PlayerRewardAllocationBps + p.ServiceRewardAllocationBps
	if allocationTotal > 10_000 {
		return fmt.Errorf("player and service reward allocations must total <= 10000 bps, got %d", allocationTotal)
	}

	serviceSplitTotal := p.CollectRewardBps + p.StoreRewardBps + p.VerifyRewardBps + p.ProposeRewardBps
	if serviceSplitTotal != 10_000 {
		return fmt.Errorf("service reward split must total 10000 bps, got %d", serviceSplitTotal)
	}

	if err := validateBps("fee_burn_bps", p.FeeBurnBps); err != nil {
		return err
	}
	if err := validateBps("reward_burn_bps", p.RewardBurnBps); err != nil {
		return err
	}
	if err := validateBps("governance_burn_bps", p.GovernanceBurnBps); err != nil {
		return err
	}

	if p.Tier1WeightPpm == 0 {
		return fmt.Errorf("tier1_weight_ppm must be greater than 0")
	}
	if p.Tier2WeightMinPpm == 0 || p.Tier2WeightMaxPpm == 0 || p.Tier2WeightMinPpm > p.Tier2WeightMaxPpm {
		return fmt.Errorf("tier2 weight bounds are invalid")
	}
	if p.Tier3WeightMinPpm == 0 || p.Tier3WeightMaxPpm == 0 || p.Tier3WeightMinPpm > p.Tier3WeightMaxPpm {
		return fmt.Errorf("tier3 weight bounds are invalid")
	}

	return nil
}

func DefaultGenesis() *GenesisState {
	params := DefaultParams()
	return &GenesisState{
		Params:            &params,
		BatchCommits:      []*BatchCommit{},
		EpochCommits:      []*EpochCommit{},
		RewardRecords:     []*RewardRecord{},
		AggregateRecords:  []*AggregateRecord{},
		Challenges:        []*Challenge{},
		Availability:      []*AvailabilityRecord{},
		GameWeightEntries: []*GameWeightEntry{},
		ClaimedRewards:    []*ClaimedReward{},
		ReplicaReceipts:   []*ReplicaReceipt{},
		Nodes:             []*NodeRecord{},
	}
}

func (g *GenesisState) Validate() error {
	if g == nil {
		return fmt.Errorf("genesis state must not be nil")
	}
	if g.Params == nil {
		return fmt.Errorf("genesis params must not be nil")
	}
	if err := g.Params.Validate(); err != nil {
		return err
	}

	seenEpochCommits := map[uint64]struct{}{}
	for _, commit := range g.EpochCommits {
		if commit == nil {
			return fmt.Errorf("epoch commit entry must not be nil")
		}
		if _, exists := seenEpochCommits[commit.EpochId]; exists {
			return fmt.Errorf("duplicate epoch commit for epoch %d", commit.EpochId)
		}
		seenEpochCommits[commit.EpochId] = struct{}{}
		if commit.ChallengeDeadlineHeight <= commit.ChallengeOpenHeight {
			return fmt.Errorf("epoch %d has invalid challenge window", commit.EpochId)
		}
	}

	seenChallenges := map[string]struct{}{}
	for _, challenge := range g.Challenges {
		if challenge == nil {
			return fmt.Errorf("challenge entry must not be nil")
		}
		if challenge.ChallengeIdHex == "" {
			return fmt.Errorf("challenge id must not be empty")
		}
		if _, exists := seenChallenges[challenge.ChallengeIdHex]; exists {
			return fmt.Errorf("duplicate challenge %s", challenge.ChallengeIdHex)
		}
		seenChallenges[challenge.ChallengeIdHex] = struct{}{}
	}

	seenWeights := map[uint32]struct{}{}
	for _, weight := range g.GameWeightEntries {
		if weight == nil {
			return fmt.Errorf("game weight entry must not be nil")
		}
		if weight.GameWeightPpm == 0 {
			return fmt.Errorf("game weight for app %d must be greater than 0", weight.AppId)
		}
		if _, exists := seenWeights[weight.AppId]; exists {
			return fmt.Errorf("duplicate game weight entry for app %d", weight.AppId)
		}
		seenWeights[weight.AppId] = struct{}{}
	}

	seenClaims := map[string]struct{}{}
	for _, claim := range g.ClaimedRewards {
		if claim == nil {
			return fmt.Errorf("claimed reward entry must not be nil")
		}
		key := fmt.Sprintf("%d/%s", claim.EpochId, claim.Recipient)
		if _, exists := seenClaims[key]; exists {
			return fmt.Errorf("duplicate claimed reward entry for %s", key)
		}
		seenClaims[key] = struct{}{}
	}

	seenReceipts := map[string]struct{}{}
	for _, receipt := range g.ReplicaReceipts {
		if receipt == nil {
			return fmt.Errorf("replica receipt entry must not be nil")
		}
		if receipt.PayloadCid == "" {
			return fmt.Errorf("replica receipt payload_cid must not be empty")
		}
		if receipt.StorerAddress == "" {
			return fmt.Errorf("replica receipt storer_address must not be empty")
		}
		key := fmt.Sprintf("%d/%s/%s", receipt.EpochId, receipt.StorerAddress, receipt.PayloadCid)
		if _, exists := seenReceipts[key]; exists {
			return fmt.Errorf("duplicate replica receipt entry for %s", key)
		}
		seenReceipts[key] = struct{}{}
	}

	seenAggregates := map[string]struct{}{}
	for _, aggregate := range g.AggregateRecords {
		if aggregate == nil {
			return fmt.Errorf("aggregate record entry must not be nil")
		}
		key := fmt.Sprintf("%d/%d", aggregate.EpochId, aggregate.AppId)
		if _, exists := seenAggregates[key]; exists {
			return fmt.Errorf("duplicate aggregate record entry for %s", key)
		}
		seenAggregates[key] = struct{}{}
	}

	seenNodes := map[string]struct{}{}
	for _, node := range g.Nodes {
		if node == nil {
			return fmt.Errorf("node record entry must not be nil")
		}
		if node.OperatorAddress == "" {
			return fmt.Errorf("node operator_address must not be empty")
		}
		if _, exists := seenNodes[node.OperatorAddress]; exists {
			return fmt.Errorf("duplicate node record for %s", node.OperatorAddress)
		}
		seenNodes[node.OperatorAddress] = struct{}{}
	}

	return nil
}

func validateBps(field string, value uint32) error {
	if value > 10_000 {
		return fmt.Errorf("%s must be <= 10000", field)
	}
	return nil
}

func RequiredBondedTokensForNode(node NodeRecord) uint64 {
	var required uint64
	switch node.Role {
	case NodeRole_NODE_ROLE_SERVICE:
		required = maxUint64(required, MinServiceBondedTokens)
	case NodeRole_NODE_ROLE_COORDINATOR:
		required = maxUint64(required, MinCoordinatorBondedTokens)
	}
	if node.Capabilities != nil {
		if node.Capabilities.Verify {
			required = maxUint64(required, MinVerifyBondedTokens)
		}
		if node.Capabilities.Propose {
			required = maxUint64(required, MinProposeBondedTokens)
		}
	}
	return required
}

func maxUint64(a, b uint64) uint64 {
	if a > b {
		return a
	}
	return b
}
