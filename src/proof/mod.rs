//! Layered Proof of Engagement (PoLE) verification module.
//!
//! Provides verifiable hardware, OS, and runtime evidence chains replacing
//! weak client heuristics with cryptographic and kernel-backed proof:
//! - **L1 (Binary Identity)**: Executable physical path, PE integrity, SHA-256 fingerprint,
//!   offline WinVerifyTrust Authenticode signature validation, and Steam appmanifest matching.
//! - **L2 (Real 3D Engagement)**: (Planned) Kernel GPU engine utilization counter (PDH),
//!   DirectX/Vulkan module detection, and WASAPI audio peak monitoring.
//! - **L3 (Hardware RoT & Non-Relayability)**: (Planned) TPM 2.0 TBS quote, Secure Boot
//!   PCR measurements, and hardware-bound unexportable keys.
//! - **L4 (Graceful Fallback)**: Transparent confidence degradation when hardware features
//!   or code signatures are absent, acknowledging residual risks honestly.

pub mod l1_binary;

pub use l1_binary::{
    generate_l1_binary_proof, inspect_foreground_process_l1, is_known_trusted_publisher,
    AuthenticodeStatus, BinaryProof, ProofError, ProofTier, SteamManifestInfo,
};
