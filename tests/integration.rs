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
    use super::harness::IntegrationHarnessBuilder;

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

        let _node = h
            .register_node(Default::default())
            .await
            .expect("register_node should succeed");

        let _tx = h
            .submit_batch(serde_json::json!({"placeholder": true}))
            .await
            .expect("submit_batch should succeed");

        let _tx = h
            .claim_reward(1)
            .await
            .expect("claim_reward should succeed");

        // Drop kills the chain process.
    }
}
