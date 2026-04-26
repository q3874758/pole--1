use pole_protocol_draft::{
    batch_announcement_from_assembled, build_inmemory_simulation_network,
    challenge_announcement_from_challenge, inmemory_simulation_listener_peer_ids,
    inmemory_simulation_retrieval_peer_id, replica_receipt_announcement_from_record, BatchBuilder,
    Challenge, ChallengeEvidenceRef, ChallengeKind, ChallengeState, FilesystemP2pNetwork,
    InMemoryP2pNetwork, LocalRetentionBook, NodePipelineError, P2pError, P2pMessage, P2pNetwork,
    P2pSimulationConfig, P2pTopic, SocketP2pNetwork, SocketPeerProfile, SteamCurrentPlayersSample,
};
use std::net::SocketAddr;
use std::thread;
use std::time::Duration;

fn fixed32(byte: u8) -> [u8; 32] {
    [byte; 32]
}

fn sample_observation(
    epoch_id: u64,
    slot_id: u64,
    collector_id: [u8; 32],
    app_id: u32,
    players: u64,
    body: &str,
) -> Result<pole_protocol_draft::ObservationRecord, NodePipelineError> {
    SteamCurrentPlayersSample::steam_current_players(app_id, players, slot_id * 1000, body)
        .into_observation(epoch_id, slot_id, collector_id, vec![1, 2, 3])
}

#[test]
fn peers_receive_batch_and_receipt_gossip() {
    let collector = fixed32(1);
    let storer = fixed32(2);
    let listener = fixed32(3);

    let mut builder = BatchBuilder::new(1, collector);
    builder
        .push(sample_observation(1, 1, collector, 730, 1000, "a").unwrap())
        .unwrap();
    let assembled = builder.finalize(7).unwrap();

    let mut book = LocalRetentionBook::with_quota_gb(1);
    let stored = book
        .record_batch_payload(storer, 1, 2, &assembled.payload_bytes)
        .unwrap();

    let mut network = InMemoryP2pNetwork::default();
    network.register_peer(collector);
    network.register_peer(storer);
    network.register_peer(listener);
    network.subscribe(listener, P2pTopic::Batches).unwrap();
    network.subscribe(listener, P2pTopic::Receipts).unwrap();

    network
        .publish(
            collector,
            P2pMessage::Batch(batch_announcement_from_assembled(&assembled)),
        )
        .unwrap();
    network
        .publish(
            storer,
            P2pMessage::ReplicaReceipt(replica_receipt_announcement_from_record(&stored)),
        )
        .unwrap();

    let inbox = network.drain_inbox(listener).unwrap();
    assert_eq!(inbox.len(), 2);
    assert!(matches!(inbox[0].message, P2pMessage::Batch(_)));
    assert!(matches!(inbox[1].message, P2pMessage::ReplicaReceipt(_)));
}

#[test]
fn payload_can_be_requested_from_provider() {
    let provider = fixed32(4);
    let requester = fixed32(5);
    let payload = b"payload-bytes".to_vec();
    let payload_hash = pole_protocol_draft::stable_hash32(&payload);
    let payload_cid = pole_protocol_draft::cid_from_hash(payload_hash, "batch-payload");

    let mut network = InMemoryP2pNetwork::default();
    network.register_peer(provider);
    network.register_peer(requester);
    network
        .advertise_payload(provider, payload_cid.clone(), payload_hash, payload.clone())
        .unwrap();

    let response = network.request_payload(requester, &payload_cid).unwrap();
    assert_eq!(response.provider, provider);
    assert_eq!(response.payload_cid, payload_cid);
    assert_eq!(response.payload_hash, payload_hash);
    assert_eq!(response.payload_bytes, payload);
}

#[test]
fn bootstrap_peer_registers_subscriptions_and_discovery_view() {
    let source = fixed32(12);
    let listener = fixed32(13);

    let mut builder = BatchBuilder::new(2, source);
    builder
        .push(sample_observation(2, 1, source, 730, 42, "bootstrap").unwrap())
        .unwrap();
    let assembled = builder.finalize(9).unwrap();

    let mut network = InMemoryP2pNetwork::default();
    network.register_peer(source);
    network
        .bootstrap_peer(listener, &[P2pTopic::Batches, P2pTopic::Receipts])
        .unwrap();

    assert_eq!(network.known_peers(), vec![source, listener]);
    assert_eq!(
        network.subscriptions_for(listener).unwrap(),
        vec![P2pTopic::Batches, P2pTopic::Receipts]
    );

    network
        .publish(
            source,
            P2pMessage::Batch(batch_announcement_from_assembled(&assembled)),
        )
        .unwrap();
    assert_eq!(network.drain_inbox(listener).unwrap().len(), 1);
}

