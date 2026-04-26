use pole_protocol_draft::{
    build_inmemory_simulation_network, current_players_url, fetch_current_players_with_client,
    hex_32, open_local_protocol_state, parse_current_players_response, retention_book_path,
    run_collect_tick_with_client_and_network, AccountState, FilesystemP2pNetwork, HttpTextClient,
    LocalNodeProgress, NodeConfig, NodeConfigError, P2pNetwork, P2pSimulationConfig, P2pTopic,
    PersistentStoreStub, ProtocolStore, ReqwestHttpTextClient, SocketP2pNetwork,
    SteamCollectorError,
};
use std::process::Command;

#[cfg(feature = "real-libp2p")]
use libp2p_identity::Keypair;

struct FakeHttpClient {
    body: String,
}

impl HttpTextClient for FakeHttpClient {
    fn get_text(&self, _url: &str) -> Result<String, SteamCollectorError> {
        Ok(self.body.clone())
    }
}

#[test]
fn node_config_roundtrip_and_hex_decode_work() {
    let path = std::env::temp_dir().join(format!("pole-node-config-{}.json", std::process::id()));
    if path.exists() {
        std::fs::remove_file(&path).unwrap();
    }

    let config = NodeConfig::default();
    config.save_json(&path).unwrap();
    let loaded = NodeConfig::load_json(&path).unwrap();

    assert_eq!(config, loaded);
    assert_eq!(loaded.node_id().unwrap(), [0x11; 32]);
    assert_eq!(loaded.reward_address().unwrap(), [0x22; 32]);

    std::fs::remove_file(path).unwrap();
}

#[test]
fn node_config_roundtrip_preserves_p2p_simulation_defaults() {
    let path =
        std::env::temp_dir().join(format!("pole-node-p2p-config-{}.json", std::process::id()));
    if path.exists() {
        std::fs::remove_file(&path).unwrap();
    }

    let mut config = NodeConfig::default();
    config.runtime.p2p_simulation = P2pSimulationConfig {
        batch_listener_count: 2,
        receipt_listener_count: 1,
        dual_listener_count: 3,
    };
    config.save_json(&path).unwrap();
    let loaded = NodeConfig::load_json(&path).unwrap();

    assert_eq!(loaded.runtime.p2p_simulation.batch_listener_count, 2);
    assert_eq!(loaded.runtime.p2p_simulation.receipt_listener_count, 1);
    assert_eq!(loaded.runtime.p2p_simulation.dual_listener_count, 3);

    std::fs::remove_file(path).unwrap();
}

#[test]
fn node_config_roundtrip_preserves_p2p_socket_defaults() {
    let path = std::env::temp_dir().join(format!(
        "pole-node-socket-config-{}.json",
        std::process::id()
    ));
    if path.exists() {
        std::fs::remove_file(&path).unwrap();
    }

    let mut config = NodeConfig::default();
    config.runtime.p2p_socket.bind_addr = "127.0.0.1:42000".into();
    config.runtime.p2p_socket.peers = vec![pole_protocol_draft::P2pSocketPeerConfig {
        peer_id_hex: pole_protocol_draft::hex_32([0xaa; 32]),
        addr: "127.0.0.1:43000".into(),
        topics: vec!["batches".into(), "receipts".into()],
    }];
    config.save_json(&path).unwrap();
    let loaded = NodeConfig::load_json(&path).unwrap();

    assert_eq!(loaded.runtime.p2p_socket.bind_addr, "127.0.0.1:42000");
    assert_eq!(loaded.runtime.p2p_socket.peers.len(), 1);
    assert_eq!(
        loaded.runtime.p2p_socket.peers[0].peer_id_hex,
        pole_protocol_draft::hex_32([0xaa; 32])
    );
    assert_eq!(loaded.runtime.p2p_socket.peers[0].addr, "127.0.0.1:43000");
    assert_eq!(
        loaded.runtime.p2p_socket.peers[0].topics,
        vec!["batches", "receipts"]
    );

    std::fs::remove_file(path).unwrap();
}

#[test]
fn node_config_rejects_bad_hex() {
    let config = NodeConfig {
        node_id_hex: "zz".repeat(32),
        ..NodeConfig::default()
    };

    let err = config.node_id().unwrap_err();
    assert!(matches!(
        err,
        NodeConfigError::InvalidHexCharacter { field, .. } if field == "node_id_hex"
    ));
}

#[test]
fn node_config_rejects_reward_block_shorter_than_poll_interval() {
    let config = NodeConfig {
        runtime: pole_protocol_draft::RuntimeConfig {
            poll_interval_secs: 300,
            ..NodeConfig::default().runtime
        },
        reward: pole_protocol_draft::RewardConfig {
            reward_block_secs: 299,
            ..NodeConfig::default().reward
        },
        ..NodeConfig::default()
    };

    let err = config.save_json(std::env::temp_dir().join(format!(
        "pole-node-invalid-config-{}.json",
        std::process::id()
    )));
    assert!(matches!(
        err,
        Err(NodeConfigError::InvalidValue { field, .. }) if field == "reward.reward_block_secs"
    ));
}

#[test]
fn tokenomics_reward_source_derives_hourly_block_reward() {
    let config = NodeConfig {
        reward: pole_protocol_draft::RewardConfig {
            reward_source: pole_protocol_draft::RewardSourceMode::Tokenomics,
            emission_year: 1,
            reward_block_secs: 3_600,
            player_block_reward: 1,
            ..NodeConfig::default().reward
        },
        ..NodeConfig::default()
    };

    assert_eq!(config.reward.base_player_block_reward(), 18_264);
}

#[test]
fn open_local_protocol_state_prefers_latest_activated_protocol_params() {
    let temp_dir =
        std::env::temp_dir().join(format!("pole-open-local-state-{}", std::process::id()));
    if temp_dir.exists() {
        std::fs::remove_dir_all(&temp_dir).unwrap();
    }
    let mut config = NodeConfig::default();
    config.runtime.data_dir = temp_dir.to_string_lossy().into_owned();
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
            emission_year: 3,
            reward_block_secs: 7_200,
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
            effective_player_block_reward: 9_132,
            target_network_weight_units: 1_800_000_000_000,
            reward_adjustment_cap_bps: 1_500,
            tier1_weight_ppm: 950_000,
            tier2_weight_min_ppm: 250_000,
            tier2_weight_max_ppm: 550_000,
            tier3_weight_min_ppm: 60_000,
            tier3_weight_max_ppm: 160_000,
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
            proposer: [0x22; 32],
            kind: pole_protocol_draft::GovernanceProposalKind::FastParams,
            effective_epoch: 2,
            submitted_height: 1,
            bond_amount: 10_000,
            params_hash: [0xbb; 32],
            params,
            state: pole_protocol_draft::GovernanceProposalState::Activated,
        },
    );
    store.flush().unwrap();

    let (_, state) =
        open_local_protocol_state(&config, config.runtime.challenge_window_blocks).unwrap();
    assert_eq!(state.params.rewards.emission_year, 3);
    assert_eq!(state.params.rewards.reward_block_secs, 7_200);
    assert_eq!(state.params.min_retention_epochs, 4);
    std::fs::remove_dir_all(temp_dir).unwrap();
}

