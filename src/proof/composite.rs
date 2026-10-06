//! Composite multi-layer Proof of Engagement (PoLE) evaluation and graceful degradation.
//!
//! Synthesizes evidence chains across L1 (Binary Identity), L2 (GPU 3D Render), and L3 (Hardware RoT)
//! to establish an authoritative, transparent confidence rating for player sessions:
//! - **Tier 1 (Gold)**: Authenticode signed by trusted publisher + active foreground 3D GPU render + physical bare metal PC.
//! - **Tier 2 (Silver)**: Standard signed or Steam Manifest verified + 3D GPU render + physical PC.
//! - **Tier 3 (Bronze)**: Verified binary with 3D graphics runtime loaded.
//! - **Tier Degraded Fallback**: Missing 3D graphics runtime, VM detected, or unverified environment.
//! - **Untrusted**: Invalid PE header or corrupted binary.
//!
//! ### Architectural Note on L4 (Human Presence & Physical Boundary)
//! In PoLE's architectural design, **L4 is reserved for Human Presence & Boundary**
//! (acknowledging that software cannot definitively prove physical human presence against
//! hardware-level macros or physical bypasses). To prevent confusing degraded states with
//! physical human presence verification, fallback operation is explicitly designated as
//! [`ConfidenceTier::TierDegradedFallback`].

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::proof::l1_binary::{BinaryProof, ProofTier};
use crate::proof::l2_render::{RenderEngagementLevel, RenderProof};
use crate::proof::l3_hardware::{HardwareProof, TpmStatus};
use crate::proof::ProofError;

/// Overall evaluated confidence tier across L1, L2, and L3 evidence chains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ConfidenceTier {
    /// Tier 1 (Gold, 100): Valid commercial publisher Authenticode signature + active 3D GPU render + physical bare metal PC
    Tier1Gold = 100,
    /// Tier 2 (Silver, 80): Valid Authenticode / Steam Manifest + active 3D render + physical PC
    Tier2Silver = 80,
    /// Tier 3 (Bronze, 50): Valid PE binary + 3D graphics runtime loaded (background or foreground)
    Tier3Bronze = 50,
    /// Degraded Fallback (20): Missing 3D graphics runtime OR virtual machine detected (honest degradation with risk disclosure)
    TierDegradedFallback = 20,
    /// Untrusted (0): Invalid PE header, corrupted file, or nonexistent process
    Untrusted = 0,
}

/// Unified composite multi-layer proof for a game play session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositeProof {
    pub pid: u32,
    pub process_name: String,
    pub l1_binary: BinaryProof,
    pub l2_render: RenderProof,
    pub l3_hardware: HardwareProof,
    pub confidence_tier: ConfidenceTier,
    pub is_eligible_for_rewards: bool,
    pub degradation_reasons: Vec<String>,
    pub generated_at_millis: u64,
}

/// Evaluates and synthesizes individual L1, L2, and L3 proofs into a unified composite proof.
pub fn evaluate_composite_proof(
    l1: BinaryProof,
    l2: RenderProof,
    l3: HardwareProof,
) -> CompositeProof {
    let mut degradation_reasons = Vec::new();

    if l1.tier == ProofTier::Untrusted {
        degradation_reasons
            .push("L1: Executable file is invalid or not a valid PE binary".to_string());
    }
    if !l1.tier.is_reward_eligible() {
        degradation_reasons.push(
            "L1: Bare PE binary without Authenticode signature or Steam manifest is ineligible for rewards"
                .to_string(),
        );
    }
    if l2.engagement_level == RenderEngagementLevel::HeadlessOrMock {
        degradation_reasons.push(
            "L2: No recognized 3D graphics runtime (DirectX/Vulkan/OpenGL) loaded".to_string(),
        );
    }
    if !l3.is_bare_metal {
        degradation_reasons
            .push("L3: Process running inside virtualized hypervisor/cloud VPS".to_string());
    }
    if matches!(
        l3.tpm_status,
        TpmStatus::NotPresent | TpmStatus::ServiceDisabled
    ) {
        degradation_reasons
            .push("L3: Hardware TPM 2.0 is not available on this device".to_string());
    }

    let confidence_tier = if l1.tier == ProofTier::Untrusted {
        ConfidenceTier::Untrusted
    } else if l1.tier == ProofTier::L1SignedTrusted
        && l2.engagement_level == RenderEngagementLevel::ActiveForeground3D
        && l3.is_bare_metal
    {
        ConfidenceTier::Tier1Gold
    } else if l1.tier >= ProofTier::L1SteamManifest
        && l2.engagement_level >= RenderEngagementLevel::BackgroundInWorld3D
        && l3.is_bare_metal
    {
        ConfidenceTier::Tier2Silver
    } else if l1.tier >= ProofTier::L1BareBinary
        && l2.engagement_level >= RenderEngagementLevel::BackgroundInWorld3D
    {
        ConfidenceTier::Tier3Bronze
    } else {
        ConfidenceTier::TierDegradedFallback
    };

    let is_eligible_for_rewards =
        confidence_tier >= ConfidenceTier::Tier3Bronze && l1.tier.is_reward_eligible();

    let generated_at_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    CompositeProof {
        pid: l1.pid,
        process_name: l1.process_name.clone(),
        l1_binary: l1,
        l2_render: l2,
        l3_hardware: l3,
        confidence_tier,
        is_eligible_for_rewards,
        degradation_reasons,
        generated_at_millis,
    }
}

