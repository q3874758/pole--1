use pole_protocol_draft::{
    CapabilityConfig, CollectConfig, EpochCommitInputs, FilesystemP2pNetwork, InMemoryP2pNetwork,
    LocalNodeRuntime, LocalRetentionBook, NodeConfig, P2pNetwork, P2pSimulationConfig, P2pTopic,
    PersistentStoreStub, ProtocolStore, RewardConfig, RuntimeConfig, SteamCurrentPlayersSample,
    StorageConfig,
};

fn fixed32(byte: u8) -> [u8; 32] {
    [byte; 32]
}

fn runtime_config() -> NodeConfig {
    NodeConfig {
        chain_id: "pole-local".into(),
        node_id_hex: pole_protocol_draft::hex_32(fixed32(1)),
        reward_address_hex: pole_protocol_draft::hex_32(fixed32(2)),
        capabilities: CapabilityConfig {
            collect: true,
            store: true,
            verify: false,
            propose: true,
            archive: false,
        },
        collect: CollectConfig {
            enabled: true,
            default_epoch_id: 1,
            default_slot_id: 1,
        },
        runtime: RuntimeConfig {
            data_dir: "./test-runtime".into(),
            poll_interval_secs: 0,
            slots_per_epoch: 2,
            challenge_window_blocks: 20,
            low_impact_mode: false,
            os_background_priority: false,
            game_active_poll_interval_secs: 0,
            game_process_names: Vec::new(),
            target_app_ids: vec![730],
            p2p_simulation: P2pSimulationConfig::default(),
            p2p_socket: pole_protocol_draft::P2pSocketConfig::default(),
            p2p_libp2p: pole_protocol_draft::P2pLibp2pConfig::default(),
            activity_sources: Vec::new(),
        },
        storage: StorageConfig {
            quota_gb: 1,
            retention_epochs: 2,
        },
        reward: RewardConfig::default(),
    }
}

fn sample(app_id: u32, players: u64, millis: u64, body: &str) -> SteamCurrentPlayersSample {
    SteamCurrentPlayersSample::steam_current_players(app_id, players, millis, body)
}

#[test]
fn runtime_collects_stores_and_publishes() {
    let mut runtime = LocalNodeRuntime::new(runtime_config(), LocalRetentionBook::with_quota_gb(1));
    let node_id = runtime.config.node_id().unwrap();

    let mut network = InMemoryP2pNetwork::default();
    let listener = fixed32(9);
    network.register_peer(node_id);
    network.register_peer(listener);
    network.subscribe(listener, P2pTopic::Batches).unwrap();
    network.subscribe(listener, P2pTopic::Receipts).unwrap();

    let outcome = runtime
        .collect_store_and_publish(
            5,
            3,
            sample(730, 500_000, 1_700_000_000_000, "steam"),
            &mut network,
        )
        .unwrap();

    assert_eq!(outcome.batch_recipients, 1);
    assert_eq!(outcome.receipt_recipients, 1);
    assert!(outcome.stored_payload.is_some());

    let payload = network
        .request_payload(listener, &outcome.assembled_batch.payload_cid)
        .unwrap();
    assert_eq!(payload.payload_bytes, outcome.assembled_batch.payload_bytes);

    let inbox = network.drain_inbox(listener).unwrap();
    assert_eq!(inbox.len(), 2);
}

#[test]
fn runtime_builds_epoch_commit_from_batches() {
    let mut runtime = LocalNodeRuntime::new(runtime_config(), LocalRetentionBook::with_quota_gb(1));
    let mut network = InMemoryP2pNetwork::default();
    let node_id = runtime.config.node_id().unwrap();
    network.register_peer(node_id);

    let outcome = runtime
        .collect_store_and_publish(
            7,
            1,
            sample(570, 123_456, 1_700_000_000_100, "dota"),
            &mut network,
        )
        .unwrap();

    let stored = outcome
        .stored_payload
        .clone()
        .into_iter()
        .collect::<Vec<_>>();
    let epoch_commit = runtime
        .build_epoch_commit(EpochCommitInputs {
            epoch_id: 7,
            current_height: 100,
            challenge_window_blocks: 20,
            batches: &[outcome.assembled_batch],
            stored_payloads: &stored,
            aggregates_root: [0xaa; 32],
            rewards_root: [0xbb; 32],
        })
        .unwrap();

    assert_eq!(epoch_commit.epoch_id, 7);
    assert_eq!(epoch_commit.proposer_id, node_id);
    assert_eq!(epoch_commit.challenge_open_height, 100);
    assert_eq!(epoch_commit.challenge_deadline_height, 120);
    assert_eq!(epoch_commit.accepted_batches.leaf_count, 1);
    assert_eq!(epoch_commit.observations.leaf_count, 1);
    assert_eq!(epoch_commit.availability.leaf_count, 1);
}