#[test]
fn bootstrap_topology_registers_multiple_peer_roles() {
    let source = fixed32(17);
    let batch_listener = fixed32(18);
    let receipt_listener = fixed32(19);
    let dual_listener = fixed32(20);

    let mut builder = BatchBuilder::new(4, source);
    builder
        .push(sample_observation(4, 1, source, 730, 77, "topology").unwrap())
        .unwrap();
    let assembled = builder.finalize(11).unwrap();

    let mut book = LocalRetentionBook::with_quota_gb(1);
    let stored = book
        .record_batch_payload(source, 4, 2, &assembled.payload_bytes)
        .unwrap();

    let mut network = InMemoryP2pNetwork::default();
    network
        .bootstrap_topology(&[
            pole_protocol_draft::InMemoryPeerProfile::new(source, []),
            pole_protocol_draft::InMemoryPeerProfile::new(batch_listener, [P2pTopic::Batches]),
            pole_protocol_draft::InMemoryPeerProfile::new(receipt_listener, [P2pTopic::Receipts]),
            pole_protocol_draft::InMemoryPeerProfile::new(
                dual_listener,
                [P2pTopic::Batches, P2pTopic::Receipts],
            ),
        ])
        .unwrap();

    assert_eq!(
        network.known_peers(),
        vec![source, batch_listener, receipt_listener, dual_listener]
    );
    assert_eq!(
        network.subscriptions_for(batch_listener).unwrap(),
        vec![P2pTopic::Batches]
    );
    assert_eq!(
        network.subscriptions_for(receipt_listener).unwrap(),
        vec![P2pTopic::Receipts]
    );
    assert_eq!(
        network.subscriptions_for(dual_listener).unwrap(),
        vec![P2pTopic::Batches, P2pTopic::Receipts]
    );

    assert_eq!(
        network
            .publish(
                source,
                P2pMessage::Batch(batch_announcement_from_assembled(&assembled)),
            )
            .unwrap(),
        2
    );
    assert_eq!(
        network
            .publish(
                source,
                P2pMessage::ReplicaReceipt(replica_receipt_announcement_from_record(&stored)),
            )
            .unwrap(),
        2
    );
    assert_eq!(network.drain_inbox(batch_listener).unwrap().len(), 1);
    assert_eq!(network.drain_inbox(receipt_listener).unwrap().len(), 1);
    assert_eq!(network.drain_inbox(dual_listener).unwrap().len(), 2);
}

#[test]
fn build_inmemory_simulation_network_uses_configured_listener_mix() {
    let topology = P2pSimulationConfig {
        batch_listener_count: 2,
        receipt_listener_count: 1,
        dual_listener_count: 2,
    };

    let network = build_inmemory_simulation_network(topology);
    let peer_ids = inmemory_simulation_listener_peer_ids(topology);

    assert_eq!(network.known_peers(), peer_ids);
    assert_eq!(peer_ids.len(), 5);
    assert_eq!(
        network.subscriptions_for(peer_ids[0]).unwrap(),
        vec![P2pTopic::Batches]
    );
    assert_eq!(
        network.subscriptions_for(peer_ids[1]).unwrap(),
        vec![P2pTopic::Batches]
    );
    assert_eq!(
        network.subscriptions_for(peer_ids[2]).unwrap(),
        vec![P2pTopic::Receipts]
    );
    assert_eq!(
        network.subscriptions_for(peer_ids[3]).unwrap(),
        vec![P2pTopic::Batches, P2pTopic::Receipts]
    );
    assert_eq!(
        network.subscriptions_for(peer_ids[4]).unwrap(),
        vec![P2pTopic::Batches, P2pTopic::Receipts]
    );
}

#[test]
fn simulation_retrieval_peer_prefers_first_available_listener() {
    let dual_first = P2pSimulationConfig {
        batch_listener_count: 1,
        receipt_listener_count: 1,
        dual_listener_count: 1,
    };
    let receipt_only = P2pSimulationConfig {
        batch_listener_count: 0,
        receipt_listener_count: 2,
        dual_listener_count: 0,
    };

    assert_eq!(
        inmemory_simulation_retrieval_peer_id(dual_first),
        inmemory_simulation_listener_peer_ids(dual_first)[0]
    );
    assert_eq!(
        inmemory_simulation_retrieval_peer_id(receipt_only),
        inmemory_simulation_listener_peer_ids(receipt_only)[0]
    );
}

