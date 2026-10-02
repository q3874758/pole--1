//! Construction of the on-chain **mutual proof** objects: a node's signed
//! [`PlaySession`] claim, the [`PlayHeartbeat`] liveness proofs that cover
//! its declared play window, and an independent node's
//! [`WitnessAttestation`] carrying its *own* observation.
//!
//! These are the Rust-side producers for the chain messages
//! `MsgSubmitPlaySession` / `MsgSubmitPlayHeartbeat` / `MsgAttestSession`.
//! The chain is the authority on whether a claim is true (see
//! `chain/x/pole/keeper/session.go`); this module only guarantees the
//! objects it hands over are internally consistent, correctly addressed and
//! signed, so they are not rejected for shape reasons:
//!
//! * `node_address` is the **account** address derived from the identity
//!   public key (`sha256(pubkey)[..20]`). This is the field the chain turns
//!   into a signer, so it must match the broadcast key.
//! * `collector_address` is the **node id**-derived address
//!   (`node_id[..20]`), matching how every node renders a peer's collector
//!   id from a batch announcement. Both the player and every witness derive
//!   it the same way, which is what makes the chain's
//!   "a witness cannot attest a session it collected" rule meaningful.
//! * Signatures cover the canonical payload with the signature field
//!   cleared, so they survive serialization round-trips.
//!
//! No policy lives here: the witness-independence rules are re-checked by
//! the chain on `AttestSession`, and the local guards below exist only to
//! fail fast before spending a broadcast.

use std::fmt;

use crate::node_config::NodeConfig;
use crate::node_daemon::NodeDaemonError;
use crate::primitives::{Address, AppId, EpochId, Hash32, NodeId, SlotId, UnixMillis};
use crate::records::{PlayHeartbeat, PlaySession, WitnessAttestation};
use crate::wallet::KeyPair;

pub const DEFAULT_BECH32_PREFIX: &str = "cosmos";

/// Everything a play-session claim needs beyond the node identity. The
/// observation fields describe the collecting node's own data for the same
/// `(app_id, epoch_id, slot_id)`; under self-collection `collector_id` is
/// the player's own node id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaySessionInputs<'a> {
    pub app_id: AppId,
    pub epoch_id: EpochId,
    pub slot_id: SlotId,
    pub play_seconds: u64,
    pub collector_id: NodeId,
    pub observation_cid: &'a str,
    pub observed_players: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MutualProofError {
    /// The witness is the playing node itself.
    WitnessIsPlayer,
    /// The witness collected the session it is trying to attest.
    WitnessIsCollector,
    /// The witness presented the session's own observation payload.
    WitnessObservationReused,
    /// A required observation payload id was empty.
    EmptyObservationCid,
    /// A bech32/hex conversion or identity lookup failed.
    Address(String),
}

impl fmt::Display for MutualProofError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WitnessIsPlayer => write!(f, "a node cannot witness its own play session"),
            Self::WitnessIsCollector => {
                write!(f, "a node cannot witness a play session it collected")
            }
            Self::WitnessObservationReused => write!(
                f,
                "the witness observation must not reuse the session's observation payload"
            ),
            Self::EmptyObservationCid => write!(f, "observation_cid is required"),
            Self::Address(err) => write!(f, "address error: {err}"),
        }
    }
}

impl std::error::Error for MutualProofError {}

/// Derive the on-chain **account** address of an Ed25519 identity, as a
/// 32-byte internal [`Address`] whose first 20 bytes are the account
/// identifier (so `address_to_bech32` reproduces it exactly).
pub fn identity_account_address(public_key: &[u8; 32]) -> Address {
    let account = crate::cosmos::address::cosmos_account_from_pubkey(public_key);
    address_from_account20(&account)
}

/// Same as [`identity_account_address`] but returns the bech32 form.
pub fn identity_account_bech32(public_key: &[u8; 32]) -> Result<String, MutualProofError> {
    crate::cosmos::address::encode_bech32(
        DEFAULT_BECH32_PREFIX,
        &crate::cosmos::address::cosmos_account_from_pubkey(public_key),
    )
    .map_err(|err| MutualProofError::Address(err.to_string()))
}

