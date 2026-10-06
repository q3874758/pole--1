//! Layered Proof of Engagement (PoLE) verification module.
//!
//! Provides verifiable hardware, OS, and runtime evidence chains replacing
//! weak client heuristics with cryptographic and kernel-backed proof:
//! - **L1 (Binary Identity)**: Executable physical path, PE integrity, SHA-256 fingerprint,
//!   offline WinVerifyTrust Authenticode signature validation, and Steam appmanifest matching.
//! - **L2 (Real 3D Engagement)**: Kernel GPU engine utilization counter (PDH),
//!   DirectX/Vulkan module detection, and WASAPI audio peak monitoring.
//! - **L3 (Hardware RoT & Non-Relayability)**: TPM 2.0 TBS quote, Secure Boot
//!   PCR measurements, and hardware-bound unexportable keys.
//! - **L4 (Human Presence & Boundary)**: Reserved for human presence attestation, acknowledging
//!   software-only boundaries against hardware macros and AFK devices.
//! - **Degraded Fallback**: Transparent confidence degradation when hardware features
//!   or code signatures are absent, acknowledging residual risks honestly.

pub mod composite;
pub mod l1_binary;
pub mod l2_render;
pub mod l3_hardware;

pub use composite::{
    evaluate_composite_proof, generate_composite_proof, CompositeProof, ConfidenceTier,
};

pub use l1_binary::{
    capture_raw_binary_evidence, evaluate_binary_tier, generate_l1_binary_proof,
    inspect_foreground_process_l1, is_known_trusted_publisher, AuthenticodeStatus, BinaryProof,
    ProofError, ProofTier, RawBinaryEvidence, SteamManifestInfo,
};

pub use l2_render::{
    classify_graphics_backend, detect_gpu_device, generate_l2_render_proof, is_process_foreground,
    query_dxgi_primary_adapter, GpuDeviceType, GraphicsBackend, RenderEngagementLevel, RenderProof,
};

pub use l3_hardware::{
    detect_platform_environment, generate_l3_hardware_proof, probe_tpm_status, HardwareProof,
    PlatformEnvironment, TpmStatus,
};
