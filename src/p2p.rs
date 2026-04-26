use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::node_config::P2pSimulationConfig;
use crate::node_pipeline::AssembledBatch;
use crate::primitives::{ChallengeKind, ContentId, EpochId, Hash32, NodeId};
use crate::records::{Challenge, ObservationRecord, ReplicaReceipt};
use crate::storage_book::StoredPayloadRecord;

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub enum P2pTopic {
    Observations,
    Batches,
    Receipts,
    Challenges,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ObservationAnnouncement {
    pub observation: ObservationRecord,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct BatchAnnouncement {
    pub epoch_id: EpochId,
    pub collector_id: NodeId,
    pub payload_cid: ContentId,
    pub payload_hash: Hash32,
    pub payload_size_bytes: u64,
    pub batch_root: Hash32,
    pub obs_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ReplicaReceiptAnnouncement {
    pub receipt: ReplicaReceipt,
    pub payload_hash: Hash32,
    pub payload_size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ChallengeAnnouncement {
    pub challenge_id: Hash32,
    pub epoch_id: EpochId,
    pub kind: ChallengeKind,
    pub target_node: Option<NodeId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum P2pMessage {
    Observation(ObservationAnnouncement),
    Batch(BatchAnnouncement),
    ReplicaReceipt(ReplicaReceiptAnnouncement),
    Challenge(ChallengeAnnouncement),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct P2pEnvelope {
    pub from: NodeId,
    pub topic: P2pTopic,
    pub message: P2pMessage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayloadProviderRecord {
    pub provider: NodeId,
    pub payload_cid: ContentId,
    pub payload_hash: Hash32,
    pub payload_bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InMemoryPeerProfile {
    pub peer_id: NodeId,
    pub topics: Vec<P2pTopic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadResponse {
    pub provider: NodeId,
    pub payload_cid: ContentId,
    pub payload_hash: Hash32,
    pub payload_bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct P2pCoordinationStats {
    pub sent_count: u64,
    pub hello_sent_count: u64,
    pub hint_sent_count: u64,
    pub goodbye_sent_count: u64,
    pub received_count: u64,
    pub hello_received_count: u64,
    pub hint_received_count: u64,
    pub goodbye_received_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum P2pError {
    UnknownPeer(NodeId),
    MissingPayload(ContentId),
    NoSubscribers(P2pTopic),
    Io(String),
    Json(String),
}

impl fmt::Display for P2pError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownPeer(peer) => write!(f, "unknown peer {}", hex32(peer)),
            Self::MissingPayload(cid) => write!(f, "missing payload {cid}"),
            Self::NoSubscribers(topic) => write!(f, "no subscribers for topic {topic:?}"),
            Self::Io(err) => write!(f, "io error: {err}"),
            Self::Json(err) => write!(f, "json error: {err}"),
        }
    }
}

impl std::error::Error for P2pError {}

impl From<io::Error> for P2pError {
    fn from(value: io::Error) -> Self {
        Self::Io(value.to_string())
    }
}

impl From<serde_json::Error> for P2pError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value.to_string())
    }
}

pub trait P2pNetwork {
    fn backend_kind(&self) -> &'static str;
    fn learned_remote_peer_count(&self) -> Result<usize, P2pError>;
    fn coordination_stats(&self) -> Result<P2pCoordinationStats, P2pError>;
    fn register_peer(&mut self, peer_id: NodeId) -> Result<(), P2pError>;
    fn bootstrap_peer(&mut self, peer_id: NodeId, topics: &[P2pTopic]) -> Result<(), P2pError>;
    fn subscribe(&mut self, peer_id: NodeId, topic: P2pTopic) -> Result<(), P2pError>;
    fn unsubscribe(&mut self, peer_id: NodeId, topic: P2pTopic) -> Result<bool, P2pError>;
    fn remove_peer(&mut self, peer_id: NodeId) -> Result<(), P2pError>;
    fn known_peers(&self) -> Result<Vec<NodeId>, P2pError>;
    fn subscriptions_for(&self, peer_id: NodeId) -> Result<Vec<P2pTopic>, P2pError>;
    fn publish(&mut self, from: NodeId, message: P2pMessage) -> Result<usize, P2pError>;
    fn drain_inbox(&mut self, peer_id: NodeId) -> Result<Vec<P2pEnvelope>, P2pError>;
    fn advertise_payload(
        &mut self,
        provider: NodeId,
        payload_cid: ContentId,
        payload_hash: Hash32,
        payload_bytes: Vec<u8>,
    ) -> Result<(), P2pError>;
    fn request_payload(
        &self,
        requester: NodeId,
        payload_cid: &str,
    ) -> Result<PayloadResponse, P2pError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InMemoryP2pNetwork {
    peers: BTreeSet<NodeId>,
    subscriptions: BTreeMap<NodeId, BTreeSet<P2pTopic>>,
    inboxes: BTreeMap<NodeId, Vec<P2pEnvelope>>,
    payloads: BTreeMap<ContentId, Vec<PayloadProviderRecord>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct FilesystemPeerState {
    topics: Vec<P2pTopic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct FilesystemPayloadRecord {
    provider: NodeId,
    payload_cid: ContentId,
    payload_hash: Hash32,
    payload_bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FilesystemP2pNetwork {
    root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SocketPeerProfile {
    pub peer_id: NodeId,
    pub addr: SocketAddr,
    pub topics: Vec<P2pTopic>,
    #[serde(default)]
    pub last_seen_millis: u64,
}

#[derive(Debug)]
pub struct SocketP2pNetwork {
    local_peer_id: NodeId,
    socket: UdpSocket,
    local_topics: BTreeSet<P2pTopic>,
    configured_remote_peer_ids: BTreeSet<NodeId>,
    remote_peers: Mutex<BTreeMap<NodeId, SocketPeerProfile>>,
    inbox: Mutex<Vec<P2pEnvelope>>,
    payloads: Mutex<BTreeMap<ContentId, PayloadProviderRecord>>,
    coordination_stats: Mutex<P2pCoordinationStats>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum SocketWireMessage {
    PeerHello {
        profile: SocketPeerProfile,
    },
    PeerHint {
        profile: SocketPeerProfile,
    },
    PeerGoodbye {
        peer_id: NodeId,
    },
    Publish {
        from: NodeId,
        topic: P2pTopic,
        message: P2pMessage,
    },
    PayloadSync {
        record: PayloadProviderRecord,
    },
}

impl P2pMessage {
    pub fn topic(&self) -> P2pTopic {
        match self {
            Self::Observation(_) => P2pTopic::Observations,
            Self::Batch(_) => P2pTopic::Batches,
            Self::ReplicaReceipt(_) => P2pTopic::Receipts,
            Self::Challenge(_) => P2pTopic::Challenges,
        }
    }
}

impl InMemoryPeerProfile {
    pub fn new<I>(peer_id: NodeId, topics: I) -> Self
    where
        I: IntoIterator<Item = P2pTopic>,
    {
        let topics = topics
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        Self { peer_id, topics }
    }
}

pub fn build_inmemory_simulation_network(topology: P2pSimulationConfig) -> InMemoryP2pNetwork {
    let mut network = InMemoryP2pNetwork::default();
    network
        .bootstrap_topology(&inmemory_simulation_topology_profiles(topology))
        .expect("valid in-memory simulation topology");
    network
}

pub fn inmemory_simulation_topology_profiles(
    topology: P2pSimulationConfig,
) -> Vec<InMemoryPeerProfile> {
    let mut profiles = Vec::with_capacity(
        topology.batch_listener_count
            + topology.receipt_listener_count
            + topology.dual_listener_count,
    );

    for index in 0..topology.batch_listener_count {
        profiles.push(InMemoryPeerProfile::new(
            inmemory_simulation_peer_id(0x91, index),
            [P2pTopic::Batches],
        ));
    }
    for index in 0..topology.receipt_listener_count {
        profiles.push(InMemoryPeerProfile::new(
            inmemory_simulation_peer_id(0x92, index),
            [P2pTopic::Receipts],
        ));
    }
    for index in 0..topology.dual_listener_count {
        profiles.push(InMemoryPeerProfile::new(
            inmemory_simulation_peer_id(0x93, index),
            [P2pTopic::Batches, P2pTopic::Receipts],
        ));
    }

    profiles
}

pub fn inmemory_simulation_listener_peer_ids(topology: P2pSimulationConfig) -> Vec<NodeId> {
    inmemory_simulation_topology_profiles(topology)
        .into_iter()
        .map(|profile| profile.peer_id)
        .collect()
}

pub fn inmemory_simulation_retrieval_peer_id(topology: P2pSimulationConfig) -> NodeId {
    inmemory_simulation_listener_peer_ids(topology)
        .into_iter()
        .next()
        .expect("validated non-empty simulation topology")
}

fn inmemory_simulation_peer_id(group_tag: u8, index: usize) -> NodeId {
    let mut peer_id = [group_tag; 32];
    peer_id[30] = ((index >> 8) & 0xff) as u8;
    peer_id[31] = (index & 0xff) as u8;
    peer_id
}

impl InMemoryP2pNetwork {
    pub fn register_peer(&mut self, peer_id: NodeId) {
        self.peers.insert(peer_id);
        self.subscriptions.entry(peer_id).or_default();
        self.inboxes.entry(peer_id).or_default();
    }

    pub fn bootstrap_peer(&mut self, peer_id: NodeId, topics: &[P2pTopic]) -> Result<(), P2pError> {
        self.register_peer(peer_id);
        for &topic in topics {
            self.subscribe(peer_id, topic)?;
        }
        Ok(())
    }

    pub fn bootstrap_topology(&mut self, profiles: &[InMemoryPeerProfile]) -> Result<(), P2pError> {
        for profile in profiles {
            self.bootstrap_peer(profile.peer_id, &profile.topics)?;
        }
        Ok(())
    }

    pub fn subscribe(&mut self, peer_id: NodeId, topic: P2pTopic) -> Result<(), P2pError> {
        if !self.peers.contains(&peer_id) {
            return Err(P2pError::UnknownPeer(peer_id));
        }
        self.subscriptions.entry(peer_id).or_default().insert(topic);
        Ok(())
    }

    pub fn unsubscribe(&mut self, peer_id: NodeId, topic: P2pTopic) -> Result<bool, P2pError> {
        if !self.peers.contains(&peer_id) {
            return Err(P2pError::UnknownPeer(peer_id));
        }
        Ok(self
            .subscriptions
            .entry(peer_id)
            .or_default()
            .remove(&topic))
    }

    pub fn remove_peer(&mut self, peer_id: NodeId) -> Result<(), P2pError> {
        if !self.peers.remove(&peer_id) {
            return Err(P2pError::UnknownPeer(peer_id));
        }

        self.subscriptions.remove(&peer_id);
        self.inboxes.remove(&peer_id);
        self.payloads.retain(|_, providers| {
            providers.retain(|provider| provider.provider != peer_id);
            !providers.is_empty()
        });
        Ok(())
    }

    pub fn known_peers(&self) -> Vec<NodeId> {
        self.peers.iter().copied().collect()
    }

    pub fn subscriptions_for(&self, peer_id: NodeId) -> Result<Vec<P2pTopic>, P2pError> {
        if !self.peers.contains(&peer_id) {
            return Err(P2pError::UnknownPeer(peer_id));
        }
        Ok(self
            .subscriptions
            .get(&peer_id)
            .map(|topics| topics.iter().copied().collect())
            .unwrap_or_default())
    }

    pub fn publish(&mut self, from: NodeId, message: P2pMessage) -> Result<usize, P2pError> {
        if !self.peers.contains(&from) {
            return Err(P2pError::UnknownPeer(from));
        }

        let topic = message.topic();
        let recipients = self
            .subscriptions
            .iter()
            .filter_map(|(peer_id, topics)| {
                (peer_id != &from && topics.contains(&topic)).then_some(*peer_id)
            })
            .collect::<Vec<_>>();
        if recipients.is_empty() {
            return Err(P2pError::NoSubscribers(topic));
        }

        for recipient in &recipients {
            self.inboxes
                .entry(*recipient)
                .or_default()
                .push(P2pEnvelope {
                    from,
                    topic,
                    message: message.clone(),
                });
        }

        Ok(recipients.len())
    }

    pub fn drain_inbox(&mut self, peer_id: NodeId) -> Result<Vec<P2pEnvelope>, P2pError> {
        if !self.peers.contains(&peer_id) {
            return Err(P2pError::UnknownPeer(peer_id));
        }
        Ok(self.inboxes.entry(peer_id).or_default().drain(..).collect())
    }

    pub fn advertise_payload(
        &mut self,
        provider: NodeId,
        payload_cid: ContentId,
        payload_hash: Hash32,
        payload_bytes: Vec<u8>,
    ) -> Result<(), P2pError> {
        if !self.peers.contains(&provider) {
            return Err(P2pError::UnknownPeer(provider));
        }
        let providers = self.payloads.entry(payload_cid.clone()).or_default();
        if let Some(existing) = providers
            .iter_mut()
            .find(|entry| entry.provider == provider)
        {
            existing.payload_hash = payload_hash;
            existing.payload_bytes = payload_bytes;
        } else {
            providers.push(PayloadProviderRecord {
                provider,
                payload_cid,
                payload_hash,
                payload_bytes,
            });
        }
        Ok(())
    }

    pub fn request_payload(
        &self,
        requester: NodeId,
        payload_cid: &str,
    ) -> Result<PayloadResponse, P2pError> {
        if !self.peers.contains(&requester) {
            return Err(P2pError::UnknownPeer(requester));
        }

        let provider = self
            .payloads
            .get(payload_cid)
            .and_then(|providers| providers.first())
            .ok_or_else(|| P2pError::MissingPayload(payload_cid.to_string()))?;

        Ok(PayloadResponse {
            provider: provider.provider,
            payload_cid: provider.payload_cid.clone(),
            payload_hash: provider.payload_hash,
            payload_bytes: provider.payload_bytes.clone(),
        })
    }
}

impl FilesystemP2pNetwork {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn peers_dir(&self) -> PathBuf {
        self.root.join("peers")
    }

    fn peer_dir(&self, peer_id: NodeId) -> PathBuf {
        self.peers_dir().join(hex32(&peer_id))
    }

    fn peer_state_path(&self, peer_id: NodeId) -> PathBuf {
        self.peer_dir(peer_id).join("state.json")
    }

    fn peer_inbox_dir(&self, peer_id: NodeId) -> PathBuf {
        self.peer_dir(peer_id).join("inbox")
    }

    fn payload_dir(&self, payload_cid: &str) -> PathBuf {
        self.root
            .join("payloads")
            .join(payload_filename_stub_local(payload_cid))
    }

    fn payload_provider_path(&self, payload_cid: &str, provider: NodeId) -> PathBuf {
        self.payload_dir(payload_cid)
            .join(format!("{}.json", hex32(&provider)))
    }

    fn ensure_peer_dir(&self, peer_id: NodeId) -> Result<(), P2pError> {
        fs::create_dir_all(self.peer_inbox_dir(peer_id))?;
        if !self.peer_state_path(peer_id).exists() {
            self.write_peer_state(peer_id, &FilesystemPeerState { topics: Vec::new() })?;
        }
        Ok(())
    }

    fn peer_exists(&self, peer_id: NodeId) -> bool {
        self.peer_state_path(peer_id).exists()
    }

    fn read_peer_state(&self, peer_id: NodeId) -> Result<FilesystemPeerState, P2pError> {
        let content = fs::read_to_string(self.peer_state_path(peer_id))?;
        Ok(serde_json::from_str(&content)?)
    }

    fn write_peer_state(
        &self,
        peer_id: NodeId,
        state: &FilesystemPeerState,
    ) -> Result<(), P2pError> {
        fs::create_dir_all(self.peer_dir(peer_id))?;
        let content = serde_json::to_string_pretty(state)?;
        fs::write(self.peer_state_path(peer_id), content)?;
        Ok(())
    }

    fn inbox_envelope_path(&self, peer_id: NodeId, sequence: usize) -> PathBuf {
        self.peer_inbox_dir(peer_id)
            .join(format!("{sequence:016}.json"))
    }

    fn list_peer_ids(&self) -> Result<Vec<NodeId>, P2pError> {
        if !self.peers_dir().exists() {
            return Ok(Vec::new());
        }
        let mut peer_ids = fs::read_dir(self.peers_dir())?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter_map(|entry| decode_hex32(entry.file_name().to_string_lossy().as_bytes()).ok())
            .collect::<Vec<_>>();
        peer_ids.sort();
        Ok(peer_ids)
    }
}

impl SocketPeerProfile {
    pub fn new<I>(peer_id: NodeId, addr: SocketAddr, topics: I) -> Self
    where
        I: IntoIterator<Item = P2pTopic>,
    {
        let topics = topics
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        Self {
            peer_id,
            addr,
            topics,
            last_seen_millis: 0,
        }
    }
}

impl SocketP2pNetwork {
    const STALE_PEER_MILLIS: u64 = 250;

    pub fn bind(
        local_peer_id: NodeId,
        bind_addr: SocketAddr,
        remote_peers: Vec<SocketPeerProfile>,
    ) -> Result<Self, P2pError> {
        let socket = UdpSocket::bind(bind_addr)?;
        socket.set_nonblocking(true)?;
        Ok(Self {
            local_peer_id,
            socket,
            local_topics: BTreeSet::new(),
            configured_remote_peer_ids: remote_peers
                .iter()
                .map(|profile| profile.peer_id)
                .collect(),
            remote_peers: Mutex::new(
                remote_peers
                    .into_iter()
                    .map(|profile| (profile.peer_id, profile))
                    .collect(),
            ),
            inbox: Mutex::new(Vec::new()),
            payloads: Mutex::new(BTreeMap::new()),
            coordination_stats: Mutex::new(P2pCoordinationStats::default()),
        })
    }

    pub fn local_addr(&self) -> Result<SocketAddr, P2pError> {
        Ok(self.socket.local_addr()?)
    }

    fn pump_incoming(&self) -> Result<(), P2pError> {
        let deadline = Instant::now() + Duration::from_millis(20);
        loop {
            let mut buffer = vec![0u8; 65535];
            match self.socket.recv_from(&mut buffer) {
                Ok((size, sender_addr)) => {
                    let wire: SocketWireMessage = serde_json::from_slice(&buffer[..size])?;
                    match wire {
                        SocketWireMessage::PeerHello { mut profile } => {
                            self.coordination_stats
                                .lock()
                                .expect("socket coordination stats")
                                .received_count += 1;
                            self.coordination_stats
                                .lock()
                                .expect("socket coordination stats")
                                .hello_received_count += 1;
                            profile.addr = sender_addr;
                            profile.last_seen_millis = current_unix_millis();
                            self.remote_peers
                                .lock()
                                .expect("socket remote peers")
                                .insert(profile.peer_id, profile);
                        }
                        SocketWireMessage::PeerHint { profile } => {
                            self.coordination_stats
                                .lock()
                                .expect("socket coordination stats")
                                .received_count += 1;
                            self.coordination_stats
                                .lock()
                                .expect("socket coordination stats")
                                .hint_received_count += 1;
                            self.learn_remote_profile(profile);
                        }
                        SocketWireMessage::PeerGoodbye { peer_id } => {
                            self.coordination_stats
                                .lock()
                                .expect("socket coordination stats")
                                .received_count += 1;
                            self.coordination_stats
                                .lock()
                                .expect("socket coordination stats")
                                .goodbye_received_count += 1;
                            self.remote_peers
                                .lock()
                                .expect("socket remote peers")
                                .remove(&peer_id);
                            self.payloads
                                .lock()
                                .expect("socket payloads")
                                .retain(|_, record| record.provider != peer_id);
                        }
                        SocketWireMessage::Publish {
                            from,
                            topic,
                            message,
                        } => {
                            self.learn_remote_topic(from, sender_addr, topic);
                            if self.local_topics.contains(&topic) && from != self.local_peer_id {
                                self.inbox.lock().expect("socket inbox").push(P2pEnvelope {
                                    from,
                                    topic,
                                    message,
                                });
                            }
                        }
                        SocketWireMessage::PayloadSync { record } => {
                            self.learn_remote_peer(record.provider, sender_addr);
                            self.payloads
                                .lock()
                                .expect("socket payloads")
                                .insert(record.payload_cid.clone(), record);
                        }
                    }
                }
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return Ok(());
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(err) if cfg!(windows) && err.kind() == io::ErrorKind::ConnectionReset => {
                    if Instant::now() >= deadline {
                        return Ok(());
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(err) => return Err(P2pError::Io(err.to_string())),
            }
        }
    }

    fn send_wire(&self, addr: SocketAddr, wire: &SocketWireMessage) -> Result<(), P2pError> {
        let payload = serde_json::to_vec(wire)?;
        self.socket.send_to(&payload, addr)?;
        match wire {
            SocketWireMessage::PeerHello { .. } => {
                self.coordination_stats
                    .lock()
                    .expect("socket coordination stats")
                    .sent_count += 1;
                self.coordination_stats
                    .lock()
                    .expect("socket coordination stats")
                    .hello_sent_count += 1;
            }
            SocketWireMessage::PeerHint { .. } => {
                self.coordination_stats
                    .lock()
                    .expect("socket coordination stats")
                    .sent_count += 1;
                self.coordination_stats
                    .lock()
                    .expect("socket coordination stats")
                    .hint_sent_count += 1;
            }
            SocketWireMessage::PeerGoodbye { .. } => {
                self.coordination_stats
                    .lock()
                    .expect("socket coordination stats")
                    .sent_count += 1;
                self.coordination_stats
                    .lock()
                    .expect("socket coordination stats")
                    .goodbye_sent_count += 1;
            }
            _ => {}
        }
        Ok(())
    }

    fn announce_local_profile(&self) -> Result<(), P2pError> {
        let profile = SocketPeerProfile::new(
            self.local_peer_id,
            self.socket.local_addr()?,
            self.local_topics.iter().copied().collect::<Vec<_>>(),
        );
        for peer in self
            .remote_peers
            .lock()
            .expect("socket remote peers")
            .values()
            .cloned()
            .collect::<Vec<_>>()
        {
            self.send_wire(
                peer.addr,
                &SocketWireMessage::PeerHello {
                    profile: profile.clone(),
                },
            )?;
        }
        Ok(())
    }

    fn announce_goodbye(&self) -> Result<(), P2pError> {
        for peer in self
            .remote_peers
            .lock()
            .expect("socket remote peers")
            .values()
            .cloned()
            .collect::<Vec<_>>()
        {
            self.send_wire(
                peer.addr,
                &SocketWireMessage::PeerGoodbye {
                    peer_id: self.local_peer_id,
                },
            )?;
        }
        Ok(())
    }

    fn announce_peer_hint(&self, profile: SocketPeerProfile) -> Result<(), P2pError> {
        for peer in self
            .remote_peers
            .lock()
            .expect("socket remote peers")
            .values()
            .filter(|peer| peer.peer_id != profile.peer_id)
            .cloned()
            .collect::<Vec<_>>()
        {
            self.send_wire(
                peer.addr,
                &SocketWireMessage::PeerHint {
                    profile: profile.clone(),
                },
            )?;
        }
        Ok(())
    }

    fn learn_remote_profile(&self, mut profile: SocketPeerProfile) {
        if profile.peer_id == self.local_peer_id {
            return;
        }
        profile.last_seen_millis = current_unix_millis();
        let profile_peer_id = profile.peer_id;
        let mut peers = self.remote_peers.lock().expect("socket remote peers");
        let mut inserted = false;
        let mut topic_changed = false;
        peers
            .entry(profile.peer_id)
            .and_modify(|existing| {
                existing.addr = profile.addr;
                for topic in profile.topics.drain(..) {
                    if !existing.topics.contains(&topic) {
                        existing.topics.push(topic);
                        topic_changed = true;
                    }
                }
                if topic_changed {
                    existing.topics.sort();
                    existing.topics.dedup();
                }
            })
            .or_insert_with(|| {
                inserted = true;
                profile
            });
        let learned_profile = peers.get(&profile_peer_id).cloned();
        drop(peers);
        if let Some(learned_profile) = learned_profile.filter(|_| inserted || topic_changed) {
            let _ = self.announce_local_profile();
            let _ = self.announce_peer_hint(learned_profile);
        }
    }

    fn prune_stale_remote_peers(&self) {
        let now = current_unix_millis();
        self.remote_peers
            .lock()
            .expect("socket remote peers")
            .retain(|peer_id, profile| {
                self.configured_remote_peer_ids.contains(peer_id)
                    || now.saturating_sub(profile.last_seen_millis) <= Self::STALE_PEER_MILLIS
            });
    }

    fn learn_remote_peer(&self, peer_id: NodeId, addr: SocketAddr) {
        if peer_id == self.local_peer_id {
            return;
        }
        let seen = current_unix_millis();
        let mut peers = self.remote_peers.lock().expect("socket remote peers");
        let mut inserted = false;
        peers
            .entry(peer_id)
            .and_modify(|profile| {
                profile.addr = addr;
                profile.last_seen_millis = seen;
            })
            .or_insert_with(|| {
                inserted = true;
                let mut profile = SocketPeerProfile::new(peer_id, addr, Vec::<P2pTopic>::new());
                profile.last_seen_millis = seen;
                profile
            });
        let learned_profile = peers.get(&peer_id).cloned();
        drop(peers);
        if inserted {
            let _ = self.announce_local_profile();
            if let Some(profile) = learned_profile {
                let _ = self.announce_peer_hint(profile);
            }
        }
    }

    fn learn_remote_topic(&self, peer_id: NodeId, addr: SocketAddr, topic: P2pTopic) {
        if peer_id == self.local_peer_id {
            return;
        }
        let seen = current_unix_millis();
        let mut peers = self.remote_peers.lock().expect("socket remote peers");
        let mut inserted = false;
        let mut topic_changed = false;
        peers
            .entry(peer_id)
            .and_modify(|profile| {
                profile.addr = addr;
                profile.last_seen_millis = seen;
                if !profile.topics.contains(&topic) {
                    profile.topics.push(topic);
                    profile.topics.sort();
                    profile.topics.dedup();
                    topic_changed = true;
                }
            })
            .or_insert_with(|| {
                inserted = true;
                let mut profile = SocketPeerProfile::new(peer_id, addr, [topic]);
                profile.last_seen_millis = seen;
                profile
            });
        let learned_profile = peers.get(&peer_id).cloned();
        drop(peers);
        if inserted {
            let _ = self.announce_local_profile();
        }
        if let Some(learned_profile) = learned_profile.filter(|_| inserted || topic_changed) {
            let _ = self.announce_peer_hint(learned_profile);
        }
    }
}

impl P2pNetwork for InMemoryP2pNetwork {
    fn backend_kind(&self) -> &'static str {
        "inmemory-sim"
    }

    fn learned_remote_peer_count(&self) -> Result<usize, P2pError> {
        Ok(0)
    }

    fn coordination_stats(&self) -> Result<P2pCoordinationStats, P2pError> {
        Ok(P2pCoordinationStats::default())
    }

    fn register_peer(&mut self, peer_id: NodeId) -> Result<(), P2pError> {
        InMemoryP2pNetwork::register_peer(self, peer_id);
        Ok(())
    }

    fn bootstrap_peer(&mut self, peer_id: NodeId, topics: &[P2pTopic]) -> Result<(), P2pError> {
        InMemoryP2pNetwork::bootstrap_peer(self, peer_id, topics)
    }

    fn subscribe(&mut self, peer_id: NodeId, topic: P2pTopic) -> Result<(), P2pError> {
        InMemoryP2pNetwork::subscribe(self, peer_id, topic)
    }

    fn unsubscribe(&mut self, peer_id: NodeId, topic: P2pTopic) -> Result<bool, P2pError> {
        InMemoryP2pNetwork::unsubscribe(self, peer_id, topic)
    }

    fn remove_peer(&mut self, peer_id: NodeId) -> Result<(), P2pError> {
        InMemoryP2pNetwork::remove_peer(self, peer_id)
    }

    fn known_peers(&self) -> Result<Vec<NodeId>, P2pError> {
        Ok(InMemoryP2pNetwork::known_peers(self))
    }

    fn subscriptions_for(&self, peer_id: NodeId) -> Result<Vec<P2pTopic>, P2pError> {
        InMemoryP2pNetwork::subscriptions_for(self, peer_id)
    }

    fn publish(&mut self, from: NodeId, message: P2pMessage) -> Result<usize, P2pError> {
        InMemoryP2pNetwork::publish(self, from, message)
    }

    fn drain_inbox(&mut self, peer_id: NodeId) -> Result<Vec<P2pEnvelope>, P2pError> {
        InMemoryP2pNetwork::drain_inbox(self, peer_id)
    }

    fn advertise_payload(
        &mut self,
        provider: NodeId,
        payload_cid: ContentId,
        payload_hash: Hash32,
        payload_bytes: Vec<u8>,
    ) -> Result<(), P2pError> {
        InMemoryP2pNetwork::advertise_payload(
            self,
            provider,
            payload_cid,
            payload_hash,
            payload_bytes,
        )
    }

    fn request_payload(
        &self,
        requester: NodeId,
        payload_cid: &str,
    ) -> Result<PayloadResponse, P2pError> {
        InMemoryP2pNetwork::request_payload(self, requester, payload_cid)
    }
}

impl P2pNetwork for FilesystemP2pNetwork {
    fn backend_kind(&self) -> &'static str {
        "filesystem"
    }

    fn learned_remote_peer_count(&self) -> Result<usize, P2pError> {
        Ok(0)
    }

    fn coordination_stats(&self) -> Result<P2pCoordinationStats, P2pError> {
        Ok(P2pCoordinationStats::default())
    }

    fn register_peer(&mut self, peer_id: NodeId) -> Result<(), P2pError> {
        self.ensure_peer_dir(peer_id)
    }

    fn bootstrap_peer(&mut self, peer_id: NodeId, topics: &[P2pTopic]) -> Result<(), P2pError> {
        self.register_peer(peer_id)?;
        for &topic in topics {
            self.subscribe(peer_id, topic)?;
        }
        Ok(())
    }

    fn subscribe(&mut self, peer_id: NodeId, topic: P2pTopic) -> Result<(), P2pError> {
        if !self.peer_exists(peer_id) {
            return Err(P2pError::UnknownPeer(peer_id));
        }
        let mut state = self.read_peer_state(peer_id)?;
        if !state.topics.contains(&topic) {
            state.topics.push(topic);
            state.topics.sort();
            state.topics.dedup();
            self.write_peer_state(peer_id, &state)?;
        }
        Ok(())
    }

    fn unsubscribe(&mut self, peer_id: NodeId, topic: P2pTopic) -> Result<bool, P2pError> {
        if !self.peer_exists(peer_id) {
            return Err(P2pError::UnknownPeer(peer_id));
        }
        let mut state = self.read_peer_state(peer_id)?;
        let before = state.topics.len();
        state.topics.retain(|item| item != &topic);
        self.write_peer_state(peer_id, &state)?;
        Ok(before != state.topics.len())
    }

    fn remove_peer(&mut self, peer_id: NodeId) -> Result<(), P2pError> {
        if !self.peer_exists(peer_id) {
            return Err(P2pError::UnknownPeer(peer_id));
        }
        fs::remove_dir_all(self.peer_dir(peer_id))?;
        if self.root.join("payloads").exists() {
            for entry in fs::read_dir(self.root.join("payloads"))?.collect::<Result<Vec<_>, _>>()? {
                let path = entry.path().join(format!("{}.json", hex32(&peer_id)));
                if path.exists() {
                    fs::remove_file(&path)?;
                }
            }
        }
        Ok(())
    }

    fn known_peers(&self) -> Result<Vec<NodeId>, P2pError> {
        self.list_peer_ids()
    }

    fn subscriptions_for(&self, peer_id: NodeId) -> Result<Vec<P2pTopic>, P2pError> {
        if !self.peer_exists(peer_id) {
            return Err(P2pError::UnknownPeer(peer_id));
        }
        Ok(self.read_peer_state(peer_id)?.topics)
    }

    fn publish(&mut self, from: NodeId, message: P2pMessage) -> Result<usize, P2pError> {
        if !self.peer_exists(from) {
            return Err(P2pError::UnknownPeer(from));
        }
        let topic = message.topic();
        let recipients = self
            .known_peers()?
            .into_iter()
            .filter(|peer_id| peer_id != &from)
            .filter(|peer_id| {
                self.subscriptions_for(*peer_id)
                    .map(|topics| topics.contains(&topic))
                    .unwrap_or(false)
            })
            .collect::<Vec<_>>();
        if recipients.is_empty() {
            return Err(P2pError::NoSubscribers(topic));
        }
        for recipient in &recipients {
            fs::create_dir_all(self.peer_inbox_dir(*recipient))?;
            let existing = fs::read_dir(self.peer_inbox_dir(*recipient))?
                .collect::<Result<Vec<_>, _>>()?
                .len();
            let content = serde_json::to_string_pretty(&FilesystemEnvelopeSerde {
                from,
                topic,
                message: message.clone(),
            })?;
            fs::write(self.inbox_envelope_path(*recipient, existing), content)?;
        }
        Ok(recipients.len())
    }

    fn drain_inbox(&mut self, peer_id: NodeId) -> Result<Vec<P2pEnvelope>, P2pError> {
        if !self.peer_exists(peer_id) {
            return Err(P2pError::UnknownPeer(peer_id));
        }
        let inbox_dir = self.peer_inbox_dir(peer_id);
        if !inbox_dir.exists() {
            return Ok(Vec::new());
        }
        let mut entries = fs::read_dir(&inbox_dir)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|entry| entry.path());
        let mut out = Vec::with_capacity(entries.len());
        for entry in entries {
            let path = entry.path();
            let content = fs::read_to_string(&path)?;
            let serde: FilesystemEnvelopeSerde = serde_json::from_str(&content)?;
            out.push(P2pEnvelope {
                from: serde.from,
                topic: serde.topic,
                message: serde.message,
            });
            fs::remove_file(path)?;
        }
        Ok(out)
    }

    fn advertise_payload(
        &mut self,
        provider: NodeId,
        payload_cid: ContentId,
        payload_hash: Hash32,
        payload_bytes: Vec<u8>,
    ) -> Result<(), P2pError> {
        if !self.peer_exists(provider) {
            return Err(P2pError::UnknownPeer(provider));
        }
        fs::create_dir_all(self.payload_dir(&payload_cid))?;
        let record = FilesystemPayloadRecord {
            provider,
            payload_cid: payload_cid.clone(),
            payload_hash,
            payload_bytes,
        };
        fs::write(
            self.payload_provider_path(&payload_cid, provider),
            serde_json::to_string_pretty(&record)?,
        )?;
        Ok(())
    }

    fn request_payload(
        &self,
        requester: NodeId,
        payload_cid: &str,
    ) -> Result<PayloadResponse, P2pError> {
        if !self.peer_exists(requester) {
            return Err(P2pError::UnknownPeer(requester));
        }
        let payload_dir = self.payload_dir(payload_cid);
        if !payload_dir.exists() {
            return Err(P2pError::MissingPayload(payload_cid.to_string()));
        }
        let mut entries = fs::read_dir(payload_dir)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|entry| entry.path());
        let first = entries
            .into_iter()
            .next()
            .ok_or_else(|| P2pError::MissingPayload(payload_cid.to_string()))?;
        let content = fs::read_to_string(first.path())?;
        let record: FilesystemPayloadRecord = serde_json::from_str(&content)?;
        Ok(PayloadResponse {
            provider: record.provider,
            payload_cid: record.payload_cid,
            payload_hash: record.payload_hash,
            payload_bytes: record.payload_bytes,
        })
    }
}

impl P2pNetwork for SocketP2pNetwork {
    fn backend_kind(&self) -> &'static str {
        "socket"
    }

    fn learned_remote_peer_count(&self) -> Result<usize, P2pError> {
        self.pump_incoming()?;
        self.prune_stale_remote_peers();
        Ok(self
            .remote_peers
            .lock()
            .expect("socket remote peers")
            .keys()
            .filter(|peer_id| !self.configured_remote_peer_ids.contains(*peer_id))
            .count())
    }

    fn coordination_stats(&self) -> Result<P2pCoordinationStats, P2pError> {
        self.pump_incoming()?;
        Ok(self
            .coordination_stats
            .lock()
            .expect("socket coordination stats")
            .clone())
    }

    fn register_peer(&mut self, peer_id: NodeId) -> Result<(), P2pError> {
        if peer_id == self.local_peer_id
            || self
                .remote_peers
                .lock()
                .expect("socket remote peers")
                .contains_key(&peer_id)
        {
            return Ok(());
        }
        Err(P2pError::UnknownPeer(peer_id))
    }

    fn bootstrap_peer(&mut self, peer_id: NodeId, topics: &[P2pTopic]) -> Result<(), P2pError> {
        if peer_id != self.local_peer_id {
            return Err(P2pError::UnknownPeer(peer_id));
        }
        self.pump_incoming()?;
        self.local_topics = topics.iter().copied().collect();
        self.announce_local_profile()?;
        Ok(())
    }

    fn subscribe(&mut self, peer_id: NodeId, topic: P2pTopic) -> Result<(), P2pError> {
        if peer_id != self.local_peer_id {
            return Err(P2pError::UnknownPeer(peer_id));
        }
        self.pump_incoming()?;
        self.local_topics.insert(topic);
        self.announce_local_profile()?;
        Ok(())
    }

    fn unsubscribe(&mut self, peer_id: NodeId, topic: P2pTopic) -> Result<bool, P2pError> {
        if peer_id != self.local_peer_id {
            return Err(P2pError::UnknownPeer(peer_id));
        }
        self.pump_incoming()?;
        let removed = self.local_topics.remove(&topic);
        if removed {
            self.announce_local_profile()?;
        }
        Ok(removed)
    }

    fn remove_peer(&mut self, peer_id: NodeId) -> Result<(), P2pError> {
        if peer_id == self.local_peer_id {
            self.announce_goodbye()?;
            self.local_topics.clear();
            self.inbox.lock().expect("socket inbox").clear();
            self.payloads.lock().expect("socket payloads").clear();
            return Ok(());
        }
        self.remote_peers
            .lock()
            .expect("socket remote peers")
            .remove(&peer_id)
            .map(|_| ())
            .ok_or(P2pError::UnknownPeer(peer_id))
    }

    fn known_peers(&self) -> Result<Vec<NodeId>, P2pError> {
        self.pump_incoming()?;
        self.prune_stale_remote_peers();
        let mut peers = vec![self.local_peer_id];
        peers.extend(
            self.remote_peers
                .lock()
                .expect("socket remote peers")
                .keys()
                .copied(),
        );
        peers.sort();
        Ok(peers)
    }

    fn subscriptions_for(&self, peer_id: NodeId) -> Result<Vec<P2pTopic>, P2pError> {
        self.pump_incoming()?;
        if peer_id == self.local_peer_id {
            return Ok(self.local_topics.iter().copied().collect());
        }
        self.remote_peers
            .lock()
            .expect("socket remote peers")
            .get(&peer_id)
            .map(|profile| profile.topics.clone())
            .ok_or(P2pError::UnknownPeer(peer_id))
    }

    fn publish(&mut self, from: NodeId, message: P2pMessage) -> Result<usize, P2pError> {
        if from != self.local_peer_id {
            return Err(P2pError::UnknownPeer(from));
        }
        self.pump_incoming()?;
        let topic = message.topic();
        let remote_peers = self.remote_peers.lock().expect("socket remote peers");
        let recipients = remote_peers
            .values()
            .filter(|profile| profile.topics.contains(&topic))
            .collect::<Vec<_>>();
        if recipients.is_empty() {
            return Err(P2pError::NoSubscribers(topic));
        }
        for recipient in &recipients {
            self.send_wire(
                recipient.addr,
                &SocketWireMessage::Publish {
                    from,
                    topic,
                    message: message.clone(),
                },
            )?;
        }
        Ok(recipients.len())
    }

    fn drain_inbox(&mut self, peer_id: NodeId) -> Result<Vec<P2pEnvelope>, P2pError> {
        if peer_id != self.local_peer_id {
            if self
                .remote_peers
                .lock()
                .expect("socket remote peers")
                .contains_key(&peer_id)
            {
                self.pump_incoming()?;
                return Ok(Vec::new());
            }
            return Err(P2pError::UnknownPeer(peer_id));
        }
        self.pump_incoming()?;
        let mut guard = self.inbox.lock().expect("socket inbox");
        Ok(guard.drain(..).collect())
    }

    fn advertise_payload(
        &mut self,
        provider: NodeId,
        payload_cid: ContentId,
        payload_hash: Hash32,
        payload_bytes: Vec<u8>,
    ) -> Result<(), P2pError> {
        if provider != self.local_peer_id {
            return Err(P2pError::UnknownPeer(provider));
        }
        let record = PayloadProviderRecord {
            provider,
            payload_cid: payload_cid.clone(),
            payload_hash,
            payload_bytes,
        };
        self.payloads
            .lock()
            .expect("socket payloads")
            .insert(payload_cid, record.clone());
        for recipient in self
            .remote_peers
            .lock()
            .expect("socket remote peers")
            .values()
            .cloned()
            .collect::<Vec<_>>()
        {
            self.send_wire(
                recipient.addr,
                &SocketWireMessage::PayloadSync {
                    record: record.clone(),
                },
            )?;
        }
        Ok(())
    }

    fn request_payload(
        &self,
        requester: NodeId,
        payload_cid: &str,
    ) -> Result<PayloadResponse, P2pError> {
        if requester != self.local_peer_id
            && !self
                .remote_peers
                .lock()
                .expect("socket remote peers")
                .contains_key(&requester)
        {
            return Err(P2pError::UnknownPeer(requester));
        }
        self.pump_incoming()?;
        let record = self
            .payloads
            .lock()
            .expect("socket payloads")
            .get(payload_cid)
            .cloned()
            .ok_or_else(|| P2pError::MissingPayload(payload_cid.to_string()))?;
        Ok(PayloadResponse {
            provider: record.provider,
            payload_cid: record.payload_cid,
            payload_hash: record.payload_hash,
            payload_bytes: record.payload_bytes,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct FilesystemEnvelopeSerde {
    from: NodeId,
    topic: P2pTopic,
    message: P2pMessage,
}

fn payload_filename_stub_local(payload_cid: &str) -> String {
    payload_cid.replace("cid://", "").replace(['/', ':'], "_")
}

fn decode_hex32(bytes: &[u8]) -> Result<NodeId, P2pError> {
    if bytes.len() != 64 {
        return Err(P2pError::Json("invalid peer id hex length".into()));
    }
    let mut out = [0u8; 32];
    for index in 0..32 {
        let hi = decode_nibble(bytes[index * 2])?;
        let lo = decode_nibble(bytes[index * 2 + 1])?;
        out[index] = (hi << 4) | lo;
    }
    Ok(out)
}

fn decode_nibble(byte: u8) -> Result<u8, P2pError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(P2pError::Json("invalid peer id hex".into())),
    }
}

pub fn batch_announcement_from_assembled(batch: &AssembledBatch) -> BatchAnnouncement {
    BatchAnnouncement {
        epoch_id: batch.batch_commit.epoch_id,
        collector_id: batch.batch_commit.collector_id,
        payload_cid: batch.payload_cid.clone(),
        payload_hash: batch.payload_hash,
        payload_size_bytes: batch.payload_bytes.len() as u64,
        batch_root: batch.batch_commit.batch.root,
        obs_count: batch.batch_commit.obs_count,
    }
}

pub fn replica_receipt_announcement_from_record(
    record: &StoredPayloadRecord,
) -> ReplicaReceiptAnnouncement {
    ReplicaReceiptAnnouncement {
        receipt: record.receipt.clone(),
        payload_hash: record.payload_hash,
        payload_size_bytes: record.size_bytes,
    }
}

pub fn challenge_announcement_from_challenge(challenge: &Challenge) -> ChallengeAnnouncement {
    ChallengeAnnouncement {
        challenge_id: challenge.challenge_id,
        epoch_id: challenge.epoch_id,
        kind: challenge.kind,
        target_node: challenge.target_node,
    }
}

fn hex32(bytes: &NodeId) -> String {
    let mut out = String::with_capacity(64);
    for byte in bytes {
        out.push(char::from(b"0123456789abcdef"[(byte >> 4) as usize]));
        out.push(char::from(b"0123456789abcdef"[(byte & 0x0f) as usize]));
    }
    out
}

fn current_unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}