#[test]
fn steam_current_players_response_parses() {
    let raw = r#"{"response":{"player_count":123456,"result":1}}"#;
    let sample = parse_current_players_response(730, 1_700_000_000_000, raw).unwrap();

    assert_eq!(sample.app_id, 730);
    assert_eq!(sample.observed_players, 123_456);
    assert_eq!(sample.observed_at_millis, 1_700_000_000_000);
    assert_eq!(sample.raw_body, raw);
}

#[test]
fn steam_fetch_with_client_uses_http_layer() {
    let client = FakeHttpClient {
        body: r#"{"response":{"player_count":654321,"result":1}}"#.to_string(),
    };

    let sample = fetch_current_players_with_client(&client, 570, 1_800_000_000_000).unwrap();
    assert_eq!(sample.app_id, 570);
    assert_eq!(sample.observed_players, 654_321);
    assert_eq!(sample.observed_at_millis, 1_800_000_000_000);
}

#[test]
fn pole_node_build_batch_from_community_json_outputs_batch_summary() {
    let config_path = std::env::temp_dir().join(format!(
        "pole-node-community-batch-{}.json",
        std::process::id()
    ));
    let json_path = std::env::temp_dir().join(format!(
        "pole-node-community-batch-{}.json.body",
        std::process::id()
    ));
    let payload_path = std::env::temp_dir().join(format!(
        "pole-node-community-batch-{}.bin",
        std::process::id()
    ));
    let config = NodeConfig::default();
    config.save_json(&config_path).unwrap();
    std::fs::write(
        &json_path,
        r#"{"estimated_players":77,"confidence_ppm":120000}"#,
    )
    .unwrap();

    let binary = env!("CARGO_BIN_EXE_pole-node");
    let output = Command::new(binary)
        .arg("build-batch-from-community-json")
        .arg(&config_path)
        .arg("1")
        .arg("1")
        .arg("9900")
        .arg("1700000000000")
        .arg(&json_path)
        .arg(&payload_path)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("payload_cid="));
    assert!(stdout.contains("obs_count=1"));
    assert!(payload_path.exists());

    let _ = std::fs::remove_file(config_path);
    let _ = std::fs::remove_file(json_path);
    let _ = std::fs::remove_file(payload_path);
}

#[test]
fn pole_node_build_batch_from_community_inline_json_outputs_batch_summary() {
    let config_path = std::env::temp_dir().join(format!(
        "pole-node-community-inline-batch-{}.json",
        std::process::id()
    ));
    let payload_path = std::env::temp_dir().join(format!(
        "pole-node-community-inline-batch-{}.bin",
        std::process::id()
    ));
    let config = NodeConfig::default();
    config.save_json(&config_path).unwrap();

    let binary = env!("CARGO_BIN_EXE_pole-node");
    let output = Command::new(binary)
        .arg("build-batch-from-community-inline-json")
        .arg(&config_path)
        .arg("1")
        .arg("1")
        .arg("9901")
        .arg("1700000000000")
        .arg(r#"{"estimated_players":88,"confidence_ppm":130000}"#)
        .arg(&payload_path)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("payload_cid="));
    assert!(stdout.contains("obs_count=1"));
    assert!(payload_path.exists());

    let _ = std::fs::remove_file(config_path);
    let _ = std::fs::remove_file(payload_path);
}

#[test]
fn pole_node_libp2p_skeleton_reports_configured_discovery() {
    let config_path = std::env::temp_dir().join(format!(
        "pole-node-libp2p-skeleton-{}.json",
        std::process::id()
    ));
    let mut config = NodeConfig::default();
    config.runtime.p2p_libp2p.enabled = true;
    config.runtime.p2p_libp2p.listen_addrs = vec!["/ip4/127.0.0.1/tcp/0".into()];
    #[cfg(feature = "real-libp2p")]
    let bootstrap_peer_id = Keypair::generate_ed25519()
        .public()
        .to_peer_id()
        .to_string();
    #[cfg(not(feature = "real-libp2p"))]
    let bootstrap_peer_id = "12D3KooWJ5Z5L6hG1Zq1x3wQ5P5ZkJ7V3xZ6QYp6iYvJpR6J8W8J".to_string();
    config.runtime.p2p_libp2p.bootstrap_peers =
        vec![pole_protocol_draft::P2pLibp2pBootstrapPeerConfig {
            peer_id: bootstrap_peer_id,
            addr: "/ip4/127.0.0.1/tcp/4002".into(),
        }];
    config.save_json(&config_path).unwrap();

    let binary = env!("CARGO_BIN_EXE_pole-node");
    let output = Command::new(binary)
        .arg("libp2p-skeleton")
        .arg(&config_path)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("local_peer_id="));
    assert!(stdout.contains("bootstrap_peer_count=1"));
    assert!(stdout.contains("kademlia_enabled=true"));
    assert!(stdout.contains("real_swarm_listener_count=1"));

    let _ = std::fs::remove_file(config_path);
}

#[test]
fn pole_node_tokenomics_reports_supply_allocations_and_schedule() {
    let binary = env!("CARGO_BIN_EXE_pole-node");
    let output = Command::new(binary)
        .arg("tokenomics")
        .arg("2")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("PoLE node tokenomics"));
    assert!(stdout.contains("total_supply=1000000000"));
    assert!(stdout.contains("tail_emission_start_year=4"));
    assert!(stdout.contains("tail_emission_rate_bps=200"));
    assert!(stdout.contains("treasury_allocation=50000000"));
    assert!(stdout.contains("team_allocation=30000000"));
    assert!(stdout.contains(
        "year=2 nominal_rate_bps=2000 annual_emission=200000000 cumulative_emission=400000000"
    ));
}