#[test]
fn unsubscribe_and_remove_peer_stop_delivery_and_drop_provider_state() {
    let provider = fixed32(14);
    let listener = fixed32(15);
    let requester = fixed32(16);
    let payload = b"payload-bytes".to_vec();
    let payload_hash = pole_protocol_draft::stable_hash32(&payload);
    let payload_cid = pole_protocol_draft::cid_from_hash(payload_hash, "batch-payload");

    let mut network = InMemoryP2pNetwork::default();
    network.bootstrap_peer(provider, &[]).unwrap();
    network
        .bootstrap_peer(listener, &[P2pTopic::Batches, P2pTopic::Receipts])
        .unwrap();
    network.bootstrap_peer(requester, &[]).unwrap();
    network
        .advertise_payload(provider, payload_cid.clone(), payload_hash, payload)
        .unwrap();

    assert!(network.unsubscribe(listener, P2pTopic::Receipts).unwrap());
    assert_eq!(
        network.subscriptions_for(listener).unwrap(),
        vec![P2pTopic::Batches]
    );

    let mut builder = BatchBuilder::new(3, provider);
    builder
        .push(sample_observation(3, 1, provider, 730, 88, "lifecycle").unwrap())
        .unwrap();
    let assembled = builder.finalize(10).unwrap();
    network
        .publish(
            provider,
            P2pMessage::Batch(batch_announcement_from_assembled(&assembled)),
        )
        .unwrap();
    assert_eq!(network.drain_inbox(listener).unwrap().len(), 1);

    network.remove_peer(provider).unwrap();
    assert_eq!(network.known_peers(), vec![listener, requester]);
    assert!(matches!(
        network.request_payload(requester, &payload_cid),
        Err(P2pError::MissingPayload(_))
    ));
}

#[test]
fn challenge_gossip_uses_challenge_topic() {
    let source = fixed32(6);
    let sink = fixed32(7);

    let challenge = Challenge {
        challenge_id: fixed32(8),
        kind: ChallengeKind::BadStorage,
        epoch_id: 3,
        target_node: Some(fixed32(9)),
        challenger: fixed32(10),
        bond: 50,
        opened_at_height: 1,
        deadline_height: 9,
        state: ChallengeState::Open,
        evidence: ChallengeEvidenceRef {
            batch_root: None,
            aggregate_root: None,
            reward_root: None,
            payload_cid: Some("cid://payload".into()),
            merkle_proof: vec![fixed32(11)],
        },
    };

    let mut network = InMemoryP2pNetwork::default();
    network.register_peer(source);
    network.register_peer(sink);
    network.subscribe(sink, P2pTopic::Challenges).unwrap();

    network
        .publish(
            source,
            P2pMessage::Challenge(challenge_announcement_from_challenge(&challenge)),
        )
        .unwrap();

    let inbox = network.drain_inbox(sink).unwrap();
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].topic, P2pTopic::Challenges);
}

