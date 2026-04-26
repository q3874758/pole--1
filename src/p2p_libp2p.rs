use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::thread;
use std::time::Duration;

use crate::node_config::P2pLibp2pConfig;

// This module intentionally stays dependency-light for now. It models the
// runtime shape, discovery state, and peer lifecycle that a real libp2p swarm
// will need, but avoids binding to libp2p crates until the dependency source is
// usable in this workspace again.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Libp2pBootstrapPeer {
    pub peer_id: String,
    pub addr: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Libp2pBackendSkeleton {
    pub local_peer_id: String,
    pub listen_addrs: Vec<String>,
    pub bootstrap_peers: Vec<Libp2pBootstrapPeer>,
    pub kademlia_enabled: bool,
    pub mdns_enabled: bool,
    pub rendezvous_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Libp2pPeerEntry {
    pub peer_id: String,
    pub addr: String,
    pub discovered_via: DiscoveryKind,
    pub state: PeerConnectionState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiscoveryKind {
    Bootstrap,
    Kademlia,
    Mdns,
    Rendezvous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerConnectionState {
    Known,
    DialQueued,
    Connected,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Libp2pPeerTable {
    pub peers: BTreeMap<String, Libp2pPeerEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Libp2pDiscoveryState {
    pub enabled_kinds: BTreeSet<DiscoveryKind>,
    pub announced_peer_count: usize,
    pub dial_queue_size: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkeletonRuntimePhase {
    Created,
    Bootstrapping,
    Discovering,
    Running,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Libp2pRuntimeStateMachine {
    pub skeleton: Libp2pBackendSkeleton,
    pub peer_table: Libp2pPeerTable,
    pub discovery_state: Libp2pDiscoveryState,
    pub phase: SkeletonRuntimePhase,
    pub ticks_completed: u64,
    next_synthetic_peer_index: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Libp2pLoopReport {
    pub ticks_completed: u64,
    pub phase: SkeletonRuntimePhase,
    pub known_peer_count: usize,
    pub connected_peer_count: usize,
    pub announced_peer_count: usize,
}

#[cfg(feature = "real-libp2p")]
#[derive(libp2p::swarm::NetworkBehaviour)]
#[behaviour(prelude = "libp2p::swarm::derive_prelude")]
struct PoleLibp2pBehaviour {
    identify: libp2p::identify::Behaviour,
    kad: libp2p::swarm::behaviour::toggle::Toggle<
        libp2p::kad::Behaviour<libp2p::kad::store::MemoryStore>,
    >,
    mdns: libp2p::swarm::behaviour::toggle::Toggle<libp2p::mdns::tokio::Behaviour>,
    rendezvous: libp2p::swarm::behaviour::toggle::Toggle<libp2p::rendezvous::client::Behaviour>,
}

#[cfg(feature = "real-libp2p")]
pub struct RealLibp2pSwarmBuildReport {
    pub local_peer_id: String,
    pub listener_count: usize,
    pub bootstrap_peer_count: usize,
    pub kademlia_enabled: bool,
    pub mdns_enabled: bool,
    pub rendezvous_enabled: bool,
}

#[cfg(not(feature = "real-libp2p"))]
pub struct RealLibp2pSwarmBuildReport {
    pub local_peer_id: String,
    pub listener_count: usize,
    pub bootstrap_peer_count: usize,
    pub kademlia_enabled: bool,
    pub mdns_enabled: bool,
    pub rendezvous_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Libp2pBackendError {
    Disabled,
    Parse(String),
}

impl fmt::Display for Libp2pBackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled => write!(f, "libp2p backend is disabled in config"),
            Self::Parse(err) => write!(f, "libp2p config parse error: {err}"),
        }
    }
}

impl std::error::Error for Libp2pBackendError {}

pub fn build_libp2p_backend_skeleton(
    config: &P2pLibp2pConfig,
) -> Result<Libp2pBackendSkeleton, Libp2pBackendError> {
    if !config.enabled {
        return Err(Libp2pBackendError::Disabled);
    }

    for addr in &config.listen_addrs {
        validate_multiaddr_like(addr)?;
    }

    let bootstrap_peers = config
        .bootstrap_peers
        .iter()
        .map(|peer| {
            validate_multiaddr_like(&peer.addr)?;
            validate_peer_id_like(&peer.peer_id)?;
            Ok(Libp2pBootstrapPeer {
                peer_id: peer.peer_id.clone(),
                addr: peer.addr.clone(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let local_peer_id = format!("libp2p-{}", hex32(&crate::stable_hash32(b"pole-libp2p")));

    Ok(Libp2pBackendSkeleton {
        local_peer_id,
        listen_addrs: config.listen_addrs.clone(),
        bootstrap_peers,
        kademlia_enabled: config.discovery.kademlia,
        mdns_enabled: config.discovery.mdns,
        rendezvous_enabled: config.discovery.rendezvous,
    })
}

impl Libp2pPeerTable {
    pub fn upsert(
        &mut self,
        peer_id: impl Into<String>,
        addr: impl Into<String>,
        discovered_via: DiscoveryKind,
        state: PeerConnectionState,
    ) {
        let peer_id = peer_id.into();
        self.peers.insert(
            peer_id.clone(),
            Libp2pPeerEntry {
                peer_id,
                addr: addr.into(),
                discovered_via,
                state,
            },
        );
    }
}

impl Libp2pDiscoveryState {
    pub fn from_skeleton(skeleton: &Libp2pBackendSkeleton) -> Self {
        let mut enabled_kinds = BTreeSet::new();
        if skeleton.kademlia_enabled {
            enabled_kinds.insert(DiscoveryKind::Kademlia);
        }
        if skeleton.mdns_enabled {
            enabled_kinds.insert(DiscoveryKind::Mdns);
        }
        if skeleton.rendezvous_enabled {
            enabled_kinds.insert(DiscoveryKind::Rendezvous);
        }
        Self {
            enabled_kinds,
            announced_peer_count: 0,
            dial_queue_size: 0,
        }
    }
}

impl Libp2pRuntimeStateMachine {
    pub fn new(skeleton: Libp2pBackendSkeleton) -> Self {
        Self {
            discovery_state: Libp2pDiscoveryState::from_skeleton(&skeleton),
            skeleton,
            peer_table: Libp2pPeerTable::default(),
            phase: SkeletonRuntimePhase::Created,
            ticks_completed: 0,
            next_synthetic_peer_index: 0,
        }
    }

    pub fn start_bootstrap(&mut self) {
        self.phase = SkeletonRuntimePhase::Bootstrapping;
        for peer in &self.skeleton.bootstrap_peers {
            self.peer_table.upsert(
                peer.peer_id.clone(),
                peer.addr.clone(),
                DiscoveryKind::Bootstrap,
                PeerConnectionState::DialQueued,
            );
        }
        self.discovery_state.dial_queue_size = self
            .peer_table
            .peers
            .values()
            .filter(|peer| peer.state == PeerConnectionState::DialQueued)
            .count();
    }

    pub fn record_discovered_peer(
        &mut self,
        peer_id: impl Into<String>,
        addr: impl Into<String>,
        discovered_via: DiscoveryKind,
    ) {
        self.peer_table
            .upsert(peer_id, addr, discovered_via, PeerConnectionState::Known);
        self.discovery_state.announced_peer_count = self.peer_table.peers.len();
        if matches!(
            self.phase,
            SkeletonRuntimePhase::Created | SkeletonRuntimePhase::Bootstrapping
        ) {
            self.phase = SkeletonRuntimePhase::Discovering;
        }
    }

    pub fn mark_connected(&mut self, peer_id: &str) {
        if let Some(peer) = self.peer_table.peers.get_mut(peer_id) {
            peer.state = PeerConnectionState::Connected;
        }
        self.discovery_state.dial_queue_size = self
            .peer_table
            .peers
            .values()
            .filter(|peer| peer.state == PeerConnectionState::DialQueued)
            .count();
        self.phase = SkeletonRuntimePhase::Running;
    }

    pub fn tick(&mut self) {
        self.ticks_completed += 1;
        match self.phase {
            SkeletonRuntimePhase::Created => self.start_bootstrap(),
            SkeletonRuntimePhase::Bootstrapping => {
                let queued_ids = self
                    .peer_table
                    .peers
                    .values()
                    .filter(|peer| peer.state == PeerConnectionState::DialQueued)
                    .map(|peer| peer.peer_id.clone())
                    .collect::<Vec<_>>();
                for peer_id in queued_ids {
                    self.mark_connected(&peer_id);
                }
                if self
                    .discovery_state
                    .enabled_kinds
                    .contains(&DiscoveryKind::Mdns)
                {
                    self.record_synthetic_discovery(DiscoveryKind::Mdns);
                }
            }
            SkeletonRuntimePhase::Discovering => {
                let known_ids = self
                    .peer_table
                    .peers
                    .values()
                    .filter(|peer| peer.state == PeerConnectionState::Known)
                    .map(|peer| peer.peer_id.clone())
                    .collect::<Vec<_>>();
                for peer_id in known_ids {
                    self.mark_connected(&peer_id);
                }
                self.phase = SkeletonRuntimePhase::Running;
            }
            SkeletonRuntimePhase::Running => {
                if self
                    .discovery_state
                    .enabled_kinds
                    .contains(&DiscoveryKind::Kademlia)
                {
                    self.record_synthetic_discovery(DiscoveryKind::Kademlia);
                }
                let known_ids = self
                    .peer_table
                    .peers
                    .values()
                    .filter(|peer| peer.state == PeerConnectionState::Known)
                    .map(|peer| peer.peer_id.clone())
                    .collect::<Vec<_>>();
                for peer_id in known_ids {
                    self.mark_connected(&peer_id);
                }
            }
        }
    }

    pub fn report(&self) -> Libp2pLoopReport {
        Libp2pLoopReport {
            ticks_completed: self.ticks_completed,
            phase: self.phase,
            known_peer_count: self.peer_table.peers.len(),
            connected_peer_count: self
                .peer_table
                .peers
                .values()
                .filter(|peer| peer.state == PeerConnectionState::Connected)
                .count(),
            announced_peer_count: self.discovery_state.announced_peer_count,
        }
    }

    fn record_synthetic_discovery(&mut self, discovered_via: DiscoveryKind) {
        self.next_synthetic_peer_index += 1;
        let peer_id = format!("syntheticpeer{:016}", self.next_synthetic_peer_index);
        let addr = format!(
            "/ip4/127.0.0.1/tcp/{}",
            5000 + self.next_synthetic_peer_index
        );
        self.record_discovered_peer(peer_id, addr, discovered_via);
    }
}

pub fn run_libp2p_skeleton_loop(
    skeleton: Libp2pBackendSkeleton,
    max_ticks: u64,
    tick_interval: Duration,
) -> Libp2pLoopReport {
    let mut runtime = Libp2pRuntimeStateMachine::new(skeleton);
    for tick in 0..max_ticks {
        runtime.tick();
        if tick + 1 < max_ticks && !tick_interval.is_zero() {
            thread::sleep(tick_interval);
        }
    }
    runtime.report()
}

#[cfg(feature = "real-libp2p")]
pub fn build_real_libp2p_swarm_report(
    config: &P2pLibp2pConfig,
) -> Result<RealLibp2pSwarmBuildReport, Libp2pBackendError> {
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|err| Libp2pBackendError::Parse(format!("tokio runtime init failed: {err}")))?;
    runtime.block_on(async { build_real_libp2p_swarm_report_inner(config).await })
}

#[cfg(feature = "real-libp2p")]
async fn build_real_libp2p_swarm_report_inner(
    config: &P2pLibp2pConfig,
) -> Result<RealLibp2pSwarmBuildReport, Libp2pBackendError> {
    use libp2p::SwarmBuilder;

    if !config.enabled {
        return Err(Libp2pBackendError::Disabled);
    }

    let listen_addrs = config
        .listen_addrs
        .iter()
        .map(|addr| parse_multiaddr(addr))
        .collect::<Result<Vec<_>, _>>()?;
    let bootstrap_peers = config
        .bootstrap_peers
        .iter()
        .map(|peer| Ok((parse_peer_id(&peer.peer_id)?, parse_multiaddr(&peer.addr)?)))
        .collect::<Result<Vec<_>, Libp2pBackendError>>()?;

    let mut swarm = SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            Default::default(),
            libp2p::noise::Config::new,
            libp2p::yamux::Config::default,
        )
        .map_err(|err| Libp2pBackendError::Parse(err.to_string()))?
        .with_quic()
        .with_behaviour(|key| {
            let local_peer_id = key.public().to_peer_id();
            let identify = libp2p::identify::Behaviour::new(libp2p::identify::Config::new(
                "/pole/libp2p/0.1.0".to_string(),
                key.public(),
            ));
            let kad = config.discovery.kademlia.then(|| {
                libp2p::kad::Behaviour::new(
                    local_peer_id,
                    libp2p::kad::store::MemoryStore::new(local_peer_id),
                )
            });
            let mdns = if config.discovery.mdns {
                Some(
                    libp2p::mdns::tokio::Behaviour::new(
                        libp2p::mdns::Config::default(),
                        local_peer_id,
                    )
                    .map_err(|err| Libp2pBackendError::Parse(err.to_string()))?,
                )
            } else {
                None
            };
            let rendezvous = config
                .discovery
                .rendezvous
                .then(|| libp2p::rendezvous::client::Behaviour::new(key.clone()));

            Ok(PoleLibp2pBehaviour {
                identify,
                kad: kad.into(),
                mdns: mdns.into(),
                rendezvous: rendezvous.into(),
            })
        })
        .map_err(|err| Libp2pBackendError::Parse(err.to_string()))?
        .build();

    for addr in &listen_addrs {
        swarm
            .listen_on(addr.clone())
            .map_err(|err| Libp2pBackendError::Parse(err.to_string()))?;
    }
    for (peer_id, addr) in &bootstrap_peers {
        if let Some(kad) = swarm.behaviour_mut().kad.as_mut() {
            kad.add_address(peer_id, addr.clone());
        }
    }

    Ok(RealLibp2pSwarmBuildReport {
        local_peer_id: swarm.local_peer_id().to_string(),
        listener_count: listen_addrs.len(),
        bootstrap_peer_count: bootstrap_peers.len(),
        kademlia_enabled: config.discovery.kademlia,
        mdns_enabled: config.discovery.mdns,
        rendezvous_enabled: config.discovery.rendezvous,
    })
}

#[cfg(not(feature = "real-libp2p"))]
pub fn build_real_libp2p_swarm_report(
    config: &P2pLibp2pConfig,
) -> Result<RealLibp2pSwarmBuildReport, Libp2pBackendError> {
    let skeleton = build_libp2p_backend_skeleton(config)?;
    Ok(RealLibp2pSwarmBuildReport {
        local_peer_id: skeleton.local_peer_id,
        listener_count: skeleton.listen_addrs.len(),
        bootstrap_peer_count: skeleton.bootstrap_peers.len(),
        kademlia_enabled: skeleton.kademlia_enabled,
        mdns_enabled: skeleton.mdns_enabled,
        rendezvous_enabled: skeleton.rendezvous_enabled,
    })
}

fn validate_multiaddr_like(addr: &str) -> Result<(), Libp2pBackendError> {
    if !addr.starts_with('/')
        || addr
            .split('/')
            .filter(|segment| !segment.is_empty())
            .count()
            < 2
    {
        return Err(Libp2pBackendError::Parse(format!(
            "multiaddr-like address {addr} must start with '/' and contain protocol/value segments"
        )));
    }
    Ok(())
}

#[cfg(feature = "real-libp2p")]
fn parse_multiaddr(addr: &str) -> Result<libp2p::Multiaddr, Libp2pBackendError> {
    addr.parse::<libp2p::Multiaddr>()
        .map_err(|err| Libp2pBackendError::Parse(format!("invalid multiaddr {addr}: {err}")))
}

fn validate_peer_id_like(peer_id: &str) -> Result<(), Libp2pBackendError> {
    if peer_id.len() < 16 || !peer_id.chars().all(|ch| ch.is_ascii_alphanumeric()) {
        return Err(Libp2pBackendError::Parse(format!(
            "peer id {peer_id} must be an ascii-alphanumeric libp2p-style identifier"
        )));
    }
    Ok(())
}

#[cfg(feature = "real-libp2p")]
fn parse_peer_id(peer_id: &str) -> Result<libp2p_identity::PeerId, Libp2pBackendError> {
    peer_id
        .parse::<libp2p_identity::PeerId>()
        .map_err(|err| Libp2pBackendError::Parse(format!("invalid peer id {peer_id}: {err}")))
}

fn hex32(bytes: &[u8; 32]) -> String {
    crate::hex_32(*bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_config::{P2pLibp2pBootstrapPeerConfig, P2pLibp2pConfig};

    #[test]
    fn builds_real_libp2p_skeleton_from_config() {
        let config = P2pLibp2pConfig {
            enabled: true,
            listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".to_string()],
            bootstrap_peers: vec![P2pLibp2pBootstrapPeerConfig {
                peer_id: "12D3KooWJ5Z5L6hG1Zq1x3wQ5P5ZkJ7V3xZ6QYp6iYvJpR6J8W8J".to_string(),
                addr: "/ip4/127.0.0.1/tcp/4001".to_string(),
            }],
            discovery: Default::default(),
        };
        let skeleton = build_libp2p_backend_skeleton(&config).unwrap();
        assert!(!skeleton.local_peer_id.is_empty());
        assert_eq!(skeleton.listen_addrs.len(), 1);
        assert_eq!(skeleton.bootstrap_peers.len(), 1);
    }

    #[test]
    fn runtime_state_machine_tracks_bootstrap_and_discovery() {
        let config = P2pLibp2pConfig {
            enabled: true,
            listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".to_string()],
            bootstrap_peers: vec![P2pLibp2pBootstrapPeerConfig {
                peer_id: "12D3KooWJ5Z5L6hG1Zq1x3wQ5P5ZkJ7V3xZ6QYp6iYvJpR6J8W8J".to_string(),
                addr: "/ip4/127.0.0.1/tcp/4001".to_string(),
            }],
            discovery: Default::default(),
        };
        let skeleton = build_libp2p_backend_skeleton(&config).unwrap();
        let mut runtime = Libp2pRuntimeStateMachine::new(skeleton);

        runtime.start_bootstrap();
        assert_eq!(runtime.phase, SkeletonRuntimePhase::Bootstrapping);
        assert_eq!(runtime.discovery_state.dial_queue_size, 1);

        runtime.record_discovered_peer(
            "12D3KooWDiscoveredPeer123456",
            "/ip4/127.0.0.1/tcp/4010",
            DiscoveryKind::Mdns,
        );
        assert_eq!(runtime.phase, SkeletonRuntimePhase::Discovering);
        assert_eq!(runtime.discovery_state.announced_peer_count, 2);

        runtime.mark_connected("12D3KooWJ5Z5L6hG1Zq1x3wQ5P5ZkJ7V3xZ6QYp6iYvJpR6J8W8J");
        assert_eq!(runtime.phase, SkeletonRuntimePhase::Running);
    }

    #[test]
    fn skeleton_loop_produces_running_report() {
        let config = P2pLibp2pConfig {
            enabled: true,
            listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".to_string()],
            bootstrap_peers: vec![P2pLibp2pBootstrapPeerConfig {
                peer_id: "12D3KooWJ5Z5L6hG1Zq1x3wQ5P5ZkJ7V3xZ6QYp6iYvJpR6J8W8J".to_string(),
                addr: "/ip4/127.0.0.1/tcp/4001".to_string(),
            }],
            discovery: Default::default(),
        };
        let skeleton = build_libp2p_backend_skeleton(&config).unwrap();
        let report = run_libp2p_skeleton_loop(skeleton, 3, Duration::ZERO);
        assert_eq!(report.phase, SkeletonRuntimePhase::Running);
        assert!(report.known_peer_count >= 1);
        assert_eq!(report.ticks_completed, 3);
    }

    #[test]
    fn builds_real_swarm_report_from_config() {
        let config = P2pLibp2pConfig {
            enabled: true,
            listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".to_string()],
            bootstrap_peers: vec![P2pLibp2pBootstrapPeerConfig {
                peer_id: "12D3KooWJ5Z5L6hG1Zq1x3wQ5P5ZkJ7V3xZ6QYp6iYvJpR6J8W8J".to_string(),
                addr: "/ip4/127.0.0.1/tcp/4001".to_string(),
            }],
            discovery: Default::default(),
        };
        let report = build_real_libp2p_swarm_report(&config).unwrap();
        assert!(!report.local_peer_id.is_empty());
        assert_eq!(report.listener_count, 1);
        assert_eq!(report.bootstrap_peer_count, 1);
        assert!(report.kademlia_enabled);
    }
}
