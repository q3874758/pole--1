use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use pole_protocol_draft::{
    aggregate_local_epoch, audit_local_retention, batch_artifact_path,
    build_epoch_commit_from_local_data, build_inmemory_simulation_network,
    effective_collect_interval_secs, epoch_aggregation_artifact_path, epoch_commit_artifact_path,
    epoch_preparation_artifact_path, epoch_reward_artifact_path, epoch_settlement_artifact_path,
    epoch_verification_artifact_path, inmemory_simulation_listener_peer_ids,
    inmemory_simulation_retrieval_peer_id, load_status, local_chain_runtime_path,
    node_daemon::local_chain_store_path, payload_path, player_reward_tick_artifact_path,
    prepare_local_epoch, progress_path, prune_retention, retention_audit_artifact_path,
    retention_book_path, reward_adjustment_artifact_path, reward_adjustment_index_path,
    reward_adjustment_summary_path, run_collect_loop_with_client, run_collect_tick_with_client,
    run_collect_tick_with_client_and_network, settle_local_epoch, summarize_auto_settlement,
    verify_local_epoch, ActivitySourceKind, BatchBuilder, CapabilityConfig, CollectConfig,
    CollectTickArtifact, HttpTextClient, InMemoryP2pNetwork, LocalNodeProgress, LocalRetentionBook,
    NodeConfig, ObservationRecord, P2pMessage, P2pSimulationConfig, P2pTopic, PersistentStoreStub,
    ProtocolParams, ProtocolStore, RewardAdjustmentArtifact, RewardAdjustmentArtifactIndex,
    RewardAdjustmentArtifactSummary, RewardConfig, RewardGameMapping, RuntimeConfig,
    SteamCollectorError, StorageConfig,
};

struct FixedHttpClient;

impl HttpTextClient for FixedHttpClient {
    fn get_text(&self, url: &str) -> Result<String, SteamCollectorError> {
        if url.contains("example.invalid/epic") {
            return Ok("{\"player_count\":1234,\"confidence_ppm\":450000}".into());
        }
        let app_id = url.split("appid=").nth(1).unwrap_or("0");
        let player_count = match app_id {
            "730" => 500_000,
            "570" => 300_000,
            _ => 1_000,
        };
        Ok(format!(
            "{{\"response\":{{\"player_count\":{player_count},\"result\":1}}}}"
        ))
    }
}

struct RuntimeCorruptingHttpClient {
    runtime_path: PathBuf,
    call_count: AtomicUsize,
    corrupt_on_call: usize,
}

impl RuntimeCorruptingHttpClient {
    fn new(runtime_path: PathBuf, corrupt_on_call: usize) -> Self {
        Self {
            runtime_path,
            call_count: AtomicUsize::new(0),
            corrupt_on_call,
        }
    }
}

impl HttpTextClient for RuntimeCorruptingHttpClient {
    fn get_text(&self, url: &str) -> Result<String, SteamCollectorError> {
        let call = self.call_count.fetch_add(1, Ordering::SeqCst) + 1;
        if call == self.corrupt_on_call {
            if let Some(parent) = self.runtime_path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&self.runtime_path, "{not-json").unwrap();
        }

        let app_id = url.split("appid=").nth(1).unwrap_or("0");
        let player_count = match app_id {
            "730" => 500_000,
            "570" => 300_000,
            _ => 1_000,
        };
        Ok(format!(
            "{{\"response\":{{\"player_count\":{player_count},\"result\":1}}}}"
        ))
    }
}

struct SequencedHttpClient {
    counts: Vec<u64>,
    call_count: AtomicUsize,
}

impl SequencedHttpClient {
    fn new(counts: Vec<u64>) -> Self {
        Self {
            counts,
            call_count: AtomicUsize::new(0),
        }
    }
}

impl HttpTextClient for SequencedHttpClient {
    fn get_text(&self, url: &str) -> Result<String, SteamCollectorError> {
        let app_id = url.split("appid=").nth(1).unwrap_or("0");
        let index = self.call_count.fetch_add(1, Ordering::SeqCst);
        let player_count = if app_id == "1" {
            self.counts
                .get(index)
                .copied()
                .or_else(|| self.counts.last().copied())
                .unwrap_or(1_000)
        } else {
            1_000
        };
        Ok(format!(
            "{{\"response\":{{\"player_count\":{player_count},\"result\":1}}}}"
        ))
    }
}

fn temp_data_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("pole-{name}-{}", std::process::id()))
}