#[test]
fn runtime_collects_stores_and_publishes_via_filesystem_network() {
    let root = std::env::temp_dir().join(format!("pole-runtime-fs-p2p-{}", std::process::id()));
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }

    let mut runtime = LocalNodeRuntime::new(runtime_config(), LocalRetentionBook::with_quota_gb(1));
    let node_id = runtime.config.node_id().unwrap();

    let mut network = FilesystemP2pNetwork::new(&root);
    let listener = fixed32(9);
    let requester = fixed32(10);
    network.bootstrap_peer(node_id, &[]).unwrap();
    network
        .bootstrap_peer(listener, &[P2pTopic::Batches, P2pTopic::Receipts])
        .unwrap();
    network.bootstrap_peer(requester, &[]).unwrap();

    let outcome = runtime
        .collect_store_and_publish(
            5,
            3,
            sample(730, 500_000, 1_700_000_000_000, "steam"),
            &mut network,
        )
        .unwrap();

    assert_eq!(outcome.batch_recipients, 1);
    assert_eq!(outcome.receipt_recipients, 1);

    let payload = network
        .request_payload(requester, &outcome.assembled_batch.payload_cid)
        .unwrap();
    assert_eq!(payload.payload_bytes, outcome.assembled_batch.payload_bytes);

    let inbox = network.drain_inbox(listener).unwrap();
    assert_eq!(inbox.len(), 2);

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn runtime_uses_activated_protocol_min_retention_epochs_for_storage() {
    let mut config = runtime_config();
    let data_dir =
        std::env::temp_dir().join(format!("pole-runtime-gov-retention-{}", std::process::id()));
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).unwrap();
    }
    config.runtime.data_dir = data_dir.to_string_lossy().into_owned();

    let store_path = pole_protocol_draft::node_daemon::local_chain_store_path(&config);
    let mut store = PersistentStoreStub::open(&store_path).unwrap();
    let params = pole_protocol_draft::ProtocolParams {
        slot_seconds: 300,
        epoch_slots: 12,
        committee_size: 21,
        unbonding_blocks: 5,
        min_verify_bond: 100,
        min_propose_bond: 10_000,
        challenge_window_blocks: 20,
        max_emergency_brake_blocks: 100,
        min_retention_epochs: 4,
        fee: pole_protocol_draft::FeeParams {
            base_gas_price_nano: 100,
            max_gas_price_nano: 1_000,
            gas_adjustment_ppm: 1_150_000,
            congestion_threshold_ppm: 500_000,
            fee_burn_bps: 2_500,
        },
        rewards: pole_protocol_draft::RewardParams {
            reward_source_is_tokenomics: false,
            emission_year: 1,
            reward_block_secs: 3_600,
            initial_emission_rate_bps: 2_000,
            tail_emission_start_year: 4,
            tail_emission_rate_bps: 200,
            player_reward_allocation_bps: 8_000,
            service_reward_allocation_bps: 1_000,
            collect_reward_bps: 5_000,
            store_reward_bps: 2_500,
            verify_reward_bps: 1_500,
            propose_reward_bps: 1_000,
            configured_player_block_reward: 1_000,
            effective_player_block_reward: 1_000,
            target_network_weight_units: 1,
            reward_adjustment_cap_bps: 2_000,
            tier1_weight_ppm: 1_000_000,
            tier2_weight_min_ppm: 300_000,
            tier2_weight_max_ppm: 600_000,
            tier3_weight_min_ppm: 50_000,
            tier3_weight_max_ppm: 150_000,
            app_weight_overrides: Vec::new(),
            reward_burn_threshold: 10_000,
            reward_burn_bps: 1_000,
            governance_burn_bps: 100,
        },
        governance: pole_protocol_draft::GovernanceParams {
            params_update_bond: 10_000,
            params_update_quorum_bps: 2_500,
            params_update_approval_bps: 6_000,
            slow_params_update_bond: 20_000,
            slow_params_update_quorum_bps: 3_300,
            slow_params_update_approval_bps: 7_500,
        },
        slashing: pole_protocol_draft::SlashingParams {
            double_sign_bps: 5_000,
            offline_bps: 100,
            medium_deviation_bps: 500,
            severe_deviation_bps: 2_000,
        },
    };
    store.insert_params_update_proposal(
        [0xaa; 32],
        pole_protocol_draft::GovernanceParamsUpdateProposalRecord {
            proposal_id: [0xaa; 32],
            proposer: fixed32(2),
            kind: pole_protocol_draft::GovernanceProposalKind::SlowParams,
            effective_epoch: 2,
            submitted_height: 1,
            bond_amount: 20_000,
            params_hash: [0xbb; 32],
            params,
            state: pole_protocol_draft::GovernanceProposalState::Activated,
        },
    );
    store.flush().unwrap();

    let mut runtime = LocalNodeRuntime::new(config.clone(), LocalRetentionBook::with_quota_gb(1));
    let outcome = runtime
        .collect_and_store_samples(5, 3, vec![sample(730, 500_000, 1_700_000_000_000, "steam")])
        .unwrap();

    assert_eq!(outcome.stored_payload.unwrap().retention_until_epoch, 9);

    std::fs::remove_dir_all(data_dir).unwrap();
}