/// The collector address a node advertises for its own batches: the first
/// 20 bytes of its node id. Storing the full 32-byte node id in an
/// [`Address`] keeps `address_to_bech32` byte-identical to
/// `node_id_to_bech32`.
pub fn collector_address(node_id: &NodeId) -> Address {
    *node_id
}

fn address_from_account20(account: &[u8; 20]) -> Address {
    let mut out = [0u8; 32];
    out[..20].copy_from_slice(account);
    out
}

fn address_to_bech32(address: &Address) -> Result<String, MutualProofError> {
    crate::cosmos::address::encode_bech32(DEFAULT_BECH32_PREFIX, &address[..20])
        .map_err(|err| MutualProofError::Address(err.to_string()))
}

/// Deterministic session id, shared by the player and every witness so they
/// all name the same on-chain object. Derived from the claim's identity and
/// scope only — never from the signature, which is attached afterwards.
pub fn session_id_from_parts(
    node_id: &NodeId,
    app_id: AppId,
    epoch_id: EpochId,
    slot_id: SlotId,
    observation_cid: &str,
) -> Hash32 {
    let mut payload = Vec::with_capacity(64 + observation_cid.len());
    payload.extend_from_slice(b"pole-play-session-v1");
    payload.extend_from_slice(node_id);
    payload.extend_from_slice(&app_id.to_le_bytes());
    payload.extend_from_slice(&epoch_id.to_le_bytes());
    payload.extend_from_slice(&slot_id.to_le_bytes());
    payload.extend_from_slice(observation_cid.as_bytes());
    crate::node_pipeline::stable_hash32(&payload)
}

/// Build the node's signed claim that it played `play_seconds` of
/// `app_id` during one slot. The signature is made with the node identity
/// key (D1: the player *is* the node, so no separate player key exists).
pub fn build_play_session(
    config: &NodeConfig,
    identity: &KeyPair,
    inputs: &PlaySessionInputs<'_>,
) -> Result<PlaySession, MutualProofError> {
    if inputs.observation_cid.is_empty() {
        return Err(MutualProofError::EmptyObservationCid);
    }
    let node_id = config
        .node_id()
        .map_err(|err| MutualProofError::Address(err.to_string()))?;
    let mut session = PlaySession {
        session_id: session_id_from_parts(
            &node_id,
            inputs.app_id,
            inputs.epoch_id,
            inputs.slot_id,
            inputs.observation_cid,
        ),
        node_address: identity_account_address(&identity.public),
        app_id: inputs.app_id,
        epoch_id: inputs.epoch_id,
        slot_id: inputs.slot_id,
        play_seconds: inputs.play_seconds,
        collector_address: collector_address(&inputs.collector_id),
        observation_cid: inputs.observation_cid.to_string(),
        observed_players: inputs.observed_players,
        // The chain overwrites this from ctx.BlockHeight(); 0 means
        // "not yet known" rather than a claim about a height.
        submitted_at_height: 0,
        session_signature: Vec::new(),
    };
    session.session_signature = identity.sign(&session.signing_payload());
    Ok(session)
}

/// Build one signed liveness proof for `session`. `bucket_index` must be
/// `floor(offset_into_slot / heartbeat_bucket_seconds)` so distinct
/// heartbeats land in distinct buckets — settlement counts coverage by
/// bucket, and two proofs in one bucket buy nothing.
pub fn build_play_heartbeat(
    _config: &NodeConfig,
    identity: &KeyPair,
    session: &PlaySession,
    bucket_index: u64,
    signed_at_millis: UnixMillis,
) -> Result<PlayHeartbeat, MutualProofError> {
    let mut heartbeat = PlayHeartbeat {
        session_id: session.session_id,
        node_address: session.node_address,
        bucket_index,
        signed_at_millis,
        heartbeat_signature: Vec::new(),
    };
    heartbeat.heartbeat_signature = identity.sign(&heartbeat.signing_payload());
    Ok(heartbeat)
}