fn foreground_override_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn test_config(name: &str) -> NodeConfig {
    NodeConfig {
        chain_id: "pole-local".into(),
        node_id_hex: pole_protocol_draft::hex_32([0x31; 32]),
        reward_address_hex: pole_protocol_draft::hex_32([0x41; 32]),
        capabilities: CapabilityConfig {
            collect: true,
            store: true,
            verify: true,
            propose: true,
            archive: false,
        },
        collect: CollectConfig {
            enabled: true,
            default_epoch_id: 1,
            default_slot_id: 1,
        },
        runtime: RuntimeConfig {
            data_dir: temp_data_dir(name).to_string_lossy().into_owned(),
            poll_interval_secs: 0,
            slots_per_epoch: 2,
            challenge_window_blocks: 20,
            low_impact_mode: false,
            os_background_priority: false,
            game_active_poll_interval_secs: 0,
            game_process_names: Vec::new(),
            target_app_ids: vec![730, 570],
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

fn persist_manual_batch(
    config: &NodeConfig,
    epoch_id: u64,
    slot_id: u64,
    collector_byte: u8,
    app_id: u32,
    observed_players: u64,
    observed_at_millis: u64,
) -> pole_protocol_draft::AssembledBatch {
    let collector_id = [collector_byte; 32];
    let raw_body = format!(
        "{{\"collector\":{collector_byte},\"slot\":{slot_id},\"players\":{observed_players}}}"
    );
    let observation = ObservationRecord {
        epoch_id,
        slot_id,
        app_id,
        source_kind: ActivitySourceKind::Steam,
        source_confidence_ppm: 1_000_000,
        observed_players,
        observed_at_millis,
        collector_id,
        raw_body_cid: format!("cid://manual/{collector_byte}-{slot_id}-{observed_players}"),
        raw_body_hash: pole_protocol_draft::stable_hash32(raw_body.as_bytes()),
        collector_signature: vec![collector_byte; 32],
    };

    let mut builder = BatchBuilder::new(epoch_id, collector_id);
    builder.push(observation).unwrap();
    let assembled = builder.finalize(0).unwrap();

    let payload_file = payload_path(config, &assembled.payload_cid);
    std::fs::create_dir_all(payload_file.parent().unwrap()).unwrap();
    std::fs::write(&payload_file, &assembled.payload_bytes).unwrap();

    let artifact = CollectTickArtifact {
        epoch_id,
        slot_id,
        payload_cid: assembled.payload_cid.clone(),
        payload_hash_hex: pole_protocol_draft::hex_32(assembled.payload_hash),
        batch_root_hex: pole_protocol_draft::hex_32(assembled.batch_commit.batch.root),
        obs_count: assembled.batch_commit.obs_count,
        player_reward_block_count: 0,
        player_reward_total: 0,
        reward_process_name: None,
        stored_payload_cid: None,
        retention_until_epoch: None,
    };
    artifact
        .save_json(batch_artifact_path(
            config,
            epoch_id,
            slot_id,
            &assembled.payload_cid,
        ))
        .unwrap();

    assembled
}

#[test]
fn run_collect_tick_persists_payload_artifact_and_progress() {
    let config = test_config("daemon-once");
    let data_dir = PathBuf::from(&config.runtime.data_dir);
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).unwrap();
    }

    let client = FixedHttpClient;
    let mut progress = LocalNodeProgress::default_from_config(&config);
    let result = run_collect_tick_with_client(&config, &mut progress, &client).unwrap();

    assert_eq!(result.artifact.epoch_id, 1);
    assert_eq!(result.artifact.slot_id, 1);
    assert_eq!(result.artifact.obs_count, 2);
    assert!(payload_path(&config, &result.artifact.payload_cid).exists());
    assert!(batch_artifact_path(&config, 1, 1, &result.artifact.payload_cid).exists());
    assert!(epoch_aggregation_artifact_path(&config, 1).exists());
    assert!(epoch_reward_artifact_path(&config, 1).exists());
    assert!(epoch_commit_artifact_path(&config, 1).exists());
    assert!(epoch_verification_artifact_path(&config, 1).exists());
    assert!(reward_adjustment_artifact_path(&config, 0).exists());
    assert!(reward_adjustment_index_path(&config).exists());
    assert!(reward_adjustment_summary_path(&config).exists());
    assert!(progress_path(&config).exists());
    assert!(retention_book_path(&config).exists());
    assert_eq!(
        result
            .aggregation_artifact
            .as_ref()
            .unwrap()
            .aggregate_count,
        2
    );
    assert_eq!(result.reward_artifact.as_ref().unwrap().reward_count, 1);
    assert!(result.verification_report.as_ref().unwrap().all_valid);
    assert_eq!(result.epoch_commit_artifact.as_ref().unwrap().epoch_id, 1);
    assert!(result.auto_settlement_enabled);
    assert!(result.auto_settlement_pending_epochs.is_empty());
    assert!(result.settlement_artifacts.is_empty());
    assert_eq!(result.retention_audit_artifact.retained_payload_count, 1);
    assert_eq!(result.retention_audit_artifact.retrievable_payload_count, 1);
    assert_eq!(result.retention_audit_artifact.missing_payload_count, 0);
    assert!(result.retention_audit_artifact.all_retrievable);
    assert!(retention_audit_artifact_path(&config, 1).exists());
    let adjustment_artifact: RewardAdjustmentArtifact = serde_json::from_str(
        &std::fs::read_to_string(reward_adjustment_artifact_path(&config, 0)).unwrap(),
    )
    .unwrap();
    let adjustment_index: RewardAdjustmentArtifactIndex = serde_json::from_str(
        &std::fs::read_to_string(reward_adjustment_index_path(&config)).unwrap(),
    )
    .unwrap();
    let adjustment_summary: RewardAdjustmentArtifactSummary = serde_json::from_str(
        &std::fs::read_to_string(reward_adjustment_summary_path(&config)).unwrap(),
    )
    .unwrap();
    assert_eq!(adjustment_artifact.adjustment_cycle_index, 0);
    assert_eq!(adjustment_artifact.fixed_player_reward, 1_000);
    assert_eq!(adjustment_index.adjustment_artifacts.len(), 1);
    assert_eq!(adjustment_summary.adjustment_artifact_count, 1);
    assert_eq!(adjustment_summary.latest_period_index, Some(0));
    assert_eq!(result.retention_prune_outcome.current_epoch, 1);
    assert!(result.retention_prune_outcome.removed_payloads.is_empty());
    assert_eq!(progress.next_epoch_id, 1);
    assert_eq!(progress.next_slot_id, 2);

    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn run_collect_loop_advances_epoch_after_slot_wrap() {
    let config = test_config("daemon-loop");
    let data_dir = PathBuf::from(&config.runtime.data_dir);
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).unwrap();
    }

    let client = FixedHttpClient;
    let results = run_collect_loop_with_client(&config, &client, Some(3)).unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].artifact.epoch_id, 1);
    assert_eq!(results[0].artifact.slot_id, 1);
    assert_eq!(results[1].artifact.epoch_id, 1);
    assert_eq!(results[1].artifact.slot_id, 2);
    assert_eq!(results[2].artifact.epoch_id, 2);
    assert_eq!(results[2].artifact.slot_id, 1);
    assert_eq!(results[1].auto_settlement_pending_epochs, vec![1]);
    assert_eq!(results[1].settlement_artifacts.len(), 1);
    assert_eq!(results[1].settlement_artifacts[0].epoch_id, 1);
    assert!(results[1].settlement_artifacts[0].local_reward_claimed);
    assert_ne!(
        results[1].settlement_artifacts[0].accepted_batches_root_hex,
        pole_protocol_draft::hex_32([0u8; 32])
    );
    assert_ne!(
        results[1].settlement_artifacts[0].observations_root_hex,
        pole_protocol_draft::hex_32([0u8; 32])
    );
    assert_ne!(
        results[1].settlement_artifacts[0].availability_root_hex,
        pole_protocol_draft::hex_32([0u8; 32])
    );
    assert_ne!(
        results[1].settlement_artifacts[0].aggregates_root_hex,
        pole_protocol_draft::hex_32([0u8; 32])
    );
    assert_ne!(
        results[1].settlement_artifacts[0].rewards_root_hex,
        pole_protocol_draft::hex_32([0u8; 32])
    );
    assert!(results[1].unresolved_auto_settlement_epochs().is_empty());
    assert!(epoch_settlement_artifact_path(&config, 1).exists());
    assert!(results[2].auto_settlement_pending_epochs.is_empty());
    assert!(results[2].settlement_artifacts.is_empty());

    let summary = summarize_auto_settlement(&config, &results);
    assert!(summary.enabled);
    assert!(summary.pending_epochs.is_empty());
    assert_eq!(summary.settled_epoch_count, 1);
    assert_eq!(summary.last_settlement_artifact.unwrap().epoch_id, 1);

    let saved_progress =
        LocalNodeProgress::load_or_default(progress_path(&config), &config).unwrap();
    assert_eq!(saved_progress.next_epoch_id, 2);
    assert_eq!(saved_progress.next_slot_id, 2);

    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn load_status_reports_last_auto_settlement_summary() {
    let config = test_config("daemon-status-auto-settlement");
    let data_dir = PathBuf::from(&config.runtime.data_dir);
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).unwrap();
    }

    let client = FixedHttpClient;
    let results = run_collect_loop_with_client(&config, &client, Some(2)).unwrap();
    assert_eq!(results[1].settlement_artifacts.len(), 1);

    let status = load_status(&config).unwrap();
    assert_eq!(status.last_auto_settlement_pending_epoch_count, Some(0));
    assert_eq!(status.last_auto_settled_epoch, Some(1));
    assert_eq!(status.last_auto_settlement_reward_claimed, Some(true));

    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn collect_loop_continues_when_auto_settlement_fails_and_status_records_error() {
    let config = test_config("daemon-auto-settlement-error");
    let data_dir = PathBuf::from(&config.runtime.data_dir);
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).unwrap();
    }

    let client = RuntimeCorruptingHttpClient::new(local_chain_runtime_path(&config), 3);
    let results = run_collect_loop_with_client(&config, &client, Some(3)).unwrap();

    assert_eq!(results.len(), 3);
    assert_eq!(results[0].artifact.epoch_id, 1);
    assert_eq!(results[1].artifact.epoch_id, 1);
    assert_eq!(results[2].artifact.epoch_id, 2);
    assert_eq!(results[1].auto_settlement_pending_epochs, vec![1]);
    assert_eq!(results[1].unresolved_auto_settlement_epochs(), vec![1]);
    assert!(results[1].settlement_artifacts.is_empty());
    assert!(epoch_commit_artifact_path(&config, 1).exists());
    assert!(epoch_commit_artifact_path(&config, 2).exists());
    assert!(!epoch_settlement_artifact_path(&config, 1).exists());

    let status = load_status(&config).unwrap();
    assert_eq!(status.last_auto_settlement_pending_epoch_count, Some(1));
    assert_eq!(status.last_auto_settled_epoch, None);
    assert_eq!(status.last_auto_settlement_reward_claimed, None);
    assert!(
        status
            .last_auto_settlement_error
            .as_deref()
            .unwrap_or_default()
            .contains("json error"),
        "{status:?}"
    );

    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn collect_tick_publishes_batch_and_receipt_when_network_is_attached() {
    let config = test_config("daemon-p2p-publish");
    let data_dir = PathBuf::from(&config.runtime.data_dir);
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).unwrap();
    }

    let client = FixedHttpClient;
    let mut progress = LocalNodeProgress::default_from_config(&config);
    let mut network = InMemoryP2pNetwork::default();
    let local_node_id = config.node_id().unwrap();
    let listener = [0x77; 32];
    network.register_peer(listener);
    network.subscribe(listener, P2pTopic::Batches).unwrap();
    network.subscribe(listener, P2pTopic::Receipts).unwrap();

    let result =
        run_collect_tick_with_client_and_network(&config, &mut progress, &client, &mut network)
            .unwrap();

    assert_eq!(result.outcome.batch_recipients, 1);
    assert_eq!(result.outcome.receipt_recipients, 1);
    assert!(result.outcome.stored_payload.is_some());

    let payload = network
        .request_payload(listener, &result.artifact.payload_cid)
        .unwrap();
    assert_eq!(payload.provider, local_node_id);
    assert_eq!(payload.payload_cid, result.artifact.payload_cid);
    assert_eq!(
        payload.payload_bytes,
        result.outcome.assembled_batch.payload_bytes
    );

    let inbox = network.drain_inbox(listener).unwrap();
    assert_eq!(inbox.len(), 2);
    assert!(matches!(inbox[0].message, P2pMessage::Batch(_)));
    assert!(matches!(inbox[1].message, P2pMessage::ReplicaReceipt(_)));

    let status = load_status(&config).unwrap();
    assert_eq!(status.configured_p2p_batch_listener_count, 1);
    assert_eq!(status.configured_p2p_receipt_listener_count, 1);
    assert_eq!(status.configured_p2p_dual_listener_count, 1);
    assert_eq!(status.last_p2p_batch_recipients, Some(1));
    assert_eq!(status.last_p2p_receipt_recipients, Some(1));
    assert_eq!(status.last_p2p_retrieval_ok, Some(true));
    assert_eq!(status.last_p2p_retrieval_error, None);
    assert_eq!(status.last_storage_challenge_all_passed, Some(true));
    assert_eq!(status.last_storage_challenge_checked_payload_count, Some(1));
    assert_eq!(status.last_storage_challenge_failed_payload_count, Some(0));
    assert_eq!(status.last_storage_challenge_error, None);

    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn collect_tick_status_records_retrieval_error_without_remote_peer() {
    let config = test_config("daemon-p2p-retrieval-error");
    let data_dir = PathBuf::from(&config.runtime.data_dir);
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).unwrap();
    }

    let client = FixedHttpClient;
    let mut progress = LocalNodeProgress::default_from_config(&config);
    let mut network = InMemoryP2pNetwork::default();

    let result =
        run_collect_tick_with_client_and_network(&config, &mut progress, &client, &mut network)
            .unwrap();

    assert_eq!(result.outcome.batch_recipients, 0);
    assert_eq!(result.outcome.receipt_recipients, 0);

    let status = load_status(&config).unwrap();
    assert_eq!(status.last_p2p_retrieval_ok, Some(false));
    assert_eq!(
        status.last_p2p_retrieval_error,
        Some("no remote retrieval peer available".into())
    );
    assert_eq!(status.last_p2p_challenge_recipients, None);
    assert_eq!(status.last_p2p_challenge_kind, None);
    assert_eq!(status.last_p2p_challenge_epoch_id, None);
    assert_eq!(status.last_p2p_challenge_payload_cid, None);
    assert_eq!(status.p2p_challenge_events_total, Some(0));
    assert_eq!(status.p2p_bad_batch_challenge_events, Some(0));
    assert_eq!(status.p2p_omission_challenge_events, Some(0));
    assert_eq!(status.p2p_bad_aggregate_challenge_events, Some(0));
    assert_eq!(status.p2p_bad_reward_challenge_events, Some(0));
    assert_eq!(status.p2p_bad_storage_challenge_events, Some(0));
    assert_eq!(status.recent_p2p_challenge_events, Some(0));
    assert_eq!(status.recent_p2p_bad_batch_challenge_events, Some(0));
    assert_eq!(status.recent_p2p_omission_challenge_events, Some(0));
    assert_eq!(status.recent_p2p_bad_aggregate_challenge_events, Some(0));
    assert_eq!(status.recent_p2p_bad_reward_challenge_events, Some(0));
    assert_eq!(status.recent_p2p_bad_storage_challenge_events, Some(0));
    assert_eq!(status.p2p_challenge_delivered_events_total, Some(0));
    assert_eq!(status.p2p_challenge_zero_recipient_events_total, Some(0));
    assert_eq!(status.recent_p2p_challenge_delivered_events, Some(0));
    assert_eq!(status.recent_p2p_challenge_zero_recipient_events, Some(0));
    assert_eq!(status.recent_p2p_challenge_recipient_sum, Some(0));

    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn collect_loop_with_config_backed_simulation_network_uses_service_topology() {
    let mut config = test_config("daemon-p2p-service-topology");
    config.runtime.p2p_simulation = P2pSimulationConfig {
        batch_listener_count: 2,
        receipt_listener_count: 1,
        dual_listener_count: 2,
    };
    let data_dir = PathBuf::from(&config.runtime.data_dir);
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).unwrap();
    }

    let client = FixedHttpClient;
    let topology = config.runtime.p2p_simulation;
    let listener_ids = inmemory_simulation_listener_peer_ids(topology);
    let retrieval_peer = inmemory_simulation_retrieval_peer_id(topology);
    let mut network = build_inmemory_simulation_network(topology);

    let results = pole_protocol_draft::run_collect_loop_with_client_and_network(
        &config,
        &client,
        Some(2),
        &mut network,
    )
    .unwrap();

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].outcome.batch_recipients, 4);
    assert_eq!(results[0].outcome.receipt_recipients, 3);
    assert_eq!(results[1].outcome.batch_recipients, 4);
    assert_eq!(results[1].outcome.receipt_recipients, 3);

    let payload = network
        .request_payload(retrieval_peer, &results[1].artifact.payload_cid)
        .unwrap();
    assert_eq!(payload.payload_cid, results[1].artifact.payload_cid);
    assert_eq!(
        payload.payload_bytes,
        results[1].outcome.assembled_batch.payload_bytes
    );

    let delivered_messages = listener_ids
        .into_iter()
        .map(|peer_id| network.drain_inbox(peer_id).unwrap().len())
        .sum::<usize>();
    assert_eq!(delivered_messages, 14);

    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn status_and_epoch_commit_can_be_built_from_local_data() {
    let config = test_config("daemon-status");
    let data_dir = PathBuf::from(&config.runtime.data_dir);
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).unwrap();
    }

    let client = FixedHttpClient;
    let mut progress = LocalNodeProgress::default_from_config(&config);
    run_collect_tick_with_client(&config, &mut progress, &client).unwrap();

    let status = load_status(&config).unwrap();
    assert_eq!(status.next_epoch_id, 1);
    assert_eq!(status.next_slot_id, 2);
    assert!(!status.low_impact_mode);
    assert!(status.inline_verify_enabled);
    assert!(status.inline_propose_enabled);
    assert_eq!(status.stored_payload_count, 1);
    assert_eq!(status.configured_p2p_batch_listener_count, 1);
    assert_eq!(status.configured_p2p_receipt_listener_count, 1);
    assert_eq!(status.configured_p2p_dual_listener_count, 1);
    assert_eq!(status.last_p2p_batch_recipients, None);
    assert_eq!(status.last_p2p_receipt_recipients, None);
    assert_eq!(status.last_retention_all_retrievable, Some(true));
    assert_eq!(status.last_retention_retained_payload_count, Some(1));
    assert_eq!(status.last_retention_retrievable_payload_count, Some(1));
    assert_eq!(status.last_retention_missing_payload_count, Some(0));
    assert_eq!(status.last_retention_corrupted_payload_count, Some(0));

    let (_commit, artifact) =
        build_epoch_commit_from_local_data(&config, 1, 50, 20, [0u8; 32], [0u8; 32]).unwrap();
    assert_eq!(artifact.epoch_id, 1);
    assert_eq!(artifact.batch_count, 1);
    assert_eq!(artifact.payload_count, 1);
    assert_ne!(
        artifact.aggregates_root_hex,
        pole_protocol_draft::hex_32([0u8; 32])
    );
    assert_ne!(
        artifact.rewards_root_hex,
        pole_protocol_draft::hex_32([0u8; 32])
    );
    assert!(epoch_aggregation_artifact_path(&config, 1).exists());
    assert!(epoch_reward_artifact_path(&config, 1).exists());

    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn prepare_local_epoch_builds_submission_ready_artifact() {
    let config = test_config("daemon-prepare");
    let data_dir = PathBuf::from(&config.runtime.data_dir);
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).unwrap();
    }

    let client = FixedHttpClient;
    let mut progress = LocalNodeProgress::default_from_config(&config);
    run_collect_tick_with_client(&config, &mut progress, &client).unwrap();

    let artifact = prepare_local_epoch(&config, 1, 50, 20).unwrap();
    assert_eq!(artifact.epoch_id, 1);
    assert_eq!(artifact.current_height, 50);
    assert_eq!(artifact.challenge_window_blocks, 20);
    assert_eq!(artifact.challenge_deadline_height, 70);
    assert_eq!(artifact.batch_count, 1);
    assert_eq!(artifact.payload_count, 1);
    assert_eq!(artifact.verification_batch_count, 1);
    assert_eq!(artifact.stored_payload_count, 1);
    assert_eq!(artifact.aggregate_count, 2);
    assert_eq!(artifact.reward_count, 1);
    assert_eq!(artifact.total_observation_count, 2);
    assert_eq!(artifact.accepted_observation_count, 2);
    assert!(artifact.verification_all_valid);
    assert!(artifact.ready_for_submission);
    assert!(epoch_preparation_artifact_path(&config, 1).exists());

    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn aggregate_local_epoch_deduplicates_collectors_and_trims_extremes() {
    let config = test_config("daemon-aggregate");
    let data_dir = PathBuf::from(&config.runtime.data_dir);
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).unwrap();
    }

    persist_manual_batch(&config, 1, 1, 0x11, 730, 100, 1_700_000_000_001);
    persist_manual_batch(&config, 1, 1, 0x11, 730, 999, 1_700_000_000_999);
    persist_manual_batch(&config, 1, 1, 0x22, 730, 101, 1_700_000_000_002);
    persist_manual_batch(&config, 1, 1, 0x33, 730, 102, 1_700_000_000_003);
    persist_manual_batch(&config, 1, 1, 0x44, 730, 103, 1_700_000_000_004);
    persist_manual_batch(&config, 1, 1, 0x55, 730, 1_000, 1_700_000_000_005);

    let artifact = aggregate_local_epoch(&config, 1).unwrap();
    assert_eq!(artifact.epoch_id, 1);
    assert_eq!(artifact.aggregate_count, 1);
    assert_eq!(artifact.total_observation_count, 6);
    assert_eq!(artifact.deduped_observation_count, 5);
    assert_eq!(artifact.accepted_observation_count, 3);
    assert_ne!(
        artifact.aggregate_root_hex,
        pole_protocol_draft::hex_32([0u8; 32])
    );
    assert_eq!(artifact.records[0].total_observations, 6);
    assert_eq!(artifact.records[0].unique_collectors, 5);
    assert_eq!(artifact.records[0].trimmed_observations, 2);
    assert_eq!(artifact.records[0].aggregate.accepted_observations, 3);
    assert_eq!(artifact.records[0].aggregate.median_players, 102);
    assert_eq!(
        artifact.records[0].aggregate.base_glv_microunits,
        102_000_000
    );
    assert_eq!(
        artifact.records[0].aggregate.gvs_tier,
        pole_protocol_draft::GvsTier::Tier2
    );
    assert_eq!(
        artifact.records[0].aggregate.primary_source_kind,
        ActivitySourceKind::Steam
    );
    assert_eq!(
        artifact.records[0].aggregate.source_confidence_ppm,
        1_000_000
    );
    assert_eq!(artifact.records[0].aggregate.tier_weight_ppm, 400_000);
    assert_eq!(artifact.records[0].aggregate.time_decay_ppm, 850_000);
    assert_eq!(artifact.records[0].aggregate.coverage_bonus_ppm, 1_200_000);
    assert_eq!(artifact.records[0].aggregate.gvs_microunits, 41_616_000);
    assert!(epoch_aggregation_artifact_path(&config, 1).exists());

    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn aggregate_local_epoch_applies_tier1_and_full_recency_weight() {
    let config = test_config("daemon-aggregate-tier1");
    let data_dir = PathBuf::from(&config.runtime.data_dir);
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).unwrap();
    }

    persist_manual_batch(&config, 1, 2, 0x11, 730, 1_150, 1_700_000_000_001);
    persist_manual_batch(&config, 1, 2, 0x22, 730, 1_200, 1_700_000_000_002);
    persist_manual_batch(&config, 1, 2, 0x33, 730, 1_250, 1_700_000_000_003);

    let artifact = aggregate_local_epoch(&config, 1).unwrap();
    let aggregate = &artifact.records[0].aggregate;
    assert_eq!(aggregate.median_players, 1_200);
    assert_eq!(aggregate.gvs_tier, pole_protocol_draft::GvsTier::Tier1);
    assert_eq!(aggregate.tier_weight_ppm, 1_000_000);
    assert_eq!(aggregate.time_decay_ppm, 1_000_000);
    assert_eq!(aggregate.coverage_bonus_ppm, 1_100_000);
    assert_eq!(aggregate.gvs_microunits, 1_320_000_000);

    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn aggregate_local_epoch_applies_tier3_for_sparse_low_confidence_group() {
    let config = test_config("daemon-aggregate-tier3");
    let data_dir = PathBuf::from(&config.runtime.data_dir);
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).unwrap();
    }

    persist_manual_batch(&config, 1, 1, 0x11, 730, 180, 1_700_000_000_001);
    persist_manual_batch(&config, 1, 1, 0x22, 730, 220, 1_700_000_000_002);

    let artifact = aggregate_local_epoch(&config, 1).unwrap();
    let aggregate = &artifact.records[0].aggregate;
    assert_eq!(aggregate.median_players, 200);
    assert_eq!(aggregate.gvs_tier, pole_protocol_draft::GvsTier::Tier3);
    assert_eq!(aggregate.tier_weight_ppm, 75_000);
    assert_eq!(aggregate.time_decay_ppm, 850_000);
    assert_eq!(aggregate.coverage_bonus_ppm, 1_050_000);
    assert_eq!(aggregate.gvs_microunits, 13_387_500);

    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn collect_tick_uses_configured_activity_sources_when_present() {
    let mut config = test_config("daemon-configured-sources");
    config.runtime.activity_sources = vec![
        pole_protocol_draft::ActivitySourceConfig {
            app_id: 730,
            source_kind: ActivitySourceKind::Epic,
            endpoint_url: Some("https://example.invalid/epic?appid={app_id}".into()),
            inline_json: None,
        },
        pole_protocol_draft::ActivitySourceConfig {
            app_id: 9900,
            source_kind: ActivitySourceKind::Community,
            endpoint_url: None,
            inline_json: Some(r#"{"estimated_players":77,"confidence_ppm":120000}"#.into()),
        },
    ];
    let data_dir = PathBuf::from(&config.runtime.data_dir);
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).unwrap();
    }

    let client = FixedHttpClient;
    let mut progress = LocalNodeProgress::default_from_config(&config);
    let result = run_collect_tick_with_client(&config, &mut progress, &client).unwrap();

    assert_eq!(result.artifact.obs_count, 2);
    assert_eq!(result.outcome.assembled_batch.observations.len(), 2);
    assert!(result
        .outcome
        .assembled_batch
        .observations
        .iter()
        .any(|obs| obs.source_kind == ActivitySourceKind::Epic));
    assert!(result
        .outcome
        .assembled_batch
        .observations
        .iter()
        .any(|obs| obs.source_kind == ActivitySourceKind::Community));

    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn load_status_reports_latest_aggregate_gvs_tier_and_source_confidence() {
    let config = test_config("daemon-status-aggregate-summary");
    let data_dir = PathBuf::from(&config.runtime.data_dir);
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).unwrap();
    }

    persist_manual_batch(&config, 1, 2, 0x11, 730, 1_150, 1_700_000_000_001);
    persist_manual_batch(&config, 1, 2, 0x22, 730, 1_200, 1_700_000_000_002);
    persist_manual_batch(&config, 1, 2, 0x33, 730, 1_250, 1_700_000_000_003);
    aggregate_local_epoch(&config, 1).unwrap();

    let status = load_status(&config).unwrap();
    assert_eq!(status.last_aggregate_epoch_id, Some(1));
    assert_eq!(status.last_aggregate_gvs_tier.as_deref(), Some("Tier1"));
    assert_eq!(status.last_aggregate_source_kind.as_deref(), Some("Steam"));
    assert_eq!(status.last_aggregate_source_confidence_ppm, Some(1_000_000));

    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn prune_retention_removes_expired_payload_file() {
    let config = test_config("daemon-prune");
    let data_dir = PathBuf::from(&config.runtime.data_dir);
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).unwrap();
    }

    let client = FixedHttpClient;
    let mut progress = LocalNodeProgress::default_from_config(&config);
    let result = run_collect_tick_with_client(&config, &mut progress, &client).unwrap();
    let payload_file = payload_path(&config, &result.artifact.payload_cid);
    let batch_file = batch_artifact_path(
        &config,
        result.artifact.epoch_id,
        result.artifact.slot_id,
        &result.artifact.payload_cid,
    );
    let retention_audit_epoch_1 = retention_audit_artifact_path(&config, 1);
    let retention_audit_epoch_2 = retention_audit_artifact_path(&config, 2);
    let retention_audit_epoch_3 = retention_audit_artifact_path(&config, 3);
    assert!(payload_file.exists());
    assert!(batch_file.exists());
    assert!(retention_audit_epoch_1.exists());
    audit_local_retention(&config, 2).unwrap();
    audit_local_retention(&config, 3).unwrap();
    assert!(retention_audit_epoch_2.exists());
    assert!(retention_audit_epoch_3.exists());

    let outcome = prune_retention(&config, 4).unwrap();
    assert_eq!(outcome.removed_payloads.len(), 1);
    assert!(!payload_file.exists());
    assert!(!batch_file.exists());
    assert!(!retention_audit_epoch_1.exists());
    assert!(retention_audit_epoch_2.exists());
    assert!(retention_audit_epoch_3.exists());

    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn collect_loop_prunes_expired_payloads_automatically() {
    let mut config = test_config("daemon-auto-prune");
    config.capabilities.verify = false;
    config.capabilities.propose = false;
    config.runtime.slots_per_epoch = 1;
    config.storage.retention_epochs = 1;
    let data_dir = PathBuf::from(&config.runtime.data_dir);
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).unwrap();
    }

    let client = FixedHttpClient;
    let results = run_collect_loop_with_client(&config, &client, Some(2)).unwrap();

    let first_payload = payload_path(&config, &results[0].artifact.payload_cid);
    let second_payload = payload_path(&config, &results[1].artifact.payload_cid);
    let first_batch = batch_artifact_path(
        &config,
        results[0].artifact.epoch_id,
        results[0].artifact.slot_id,
        &results[0].artifact.payload_cid,
    );
    let second_batch = batch_artifact_path(
        &config,
        results[1].artifact.epoch_id,
        results[1].artifact.slot_id,
        &results[1].artifact.payload_cid,
    );

    assert_eq!(results[0].retention_prune_outcome.current_epoch, 2);
    assert!(results[0]
        .retention_prune_outcome
        .removed_payloads
        .is_empty());
    assert_eq!(results[1].retention_prune_outcome.current_epoch, 3);
    assert_eq!(results[1].retention_prune_outcome.removed_payloads.len(), 1);
    assert_eq!(
        results[1].retention_prune_outcome.removed_payloads[0],
        results[0].artifact.payload_cid
    );
    assert!(!first_payload.exists());
    assert!(second_payload.exists());
    assert!(!first_batch.exists());
    assert!(second_batch.exists());

    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn collect_loop_prunes_expired_retention_audit_artifacts_automatically() {
    let mut config = test_config("daemon-auto-prune-retention-audits");
    config.capabilities.verify = false;
    config.capabilities.propose = false;
    config.runtime.slots_per_epoch = 1;
    config.storage.retention_epochs = 1;
    let data_dir = PathBuf::from(&config.runtime.data_dir);
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).unwrap();
    }

    let audit_epoch_1 = retention_audit_artifact_path(&config, 1);
    audit_local_retention(&config, 1).unwrap();
    assert!(audit_epoch_1.exists());

    let client = FixedHttpClient;
    let _results = run_collect_loop_with_client(&config, &client, Some(2)).unwrap();

    let audit_epoch_2 = retention_audit_artifact_path(&config, 2);
    let audit_epoch_3 = retention_audit_artifact_path(&config, 3);
    assert!(!audit_epoch_1.exists());
    assert!(audit_epoch_2.exists());
    assert!(audit_epoch_3.exists());

    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn prune_retention_removes_expired_settled_prepared_epoch_artifact() {
    let config = test_config("daemon-prune-prepared-epoch");
    let data_dir = PathBuf::from(&config.runtime.data_dir);
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).unwrap();
    }

    let client = FixedHttpClient;
    let mut progress = LocalNodeProgress::default_from_config(&config);
    run_collect_tick_with_client(&config, &mut progress, &client).unwrap();
    settle_local_epoch(&config, 1, 50, 20).unwrap();

    let prepared_epoch = epoch_preparation_artifact_path(&config, 1);
    let epoch_commit = epoch_commit_artifact_path(&config, 1);
    let verification = epoch_verification_artifact_path(&config, 1);
    let aggregate = epoch_aggregation_artifact_path(&config, 1);
    let reward = epoch_reward_artifact_path(&config, 1);
    let player_reward_tick = player_reward_tick_artifact_path(&config, 1, 1);
    let settlement = epoch_settlement_artifact_path(&config, 1);
    assert!(prepared_epoch.exists());
    assert!(epoch_commit.exists());
    assert!(verification.exists());
    assert!(aggregate.exists());
    assert!(reward.exists());
    assert!(player_reward_tick.exists());
    assert!(settlement.exists());

    let outcome = prune_retention(&config, 4).unwrap();
    assert_eq!(outcome.removed_payloads.len(), 1);
    assert!(!prepared_epoch.exists());
    assert!(!epoch_commit.exists());
    assert!(!verification.exists());
    assert!(!aggregate.exists());
    assert!(!reward.exists());
    assert!(!player_reward_tick.exists());
    assert!(settlement.exists());

    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn collect_loop_prunes_expired_settled_prepared_epoch_artifacts_automatically() {
    let mut config = test_config("daemon-auto-prune-prepared-epochs");
    config.runtime.slots_per_epoch = 1;
    config.storage.retention_epochs = 1;
    let data_dir = PathBuf::from(&config.runtime.data_dir);
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).unwrap();
    }

    let client = FixedHttpClient;
    let results = run_collect_loop_with_client(&config, &client, Some(2)).unwrap();
    assert_eq!(results[1].settlement_artifacts.len(), 1);

    let prepared_epoch_1 = epoch_preparation_artifact_path(&config, 1);
    let epoch_commit_1 = epoch_commit_artifact_path(&config, 1);
    let verification_epoch_1 = epoch_verification_artifact_path(&config, 1);
    let aggregate_epoch_1 = epoch_aggregation_artifact_path(&config, 1);
    let reward_epoch_1 = epoch_reward_artifact_path(&config, 1);
    let player_reward_tick_epoch_1 = player_reward_tick_artifact_path(&config, 1, 1);
    let settlement_epoch_1 = epoch_settlement_artifact_path(&config, 1);
    assert!(!prepared_epoch_1.exists());
    assert!(!epoch_commit_1.exists());
    assert!(!verification_epoch_1.exists());
    assert!(!aggregate_epoch_1.exists());
    assert!(!reward_epoch_1.exists());
    assert!(!player_reward_tick_epoch_1.exists());
    assert!(settlement_epoch_1.exists());

    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn retention_audit_detects_missing_payload_during_active_retention() {
    let mut config = test_config("daemon-retention-audit-missing");
    config.capabilities.verify = false;
    config.capabilities.propose = false;
    config.runtime.slots_per_epoch = 10;
    config.storage.retention_epochs = 3;
    let data_dir = PathBuf::from(&config.runtime.data_dir);
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).unwrap();
    }

    let client = FixedHttpClient;
    let mut progress = LocalNodeProgress::default_from_config(&config);
    let first = run_collect_tick_with_client(&config, &mut progress, &client).unwrap();
    let first_payload = payload_path(&config, &first.artifact.payload_cid);
    std::fs::remove_file(&first_payload).unwrap();

    let second = run_collect_tick_with_client(&config, &mut progress, &client).unwrap();
    assert_eq!(second.retention_audit_artifact.retained_payload_count, 2);
    assert_eq!(second.retention_audit_artifact.retrievable_payload_count, 1);
    assert_eq!(second.retention_audit_artifact.missing_payload_count, 1);
    assert_eq!(second.retention_audit_artifact.corrupted_payload_count, 0);
    assert!(!second.retention_integrity_healthy());
    let status = load_status(&config).unwrap();
    assert_eq!(status.last_retention_all_retrievable, Some(false));
    assert_eq!(status.last_retention_retained_payload_count, Some(2));
    assert_eq!(status.last_retention_retrievable_payload_count, Some(1));
    assert_eq!(status.last_retention_missing_payload_count, Some(1));
    assert_eq!(status.last_retention_corrupted_payload_count, Some(0));
    assert_eq!(status.last_storage_challenge_all_passed, Some(false));
    assert_eq!(status.last_storage_challenge_checked_payload_count, Some(2));
    assert_eq!(status.last_storage_challenge_failed_payload_count, Some(1));
    assert_eq!(status.last_storage_challenge_error, None);
    let missing_record = second
        .retention_audit_artifact
        .records
        .iter()
        .find(|record| record.payload_cid == first.artifact.payload_cid)
        .unwrap();
    assert!(!missing_record.file_present);

    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn storage_challenge_detects_corrupted_receipt_signature_during_loop() {
    let mut config = test_config("daemon-storage-challenge-receipt");
    config.capabilities.verify = false;
    config.capabilities.propose = false;
    config.runtime.slots_per_epoch = 10;
    config.storage.retention_epochs = 3;
    let data_dir = PathBuf::from(&config.runtime.data_dir);
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).unwrap();
    }

    let client = FixedHttpClient;
    let mut progress = LocalNodeProgress::default_from_config(&config);
    let first = run_collect_tick_with_client(&config, &mut progress, &client).unwrap();
    assert!(first.storage_challenge_healthy());

    let ledger_path = retention_book_path(&config);
    let mut retention_book =
        LocalRetentionBook::load_or_default_json(&ledger_path, config.storage.quota_gb).unwrap();
    let record = retention_book
        .payloads
        .get_mut(&first.artifact.payload_cid)
        .unwrap();
    record.receipt.receipt_signature = vec![0u8; 32];
    retention_book.save_json(&ledger_path).unwrap();

    let second = run_collect_tick_with_client(&config, &mut progress, &client).unwrap();
    assert!(!second.storage_challenge_healthy());
    let challenge = second.storage_challenge_artifact.as_ref().unwrap();
    assert_eq!(challenge.checked_payload_count, 2);
    assert_eq!(challenge.failed_payload_count, 1);
    assert!(!challenge.all_passed);
    let corrupted = challenge
        .records
        .iter()
        .find(|record| record.payload_cid == first.artifact.payload_cid)
        .unwrap();
    assert!(!corrupted.receipt_signature_matches);
    assert!(corrupted.payload_retrievable);

    let status = load_status(&config).unwrap();
    assert_eq!(status.last_storage_challenge_all_passed, Some(false));
    assert_eq!(status.last_storage_challenge_checked_payload_count, Some(2));
    assert_eq!(status.last_storage_challenge_failed_payload_count, Some(1));
    assert_eq!(status.last_storage_challenge_error, None);

    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn storage_challenge_failure_publishes_bad_storage_challenge_to_attached_network() {
    let mut config = test_config("daemon-storage-challenge-p2p");
    config.capabilities.verify = false;
    config.capabilities.propose = false;
    config.runtime.slots_per_epoch = 10;
    config.storage.retention_epochs = 3;
    let data_dir = PathBuf::from(&config.runtime.data_dir);
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).unwrap();
    }

    let client = FixedHttpClient;
    let mut progress = LocalNodeProgress::default_from_config(&config);
    let mut network = InMemoryP2pNetwork::default();
    let listener = [0x88; 32];
    network.register_peer(listener);
    network.subscribe(listener, P2pTopic::Challenges).unwrap();

    let first =
        run_collect_tick_with_client_and_network(&config, &mut progress, &client, &mut network)
            .unwrap();
    assert_eq!(first.p2p_challenge_recipients, None);

    let ledger_path = retention_book_path(&config);
    let mut retention_book =
        LocalRetentionBook::load_or_default_json(&ledger_path, config.storage.quota_gb).unwrap();
    let record = retention_book
        .payloads
        .get_mut(&first.artifact.payload_cid)
        .unwrap();
    record.receipt.receipt_signature = vec![0u8; 32];
    retention_book.save_json(&ledger_path).unwrap();

    let second =
        run_collect_tick_with_client_and_network(&config, &mut progress, &client, &mut network)
            .unwrap();
    assert_eq!(second.p2p_challenge_recipients, Some(1));
    assert_eq!(second.p2p_challenge_kind.as_deref(), Some("BadStorage"));
    assert_eq!(second.p2p_challenge_epoch_id, Some(1));
    assert_eq!(
        second.p2p_challenge_payload_cid.as_deref(),
        Some(first.artifact.payload_cid.as_str())
    );

    let inbox = network.drain_inbox(listener).unwrap();
    let challenge = inbox
        .iter()
        .find(|envelope| matches!(envelope.message, P2pMessage::Challenge(_)))
        .unwrap();
    assert_eq!(challenge.topic, P2pTopic::Challenges);

    let status = load_status(&config).unwrap();
    assert_eq!(status.last_p2p_challenge_recipients, Some(1));
    assert_eq!(
        status.last_p2p_challenge_kind.as_deref(),
        Some("BadStorage")
    );
    assert_eq!(status.last_p2p_challenge_epoch_id, Some(1));
    assert_eq!(
        status.last_p2p_challenge_payload_cid.as_deref(),
        Some(first.artifact.payload_cid.as_str())
    );
    assert_eq!(status.p2p_challenge_events_total, Some(1));
    assert_eq!(status.p2p_bad_batch_challenge_events, Some(0));
    assert_eq!(status.p2p_omission_challenge_events, Some(0));
    assert_eq!(status.p2p_bad_aggregate_challenge_events, Some(0));
    assert_eq!(status.p2p_bad_reward_challenge_events, Some(0));
    assert_eq!(status.p2p_bad_storage_challenge_events, Some(1));
    assert_eq!(status.recent_p2p_challenge_events, Some(1));
    assert_eq!(status.recent_p2p_bad_batch_challenge_events, Some(0));
    assert_eq!(status.recent_p2p_omission_challenge_events, Some(0));
    assert_eq!(status.recent_p2p_bad_aggregate_challenge_events, Some(0));
    assert_eq!(status.recent_p2p_bad_reward_challenge_events, Some(0));
    assert_eq!(status.recent_p2p_bad_storage_challenge_events, Some(1));
    assert_eq!(status.p2p_challenge_delivered_events_total, Some(1));
    assert_eq!(status.p2p_challenge_zero_recipient_events_total, Some(0));
    assert_eq!(status.recent_p2p_challenge_delivered_events, Some(1));
    assert_eq!(status.recent_p2p_challenge_zero_recipient_events, Some(0));
    assert_eq!(status.recent_p2p_challenge_recipient_sum, Some(1));

    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn storage_challenge_with_no_challenge_subscribers_records_zero_recipient_stats() {
    let mut config = test_config("daemon-storage-challenge-no-subscribers");
    config.capabilities.verify = false;
    config.capabilities.propose = false;
    config.runtime.slots_per_epoch = 10;
    config.storage.retention_epochs = 3;
    let data_dir = PathBuf::from(&config.runtime.data_dir);
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).unwrap();
    }

    let client = FixedHttpClient;
    let mut progress = LocalNodeProgress::default_from_config(&config);
    let mut network = InMemoryP2pNetwork::default();
    let non_challenge_peer = [0x77; 32];
    network.register_peer(non_challenge_peer);
    network
        .subscribe(non_challenge_peer, P2pTopic::Batches)
        .unwrap();

    let first =
        run_collect_tick_with_client_and_network(&config, &mut progress, &client, &mut network)
            .unwrap();
    let ledger_path = retention_book_path(&config);
    let mut retention_book =
        LocalRetentionBook::load_or_default_json(&ledger_path, config.storage.quota_gb).unwrap();
    let record = retention_book
        .payloads
        .get_mut(&first.artifact.payload_cid)
        .unwrap();
    record.receipt.receipt_signature = vec![0u8; 32];
    retention_book.save_json(&ledger_path).unwrap();

    let second =
        run_collect_tick_with_client_and_network(&config, &mut progress, &client, &mut network)
            .unwrap();
    assert_eq!(second.p2p_challenge_recipients, Some(0));

    let status = load_status(&config).unwrap();
    assert_eq!(status.p2p_challenge_events_total, Some(1));
    assert_eq!(status.p2p_challenge_delivered_events_total, Some(0));
    assert_eq!(status.p2p_challenge_zero_recipient_events_total, Some(1));
    assert_eq!(status.recent_p2p_challenge_events, Some(1));
    assert_eq!(status.recent_p2p_challenge_delivered_events, Some(0));
    assert_eq!(status.recent_p2p_challenge_zero_recipient_events, Some(1));
    assert_eq!(status.recent_p2p_challenge_recipient_sum, Some(0));

    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn verify_epoch_reports_local_data_as_valid() {
    let config = test_config("daemon-verify");
    let data_dir = PathBuf::from(&config.runtime.data_dir);
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).unwrap();
    }

    let client = FixedHttpClient;
    let mut progress = LocalNodeProgress::default_from_config(&config);
    run_collect_tick_with_client(&config, &mut progress, &client).unwrap();

    let report = verify_local_epoch(&config, 1).unwrap();
    assert_eq!(report.epoch_id, 1);
    assert_eq!(report.batch_count, 1);
    assert_eq!(report.stored_payload_count, 1);
    assert!(report.all_valid);
    assert!(report.reports[0].payload_hash_matches);
    assert!(report.reports[0].batch_root_matches);
    assert!(report.reports[0].obs_count_matches);
    assert!(report.reports[0].retention_record_present);
    assert!(report.reports[0].retention_hash_matches);

    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn verify_epoch_without_local_batches_is_not_valid() {
    let config = test_config("daemon-verify-empty");
    let data_dir = PathBuf::from(&config.runtime.data_dir);
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).unwrap();
    }

    let report = verify_local_epoch(&config, 1).unwrap();
    assert_eq!(report.epoch_id, 1);
    assert_eq!(report.batch_count, 0);
    assert_eq!(report.stored_payload_count, 0);
    assert!(!report.all_valid);
    assert!(report.reports.is_empty());
}