#[test]
fn filesystem_network_publishes_and_retrieves_payloads() {
    let root = std::env::temp_dir().join(format!("pole-fs-p2p-{}", std::process::id()));
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }

    let source = fixed32(21);
    let sink = fixed32(22);
    let requester = fixed32(23);
    let payload = b"filesystem-payload".to_vec();
    let payload_hash = pole_protocol_draft::stable_hash32(&payload);
    let payload_cid = pole_protocol_draft::cid_from_hash(payload_hash, "batch-payload");

    let mut network = FilesystemP2pNetwork::new(&root);
    network.bootstrap_peer(source, &[]).unwrap();
    network
        .bootstrap_peer(sink, &[P2pTopic::Batches, P2pTopic::Receipts])
        .unwrap();
    network.bootstrap_peer(requester, &[]).unwrap();

    let mut builder = BatchBuilder::new(9, source);
    builder
        .push(sample_observation(9, 1, source, 730, 12, "fs").unwrap())
        .unwrap();
    let assembled = builder.finalize(1).unwrap();
    let mut book = LocalRetentionBook::with_quota_gb(1);
    let stored = book
        .record_batch_payload(source, 9, 2, &assembled.payload_bytes)
        .unwrap();

    network
        .advertise_payload(source, payload_cid.clone(), payload_hash, payload.clone())
        .unwrap();
    network
        .publish(
            source,
            P2pMessage::Batch(batch_announcement_from_assembled(&assembled)),
        )
        .unwrap();
    network
        .publish(
            source,
            P2pMessage::ReplicaReceipt(replica_receipt_announcement_from_record(&stored)),
        )
        .unwrap();

    let response = network.request_payload(requester, &payload_cid).unwrap();
    assert_eq!(response.payload_bytes, payload);

    let inbox = network.drain_inbox(sink).unwrap();
    assert_eq!(inbox.len(), 2);

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn socket_network_publishes_and_retrieves_payloads() {
    let source = fixed32(31);
    let sink = fixed32(32);

    let sink_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let source_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let mut sink_network = SocketP2pNetwork::bind(sink, sink_addr, vec![]).unwrap();
    let sink_bound = sink_network.local_addr().unwrap();
    let mut source_network = SocketP2pNetwork::bind(
        source,
        source_addr,
        vec![SocketPeerProfile::new(
            sink,
            sink_bound,
            [P2pTopic::Batches, P2pTopic::Receipts],
        )],
    )
    .unwrap();
    source_network
        .bootstrap_peer(source, &[P2pTopic::Batches, P2pTopic::Receipts])
        .unwrap();
    sink_network
        .bootstrap_peer(sink, &[P2pTopic::Batches, P2pTopic::Receipts])
        .unwrap();

    let mut builder = BatchBuilder::new(11, source);
    builder
        .push(sample_observation(11, 1, source, 730, 77, "socket").unwrap())
        .unwrap();
    let assembled = builder.finalize(2).unwrap();
    let mut book = LocalRetentionBook::with_quota_gb(1);
    let stored = book
        .record_batch_payload(source, 11, 2, &assembled.payload_bytes)
        .unwrap();

    source_network
        .advertise_payload(
            source,
            assembled.payload_cid.clone(),
            assembled.payload_hash,
            assembled.payload_bytes.clone(),
        )
        .unwrap();
    source_network
        .publish(
            source,
            P2pMessage::Batch(batch_announcement_from_assembled(&assembled)),
        )
        .unwrap();
    source_network
        .publish(
            source,
            P2pMessage::ReplicaReceipt(replica_receipt_announcement_from_record(&stored)),
        )
        .unwrap();

    thread::sleep(Duration::from_millis(25));
    let inbox = sink_network.drain_inbox(sink).unwrap();
    assert_eq!(inbox.len(), 2);
    let response = sink_network
        .request_payload(sink, &assembled.payload_cid)
        .unwrap();
    assert_eq!(response.payload_bytes, assembled.payload_bytes);
}

#[test]
fn socket_network_learns_remote_peer_profile_from_hello() {
    let source = fixed32(41);
    let sink = fixed32(42);

    let mut sink_network =
        SocketP2pNetwork::bind(sink, "127.0.0.1:0".parse().unwrap(), vec![]).unwrap();
    let sink_bound = sink_network.local_addr().unwrap();
    let mut source_network = SocketP2pNetwork::bind(
        source,
        "127.0.0.1:0".parse().unwrap(),
        vec![SocketPeerProfile::new(
            sink,
            sink_bound,
            [P2pTopic::Batches, P2pTopic::Receipts],
        )],
    )
    .unwrap();

    sink_network
        .bootstrap_peer(sink, &[P2pTopic::Batches])
        .unwrap();
    source_network
        .bootstrap_peer(source, &[P2pTopic::Batches, P2pTopic::Receipts])
        .unwrap();

    std::thread::sleep(Duration::from_millis(25));
    let _ = sink_network.drain_inbox(sink).unwrap();

    let peers = sink_network.known_peers().unwrap();
    assert!(peers.contains(&source));
    let subscriptions = sink_network.subscriptions_for(source).unwrap();
    assert!(subscriptions.contains(&P2pTopic::Batches));
    assert!(subscriptions.contains(&P2pTopic::Receipts));
}