/// Generates a complete end-to-end composite proof for a target process ID.
pub fn generate_composite_proof(
    pid: u32,
    expected_app_id: Option<u32>,
) -> Result<CompositeProof, ProofError> {
    let l1 = crate::proof::l1_binary::generate_l1_binary_proof(pid, expected_app_id)?;
    let l2 = crate::proof::l2_render::generate_l2_render_proof(pid)?;
    let l3 = crate::proof::l3_hardware::generate_l3_hardware_proof();

    Ok(evaluate_composite_proof(l1, l2, l3))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_process_composite_proof() {
        let current_pid = std::process::id();
        let composite = generate_composite_proof(current_pid, None).expect("composite proof");
        assert_eq!(composite.pid, current_pid);
        assert!(!composite.process_name.is_empty());
        assert!(composite.generated_at_millis > 0);
    }

    #[test]
    fn test_composite_evaluation_tiers() {
        let l1 = crate::proof::l1_binary::generate_l1_binary_proof(std::process::id(), None)
            .expect("l1");
        let l2 = crate::proof::l2_render::generate_l2_render_proof(std::process::id()).expect("l2");
        let l3 = crate::proof::l3_hardware::generate_l3_hardware_proof();

        let evaluated = evaluate_composite_proof(l1, l2, l3);
        assert!(evaluated.confidence_tier <= ConfidenceTier::Tier1Gold);
    }

    #[test]
    fn test_bare_binary_ineligible_for_rewards() {
        use std::path::PathBuf;
        let raw = crate::proof::RawBinaryEvidence {
            pid: 1234,
            process_name: "notepad.exe".to_string(),
            full_path: PathBuf::from("C:\\Windows\\notepad.exe"),
            sha256: "abc...".to_string(),
            authenticode: crate::proof::AuthenticodeStatus::NotSigned,
            steam_manifest: None,
            captured_at_millis: 1000,
        };
        let l1 = BinaryProof {
            pid: 1234,
            process_name: "notepad.exe".to_string(),
            full_path: PathBuf::from("C:\\Windows\\notepad.exe"),
            sha256: "abc...".to_string(),
            tier: ProofTier::L1BareBinary,
            authenticode: crate::proof::AuthenticodeStatus::NotSigned,
            steam_manifest: None,
            verified_at_millis: 1000,
            raw_evidence: raw,
        };
        let l2 = RenderProof {
            pid: 1234,
            process_name: "notepad.exe".to_string(),
            is_foreground: true,
            primary_backend: crate::proof::GraphicsBackend::DirectX11,
            loaded_render_modules: vec!["d3d11.dll".to_string()],
            engagement_level: RenderEngagementLevel::ActiveForeground3D,
            working_set_bytes: 1024 * 1024,
            verified_at_millis: 1000,
        };
        let l3 = HardwareProof {
            tpm_status: TpmStatus::Tpm2Available {
                provider_name: "Microsoft Platform Crypto Provider".to_string(),
                is_hardware_bound: true,
            },
            environment: crate::proof::PlatformEnvironment::PhysicalBareMetal,
            is_bare_metal: true,
            verified_at_millis: 1000,
        };

        let evaluated = evaluate_composite_proof(l1, l2, l3);
        assert_eq!(evaluated.confidence_tier, ConfidenceTier::Tier3Bronze);
        assert!(
            !evaluated.is_eligible_for_rewards,
            "Bare PE binary must receive 0 reward weight"
        );
        assert!(evaluated
            .degradation_reasons
            .iter()
            .any(|r| r.contains("ineligible for rewards")));
    }
}