#[test]
fn low_impact_mode_skips_inline_verify_and_propose_work() {
    let mut config = test_config("daemon-low-impact");
    config.runtime.low_impact_mode = true;
    let data_dir = PathBuf::from(&config.runtime.data_dir);
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).unwrap();
    }

    let client = FixedHttpClient;
    let mut progress = LocalNodeProgress::default_from_config(&config);
    let result = run_collect_tick_with_client(&config, &mut progress, &client).unwrap();

    assert!(result.verification_report.is_none());
    assert!(result.aggregation_artifact.is_none());
    assert!(result.reward_artifact.is_none());
    assert!(result.epoch_commit_artifact.is_none());
    assert!(!epoch_aggregation_artifact_path(&config, 1).exists());
    assert!(!epoch_reward_artifact_path(&config, 1).exists());
    assert!(!epoch_verification_artifact_path(&config, 1).exists());
    assert!(!epoch_commit_artifact_path(&config, 1).exists());

    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn completed_epochs_remain_pending_without_propose_capability() {
    let mut config = test_config("daemon-auto-settlement-disabled");
    config.capabilities.propose = false;
    let data_dir = PathBuf::from(&config.runtime.data_dir);
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).unwrap();
    }

    let client = FixedHttpClient;
    let results = run_collect_loop_with_client(&config, &client, Some(2)).unwrap();

    assert_eq!(results.len(), 2);
    assert!(!results[1].auto_settlement_enabled);
    assert_eq!(results[1].auto_settlement_pending_epochs, vec![1]);
    assert_eq!(results[1].unresolved_auto_settlement_epochs(), vec![1]);
    assert!(results[1].settlement_artifacts.is_empty());
    assert!(!epoch_settlement_artifact_path(&config, 1).exists());

    let summary = summarize_auto_settlement(&config, &results);
    assert!(!summary.enabled);
    assert_eq!(summary.pending_epochs, vec![1]);
    assert_eq!(summary.settled_epoch_count, 0);
    assert!(summary.skipped());

    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn active_game_processes_raise_effective_poll_interval() {
    let mut config = test_config("daemon-game-interval");
    config.runtime.low_impact_mode = true;
    config.runtime.poll_interval_secs = 60;
    config.runtime.game_active_poll_interval_secs = 900;

    let effective = effective_collect_interval_secs(&config, &[String::from("cs2")]);
    assert_eq!(effective, 900);

    let idle = effective_collect_interval_secs(&config, &[]);
    assert_eq!(idle, 60);
}