#[test]
fn socket_network_can_publish_back_to_learned_peer() {
    let source = fixed32(43);
    let sink = fixed32(44);

    let mut sink_network =
        SocketP2pNetwork::bind(sink, "127.0.0.1:0".parse().unwrap(), vec![]).unwrap();
    let sink_bound = sink_network.local_addr().unwrap();
    let mut source_network = SocketP2pNetwork::bind(
        source,
        "127.0.0.1:0".parse().unwrap(),
        vec![SocketPeerProfile::new(
            sink,
            sink_bound,
            [P2pTopic::Batches],
        )],
    )
    .unwrap();

    sink_network
        .bootstrap_peer(sink, &[P2pTopic::Batches, P2pTopic::Receipts])
        .unwrap();
    source_network
        .bootstrap_peer(source, &[P2pTopic::Batches, P2pTopic::Receipts])
        .unwrap();

    std::thread::sleep(Duration::from_millis(25));
    let _ = sink_network.drain_inbox(sink).unwrap();

    let mut builder = BatchBuilder::new(12, source);
    builder
        .push(sample_observation(12, 1, source, 730, 88, "backflow").unwrap())
        .unwrap();
    let assembled = builder.finalize(3).unwrap();
    source_network
        .publish(
            source,
            P2pMessage::Batch(batch_announcement_from_assembled(&assembled)),
        )
        .unwrap();
    std::thread::sleep(Duration::from_millis(25));
    assert_eq!(sink_network.drain_inbox(sink).unwrap().len(), 1);

    let mut book = LocalRetentionBook::with_quota_gb(1);
    let stored = book
        .record_batch_payload(sink, 12, 2, b"reply-payload")
        .unwrap();
    sink_network
        .publish(
            sink,
            P2pMessage::ReplicaReceipt(replica_receipt_announcement_from_record(&stored)),
        )
        .unwrap();
    std::thread::sleep(Duration::from_millis(25));
    let inbox = source_network.drain_inbox(source).unwrap();
    assert_eq!(inbox.len(), 1);
    assert!(matches!(inbox[0].message, P2pMessage::ReplicaReceipt(_)));
}

#[test]
fn socket_network_removes_learned_peer_after_goodbye() {
    let source = fixed32(45);
    let sink = fixed32(46);

    let mut sink_network =
        SocketP2pNetwork::bind(sink, "127.0.0.1:0".parse().unwrap(), vec![]).unwrap();
    let sink_bound = sink_network.local_addr().unwrap();
    let mut source_network = SocketP2pNetwork::bind(
        source,
        "127.0.0.1:0".parse().unwrap(),
        vec![SocketPeerProfile::new(
            sink,
            sink_bound,
            [P2pTopic::Batches],
        )],
    )
    .unwrap();

    sink_network
        .bootstrap_peer(sink, &[P2pTopic::Batches])
        .unwrap();
    source_network
        .bootstrap_peer(source, &[P2pTopic::Batches])
        .unwrap();

    std::thread::sleep(Duration::from_millis(25));
    let _ = sink_network.drain_inbox(sink).unwrap();
    assert!(sink_network.known_peers().unwrap().contains(&source));

    source_network.remove_peer(source).unwrap();
    std::thread::sleep(Duration::from_millis(25));
    let peers = sink_network.known_peers().unwrap();
    assert!(!peers.contains(&source));
}

#[test]
fn socket_network_goodbye_removes_learned_payload_provider_state() {
    let source = fixed32(47);
    let sink = fixed32(48);

    let mut sink_network =
        SocketP2pNetwork::bind(sink, "127.0.0.1:0".parse().unwrap(), vec![]).unwrap();
    let sink_bound = sink_network.local_addr().unwrap();
    let mut source_network = SocketP2pNetwork::bind(
        source,
        "127.0.0.1:0".parse().unwrap(),
        vec![SocketPeerProfile::new(
            sink,
            sink_bound,
            [P2pTopic::Batches, P2pTopic::Receipts],
        )],
    )
    .unwrap();

    sink_network
        .bootstrap_peer(sink, &[P2pTopic::Batches, P2pTopic::Receipts])
        .unwrap();
    source_network
        .bootstrap_peer(source, &[P2pTopic::Batches, P2pTopic::Receipts])
        .unwrap();

    let payload = b"goodbye-payload".to_vec();
    let payload_hash = pole_protocol_draft::stable_hash32(&payload);
    let payload_cid = pole_protocol_draft::cid_from_hash(payload_hash, "batch-payload");
    source_network
        .advertise_payload(source, payload_cid.clone(), payload_hash, payload.clone())
        .unwrap();
    std::thread::sleep(Duration::from_millis(25));
    let response = sink_network.request_payload(sink, &payload_cid).unwrap();
    assert_eq!(response.payload_bytes, payload);

    source_network.remove_peer(source).unwrap();
    std::thread::sleep(Duration::from_millis(25));
    assert!(matches!(
        sink_network.request_payload(sink, &payload_cid),
        Err(P2pError::MissingPayload(_))
    ));
}