/// Build the caller's attestation of someone else's session, carrying the
/// caller's OWN observation. `own_observation_cid` must be a payload the
/// caller actually collected for the session's `(app_id, epoch_id, slot)`;
/// the chain rejects a witness whose observation equals the session's or
/// who collected that session.
pub fn build_witness_attestation(
    config: &NodeConfig,
    identity: &KeyPair,
    session: &PlaySession,
    own_observation_cid: &str,
    observed_play_seconds: u64,
    observed_players: u64,
) -> Result<WitnessAttestation, MutualProofError> {
    if own_observation_cid.is_empty() {
        return Err(MutualProofError::EmptyObservationCid);
    }
    let witness_address = identity_account_address(&identity.public);
    if witness_address == session.node_address {
        return Err(MutualProofError::WitnessIsPlayer);
    }
    // `collector_address` holds the collector's node id (the same 32 bytes
    // every peer derives from a batch announcement), so collector identity
    // is compared on node id, not on the account-address derivation.
    let witness_node_id = config
        .node_id()
        .unwrap_or_else(|_| crate::node_pipeline::stable_hash32(&identity.public));
    if witness_node_id == session.collector_address {
        return Err(MutualProofError::WitnessIsCollector);
    }
    if !session.observation_cid.is_empty() && own_observation_cid == session.observation_cid {
        return Err(MutualProofError::WitnessObservationReused);
    }
    let mut attestation = WitnessAttestation {
        session_id: session.session_id,
        witness_address,
        observed_play_seconds,
        witness_observation_cid: own_observation_cid.to_string(),
        observed_players,
        attested_at_height: 0,
        witness_signature: Vec::new(),
    };
    attestation.witness_signature = identity.sign(&attestation.signing_payload());
    Ok(attestation)
}

// ---------------------------------------------------------------------------
// Wire projection (records::* → pole.chain.pole.v1 wire types)
// ---------------------------------------------------------------------------

pub fn play_session_to_wire(
    session: &PlaySession,
) -> Result<crate::cosmos::wire_types::PlaySessionWire, MutualProofError> {
    Ok(crate::cosmos::wire_types::PlaySessionWire {
        session_id_hex: crate::hex_32(session.session_id),
        node_address: address_to_bech32(&session.node_address)?,
        app_id: session.app_id,
        epoch_id: session.epoch_id,
        slot_id: session.slot_id,
        play_seconds: session.play_seconds,
        collector_address: address_to_bech32(&session.collector_address)?,
        observation_cid: session.observation_cid.clone(),
        observed_players: session.observed_players,
        submitted_at_height: session.submitted_at_height as i64,
        session_signature: hex::encode(&session.session_signature),
    })
}

pub fn play_heartbeat_to_wire(
    heartbeat: &PlayHeartbeat,
) -> Result<crate::cosmos::wire_types::PlayHeartbeatWire, MutualProofError> {
    Ok(crate::cosmos::wire_types::PlayHeartbeatWire {
        session_id_hex: crate::hex_32(heartbeat.session_id),
        node_address: address_to_bech32(&heartbeat.node_address)?,
        bucket_index: heartbeat.bucket_index,
        signed_at_millis: heartbeat.signed_at_millis as i64,
        heartbeat_signature: hex::encode(&heartbeat.heartbeat_signature),
    })
}

pub fn witness_attestation_to_wire(
    attestation: &WitnessAttestation,
) -> Result<crate::cosmos::wire_types::WitnessAttestationWire, MutualProofError> {
    Ok(crate::cosmos::wire_types::WitnessAttestationWire {
        session_id_hex: crate::hex_32(attestation.session_id),
        witness_address: address_to_bech32(&attestation.witness_address)?,
        observed_play_seconds: attestation.observed_play_seconds,
        witness_observation_cid: attestation.witness_observation_cid.clone(),
        observed_players: attestation.observed_players,
        attested_at_height: attestation.attested_at_height as i64,
        witness_signature: hex::encode(&attestation.witness_signature),
    })
}

/// Persistence helpers, mirroring the verification-credential layout so all
/// mutual-proof artifacts live in one predictable place under `data_dir`.
pub fn play_session_path(config: &NodeConfig, session_id_hex: &str) -> std::path::PathBuf {
    std::path::Path::new(&config.runtime.data_dir)
        .join("play-sessions")
        .join(format!("{session_id_hex}.json"))
}