#[test]
fn collect_tick_records_player_reward_blocks_when_foreground_game_matches_mapping() {
    let _guard = foreground_override_lock().lock().unwrap();
    let mut config = test_config("daemon-player-reward");
    config.runtime.target_app_ids = vec![1];
    config.runtime.poll_interval_secs = 300;
    config.runtime.game_process_names = vec!["TestGame.exe".into()];
    config.reward = RewardConfig {
        reward_source: pole_protocol_draft::RewardSourceMode::Static,
        emission_year: 1,
        reward_block_secs: 300,
        player_block_reward: 1_000,
        reward_adjustment_period_blocks: 288,
        target_network_weight_units: 300_000_000_000,
        reward_adjustment_cap_bps: 2_000,
        collect_reward_bps: 5_000,
        store_reward_bps: 2_500,
        verify_reward_bps: 1_500,
        propose_reward_bps: 1_000,
        tail_emission_start_year: 4,
        tail_emission_rate_bps: 200,
        game_mappings: vec![RewardGameMapping {
            process_name: "TestGame.exe".into(),
            app_id: 1,
            game_coefficient_ppm: 1_000_000,
        }],
    };
    let data_dir = PathBuf::from(&config.runtime.data_dir);
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).unwrap();
    }

    let previous = std::env::var_os("POLE_CLIENT_FOREGROUND_PROCESS_OVERRIDE");
    std::env::set_var("POLE_CLIENT_FOREGROUND_PROCESS_OVERRIDE", "TestGame.exe");

    let client = FixedHttpClient;
    let mut progress = LocalNodeProgress::default_from_config(&config);
    let result = run_collect_tick_with_client(&config, &mut progress, &client).unwrap();

    match previous {
        Some(value) => std::env::set_var("POLE_CLIENT_FOREGROUND_PROCESS_OVERRIDE", value),
        None => std::env::remove_var("POLE_CLIENT_FOREGROUND_PROCESS_OVERRIDE"),
    }

    assert_eq!(result.artifact.player_reward_block_count, 1);
    assert_eq!(result.player_reward_tick_artifact.records.len(), 1);
    assert_eq!(
        result.player_reward_tick_artifact.records[0].process_name,
        "TestGame.exe"
    );
    assert_eq!(result.player_reward_tick_artifact.records[0].app_id, 1);
    assert_eq!(
        result.player_reward_tick_artifact.records[0].play_seconds,
        300
    );
    assert_eq!(
        result.player_reward_tick_artifact.records[0].block_reward,
        1_000
    );
    assert_eq!(result.artifact.player_reward_total, 1);

    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn load_status_uses_activated_protocol_reward_tuning() {
    let config = test_config("daemon-status-activated-protocol-params");
    let data_dir = PathBuf::from(&config.runtime.data_dir);
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).unwrap();
    }

    let store_path = local_chain_store_path(&config);
    let mut store = PersistentStoreStub::open(&store_path).unwrap();
    let params = ProtocolParams {
        slot_seconds: 300,
        epoch_slots: 12,
        committee_size: 21,
        unbonding_blocks: 5,
        min_verify_bond: 100,
        min_propose_bond: 10_000,
        challenge_window_blocks: 25,
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
            emission_year: 4,
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
            effective_player_block_reward: 15_000,
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
    params.validate().unwrap();
    store.insert_params_update_proposal(
        [0xaa; 32],
        pole_protocol_draft::GovernanceParamsUpdateProposalRecord {
            proposal_id: [0xaa; 32],
            proposer: [0x41; 32],
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

    let status = load_status(&config).unwrap();
    assert_eq!(status.reward_block_secs, 7_200);
    assert_eq!(status.challenge_window_blocks, 25);
    assert_eq!(status.effective_player_block_reward, 15_000);
    assert_eq!(status.target_network_weight_units, 1_800_000_000_000);
    assert_eq!(status.reward_adjustment_cap_bps, 1_500);

    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn collect_tick_uses_previous_period_weight_to_adjust_next_fixed_block_reward() {
    let _guard = foreground_override_lock().lock().unwrap();
    let mut config = test_config("daemon-player-reward-adjustment");
    config.runtime.target_app_ids = vec![1];
    config.runtime.poll_interval_secs = 300;
    config.runtime.game_process_names = vec!["TestGame.exe".into()];
    config.reward = RewardConfig {
        reward_source: pole_protocol_draft::RewardSourceMode::Static,
        emission_year: 1,
        reward_block_secs: 300,
        player_block_reward: 1_000,
        reward_adjustment_period_blocks: 1,
        target_network_weight_units: 600_000_000_000_000,
        reward_adjustment_cap_bps: 2_000,
        collect_reward_bps: 5_000,
        store_reward_bps: 2_500,
        verify_reward_bps: 1_500,
        propose_reward_bps: 1_000,
        tail_emission_start_year: 4,
        tail_emission_rate_bps: 200,
        game_mappings: vec![RewardGameMapping {
            process_name: "TestGame.exe".into(),
            app_id: 1,
            game_coefficient_ppm: 1_000_000,
        }],
    };
    let data_dir = PathBuf::from(&config.runtime.data_dir);
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).unwrap();
    }

    let previous = std::env::var_os("POLE_CLIENT_FOREGROUND_PROCESS_OVERRIDE");
    std::env::set_var("POLE_CLIENT_FOREGROUND_PROCESS_OVERRIDE", "TestGame.exe");

    let client = FixedHttpClient;
    let mut progress = LocalNodeProgress::default_from_config(&config);
    let first = run_collect_tick_with_client(&config, &mut progress, &client).unwrap();
    let second = run_collect_tick_with_client(&config, &mut progress, &client).unwrap();

    match previous {
        Some(value) => std::env::set_var("POLE_CLIENT_FOREGROUND_PROCESS_OVERRIDE", value),
        None => std::env::remove_var("POLE_CLIENT_FOREGROUND_PROCESS_OVERRIDE"),
    }

    let first_reward_block = &first.player_reward_tick_artifact.records[0];
    let second_reward_block = &second.player_reward_tick_artifact.records[0];
    assert_eq!(first_reward_block.block_reward, 1_000);
    assert_eq!(first.player_reward_tick_artifact.block_reward, 1_000);
    assert_eq!(
        progress.previous_reward_adjustment_period_network_weight_units,
        300_000_000_000
    );
    assert_eq!(
        progress.previous_adjustment_cycle_total_network_weight_units,
        300_000_000_000
    );
    assert_eq!(progress.current_fixed_block_reward_basis_period_index, 0);
    assert_eq!(progress.current_fixed_player_reward_basis_cycle_index, 0);
    assert_eq!(
        progress.current_fixed_block_reward_basis_network_weight_units,
        300_000_000_000
    );
    assert_eq!(
        progress.current_fixed_player_reward_basis_total_network_weight_units,
        300_000_000_000
    );
    assert_eq!(
        first
            .player_reward_tick_artifact
            .records
            .first()
            .unwrap()
            .adjustment_cycle_index,
        0
    );
    let pending = first.player_reward_tick_artifact.records.first().unwrap();
    assert_eq!(pending.fixed_player_reward, 1_000);
    assert_eq!(progress.current_adjustment_cycle_index, 1);
    assert_eq!(progress.current_fixed_player_reward, 1_200);
    assert_eq!(
        progress.current_reward_adjustment_period_network_weight_units,
        300_000_000_000
    );
    assert_eq!(
        progress.current_adjustment_cycle_total_network_weight_units,
        300_000_000_000
    );
    let reward_block = second_reward_block;
    assert_eq!(reward_block.total_network_weight_units, 300_000_000_000);
    assert_eq!(
        reward_block.target_network_weight_units,
        600_000_000_000_000
    );
    assert_eq!(reward_block.reward_adjustment_cap_bps, 2_000);
    assert_eq!(reward_block.reward_adjustment_period_blocks, 1);
    assert_eq!(reward_block.adjustment_cycle_index, 1);
    assert_eq!(reward_block.adjustment_cycle_blocks, 1);
    assert_eq!(reward_block.fixed_block_reward_basis_period_index, 0);
    assert_eq!(reward_block.fixed_player_reward_basis_cycle_index, 0);
    assert_eq!(
        reward_block.fixed_block_reward_basis_network_weight_units,
        300_000_000_000
    );
    assert_eq!(
        reward_block.fixed_player_reward_basis_total_network_weight_units,
        300_000_000_000
    );
    assert_eq!(reward_block.block_reward, 1_200);
    assert_eq!(reward_block.fixed_player_reward, 1_200);
    assert_eq!(second.player_reward_tick_artifact.block_reward, 1_200);
    assert_eq!(
        second.player_reward_tick_artifact.fixed_player_reward,
        1_200
    );

    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn collect_tick_accumulates_twelve_five_minute_ticks_into_one_hour_reward_block() {
    let _guard = foreground_override_lock().lock().unwrap();
    let mut config = test_config("daemon-player-reward-hour-block");
    config.runtime.target_app_ids = vec![1];
    config.runtime.poll_interval_secs = 300;
    config.runtime.slots_per_epoch = 24;
    config.runtime.game_process_names = vec!["TestGame.exe".into()];
    config.reward = RewardConfig {
        reward_source: pole_protocol_draft::RewardSourceMode::Static,
        emission_year: 1,
        reward_block_secs: 3_600,
        player_block_reward: 1_000,
        reward_adjustment_period_blocks: 2,
        target_network_weight_units: 3_600_000_000_000,
        reward_adjustment_cap_bps: 2_000,
        collect_reward_bps: 5_000,
        store_reward_bps: 2_500,
        verify_reward_bps: 1_500,
        propose_reward_bps: 1_000,
        tail_emission_start_year: 4,
        tail_emission_rate_bps: 200,
        game_mappings: vec![RewardGameMapping {
            process_name: "TestGame.exe".into(),
            app_id: 1,
            game_coefficient_ppm: 1_000_000,
        }],
    };
    let data_dir = PathBuf::from(&config.runtime.data_dir);
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir).unwrap();
    }

    let previous = std::env::var_os("POLE_CLIENT_FOREGROUND_PROCESS_OVERRIDE");
    std::env::set_var("POLE_CLIENT_FOREGROUND_PROCESS_OVERRIDE", "TestGame.exe");

    let client = SequencedHttpClient::new(vec![1_000; 12]);
    let mut progress = LocalNodeProgress::default_from_config(&config);
    let mut results = Vec::new();
    for _ in 0..12 {
        results.push(run_collect_tick_with_client(&config, &mut progress, &client).unwrap());
    }

    match previous {
        Some(value) => std::env::set_var("POLE_CLIENT_FOREGROUND_PROCESS_OVERRIDE", value),
        None => std::env::remove_var("POLE_CLIENT_FOREGROUND_PROCESS_OVERRIDE"),
    }

    for result in &results[..11] {
        assert_eq!(result.artifact.player_reward_block_count, 0);
        assert_eq!(
            result
                .player_reward_tick_artifact
                .completed_reward_block_count,
            0
        );
        assert!(result.player_reward_tick_artifact.records.is_empty());
    }

    let final_result = results.last().unwrap();
    assert_eq!(final_result.artifact.player_reward_block_count, 1);
    assert_eq!(
        final_result
            .player_reward_tick_artifact
            .completed_reward_block_count,
        1
    );
    assert_eq!(final_result.player_reward_tick_artifact.records.len(), 1);
    assert_eq!(
        final_result.player_reward_tick_artifact.records[0].play_seconds,
        3_600
    );
    assert_eq!(
        final_result.player_reward_tick_artifact.records[0].sampled_interval_secs,
        3_600
    );
    assert_eq!(
        final_result.player_reward_tick_artifact.records[0].total_network_weight_units,
        3_600_000_000_000
    );
    assert_eq!(final_result.player_reward_tick_artifact.block_reward, 1_000);

    std::fs::remove_dir_all(data_dir).unwrap();
}
