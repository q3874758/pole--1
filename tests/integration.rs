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
mod integration_scenarios {
    use super::harness::{IntegrationHarnessBuilder, RegisteredNodeCapabilities};

    /// Scenario 1: register a node, submit a batch, claim a reward.
    /// Skipped unless `--features integration` is enabled and a
    /// `poled` binary is on $PATH.
    #[tokio::test]
    async fn register_submit_claim_happy_path() {
        let h = IntegrationHarnessBuilder::new()
            .chain_id("pole-it-1")
            .boot()
            .await
            .expect("harness should boot");

        // `collect` capability is required for `MsgSubmitBatch` to pass
        // `requireNodeCapability(..., "collect")`.
        let node = h
            .register_node(RegisteredNodeCapabilities {
                collect: true,
                ..Default::default()
            })
            .await
            .unwrap_or_else(|e| panic!("register_node should succeed: {e}\n--- poled log ---\n{}", h.poled_log_text()));
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
}
