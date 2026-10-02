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
    use super::harness::{
        self, HarnessError, HarnessIdentity, IntegrationHarnessBuilder, RegisteredNodeCapabilities,
    };
    use pole_protocol_draft::cosmos::wire_types::NodeRoleWire;

    /// Each scenario boots its own `poled` on the default ports
    /// (26657/1317), so scenarios must run serially.
    static BOOT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn boot_lock() -> std::sync::MutexGuard<'static, ()> {
        BOOT_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    async fn boot(chain_id: &str) -> harness::IntegrationHarness {
        IntegrationHarnessBuilder::new()
            .chain_id(chain_id)
            .boot()
            .await
            .unwrap_or_else(|e| panic!("harness should boot: {e}"))
    }

    /// Boot with extra signers. Mutual-proof scenarios need several
    /// distinct on-chain accounts because every message names its own
    /// signer, and the chain derives that signer from a bech32 field.
    async fn boot_with_identities(
        chain_id: &str,
        identities: &[(&str, u8)],
    ) -> harness::IntegrationHarness {
        let mut builder = IntegrationHarnessBuilder::new().chain_id(chain_id);
        for (name, seed) in identities {
            builder = builder.identity(*name, [*seed; 32]);
        }
        builder
            .boot()
            .await
            .unwrap_or_else(|e| panic!("harness should boot: {e}"))
    }

    fn identity<'a>(h: &'a harness::IntegrationHarness, name: &str) -> &'a HarnessIdentity {
        h.identities
            .iter()
            .find(|candidate| candidate.name == name)
            .unwrap_or_else(|| {
                panic!(
                    "identity {name:?} should be provisioned; have {:?}",
                    h.identities
                        .iter()
                        .map(|candidate| candidate.name.as_str())
                        .collect::<Vec<_>>()
                )
            })
    }

    /// Register `identity` as a zero-bond service node with the
    /// collect/verify capabilities the mutual-proof rules require.
    /// `is_player` makes `RequiredBondedTokensForNode` return 0, so no
    /// stake or consensus address is needed.
    async fn register_witness(
        h: &harness::IntegrationHarness,
        identity: &HarnessIdentity,
    ) -> harness::RegisteredNode {
        h.register_identity(
            identity,
            NodeRoleWire::Service,
            true,
            RegisteredNodeCapabilities {
                collect: true,
                store: false,
                verify: true,
                propose: false,
            },
        )
        .await
        .unwrap_or_else(|e| {
            panic!(
                "register_identity({}) should succeed: {e}\n--- poled log ---\n{}",
                identity.name,
                h.poled_log_text()
            )
        })
    }

    async fn expect_ok(
        h: &harness::IntegrationHarness,
        what: &str,
        result: Result<String, HarnessError>,
    ) -> String {
        result.unwrap_or_else(|e| {
            panic!(
                "{what} should be accepted: {e}\n--- poled log ---\n{}",
                h.poled_log_text()
            )
        })
    }

    /// Assert the chain rejected the message with `needle` somewhere in
    /// its log. `needle` is a phrase from `keeper/session.go`, which is
    /// the contract these tests pin down.
    fn expect_rejected(
        h: &harness::IntegrationHarness,
        what: &str,
        result: Result<String, HarnessError>,
        needle: &str,
    ) {
        match result {
            Ok(tx) => panic!(
                "{what} should be rejected, but the chain accepted tx {tx}\n--- poled log ---\n{}",
                h.poled_log_text()
            ),
            Err(HarnessError::ChainRejected { code, log }) => {
                assert!(
                    log.contains(needle),
                    "{what} should be rejected with {needle:?} (code {code}), got: {log}"
                );
            }
            Err(other) => panic!(
                "{what} should be rejected with a non-zero chain code, got {other}\n--- poled log ---\n{}",
                h.poled_log_text()
            ),
        }
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
        let _guard = boot_lock();
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

    /// Scenario 2: full epoch lifecycle for a fresh epoch —register,
    /// submit a batch, commit the epoch, upsert an aggregate (which
    /// refreshes the aggregates commitment), wait out the challenge
    /// window and finalize.
    #[tokio::test]
    async fn epoch_lifecycle_submit_commit_aggregate_finalize() {
        let _guard = boot_lock();
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
        let _guard = boot_lock();
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

    // -----------------------------------------------------------------
    // Mutual proof (play session ->witnesses ->settlement)
    // -----------------------------------------------------------------

    /// Epoch used by the play-proof scenarios. Epoch 1 is already
    /// finalized in the genesis seed, so the play scenarios stay on 2.
    const PLAY_EPOCH: u64 = 2;

    /// Must stay at or below the chain's `reward_block_duration_seconds`
    /// (3600), which `MsgSubmitPlaySession` enforces as the slot length.
    const PLAY_SECONDS: u64 = 1800;

    const PLAY_OBSERVED_PLAYERS: u64 = 12;
    const PLAY_OBSERVATION_CID: &str = "bafy-player-observation";

    /// Register one of the extra identities as a zero-bond service node
    /// with collect+verify.
    async fn register_participant(
        h: &harness::IntegrationHarness,
        name: &str,
    ) -> harness::RegisteredNode {
        register_witness(h, identity(h, name)).await
    }

    /// Submit `name`'s own batch observation for `epoch`, giving it the
    /// on-chain evidence `validateWitnessIndependence` demands. `index`
    /// makes the batch root unique per collector —the chain keys batch
    /// commits by `(epoch, collector, root)`.
    async fn collect_observation(h: &harness::IntegrationHarness, name: &str, index: u8) -> String {
        let root = format!("{:02x}", index).repeat(32);
        expect_ok(
            h,
            &format!("submit_batch_for({name})"),
            h.submit_batch_for(identity(h, name), PLAY_EPOCH, &root)
                .await,
        )
        .await
    }

    /// Build and submit the player's play-session claim for `epoch`.
    /// Self-collection is used (`collector = player`) so the session
    /// needs no third party to exist first.
    async fn submit_player_session(
        h: &harness::IntegrationHarness,
        player_name: &str,
        observation_cid: &str,
    ) -> (
        String,
        pole_protocol_draft::cosmos::wire_types::PlaySessionWire,
    ) {
        let player = identity(h, player_name);
        let built = h.build_play_session_wire(
            player,
            730,
            PLAY_EPOCH,
            1,
            PLAY_SECONDS,
            player.node_id(),
            observation_cid,
            PLAY_OBSERVED_PLAYERS,
        );
        let tx = expect_ok(
            h,
            &format!("submit_play_session({player_name})"),
            h.submit_play_session_for(player, &built.wire).await,
        )
        .await;
        assert!(!tx.is_empty());
        (built.session_id_hex, built.wire)
    }

    /// Submit `count` heartbeats in distinct buckets. Settlement wants
    /// `>= 2` heartbeats covering `>= 5000` bps of the declared window;
    /// 1800s in 300s buckets is 6 expected buckets, so 4 distinct
    /// buckets cover 6666 bps.
    async fn submit_heartbeats(
        h: &harness::IntegrationHarness,
        player_name: &str,
        session: &pole_protocol_draft::cosmos::wire_types::PlaySessionWire,
        count: u64,
    ) {
        let player = identity(h, player_name);
        for bucket in 0..count {
            let heartbeat =
                h.build_play_heartbeat_wire(player, session, bucket, 1_700_000_000_000 + bucket);
            expect_ok(
                h,
                &format!("submit_heartbeat({player_name}, bucket {bucket})"),
                h.submit_heartbeat_for(player, &heartbeat).await,
            )
            .await;
        }
    }

    /// Scenario 4: a play session corroborated by two independent
    /// witnesses settles as valid, and the settlement records the
    /// evidence actually used (2 witnesses, 2 distinct observations,
    /// 4 heartbeats).
    #[tokio::test]
    async fn play_session_with_two_independent_witnesses_settles() {
        let _guard = boot_lock();
        let h = boot_with_identities(
            "pole-it-play-1",
            &[("player", 0xA1), ("w1", 0xB1), ("w2", 0xC1)],
        )
        .await;

        register_participant(&h, "player").await;
        register_participant(&h, "w1").await;
        register_participant(&h, "w2").await;

        // Each witness must hold an observation of its own for the epoch.
        collect_observation(&h, "w1", 1).await;
        collect_observation(&h, "w2", 2).await;

        let (session_id_hex, session) =
            submit_player_session(&h, "player", PLAY_OBSERVATION_CID).await;
        submit_heartbeats(&h, "player", &session, 4).await;

        for (name, cid) in [("w1", "bafy-w1-observation"), ("w2", "bafy-w2-observation")] {
            let attestation = h.build_witness_attestation_wire(
                identity(&h, name),
                &session,
                cid,
                PLAY_SECONDS,
                PLAY_OBSERVED_PLAYERS,
            );
            expect_ok(
                &h,
                &format!("attest_session({name})"),
                h.attest_session_for(identity(&h, name), &attestation).await,
            )
            .await;
        }

        expect_ok(
            &h,
            "settle_session",
            h.settle_session_for(identity(&h, "w1"), &session_id_hex)
                .await,
        )
        .await;

        let settlement = h
            .session_settlement(&session_id_hex)
            .await
            .unwrap_or_else(|e| {
                panic!(
                    "session_settlement should resolve: {e}\n--- poled log ---\n{}",
                    h.poled_log_text()
                )
            });
        assert!(
            settlement.valid,
            "two independent witnesses with 4 heartbeats should settle valid, got {:?}",
            settlement.invalid_reason
        );
        assert_eq!(settlement.session_id_hex, session_id_hex);
        assert_eq!(settlement.epoch_id, PLAY_EPOCH);
        assert_eq!(settlement.play_seconds, PLAY_SECONDS);
        assert_eq!(settlement.witness_count, 2);
        assert_eq!(settlement.distinct_observation_count, 2);
        assert_eq!(settlement.heartbeat_count, 4);
        assert!(
            settlement.heartbeat_coverage_bps >= 5_000,
            "coverage should clear the 5000 bps floor, got {}",
            settlement.heartbeat_coverage_bps
        );
        assert!(
            settlement.player_weight_units > 0,
            "a valid settlement must carry weight"
        );
    }

    /// Scenario 5: a node may not witness its own play session. The
    /// self-attestation is rejected before any capability or
    /// observation check runs.
    #[tokio::test]
    async fn witness_cannot_attest_own_session() {
        let _guard = boot_lock();
        let h = boot_with_identities("pole-it-play-2", &[("player", 0xA1), ("w1", 0xB1)]).await;

        register_participant(&h, "player").await;
        register_participant(&h, "w1").await;
        collect_observation(&h, "w1", 1).await;

        let (_session_id_hex, session) =
            submit_player_session(&h, "player", PLAY_OBSERVATION_CID).await;

        let self_attestation = h.build_witness_attestation_wire(
            identity(&h, "player"),
            &session,
            "bafy-player-second-observation",
            PLAY_SECONDS,
            PLAY_OBSERVED_PLAYERS,
        );
        expect_rejected(
            &h,
            "self attestation",
            h.attest_session_for(identity(&h, "player"), &self_attestation)
                .await,
            "cannot attest its own play session",
        );
    }

    /// Scenario 6: a registered witness with no observation of its own
    /// for the epoch is just echoing the claim, and is rejected.
    #[tokio::test]
    async fn witness_without_own_observation_is_rejected() {
        let _guard = boot_lock();
        let h = boot_with_identities(
            "pole-it-play-3",
            &[("player", 0xA1), ("w1", 0xB1), ("w2", 0xC1)],
        )
        .await;

        register_participant(&h, "player").await;
        register_participant(&h, "w1").await;
        register_participant(&h, "w2").await;

        // Only w1 collects anything this epoch; w2 stays empty-handed.
        collect_observation(&h, "w1", 1).await;

        let (_session_id_hex, session) =
            submit_player_session(&h, "player", PLAY_OBSERVATION_CID).await;

        let echo = h.build_witness_attestation_wire(
            identity(&h, "w2"),
            &session,
            "bafy-w2-observation",
            PLAY_SECONDS,
            PLAY_OBSERVED_PLAYERS,
        );
        expect_rejected(
            &h,
            "attestation from a witness with no observation",
            h.attest_session_for(identity(&h, "w2"), &echo).await,
            "has no observation of its own for epoch",
        );
    }

    /// Scenario 7: a witness may not reuse the player's observation
    /// payload —that would let one payload be presented as N
    /// independent corroborations.
    #[tokio::test]
    async fn witness_reusing_player_observation_cid_is_rejected() {
        let _guard = boot_lock();
        let h = boot_with_identities(
            "pole-it-play-4",
            &[("player", 0xA1), ("w1", 0xB1), ("w2", 0xC1)],
        )
        .await;

        register_participant(&h, "player").await;
        register_participant(&h, "w1").await;
        register_participant(&h, "w2").await;
        collect_observation(&h, "w1", 1).await;
        collect_observation(&h, "w2", 2).await;

        let (_session_id_hex, session) =
            submit_player_session(&h, "player", PLAY_OBSERVATION_CID).await;

        let copy = h.build_witness_attestation_wire(
            identity(&h, "w1"),
            &session,
            PLAY_OBSERVATION_CID,
            PLAY_SECONDS,
            PLAY_OBSERVED_PLAYERS,
        );
        expect_rejected(
            &h,
            "attestation reusing the session observation",
            h.attest_session_for(identity(&h, "w1"), &copy).await,
            "must not reuse the session observation payload",
        );
    }
}