#[test]
fn pole_node_governance_commands_manage_future_params_update() {
    let root = std::env::temp_dir().join(format!("pole-node-governance-{}", std::process::id()));
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("node.json");
    let mut config = NodeConfig::default();
    config.capabilities.propose = true;
    config.capabilities.verify = true;
    config.save_json(&config_path).unwrap();

    let (_, loaded) = NodeConfig::load_json_with_runtime_paths(&config_path).unwrap();
    let (runtime, mut state) =
        open_local_protocol_state(&loaded, loaded.runtime.challenge_window_blocks).unwrap();
    let reward_address = loaded.reward_address().unwrap();
    state.upsert_account(AccountState {
        address: reward_address,
        balance: 100_000,
        staked: 0,
        locked: 0,
        nonce: 0,
    });
    runtime
        .save_json(pole_protocol_draft::local_chain_runtime_path(&loaded))
        .unwrap();
    state.store.flush().unwrap();

    let binary = env!("CARGO_BIN_EXE_pole-node");
    let proposal_id =
        "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd".to_string();

    let propose = Command::new(binary)
        .arg("governance-propose-params")
        .arg(&config_path)
        .arg(&proposal_id)
        .arg("2")
        .arg("4")
        .arg("9132")
        .arg("5")
        .arg("180")
        .output()
        .unwrap();
    assert!(propose.status.success());
    let proposal_artifact_path =
        pole_protocol_draft::governance_proposal_artifact_path(&loaded, &proposal_id);
    let index_artifact_path = pole_protocol_draft::governance_index_artifact_path(&loaded);
    assert!(proposal_artifact_path.exists());
    assert!(index_artifact_path.exists());

    let vote = Command::new(binary)
        .arg("governance-vote")
        .arg(&config_path)
        .arg(&proposal_id)
        .arg("yes")
        .arg("25000")
        .output()
        .unwrap();
    assert!(vote.status.success());
    let stdout = String::from_utf8_lossy(&vote.stdout);
    assert!(stdout.contains("scheduled_next_epoch=true"));
    let scheduled_artifact_path =
        pole_protocol_draft::governance_scheduled_artifact_path(&loaded, 2);
    assert!(scheduled_artifact_path.exists());

    let show_proposal = Command::new(binary)
        .arg("governance-show-proposal")
        .arg(&config_path)
        .arg(&proposal_id)
        .output()
        .unwrap();
    assert!(show_proposal.status.success());
    let stdout = String::from_utf8_lossy(&show_proposal.stdout);
    assert!(stdout.contains("proposal_state=Scheduled"));
    assert!(stdout.contains("bond_amount=10000"));
    assert!(stdout.contains("emission_year=4"));
    assert!(stdout.contains("tail_emission_start_year=5"));
    assert!(stdout.contains("tail_emission_rate_bps=180"));
    assert!(stdout.contains("artifact_path="));
    assert!(stdout.contains("artifact_index_path="));

    let show_scheduled = Command::new(binary)
        .arg("governance-show-scheduled")
        .arg(&config_path)
        .arg("2")
        .output()
        .unwrap();
    assert!(show_scheduled.status.success());
    let stdout = String::from_utf8_lossy(&show_scheduled.stdout);
    assert!(stdout.contains("scheduled=true"));
    assert!(stdout.contains("epoch_id=2"));
    assert!(stdout.contains("artifact_path="));
    assert!(stdout.contains("artifact_index_path="));

    assert!(proposal_artifact_path.exists());
    assert!(scheduled_artifact_path.exists());
    assert!(index_artifact_path.exists());
    let index: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&index_artifact_path).unwrap()).unwrap();
    assert!(index["proposal_artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["proposal_id_hex"] == proposal_id));
    assert!(index["scheduled_artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["epoch_id"] == 2 && entry["scheduled"] == true));

    let show_index = Command::new(binary)
        .arg("governance-show-index")
        .arg(&config_path)
        .output()
        .unwrap();
    assert!(show_index.status.success());
    let stdout = String::from_utf8_lossy(&show_index.stdout);
    assert!(stdout.contains("proposal_artifact_count=1"));
    assert!(stdout.contains("scheduled_artifact_count=1"));
    assert!(stdout.contains("proposal_artifact proposal_id="));
    assert!(stdout.contains("scheduled_artifact epoch_id=2 scheduled=true"));

    let show_summary = Command::new(binary)
        .arg("governance-show-summary")
        .arg(&config_path)
        .output()
        .unwrap();
    assert!(show_summary.status.success());
    let stdout = String::from_utf8_lossy(&show_summary.stdout);
    assert!(stdout.contains("scheduled_proposal_count=1"));
    assert!(stdout.contains("proposal_artifact_count=1"));
    assert!(stdout.contains("scheduled_artifact_count=1"));
    assert!(stdout.contains("latest_effective_epoch=Some(2)"));

    let tuning_id = "efefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefef".to_string();
    let tuning = Command::new(binary)
        .arg("governance-propose-reward-tuning")
        .arg(&config_path)
        .arg(&tuning_id)
        .arg("2")
        .arg("1800000000000")
        .arg("1500")
        .arg("25")
        .arg("12000")
        .output()
        .unwrap();
    assert!(tuning.status.success());

    let show_tuning = Command::new(binary)
        .arg("governance-show-proposal")
        .arg(&config_path)
        .arg(&tuning_id)
        .output()
        .unwrap();
    assert!(show_tuning.status.success());
    let stdout = String::from_utf8_lossy(&show_tuning.stdout);
    assert!(stdout.contains("target_network_weight_units=1800000000000"));
    assert!(stdout.contains("reward_adjustment_cap_bps=1500"));
    assert!(stdout.contains("challenge_window_blocks=25"));
    assert!(stdout.contains("effective_player_block_reward=12000"));

    let slow_id = "cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd".to_string();
    let slow = Command::new(binary)
        .arg("governance-propose-slow-params")
        .arg(&config_path)
        .arg(&slow_id)
        .arg("3")
        .arg("7200")
        .arg("15000")
        .output()
        .unwrap();
    assert!(slow.status.success());

    let show_slow = Command::new(binary)
        .arg("governance-show-proposal")
        .arg(&config_path)
        .arg(&slow_id)
        .output()
        .unwrap();
    assert!(show_slow.status.success());
    let stdout = String::from_utf8_lossy(&show_slow.stdout);
    assert!(stdout.contains("reward_block_secs=7200"));
    assert!(stdout.contains("effective_player_block_reward=15000"));

    let retention_id =
        "cececececececececececececececececececececececececececececececece".to_string();
    let retention = Command::new(binary)
        .arg("governance-propose-retention")
        .arg(&config_path)
        .arg(&retention_id)
        .arg("3")
        .arg("4")
        .arg("30")
        .output()
        .unwrap();
    assert!(retention.status.success());

    let show_retention = Command::new(binary)
        .arg("governance-show-proposal")
        .arg(&config_path)
        .arg(&retention_id)
        .output()
        .unwrap();
    assert!(show_retention.status.success());
    let stdout = String::from_utf8_lossy(&show_retention.stdout);
    assert!(stdout.contains("min_retention_epochs=4"));
    assert!(stdout.contains("challenge_window_blocks=30"));

    let app_weight_id =
        "dededededededededededededededededededededededededededededededede".to_string();
    let app_weight = Command::new(binary)
        .arg("governance-propose-app-weight")
        .arg(&config_path)
        .arg(&app_weight_id)
        .arg("3")
        .arg("730")
        .arg("850000")
        .output()
        .unwrap();
    assert!(app_weight.status.success());

    let show_app_weight = Command::new(binary)
        .arg("governance-show-proposal")
        .arg(&config_path)
        .arg(&app_weight_id)
        .output()
        .unwrap();
    assert!(show_app_weight.status.success());
    let stdout = String::from_utf8_lossy(&show_app_weight.stdout);
    assert!(stdout.contains("app_weight_override_count=1"));
    assert!(stdout.contains("app_weight_override app_id=730 game_coefficient_ppm=850000"));

    let tier_id = "dfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdfdf".to_string();
    let tier = Command::new(binary)
        .arg("governance-propose-tier-weights")
        .arg(&config_path)
        .arg(&tier_id)
        .arg("2")
        .arg("950000")
        .arg("250000")
        .arg("550000")
        .arg("60000")
        .arg("160000")
        .output()
        .unwrap();
    assert!(tier.status.success());

    let show_tier = Command::new(binary)
        .arg("governance-show-proposal")
        .arg(&config_path)
        .arg(&tier_id)
        .output()
        .unwrap();
    assert!(show_tier.status.success());
    let stdout = String::from_utf8_lossy(&show_tier.stdout);
    assert!(stdout.contains("tier1_weight_ppm=950000"));
    assert!(stdout.contains("tier2_weight_min_ppm=250000"));
    assert!(stdout.contains("tier2_weight_max_ppm=550000"));
    assert!(stdout.contains("tier3_weight_min_ppm=60000"));
    assert!(stdout.contains("tier3_weight_max_ppm=160000"));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn pole_node_libp2p_loop_reports_running_state() {
    let config_path =
        std::env::temp_dir().join(format!("pole-node-libp2p-loop-{}.json", std::process::id()));
    let mut config = NodeConfig::default();
    config.runtime.p2p_libp2p.enabled = true;
    config.runtime.p2p_libp2p.listen_addrs = vec!["/ip4/127.0.0.1/tcp/4001".into()];
    config.runtime.p2p_libp2p.bootstrap_peers =
        vec![pole_protocol_draft::P2pLibp2pBootstrapPeerConfig {
            peer_id: "12D3KooWJ5Z5L6hG1Zq1x3wQ5P5ZkJ7V3xZ6QYp6iYvJpR6J8W8J".into(),
            addr: "/ip4/127.0.0.1/tcp/4002".into(),
        }];
    config.save_json(&config_path).unwrap();

    let binary = env!("CARGO_BIN_EXE_pole-node");
    let output = Command::new(binary)
        .arg("libp2p-loop")
        .arg(&config_path)
        .arg("3")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ticks_completed=3"));
    assert!(stdout.contains("phase=Running"));

    let _ = std::fs::remove_file(config_path);
}

#[test]
fn pole_node_service_commands_are_exposed() {
    let root = std::env::temp_dir().join(format!("pole-node-service-cmds-{}", std::process::id()));
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("node.json");
    NodeConfig::default().save_json(&config_path).unwrap();
    let binary = env!("CARGO_BIN_EXE_pole-node");

    #[cfg(not(windows))]
    let unit_root = root.join("systemd");
    #[cfg(windows)]
    let service_root = root.join("windows-service");
    #[cfg(not(windows))]
    let control_binary = root.join("systemctl.cmd");
    #[cfg(windows)]
    let control_binary = root.join("sc.cmd");
    let control_log = root.join("control.log");

    #[cfg(not(windows))]
    std::fs::write(
        &control_binary,
        format!(
            "@echo off\r\necho %*>>\"{}\"\r\nexit /b 0\r\n",
            control_log.display()
        ),
    )
    .unwrap();
    #[cfg(windows)]
    std::fs::write(
        &control_binary,
        format!(
            "@echo off\r\necho %*>>\"{}\"\r\nexit /b 0\r\n",
            control_log.display()
        ),
    )
    .unwrap();

    let mut status_command = Command::new(binary);
    status_command.arg("service-status").arg(&config_path);
    #[cfg(not(windows))]
    {
        status_command.env("POLE_SYSTEMD_UNIT_ROOT", &unit_root);
        status_command.env("POLE_SYSTEMCTL_BINARY", &control_binary);
    }
    #[cfg(windows)]
    {
        status_command.env("POLE_WINDOWS_SERVICE_ROOT", &service_root);
        status_command.env("POLE_WINDOWS_SC_BINARY", &control_binary);
    }
    let status = status_command.output().unwrap();
    assert!(
        status.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&status.stdout),
        String::from_utf8_lossy(&status.stderr)
    );
    let stdout = String::from_utf8_lossy(&status.stdout);
    assert!(stdout.contains("service_name="));
    #[cfg(windows)]
    {
        assert!(stdout.contains("service_status=NotInstalled"));
        assert!(stdout.contains(&format!(
            "service_registration_path={}",
            service_root.join("PoLENode.service.json").to_string_lossy()
        )));
    }
    #[cfg(not(windows))]
    {
        assert!(stdout.contains("service_status=NotInstalled"));
        assert!(stdout.contains(&format!(
            "service_unit_path={}",
            unit_root.join("pole-node.service").to_string_lossy()
        )));
    }

    let mut install_command = Command::new(binary);
    install_command.arg("service-install").arg(&config_path);
    #[cfg(not(windows))]
    {
        install_command.env("POLE_SYSTEMD_UNIT_ROOT", &unit_root);
        install_command.env("POLE_SYSTEMCTL_BINARY", &control_binary);
    }
    #[cfg(windows)]
    {
        install_command.env("POLE_WINDOWS_SERVICE_ROOT", &service_root);
        install_command.env("POLE_WINDOWS_SC_BINARY", &control_binary);
    }
    let install = install_command.output().unwrap();
    assert!(install.status.success());
    let stdout = String::from_utf8_lossy(&install.stdout);
    #[cfg(windows)]
    {
        assert!(stdout.contains("service_install_supported=true"));
        assert!(service_root.join("PoLENode.service.json").exists());
    }
    #[cfg(not(windows))]
    {
        assert!(stdout.contains("service_install_supported=true"));
        assert!(unit_root.join("pole-node.service").exists());
    }

    let run = Command::new(binary)
        .arg("service-run")
        .arg(&config_path)
        .output()
        .unwrap();
    assert!(run.status.success());
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("service_mode=true"));

    let mut start_command = Command::new(binary);
    start_command.arg("service-start").arg(&config_path);
    #[cfg(not(windows))]
    {
        start_command.env("POLE_SYSTEMD_UNIT_ROOT", &unit_root);
        start_command.env("POLE_SYSTEMCTL_BINARY", &control_binary);
    }
    #[cfg(windows)]
    {
        start_command.env("POLE_WINDOWS_SERVICE_ROOT", &service_root);
        start_command.env("POLE_WINDOWS_SC_BINARY", &control_binary);
    }
    let start = start_command.output().unwrap();
    assert!(start.status.success());
    let stdout = String::from_utf8_lossy(&start.stdout);
    assert!(stdout.contains("service_start_supported=true"));

    let mut stop_command = Command::new(binary);
    stop_command.arg("service-stop").arg(&config_path);
    #[cfg(not(windows))]
    {
        stop_command.env("POLE_SYSTEMD_UNIT_ROOT", &unit_root);
        stop_command.env("POLE_SYSTEMCTL_BINARY", &control_binary);
    }
    #[cfg(windows)]
    {
        stop_command.env("POLE_WINDOWS_SERVICE_ROOT", &service_root);
        stop_command.env("POLE_WINDOWS_SC_BINARY", &control_binary);
    }
    let stop = stop_command.output().unwrap();
    assert!(stop.status.success());
    let stdout = String::from_utf8_lossy(&stop.stdout);
    assert!(stdout.contains("service_stop_supported=true"));

    let control_log_contents = std::fs::read_to_string(&control_log).unwrap();
    #[cfg(windows)]
    {
        assert!(control_log_contents.contains("start PoLENode"));
        assert!(control_log_contents.contains("stop PoLENode"));
    }
    #[cfg(not(windows))]
    {
        assert!(control_log_contents.contains("start pole-node.service"));
        assert!(control_log_contents.contains("stop pole-node.service"));
    }

    #[cfg(not(windows))]
    {
        let mut uninstall_command = Command::new(binary);
        uninstall_command
            .arg("service-uninstall")
            .arg(&config_path)
            .env("POLE_SYSTEMD_UNIT_ROOT", &unit_root)
            .env("POLE_SYSTEMCTL_BINARY", &control_binary);
        let uninstall = uninstall_command.output().unwrap();
        assert!(uninstall.status.success());
        let stdout = String::from_utf8_lossy(&uninstall.stdout);
        assert!(stdout.contains("service_uninstall_supported=true"));
        assert!(!unit_root.join("pole-node.service").exists());
    }
    #[cfg(windows)]
    {
        let mut uninstall_command = Command::new(binary);
        uninstall_command
            .arg("service-uninstall")
            .arg(&config_path)
            .env("POLE_WINDOWS_SERVICE_ROOT", &service_root)
            .env("POLE_WINDOWS_SC_BINARY", &control_binary);
        let uninstall = uninstall_command.output().unwrap();
        assert!(uninstall.status.success());
        let stdout = String::from_utf8_lossy(&uninstall.stdout);
        assert!(stdout.contains("service_uninstall_supported=true"));
        assert!(!service_root.join("PoLENode.service.json").exists());
    }

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "requires live Steam API access"]
fn steam_live_fetch_returns_real_current_players() {
    let client = ReqwestHttpTextClient;
    let observed_at = 1_900_000_000_000;
    let sample = fetch_current_players_with_client(&client, 730, observed_at).unwrap();

    assert_eq!(sample.app_id, 730);
    assert_eq!(sample.observed_at_millis, observed_at);
    assert!(sample.observed_players > 1000);
    assert!(sample.raw_body.contains("player_count"));
}

#[test]
fn steam_url_and_hex_helpers_are_stable() {
    assert_eq!(
        current_players_url(730),
        "https://api.steampowered.com/ISteamUserStats/GetNumberOfCurrentPlayers/v1/?appid=730"
    );
    assert_eq!(hex_32([0xab; 32]), "ab".repeat(32));
}

#[test]
fn pole_node_status_prints_last_p2p_propagation_state() {
    let root = std::env::temp_dir().join(format!("pole-node-status-p2p-{}", std::process::id()));
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("node.json");
    let mut config = NodeConfig::default();
    config.runtime.data_dir = root.join("pole-node-data").to_string_lossy().into_owned();
    config.capabilities.store = true;
    config.runtime.poll_interval_secs = 0;
    config.runtime.p2p_simulation = P2pSimulationConfig {
        batch_listener_count: 2,
        receipt_listener_count: 1,
        dual_listener_count: 1,
    };
    config.save_json(&config_path).unwrap();

    let mut progress = LocalNodeProgress::default_from_config(&config);
    let client = FakeHttpClient {
        body: r#"{"response":{"player_count":654321,"result":1}}"#.to_string(),
    };
    let mut network = build_inmemory_simulation_network(config.runtime.p2p_simulation);
    run_collect_tick_with_client_and_network(&config, &mut progress, &client, &mut network)
        .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_pole-node"))
        .arg("status")
        .arg(&config_path)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("configured_p2p_batch_listeners=2"));
    assert!(stdout.contains("configured_p2p_receipt_listeners=1"));
    assert!(stdout.contains("configured_p2p_dual_listeners=1"));
    assert!(stdout.contains("last_p2p_batch_recipients=Some(3)"));
    assert!(stdout.contains("last_p2p_receipt_recipients=Some(2)"));
    assert!(stdout.contains("last_p2p_retrieval_ok=Some(true)"));
    assert!(stdout.contains("last_p2p_retrieval_error=None"));
    assert!(stdout.contains("last_p2p_transport=Some(\"inmemory-sim\")"));
    assert!(stdout.contains("last_p2p_known_peer_count=Some(5)"));
    assert!(stdout.contains("last_p2p_learned_remote_peer_count=Some(0)"));
    assert!(stdout.contains("last_p2p_batch_listener_count=Some(3)"));
    assert!(stdout.contains("last_p2p_receipt_listener_count=Some(2)"));
    assert!(stdout.contains("last_p2p_challenge_listener_count=Some(0)"));
    assert!(stdout.contains("last_p2p_coordination_sent_count=Some(0)"));
    assert!(stdout.contains("last_p2p_coordination_received_count=Some(0)"));
    assert!(stdout.contains("last_p2p_hello_sent_count=Some(0)"));
    assert!(stdout.contains("last_p2p_hint_sent_count=Some(0)"));
    assert!(stdout.contains("last_p2p_goodbye_sent_count=Some(0)"));
    assert!(stdout.contains("last_p2p_challenge_recipients=None"));
    assert!(stdout.contains("p2p_challenge_events_total=Some(0)"));
    assert!(stdout.contains("p2p_bad_batch_challenge_events=Some(0)"));
    assert!(stdout.contains("p2p_omission_challenge_events=Some(0)"));
    assert!(stdout.contains("p2p_bad_aggregate_challenge_events=Some(0)"));
    assert!(stdout.contains("p2p_bad_reward_challenge_events=Some(0)"));
    assert!(stdout.contains("p2p_bad_storage_challenge_events=Some(0)"));
    assert!(stdout.contains("recent_p2p_challenge_events=Some(0)"));
    assert!(stdout.contains("recent_p2p_bad_batch_challenge_events=Some(0)"));
    assert!(stdout.contains("recent_p2p_omission_challenge_events=Some(0)"));
    assert!(stdout.contains("recent_p2p_bad_aggregate_challenge_events=Some(0)"));
    assert!(stdout.contains("recent_p2p_bad_reward_challenge_events=Some(0)"));
    assert!(stdout.contains("recent_p2p_bad_storage_challenge_events=Some(0)"));
    assert!(stdout.contains("p2p_challenge_delivered_events_total=Some(0)"));
    assert!(stdout.contains("p2p_challenge_zero_recipient_events_total=Some(0)"));
    assert!(stdout.contains("recent_p2p_challenge_delivered_events=Some(0)"));
    assert!(stdout.contains("recent_p2p_challenge_zero_recipient_events=Some(0)"));
    assert!(stdout.contains("recent_p2p_challenge_recipient_sum=Some(0)"));
    assert!(stdout.contains("last_retention_all_retrievable=Some(true)"));
    assert!(stdout.contains("last_retention_retained_payload_count=Some(1)"));
    assert!(stdout.contains("last_retention_retrievable_payload_count=Some(1)"));
    assert!(stdout.contains("last_retention_missing_payload_count=Some(0)"));
    assert!(stdout.contains("last_retention_corrupted_payload_count=Some(0)"));
    assert!(stdout.contains("last_storage_challenge_all_passed=Some(true)"));
    assert!(stdout.contains("last_storage_challenge_checked_payload_count=Some(1)"));
    assert!(stdout.contains("last_storage_challenge_failed_payload_count=Some(0)"));
    assert!(stdout.contains("last_storage_challenge_error=None"));

    let show_index = Command::new(env!("CARGO_BIN_EXE_pole-node"))
        .arg("reward-adjustment-show-index")
        .arg(&config_path)
        .output()
        .unwrap();
    assert!(show_index.status.success());
    let stdout = String::from_utf8_lossy(&show_index.stdout);
    assert!(stdout.contains("adjustment_artifact_count=1"));
    assert!(stdout.contains("adjustment_cycle_artifact_count=1"));
    assert!(stdout.contains("adjustment_artifact period_index=0"));
    assert!(stdout.contains("adjustment_cycle cycle_index=0"));

    let show_summary = Command::new(env!("CARGO_BIN_EXE_pole-node"))
        .arg("reward-adjustment-show-summary")
        .arg(&config_path)
        .output()
        .unwrap();
    assert!(show_summary.status.success());
    let stdout = String::from_utf8_lossy(&show_summary.stdout);
    assert!(stdout.contains("adjustment_artifact_count=1"));
    assert!(stdout.contains("adjustment_cycle_artifact_count=1"));
    assert!(stdout.contains("latest_period_index=Some(0)"));
    assert!(stdout.contains("latest_adjustment_cycle_index=Some(0)"));
    assert!(stdout.contains("latest_adjusted_player_block_reward=Some(1000)"));
    assert!(stdout.contains("latest_fixed_player_reward=Some(1000)"));

    let alias_index = Command::new(env!("CARGO_BIN_EXE_pole-node"))
        .arg("adjustment-cycle-show-index")
        .arg(&config_path)
        .output()
        .unwrap();
    assert!(alias_index.status.success());
    let stdout = String::from_utf8_lossy(&alias_index.stdout);
    assert!(stdout.contains("adjustment_cycle cycle_index=0"));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn pole_node_status_reports_last_challenge_activity_summary() {
    let root =
        std::env::temp_dir().join(format!("pole-node-status-challenge-{}", std::process::id()));
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("node.json");
    let mut config = NodeConfig::default();
    config.runtime.data_dir = root.join("pole-node-data").to_string_lossy().into_owned();
    config.capabilities.store = true;
    config.capabilities.verify = false;
    config.capabilities.propose = false;
    config.runtime.poll_interval_secs = 0;
    config.runtime.slots_per_epoch = 10;
    config.storage.retention_epochs = 3;
    config.runtime.p2p_simulation = P2pSimulationConfig {
        batch_listener_count: 1,
        receipt_listener_count: 1,
        dual_listener_count: 1,
    };
    config.save_json(&config_path).unwrap();

    let mut progress = LocalNodeProgress::default_from_config(&config);
    let client = FakeHttpClient {
        body: r#"{"response":{"player_count":654321,"result":1}}"#.to_string(),
    };
    let mut network = build_inmemory_simulation_network(config.runtime.p2p_simulation);
    let challenge_listener = [0x89; 32];
    network.register_peer(challenge_listener);
    network
        .subscribe(challenge_listener, P2pTopic::Challenges)
        .unwrap();
    let first =
        run_collect_tick_with_client_and_network(&config, &mut progress, &client, &mut network)
            .unwrap();
    let ledger_path = retention_book_path(&config);
    let mut retention_book =
        pole_protocol_draft::LocalRetentionBook::load_or_default_json(&ledger_path, 1).unwrap();
    let record = retention_book
        .payloads
        .get_mut(&first.artifact.payload_cid)
        .unwrap();
    record.receipt.receipt_signature = vec![0u8; 32];
    retention_book.save_json(&ledger_path).unwrap();
    run_collect_tick_with_client_and_network(&config, &mut progress, &client, &mut network)
        .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_pole-node"))
        .arg("status")
        .arg(&config_path)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("last_p2p_challenge_recipients=Some(1)"));
    assert!(stdout.contains("last_p2p_challenge_kind=Some(\"BadStorage\")"));
    assert!(stdout.contains("last_p2p_challenge_epoch_id=Some(1)"));
    assert!(stdout.contains(&format!(
        "last_p2p_challenge_payload_cid=Some(\"{}\")",
        first.artifact.payload_cid
    )));
    assert!(stdout.contains("p2p_challenge_events_total=Some(1)"));
    assert!(stdout.contains("p2p_bad_batch_challenge_events=Some(0)"));
    assert!(stdout.contains("p2p_omission_challenge_events=Some(0)"));
    assert!(stdout.contains("p2p_bad_aggregate_challenge_events=Some(0)"));
    assert!(stdout.contains("p2p_bad_reward_challenge_events=Some(0)"));
    assert!(stdout.contains("p2p_bad_storage_challenge_events=Some(1)"));
    assert!(stdout.contains("recent_p2p_challenge_events=Some(1)"));
    assert!(stdout.contains("recent_p2p_bad_batch_challenge_events=Some(0)"));
    assert!(stdout.contains("recent_p2p_omission_challenge_events=Some(0)"));
    assert!(stdout.contains("recent_p2p_bad_aggregate_challenge_events=Some(0)"));
    assert!(stdout.contains("recent_p2p_bad_reward_challenge_events=Some(0)"));
    assert!(stdout.contains("recent_p2p_bad_storage_challenge_events=Some(1)"));
    assert!(stdout.contains("p2p_challenge_delivered_events_total=Some(1)"));
    assert!(stdout.contains("p2p_challenge_zero_recipient_events_total=Some(0)"));
    assert!(stdout.contains("recent_p2p_challenge_delivered_events=Some(1)"));
    assert!(stdout.contains("recent_p2p_challenge_zero_recipient_events=Some(0)"));
    assert!(stdout.contains("recent_p2p_challenge_recipient_sum=Some(1)"));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn pole_node_run_once_p2p_fs_reports_filesystem_transport_summary() {
    let root = std::env::temp_dir().join(format!("pole-node-run-once-fs-{}", std::process::id()));
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("node.json");
    let network_dir = root.join("network");
    let mut config = NodeConfig::default();
    config.runtime.data_dir = root.join("pole-node-data").to_string_lossy().into_owned();
    config.capabilities.store = true;
    config.runtime.poll_interval_secs = 0;
    config.runtime.slots_per_epoch = 2;
    config.save_json(&config_path).unwrap();

    let mut seed = FilesystemP2pNetwork::new(&network_dir);
    seed.bootstrap_peer([0x66; 32], &[P2pTopic::Batches, P2pTopic::Receipts])
        .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_pole-node"))
        .arg("run-once-p2p-fs")
        .arg(&config_path)
        .arg(&network_dir)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("p2p_simulation_enabled=true"));
    assert!(stdout.contains("p2p_topology_peers=2"));
    assert!(stdout.contains("p2p_batch_recipients=1"));
    assert!(stdout.contains("p2p_receipt_recipients=1"));
    assert!(stdout.contains("p2p_payload_retrieval_ok=true"));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn pole_node_run_loop_p2p_fs_reports_filesystem_transport_summary() {
    let root = std::env::temp_dir().join(format!("pole-node-run-loop-fs-{}", std::process::id()));
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("node.json");
    let network_dir = root.join("network");
    let mut config = NodeConfig::default();
    config.runtime.data_dir = root.join("pole-node-data").to_string_lossy().into_owned();
    config.capabilities.store = true;
    config.runtime.poll_interval_secs = 0;
    config.runtime.slots_per_epoch = 2;
    config.save_json(&config_path).unwrap();

    let mut seed = FilesystemP2pNetwork::new(&network_dir);
    seed.bootstrap_peer([0x67; 32], &[P2pTopic::Batches, P2pTopic::Receipts])
        .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_pole-node"))
        .arg("run-loop-p2p-fs")
        .arg(&config_path)
        .arg(&network_dir)
        .arg("2")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ticks_completed=2"));
    assert!(stdout.contains("p2p_simulation_enabled=true"));
    assert!(stdout.contains("p2p_topology_peers=2"));
    assert!(stdout.contains("p2p_batch_recipients=2"));
    assert!(stdout.contains("p2p_receipt_recipients=2"));
    assert!(stdout.contains("p2p_payload_retrieval_ok=true"));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn pole_node_run_once_p2p_socket_reports_socket_transport_summary() {
    let root =
        std::env::temp_dir().join(format!("pole-node-run-once-socket-{}", std::process::id()));
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("node.json");
    let mut config = NodeConfig::default();
    config.runtime.data_dir = root.join("pole-node-data").to_string_lossy().into_owned();
    config.capabilities.store = true;
    config.runtime.poll_interval_secs = 0;
    config.runtime.slots_per_epoch = 2;
    config.save_json(&config_path).unwrap();

    let sink = [0x70; 32];
    let mut sink_network =
        SocketP2pNetwork::bind(sink, "127.0.0.1:0".parse().unwrap(), vec![]).unwrap();
    sink_network
        .bootstrap_peer(sink, &[P2pTopic::Batches, P2pTopic::Receipts])
        .unwrap();
    let sink_addr = sink_network.local_addr().unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_pole-node"))
        .arg("run-once-p2p-socket")
        .arg(&config_path)
        .arg("127.0.0.1:0")
        .arg(pole_protocol_draft::hex_32(sink))
        .arg(sink_addr.to_string())
        .arg("batches,receipts")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("p2p_simulation_enabled=true"));
    assert!(stdout.contains("p2p_topology_peers=2"));
    assert!(stdout.contains("p2p_batch_recipients=1"));
    assert!(stdout.contains("p2p_receipt_recipients=1"));
    assert!(stdout.contains("p2p_payload_retrieval_ok=true"));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn pole_node_run_once_p2p_socket_can_use_config_backed_peer_specs() {
    let root = std::env::temp_dir().join(format!(
        "pole-node-run-once-socket-config-{}",
        std::process::id()
    ));
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("node.json");
    let mut config = NodeConfig::default();
    config.runtime.data_dir = root.join("pole-node-data").to_string_lossy().into_owned();
    config.capabilities.store = true;
    config.runtime.poll_interval_secs = 0;
    config.runtime.slots_per_epoch = 2;

    let sink = [0x78; 32];
    let mut sink_network =
        SocketP2pNetwork::bind(sink, "127.0.0.1:0".parse().unwrap(), vec![]).unwrap();
    sink_network
        .bootstrap_peer(sink, &[P2pTopic::Batches, P2pTopic::Receipts])
        .unwrap();
    let sink_addr = sink_network.local_addr().unwrap();
    config.runtime.p2p_socket.bind_addr = "127.0.0.1:0".into();
    config.runtime.p2p_socket.peers = vec![pole_protocol_draft::P2pSocketPeerConfig {
        peer_id_hex: pole_protocol_draft::hex_32(sink),
        addr: sink_addr.to_string(),
        topics: vec!["batches".into(), "receipts".into()],
    }];
    config.save_json(&config_path).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_pole-node"))
        .arg("run-once-p2p-socket")
        .arg(&config_path)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("p2p_topology_peers=2"));
    assert!(stdout.contains("p2p_batch_recipients=1"));
    assert!(stdout.contains("p2p_payload_retrieval_ok=true"));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn pole_node_run_loop_p2p_socket_reports_socket_transport_summary() {
    let root =
        std::env::temp_dir().join(format!("pole-node-run-loop-socket-{}", std::process::id()));
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("node.json");
    let mut config = NodeConfig::default();
    config.runtime.data_dir = root.join("pole-node-data").to_string_lossy().into_owned();
    config.capabilities.store = true;
    config.runtime.poll_interval_secs = 0;
    config.runtime.slots_per_epoch = 2;
    config.save_json(&config_path).unwrap();

    let sink = [0x71; 32];
    let mut sink_network =
        SocketP2pNetwork::bind(sink, "127.0.0.1:0".parse().unwrap(), vec![]).unwrap();
    sink_network
        .bootstrap_peer(sink, &[P2pTopic::Batches, P2pTopic::Receipts])
        .unwrap();
    let sink_addr = sink_network.local_addr().unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_pole-node"))
        .arg("run-loop-p2p-socket")
        .arg(&config_path)
        .arg("127.0.0.1:0")
        .arg(pole_protocol_draft::hex_32(sink))
        .arg(sink_addr.to_string())
        .arg("batches,receipts")
        .arg("2")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ticks_completed=2"));
    assert!(stdout.contains("p2p_simulation_enabled=true"));
    assert!(stdout.contains("p2p_topology_peers=2"));
    assert!(stdout.contains("p2p_batch_recipients=2"));
    assert!(stdout.contains("p2p_receipt_recipients=2"));
    assert!(stdout.contains("p2p_payload_retrieval_ok=true"));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn pole_node_run_once_p2p_socket_multi_reports_multi_peer_summary() {
    let root = std::env::temp_dir().join(format!(
        "pole-node-run-once-socket-multi-{}",
        std::process::id()
    ));
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("node.json");
    let mut config = NodeConfig::default();
    config.runtime.data_dir = root.join("pole-node-data").to_string_lossy().into_owned();
    config.capabilities.store = true;
    config.runtime.poll_interval_secs = 0;
    config.runtime.slots_per_epoch = 2;
    config.save_json(&config_path).unwrap();

    let sink_a = [0x72; 32];
    let sink_b = [0x73; 32];
    let mut sink_network_a =
        SocketP2pNetwork::bind(sink_a, "127.0.0.1:0".parse().unwrap(), vec![]).unwrap();
    sink_network_a
        .bootstrap_peer(sink_a, &[P2pTopic::Batches, P2pTopic::Receipts])
        .unwrap();
    let sink_addr_a = sink_network_a.local_addr().unwrap();
    let mut sink_network_b =
        SocketP2pNetwork::bind(sink_b, "127.0.0.1:0".parse().unwrap(), vec![]).unwrap();
    sink_network_b
        .bootstrap_peer(sink_b, &[P2pTopic::Batches, P2pTopic::Receipts])
        .unwrap();
    let sink_addr_b = sink_network_b.local_addr().unwrap();

    let peer_specs = format!(
        "{}@{}@batches,receipts;{}@{}@batches,receipts",
        pole_protocol_draft::hex_32(sink_a),
        sink_addr_a,
        pole_protocol_draft::hex_32(sink_b),
        sink_addr_b
    );

    let output = Command::new(env!("CARGO_BIN_EXE_pole-node"))
        .arg("run-once-p2p-socket")
        .arg(&config_path)
        .arg("127.0.0.1:0")
        .arg(&peer_specs)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("p2p_simulation_enabled=true"));
    assert!(stdout.contains("p2p_topology_peers=3"));
    assert!(stdout.contains("p2p_batch_recipients=2"));
    assert!(stdout.contains("p2p_receipt_recipients=2"));
    assert!(stdout.contains("p2p_payload_retrieval_ok=true"));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn pole_node_run_loop_p2p_socket_multi_reports_multi_peer_summary() {
    let root = std::env::temp_dir().join(format!(
        "pole-node-run-loop-socket-multi-{}",
        std::process::id()
    ));
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();

    let config_path = root.join("node.json");
    let mut config = NodeConfig::default();
    config.runtime.data_dir = root.join("pole-node-data").to_string_lossy().into_owned();
    config.capabilities.store = true;
    config.runtime.poll_interval_secs = 0;
    config.runtime.slots_per_epoch = 2;
    config.save_json(&config_path).unwrap();

    let sink_a = [0x74; 32];
    let sink_b = [0x75; 32];
    let mut sink_network_a =
        SocketP2pNetwork::bind(sink_a, "127.0.0.1:0".parse().unwrap(), vec![]).unwrap();
    sink_network_a
        .bootstrap_peer(sink_a, &[P2pTopic::Batches, P2pTopic::Receipts])
        .unwrap();
    let sink_addr_a = sink_network_a.local_addr().unwrap();
    let mut sink_network_b =
        SocketP2pNetwork::bind(sink_b, "127.0.0.1:0".parse().unwrap(), vec![]).unwrap();
    sink_network_b
        .bootstrap_peer(sink_b, &[P2pTopic::Batches, P2pTopic::Receipts])
        .unwrap();
    let sink_addr_b = sink_network_b.local_addr().unwrap();

    let peer_specs = format!(
        "{}@{}@batches,receipts;{}@{}@batches,receipts",
        pole_protocol_draft::hex_32(sink_a),
        sink_addr_a,
        pole_protocol_draft::hex_32(sink_b),
        sink_addr_b
    );

    let output = Command::new(env!("CARGO_BIN_EXE_pole-node"))
        .arg("run-loop-p2p-socket")
        .arg(&config_path)
        .arg("127.0.0.1:0")
        .arg(&peer_specs)
        .arg("2")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ticks_completed=2"));
    assert!(stdout.contains("p2p_simulation_enabled=true"));
    assert!(stdout.contains("p2p_topology_peers=3"));
    assert!(stdout.contains("p2p_batch_recipients=4"));
    assert!(stdout.contains("p2p_receipt_recipients=4"));
    assert!(stdout.contains("p2p_payload_retrieval_ok=true"));

    std::fs::remove_dir_all(root).unwrap();
}
