//! End-to-end integration tests for the PoLE bridge layer.
//!
//! These tests are gated on `--features integration` because they
//! require a built `poled` binary on $PATH. Without the feature only
//! the compile-time shape check runs.

mod harness;

use harness::{IntegrationHarnessBuilder, RegisteredNodeCapabilities};

/// Compile-time check: the harness types compose.
#[test]
fn harness_types_are_constructible() {
    // Building a builder exercises the public API.
    let _b = IntegrationHarnessBuilder::new().chain_id("pole-it-1");
    let _c = RegisteredNodeCapabilities::default();
}

#[cfg(feature = "integration")]
#[allow(clippy::await_holding_lock)]
mod integration_scenarios {
    use super::harness::{self, IntegrationHarnessBuilder, RegisteredNodeCapabilities};

    /// Each scenario boots its own `poled` on the default ports
    /// (26657/1317), so scenarios must run serially.
    static BOOT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    async fn boot(chain_id: &str) -> harness::IntegrationHarness {
        IntegrationHarnessBuilder::new()
            .chain_id(chain_id)
            .boot()
            .await
            .unwrap_or_else(|e| panic!("harness should boot: {e}"))
    }

    async fn register(
        h: &harness::IntegrationHarness,
        caps: RegisteredNodeCapabilities,
    ) -> harness::RegisteredNode {
        h.register_node(caps).await.unwrap_or_else(|e| {
            panic!(
                "register_node should succeed: {e}\n--- poled log ---\n{}",
                h.poled_log_text()
            )
        })
    }

    /// Scenario 1: register a node, submit a batch, claim a reward.
    /// Skipped unless `--features integration` is enabled and a
    /// `poled` binary is on $PATH.
    #[tokio::test]
    async fn register_submit_claim_happy_path() {
        let _guard = BOOT_LOCK.lock().unwrap();
        let h = boot("pole-it-1").await;

        // `collect` capability is required for `MsgSubmitBatch` to pass
        // `requireNodeCapability(..., "collect")`.
        let node = register(
            &h,
            RegisteredNodeCapabilities {
                collect: true,
                ..Default::default()
            },
        )
        .await;
        assert!(node.capabilities.collect);

        let tx = h
            .submit_batch(serde_json::json!({"epoch_id": 1}))
            .await
            .expect("submit_batch should succeed");
        assert!(!tx.is_empty());

        let tx = h
            .claim_reward(1)
            .await
            .expect("claim_reward should succeed");
        assert!(!tx.is_empty());

        // Drop kills the chain process.
    }

    /// Scenario 2: full epoch lifecycle for a fresh epoch — register,
    /// submit a batch, commit the epoch, upsert an aggregate (which
    /// refreshes the aggregates commitment), wait out the challenge
    /// window and finalize.
    #[tokio::test]
    async fn epoch_lifecycle_submit_commit_aggregate_finalize() {
        let _guard = BOOT_LOCK.lock().unwrap();
        let h = boot("pole-it-2").await;

        register(
            &h,
            RegisteredNodeCapabilities {
                collect: true,
                store: true,
                verify: true,
                propose: true,
            },
        )
        .await;

        let tx = h
            .submit_batch(serde_json::json!({"epoch_id": 2}))
            .await
            .unwrap_or_else(|e| {
                panic!(
                    "submit_batch(2): {e}\n--- poled log ---\n{}",
                    h.poled_log_text()
                )
            });
        assert!(!tx.is_empty());

        let tx = h.commit_epoch(2, 0).await.unwrap_or_else(|e| {
            panic!(
                "commit_epoch(2): {e}\n--- poled log ---\n{}",
                h.poled_log_text()
            )
        });
        assert!(!tx.is_empty());

        let tx = h.upsert_aggregate_record(2).await.unwrap_or_else(|e| {
            panic!(
                "upsert_aggregate(2): {e}\n--- poled log ---\n{}",
                h.poled_log_text()
            )
        });
        assert!(!tx.is_empty());

        // Wait for the challenge window to elapse, then finalize.
        let tx = h.finalize_epoch(2, 3).await.unwrap_or_else(|e| {
            panic!(
                "finalize_epoch(2): {e}\n--- poled log ---\n{}",
                h.poled_log_text()
            )
        });
        assert!(!tx.is_empty());
    }

    /// Scenario 3: open a challenge against the genesis-seeded epoch-1
    /// commit. Requires the verify capability and a committed epoch.
    #[tokio::test]
    async fn open_challenge_for_committed_epoch() {
        let _guard = BOOT_LOCK.lock().unwrap();
        let h = boot("pole-it-3").await;

        let node = register(
            &h,
            RegisteredNodeCapabilities {
                collect: true,
                verify: true,
                ..Default::default()
            },
        )
        .await;

        let tx = h
            .open_challenge(1, &node.node_id_hex, 1_000_000, [0xE5; 32])
            .await
            .unwrap_or_else(|e| {
                panic!(
                    "open_challenge: {e}\n--- poled log ---\n{}",
                    h.poled_log_text()
                )
            });
        assert!(!tx.is_empty());
    }
}