pub fn play_heartbeat_path(config: &NodeConfig, session_id_hex: &str) -> std::path::PathBuf {
    std::path::Path::new(&config.runtime.data_dir)
        .join("play-heartbeats")
        .join(format!("{session_id_hex}.json"))
}

pub fn witness_attestation_path(config: &NodeConfig, session_id_hex: &str) -> std::path::PathBuf {
    std::path::Path::new(&config.runtime.data_dir)
        .join("witness-attestations")
        .join(format!("{session_id_hex}.json"))
}

pub fn save_play_session(
    config: &NodeConfig,
    session: &PlaySession,
) -> Result<(), NodeDaemonError> {
    let session_id_hex = crate::hex_32(session.session_id);
    crate::json_file::save_pretty_json(session, play_session_path(config, &session_id_hex))
}

pub fn load_play_sessions(config: &NodeConfig) -> Vec<PlaySession> {
    load_directory::<PlaySession>(config, "play-sessions")
}

/// Heartbeats already recorded for one session, ordered by bucket index.
pub fn load_play_heartbeats(config: &NodeConfig, session_id_hex: &str) -> Vec<PlayHeartbeat> {
    crate::json_file::load_json_or_default::<Vec<PlayHeartbeat>, NodeDaemonError>(
        play_heartbeat_path(config, session_id_hex),
    )
    .unwrap_or_default()
}

pub fn save_play_heartbeat(
    config: &NodeConfig,
    heartbeat: &PlayHeartbeat,
) -> Result<(), NodeDaemonError> {
    let session_id_hex = crate::hex_32(heartbeat.session_id);
    let path = play_heartbeat_path(config, &session_id_hex);
    let mut existing: Vec<PlayHeartbeat> =
        crate::json_file::load_json_or_default::<Vec<PlayHeartbeat>, NodeDaemonError>(path.clone())
            .unwrap_or_default();
    if !existing
        .iter()
        .any(|current| current.bucket_index == heartbeat.bucket_index)
    {
        existing.push(heartbeat.clone());
        existing.sort_by_key(|current| current.bucket_index);
    }
    crate::json_file::save_pretty_json(&existing, path)
}

pub fn save_witness_attestation(
    config: &NodeConfig,
    attestation: &WitnessAttestation,
) -> Result<(), NodeDaemonError> {
    let session_id_hex = crate::hex_32(attestation.session_id);
    crate::json_file::save_pretty_json(
        attestation,
        witness_attestation_path(config, &session_id_hex),
    )
}

/// Hex session ids of this node's stored play sessions for one exact
/// `(epoch_id, slot_id)`, keyed by `app_id`.
///
/// Every session under `data_dir/play-sessions` was written by this node (the
/// directory is local), so `(epoch, slot, app)` identity is enough to pick the
/// right one. Used by the reward tick to stamp each `PlayerRewardBlockRecord`
/// with the chain session that justifies it; empty for offline / pre-chain runs
/// where no session was ever committed.
pub fn local_session_ids_for_slot(
    config: &NodeConfig,
    epoch_id: EpochId,
    slot_id: SlotId,
) -> std::collections::BTreeMap<AppId, String> {
    let mut by_app = std::collections::BTreeMap::new();
    for session in load_play_sessions(config) {
        if session.epoch_id == epoch_id && session.slot_id == slot_id {
            by_app
                .entry(session.app_id)
                .or_insert_with(|| crate::hex_32(session.session_id));
        }
    }
    by_app
}

