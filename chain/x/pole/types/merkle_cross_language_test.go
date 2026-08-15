package types

import (
	"testing"
)

// Cross-language Merkle fixtures.
//
// The same logical record sets are hashed by the Rust off-chain node
// (src/node_rewards.rs reward_record_root / src/node_aggregator.rs
// aggregate_record_root — now using chain-json leaves) and by the chain
// here. Both sides assert the SAME root hex, so any drift in field
// names, field order, omitempty behavior, or Merkle algorithm breaks
// this test and its Rust counterpart together.
//
// Golden values were produced by Go (this file's fixtures) and
// independently re-derived in Rust; see
// src/node_pipeline.rs::tests::{reward_record_chain_json_matches_go_json_marshal,
// aggregate_record_chain_json_matches_go_json_marshal} and
// src/node_rewards.rs::tests::reward_record_root_matches_chain_go_fixture.

func TestRewardRecordRootCrossLanguageFixture(t *testing.T) {
	// Reward recipients are the bech32 accounts Rust derives from its
	// fixture node_ids 0x31/0x32/0x33; key order = recipient ascending.
	records := []RewardRecord{
		{EpochId: 9, Recipient: "cosmos1xgqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqcwe64f", CollectReward: 10, StoreReward: 20, NetReward: 30},
		{EpochId: 9, Recipient: "cosmos1xvqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqpl7cd2", NetReward: 77},
		{EpochId: 9, Recipient: "cosmos1xyqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq65su5v", PlayerReward: 50, NetReward: 50},
	}
	root, leaves, err := MerkleRootHexForRecords(records)
	if err != nil {
		t.Fatalf("rewards root: %v", err)
	}
	if leaves != 3 {
		t.Fatalf("expected 3 reward leaves, got %d", leaves)
	}
	const want = "7b5705b4575beb29632679dc1ec335d98dadd52fd2c3ca33f0e084dda57cc33f"
	if root != want {
		t.Fatalf("rewards root mismatch:\n got %s\nwant %s", root, want)
	}
}

func TestAggregateRecordRootCrossLanguageFixture(t *testing.T) {
	// Key order = app_id ascending.
	records := []AggregateRecord{
		{EpochId: 9, AppId: 7, TotalWeightUnits: 5, PlayerCount: 0},
		{EpochId: 9, AppId: 42, TotalWeightUnits: 100, PlayerCount: 1},
		{EpochId: 9, AppId: 730, TotalWeightUnits: 88, PlayerCount: 2},
	}
	root, leaves, err := MerkleRootHexForRecords(records)
	if err != nil {
		t.Fatalf("aggregates root: %v", err)
	}
	if leaves != 3 {
		t.Fatalf("expected 3 aggregate leaves, got %d", leaves)
	}
	const want = "29ec12416d68c6ae3c5e0d86f57e6501a2571086788252bbb07644eb05dcc7a8"
	if root != want {
		t.Fatalf("aggregates root mismatch:\n got %s\nwant %s", root, want)
	}
}