#[test]
fn socket_network_can_learn_peer_from_publish_without_hello() {
    let source = fixed32(51);
    let sink = fixed32(52);

    let mut sink_network =
        SocketP2pNetwork::bind(sink, "127.0.0.1:0".parse().unwrap(), vec![]).unwrap();
    let sink_bound = sink_network.local_addr().unwrap();
    let mut source_network = SocketP2pNetwork::bind(
        source,
        "127.0.0.1:0".parse().unwrap(),
        vec![SocketPeerProfile::new(
            sink,
            sink_bound,
            [P2pTopic::Batches],
        )],
    )
    .unwrap();

    sink_network
        .bootstrap_peer(sink, &[P2pTopic::Batches])
        .unwrap();

    let mut builder = BatchBuilder::new(14, source);
    builder
        .push(sample_observation(14, 1, source, 730, 42, "learn-from-publish").unwrap())
        .unwrap();
    let assembled = builder.finalize(4).unwrap();
    source_network
        .publish(
            source,
            P2pMessage::Batch(batch_announcement_from_assembled(&assembled)),
        )
        .unwrap();

    std::thread::sleep(Duration::from_millis(25));
    let inbox = sink_network.drain_inbox(sink).unwrap();
    assert_eq!(inbox.len(), 1);
    let peers = sink_network.known_peers().unwrap();
    assert!(peers.contains(&source));
    let subscriptions = sink_network.subscriptions_for(source).unwrap();
    assert!(subscriptions.contains(&P2pTopic::Batches));
}

#[test]
fn socket_network_learned_peer_triggers_reverse_profile_announcement() {
    let source = fixed32(55);
    let sink = fixed32(56);

    let mut sink_network =
        SocketP2pNetwork::bind(sink, "127.0.0.1:0".parse().unwrap(), vec![]).unwrap();
    let sink_bound = sink_network.local_addr().unwrap();
    let mut source_network = SocketP2pNetwork::bind(
        source,
        "127.0.0.1:0".parse().unwrap(),
        vec![SocketPeerProfile::new(
            sink,
            sink_bound,
            [P2pTopic::Batches],
        )],
    )
    .unwrap();

    sink_network
        .bootstrap_peer(sink, &[P2pTopic::Batches, P2pTopic::Receipts])
        .unwrap();

    let mut builder = BatchBuilder::new(16, source);
    builder
        .push(sample_observation(16, 1, source, 730, 24, "reverse-hello").unwrap())
        .unwrap();
    let assembled = builder.finalize(5).unwrap();
    source_network
        .publish(
            source,
            P2pMessage::Batch(batch_announcement_from_assembled(&assembled)),
        )
        .unwrap();

    std::thread::sleep(Duration::from_millis(25));
    let _ = sink_network.drain_inbox(sink).unwrap();
    std::thread::sleep(Duration::from_millis(25));
    let subscriptions = source_network.subscriptions_for(sink).unwrap();
    assert!(subscriptions.contains(&P2pTopic::Batches));
    assert!(subscriptions.contains(&P2pTopic::Receipts));
}