fn load_directory<T>(config: &NodeConfig, name: &str) -> Vec<T>
where
    T: serde::de::DeserializeOwned,
{
    let dir = std::path::Path::new(&config.runtime.data_dir).join(name);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut paths = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    paths.sort();
    let mut values = Vec::new();
    for path in paths {
        if let Ok(value) = crate::json_file::load_json::<T, NodeDaemonError>(&path) {
            values.push(value);
        }
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_config(root: &std::path::Path, node_id_hex: String) -> NodeConfig {
        let mut config = NodeConfig::default();
        config.runtime.data_dir = root.to_string_lossy().into_owned();
        config.node_id_hex = node_id_hex;
        config
    }

    fn identity(seed: u8) -> KeyPair {
        KeyPair::from_seed(&[seed; 32])
    }

    fn inputs<'a>(collector_id: NodeId, cid: &'a str) -> PlaySessionInputs<'a> {
        PlaySessionInputs {
            app_id: 730,
            epoch_id: 7,
            slot_id: 3,
            play_seconds: 600,
            collector_id,
            observation_cid: cid,
            observed_players: 42,
        }
    }

    #[test]
    fn play_session_is_signed_and_addressable() {
        let root = std::env::temp_dir().join(format!("pole-mp-session-{}", std::process::id()));
        let keypair = identity(9);
        let node_id = crate::node_pipeline::stable_hash32(&keypair.public);
        let config = fixture_config(&root, crate::hex_32(node_id));
        let session = build_play_session(
            &config,
            &keypair,
            &inputs(node_id, "cid://batch-payload/aa"),
        )
        .unwrap();

        assert!(!session.session_signature.is_empty());
        // Signature covers the payload with the signature field cleared, so
        // it verifies against the pre-signing form.
        let mut unsigned = session.clone();
        unsigned.session_signature = Vec::new();
        assert!(crate::wallet::verify_signature(
            &keypair.public,
            &session.signing_payload(),
            &session.session_signature
        ));
        assert_eq!(session.signing_payload(), unsigned.signing_payload());

        // node_address is the *account* address, not the node-id address:
        // it must match the broadcasting key.
        let wire = play_session_to_wire(&session).unwrap();
        assert_eq!(
            wire.node_address,
            identity_account_bech32(&keypair.public).unwrap()
        );
        assert_eq!(
            wire.collector_address,
            crate::cosmos::address::node_id_to_bech32("cosmos", &node_id).unwrap()
        );
        assert_eq!(wire.session_id_hex, crate::hex_32(session.session_id));

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn session_id_is_deterministic_across_parties() {
        let node_id = [7u8; 32];
        let a = session_id_from_parts(&node_id, 730, 7, 3, "cid://x");
        let b = session_id_from_parts(&node_id, 730, 7, 3, "cid://x");
        assert_eq!(a, b);
        // Any scope change yields a different session.
        assert_ne!(a, session_id_from_parts(&node_id, 730, 7, 4, "cid://x"));
        assert_ne!(a, session_id_from_parts(&node_id, 730, 8, 3, "cid://x"));
        assert_ne!(a, session_id_from_parts(&node_id, 730, 7, 3, "cid://y"));
    }

    #[test]
    fn play_session_rejects_empty_observation() {
        let keypair = identity(1);
        let node_id = crate::node_pipeline::stable_hash32(&keypair.public);
        let config = fixture_config(
            &std::env::temp_dir().join("pole-mp-empty"),
            crate::hex_32(node_id),
        );
        let err = build_play_session(&config, &keypair, &inputs(node_id, "")).unwrap_err();
        assert_eq!(err, MutualProofError::EmptyObservationCid);
    }

    #[test]
    fn heartbeat_signature_is_verifiable_and_bucketed() {
        let keypair = identity(3);
        let node_id = crate::node_pipeline::stable_hash32(&keypair.public);
        let config = fixture_config(
            &std::env::temp_dir().join("pole-mp-heartbeat"),
            crate::hex_32(node_id),
        );
        let session = build_play_session(&config, &keypair, &inputs(node_id, "cid://s")).unwrap();
        let heartbeat =
            build_play_heartbeat(&config, &keypair, &session, 2, 1_700_000_000_000).unwrap();

        assert_eq!(heartbeat.session_id, session.session_id);
        assert_eq!(heartbeat.node_address, session.node_address);
        assert!(crate::wallet::verify_signature(
            &keypair.public,
            &heartbeat.signing_payload(),
            &heartbeat.heartbeat_signature
        ));
        let wire = play_heartbeat_to_wire(&heartbeat).unwrap();
        assert_eq!(wire.bucket_index, 2);
        assert_eq!(wire.signed_at_millis, 1_700_000_000_000);
        assert_eq!(wire.session_id_hex, crate::hex_32(session.session_id));

        // Persisting the same bucket twice must not double-count it.
        save_play_heartbeat(&config, &heartbeat).unwrap();
        save_play_heartbeat(&config, &heartbeat).unwrap();
        let stored: Vec<PlayHeartbeat> =
            crate::json_file::load_json::<Vec<PlayHeartbeat>, NodeDaemonError>(
                play_heartbeat_path(&config, &crate::hex_32(session.session_id)),
            )
            .unwrap();
        assert_eq!(stored.len(), 1);

        std::fs::remove_dir_all(&config.runtime.data_dir).ok();
    }

    #[test]
    fn witness_rejects_self_and_collector_attestation() {
        let player = identity(4);
        let player_node_id = crate::node_pipeline::stable_hash32(&player.public);
        let config = fixture_config(
            &std::env::temp_dir().join("pole-mp-witness"),
            crate::hex_32(player_node_id),
        );
        let session =
            build_play_session(&config, &player, &inputs(player_node_id, "cid://session")).unwrap();

        // The player cannot witness itself.
        assert_eq!(
            build_witness_attestation(&config, &player, &session, "cid://mine", 600, 42)
                .unwrap_err(),
            MutualProofError::WitnessIsPlayer
        );

        // A stranger's observation must differ from the session's. The
        // witness runs under its own node id, which is also how the chain
        // tells it apart from the session's collector.
        let witness = identity(5);
        let witness_node_id = crate::node_pipeline::stable_hash32(&witness.public);
        let witness_config = fixture_config(
            &std::env::temp_dir().join("pole-mp-witness-own"),
            crate::hex_32(witness_node_id),
        );
        assert_eq!(
            build_witness_attestation(
                &witness_config,
                &witness,
                &session,
                "cid://session",
                600,
                42
            )
            .unwrap_err(),
            MutualProofError::WitnessObservationReused
        );

        // A genuine independent attestation is produced and signed.
        let attestation = build_witness_attestation(
            &witness_config,
            &witness,
            &session,
            "cid://witness",
            600,
            43,
        )
        .unwrap();
        assert_eq!(attestation.session_id, session.session_id);
        assert!(crate::wallet::verify_signature(
            &witness.public,
            &attestation.signing_payload(),
            &attestation.witness_signature
        ));
        let wire = witness_attestation_to_wire(&attestation).unwrap();
        assert_eq!(
            wire.witness_address,
            identity_account_bech32(&witness.public).unwrap()
        );
        assert_eq!(wire.witness_observation_cid, "cid://witness");
        assert_ne!(wire.witness_address, wire.session_id_hex);

        // A witness whose own node id is the session's collector is refused.
        let collector_node_id = crate::node_pipeline::stable_hash32(&witness.public);
        let session2 = build_play_session(
            &config,
            &player,
            &inputs(collector_node_id, "cid://session2"),
        )
        .unwrap();
        assert_eq!(
            build_witness_attestation(&witness_config, &witness, &session2, "cid://w", 600, 42)
                .unwrap_err(),
            MutualProofError::WitnessIsCollector
        );

        std::fs::remove_dir_all(&config.runtime.data_dir).ok();
    }

    /// The reward tick must be able to name the session that justifies a reward
    /// block without any extra wiring: it looks up its own stored sessions by
    /// `(epoch_id, slot_id)`.
    #[test]
    fn stored_sessions_are_lookupable_by_slot() {
        let root = std::env::temp_dir().join(format!("pole-mp-slotlookup-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let keypair = identity(11);
        let node_id = crate::node_pipeline::stable_hash32(&keypair.public);
        let config = fixture_config(&root, crate::hex_32(node_id));

        assert!(local_session_ids_for_slot(&config, 7, 3).is_empty());

        let session =
            build_play_session(&config, &keypair, &inputs(node_id, "cid://slot")).unwrap();
        save_play_session(&config, &session).unwrap();

        let by_app = local_session_ids_for_slot(&config, 7, 3);
        assert_eq!(by_app.len(), 1);
        assert_eq!(by_app.get(&730), Some(&crate::hex_32(session.session_id)));
        // A different slot must not pick it up.
        assert!(local_session_ids_for_slot(&config, 7, 4).is_empty());
        assert!(local_session_ids_for_slot(&config, 8, 3).is_empty());

        std::fs::remove_dir_all(&root).ok();
    }
}
