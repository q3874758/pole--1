use std::fmt;
use std::net::SocketAddr;

use crate::node_config::P2pSocketPeerConfig;
use crate::p2p::{P2pTopic, SocketPeerProfile};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliParseError {
    EmptySocketPeers,
    EmptySocketTopics,
    MissingPeerId,
    MissingPeerAddr,
    MissingTopics,
    TooManyPeerSpecSegments,
    InvalidHexLength {
        field: &'static str,
        expected: usize,
        actual: usize,
    },
    InvalidHexCharacter {
        field: &'static str,
        index: usize,
        byte: u8,
    },
    InvalidSocketAddr {
        field: &'static str,
        value: String,
        message: String,
    },
    UnknownSocketTopic(String),
}

impl fmt::Display for CliParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptySocketPeers => write!(f, "at least one socket peer is required"),
            Self::EmptySocketTopics => write!(f, "at least one socket topic is required"),
            Self::MissingPeerId => write!(f, "missing peer id in socket peer spec"),
            Self::MissingPeerAddr => write!(f, "missing peer addr in socket peer spec"),
            Self::MissingTopics => write!(f, "missing topics in socket peer spec"),
            Self::TooManyPeerSpecSegments => write!(f, "too many @ segments in socket peer spec"),
            Self::InvalidHexLength {
                field,
                expected,
                actual,
            } => write!(
                f,
                "invalid hex length for {field}: expected {expected}, got {actual}"
            ),
            Self::InvalidHexCharacter { field, index, byte } => write!(
                f,
                "invalid hex character for {field} at {index}: 0x{byte:02x}"
            ),
            Self::InvalidSocketAddr {
                field,
                value,
                message,
            } => write!(f, "invalid socket address for {field} ({value}): {message}"),
            Self::UnknownSocketTopic(topic) => write!(f, "unknown socket topic {topic}"),
        }
    }
}

impl std::error::Error for CliParseError {}

pub fn parse_socket_peer_specs(specs: &str) -> Result<Vec<SocketPeerProfile>, CliParseError> {
    let peers = specs
        .split(';')
        .filter(|item| !item.is_empty())
        .map(parse_socket_peer_spec)
        .collect::<Result<Vec<_>, _>>()?;
    if peers.is_empty() {
        return Err(CliParseError::EmptySocketPeers);
    }
    Ok(peers)
}

pub fn socket_peers_from_config(
    peers: &[P2pSocketPeerConfig],
) -> Result<Vec<SocketPeerProfile>, CliParseError> {
    let parsed = peers
        .iter()
        .map(|peer| {
            Ok(SocketPeerProfile::new(
                decode_hex32(&peer.peer_id_hex, "runtime.p2p_socket.peers[].peer_id_hex")?,
                parse_socket_addr(&peer.addr, "runtime.p2p_socket.peers[].addr")?,
                parse_socket_topics(&peer.topics.join(","))?,
            ))
        })
        .collect::<Result<Vec<_>, CliParseError>>()?;
    if parsed.is_empty() {
        return Err(CliParseError::EmptySocketPeers);
    }
    Ok(parsed)
}

pub fn parse_socket_peer_spec(spec: &str) -> Result<SocketPeerProfile, CliParseError> {
    let mut parts = spec.split('@');
    let peer_id_hex = parts.next().ok_or(CliParseError::MissingPeerId)?;
    let peer_addr = parts.next().ok_or(CliParseError::MissingPeerAddr)?;
    let topics = parts.next().ok_or(CliParseError::MissingTopics)?;
    if parts.next().is_some() {
        return Err(CliParseError::TooManyPeerSpecSegments);
    }
    Ok(SocketPeerProfile::new(
        decode_hex32(peer_id_hex, "peer_id_hex")?,
        parse_socket_addr(peer_addr, "peer_addr")?,
        parse_socket_topics(topics)?,
    ))
}

pub fn parse_socket_topics(topics: &str) -> Result<Vec<P2pTopic>, CliParseError> {
    let parsed = topics
        .split(',')
        .filter(|item| !item.is_empty())
        .map(|item| match item {
            "batches" => Ok(P2pTopic::Batches),
            "receipts" => Ok(P2pTopic::Receipts),
            "challenges" => Ok(P2pTopic::Challenges),
            "observations" => Ok(P2pTopic::Observations),
            _ => Err(CliParseError::UnknownSocketTopic(item.to_string())),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if parsed.is_empty() {
        return Err(CliParseError::EmptySocketTopics);
    }
    Ok(parsed)
}

pub fn decode_hex32(input: &str, field: &'static str) -> Result<[u8; 32], CliParseError> {
    if input.len() != 64 {
        return Err(CliParseError::InvalidHexLength {
            field,
            expected: 64,
            actual: input.len(),
        });
    }
    let bytes = input.as_bytes();
    let mut out = [0u8; 32];
    for index in 0..32 {
        let hi = decode_nibble(bytes[index * 2], field, index * 2)?;
        let lo = decode_nibble(bytes[index * 2 + 1], field, index * 2 + 1)?;
        out[index] = (hi << 4) | lo;
    }
    Ok(out)
}

pub fn parse_socket_addr(input: &str, field: &'static str) -> Result<SocketAddr, CliParseError> {
    input
        .parse::<SocketAddr>()
        .map_err(|err| CliParseError::InvalidSocketAddr {
            field,
            value: input.to_string(),
            message: err.to_string(),
        })
}

fn decode_nibble(byte: u8, field: &'static str, index: usize) -> Result<u8, CliParseError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(CliParseError::InvalidHexCharacter { field, index, byte }),
    }
}