#[test]
fn socket_network_updates_remote_topic_membership_after_unsubscribe() {
    let source = fixed32(49);
    let sink = fixed32(50);

    let mut sink_network =
        SocketP2pNetwork::bind(sink, "127.0.0.1:0".parse().unwrap(), vec![]).unwrap();
    let sink_bound = sink_network.local_addr().unwrap();
    let mut source_network = SocketP2pNetwork::bind(
        source,
        "127.0.0.1:0".parse().unwrap(),
        vec![SocketPeerProfile::new(
            sink,
            sink_bound,
            [P2pTopic::Batches, P2pTopic::Receipts],
        )],
    )
    .unwrap();

    sink_network
        .bootstrap_peer(sink, &[P2pTopic::Batches, P2pTopic::Receipts])
        .unwrap();
    source_network
        .bootstrap_peer(source, &[P2pTopic::Batches, P2pTopic::Receipts])
        .unwrap();

    std::thread::sleep(Duration::from_millis(25));
    let _ = source_network.known_peers().unwrap();
    sink_network.unsubscribe(sink, P2pTopic::Receipts).unwrap();
    std::thread::sleep(Duration::from_millis(25));
    let _ = source_network.known_peers().unwrap();
    let subscriptions = source_network.subscriptions_for(sink).unwrap();
    assert!(subscriptions.contains(&P2pTopic::Batches));
    assert!(!subscriptions.contains(&P2pTopic::Receipts));

    let mut book = LocalRetentionBook::with_quota_gb(1);
    let stored = book
        .record_batch_payload(source, 13, 2, b"receipt-after-unsub")
        .unwrap();
    assert!(matches!(
        source_network.publish(
            source,
            P2pMessage::ReplicaReceipt(replica_receipt_announcement_from_record(&stored))
        ),
        Err(P2pError::NoSubscribers(P2pTopic::Receipts))
    ));
}

#[test]
fn socket_network_updates_remote_topic_membership_after_subscribe() {
    let source = fixed32(53);
    let sink = fixed32(54);

    let mut sink_network =
        SocketP2pNetwork::bind(sink, "127.0.0.1:0".parse().unwrap(), vec![]).unwrap();
    let sink_bound = sink_network.local_addr().unwrap();
    let mut source_network = SocketP2pNetwork::bind(
        source,
        "127.0.0.1:0".parse().unwrap(),
        vec![SocketPeerProfile::new(
            sink,
            sink_bound,
            [P2pTopic::Batches],
        )],
    )
    .unwrap();

    sink_network
        .bootstrap_peer(sink, &[P2pTopic::Batches])
        .unwrap();
    source_network
        .bootstrap_peer(source, &[P2pTopic::Batches, P2pTopic::Receipts])
        .unwrap();

    std::thread::sleep(Duration::from_millis(25));
    let _ = source_network.known_peers().unwrap();
    let subscriptions = source_network.subscriptions_for(sink).unwrap();
    assert!(!subscriptions.contains(&P2pTopic::Receipts));

    sink_network.subscribe(sink, P2pTopic::Receipts).unwrap();
    std::thread::sleep(Duration::from_millis(25));
    let _ = source_network.known_peers().unwrap();
    let updated = source_network.subscriptions_for(sink).unwrap();
    assert!(updated.contains(&P2pTopic::Receipts));

    let mut book = LocalRetentionBook::with_quota_gb(1);
    let stored = book
        .record_batch_payload(source, 15, 2, b"receipt-after-sub")
        .unwrap();
    source_network
        .publish(
            source,
            P2pMessage::ReplicaReceipt(replica_receipt_announcement_from_record(&stored)),
        )
        .unwrap();
    std::thread::sleep(Duration::from_millis(25));
    let inbox = sink_network.drain_inbox(sink).unwrap();
    assert_eq!(inbox.len(), 1);
    assert!(matches!(inbox[0].message, P2pMessage::ReplicaReceipt(_)));
}

#[test]
fn socket_network_prunes_stale_learned_peer() {
    let source = fixed32(60);
    let sink = fixed32(61);

    let mut sink_network =
        SocketP2pNetwork::bind(sink, "127.0.0.1:0".parse().unwrap(), vec![]).unwrap();
    let sink_bound = sink_network.local_addr().unwrap();
    let mut source_network = SocketP2pNetwork::bind(
        source,
        "127.0.0.1:0".parse().unwrap(),
        vec![SocketPeerProfile::new(
            sink,
            sink_bound,
            [P2pTopic::Batches],
        )],
    )
    .unwrap();

    sink_network
        .bootstrap_peer(sink, &[P2pTopic::Batches])
        .unwrap();

    let mut builder = BatchBuilder::new(17, source);
    builder
        .push(sample_observation(17, 1, source, 730, 9, "stale-peer").unwrap())
        .unwrap();
    let assembled = builder.finalize(6).unwrap();
    source_network
        .publish(
            source,
            P2pMessage::Batch(batch_announcement_from_assembled(&assembled)),
        )
        .unwrap();

    std::thread::sleep(Duration::from_millis(25));
    let peers = sink_network.known_peers().unwrap();
    assert!(peers.contains(&source));

    std::thread::sleep(Duration::from_millis(350));
    let peers_after = sink_network.known_peers().unwrap();
    assert!(!peers_after.contains(&source));
}
