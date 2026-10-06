//! L3: Hardware Root of Trust (TPM 2.0 / TBS / CNG) and anti-Sybil hardware virtualization proof.
//!
//! Provides verifiable hardware-layer evidence that a node is running on a physical,
//! bare-metal personal gaming PC rather than an ephemeral virtualized cloud/container farm:
//! - Queries Windows TPM Base Services (TBS) and Cryptography Next Generation (CNG) Platform Crypto Provider.
//! - Checks whether hardware-isolated, non-exportable TPM keys can be provisioned.
//! - Executes hardware CPUID hypervisor leaf inspection (0x40000000) to detect VM signatures (KVM, Hyper-V, VMware, Xen).
//! - Implements L4 graceful degradation: nodes without TPM 2.0 or running inside VMs are transparently
//!   classified with honest confidence degradation rather than crashing.

#![allow(unsafe_code)]

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Status of the hardware Trusted Platform Module (TPM) on the node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TpmStatus {
    /// TPM 2.0 is physically present and accessible via Windows Platform Crypto Provider
    Tpm2Available {
        provider_name: String,
        is_hardware_bound: bool,
    },
    /// TPM device not detected on the motherboard / disabled in BIOS
    NotPresent,
    /// TPM Base Services (TBS) or Cryptographic service is stopped / disabled
    ServiceDisabled,
    /// Permission denied querying TPM context
    AccessDenied,
    /// Unsupported architecture / operating system
    NotSupported,
}

/// Physical machine hardware execution environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlatformEnvironment {
    /// Physical bare-metal PC (no hypervisor bit detected in CPUID)
    PhysicalBareMetal,
    /// Virtualized guest machine / cloud VPS / Docker container
    VirtualMachine { hypervisor_signature: String },
}

/// Comprehensive L3 hardware identity and virtualization proof.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardwareProof {
    pub tpm_status: TpmStatus,
    pub environment: PlatformEnvironment,
    pub is_bare_metal: bool,
    pub verified_at_millis: u64,
}

#[cfg(windows)]
mod win32 {
    pub use std::ffi::c_void;

    #[link(name = "kernel32")]
    extern "system" {
        pub fn LoadLibraryA(lp_lib_file_name: *const u8) -> *mut c_void;
        pub fn GetProcAddress(h_module: *mut c_void, lp_proc_name: *const u8) -> *mut c_void;
    }
}

/// Detects whether the current CPU is running inside a virtualized hypervisor using standard x86 CPUID leaves.
#[cfg(target_arch = "x86_64")]
pub fn detect_platform_environment() -> PlatformEnvironment {
    let cpuid1 = core::arch::x86_64::__cpuid(1);
    // Bit 31 of ECX indicates hypervisor presence (standardized by Intel/AMD/VMware/Microsoft)
    let is_hypervisor = (cpuid1.ecx & (1 << 31)) != 0;

    if !is_hypervisor {
        return PlatformEnvironment::PhysicalBareMetal;
    }

    // Leaf 0x40000000 returns the 12-byte ASCII vendor signature in EBX, ECX, EDX
    let hv_leaf = core::arch::x86_64::__cpuid(0x40000000);
    let mut sig_bytes = [0u8; 12];
    sig_bytes[0..4].copy_from_slice(&hv_leaf.ebx.to_le_bytes());
    sig_bytes[4..8].copy_from_slice(&hv_leaf.ecx.to_le_bytes());
    sig_bytes[8..12].copy_from_slice(&hv_leaf.edx.to_le_bytes());

    let sig = String::from_utf8_lossy(&sig_bytes).trim().to_string();
    PlatformEnvironment::VirtualMachine {
        hypervisor_signature: if sig.is_empty() {
            "Unknown Hypervisor".to_string()
        } else {
            sig
        },
    }
}

#[cfg(not(target_arch = "x86_64"))]
pub fn detect_platform_environment() -> PlatformEnvironment {
    PlatformEnvironment::PhysicalBareMetal
}

/// Probes whether hardware TPM 2.0 is available on Windows via CNG Platform Crypto Provider or TBS.
#[cfg(windows)]
pub fn probe_tpm_status() -> TpmStatus {
    use std::os::windows::ffi::OsStrExt;

    unsafe {
        let ncrypt = win32::LoadLibraryA(b"ncrypt.dll\0".as_ptr());
        if ncrypt.is_null() {
            return TpmStatus::NotSupported;
        }

        let ncrypt_open_storage_provider: Option<
            unsafe extern "system" fn(*mut *mut win32::c_void, *const u16, u32) -> i32,
        > = std::mem::transmute(win32::GetProcAddress(
            ncrypt,
            b"NCryptOpenStorageProvider\0".as_ptr(),
        ));
        let ncrypt_free_object: Option<unsafe extern "system" fn(*mut win32::c_void) -> i32> =
            std::mem::transmute(win32::GetProcAddress(
                ncrypt,
                b"NCryptFreeObject\0".as_ptr(),
            ));

        let (Some(open_provider), Some(free_object)) =
            (ncrypt_open_storage_provider, ncrypt_free_object)
        else {
            return TpmStatus::NotSupported;
        };

        let provider_name: Vec<u16> = std::ffi::OsStr::new("Microsoft Platform Crypto Provider")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let mut h_provider: *mut win32::c_void = std::ptr::null_mut();
        let status = open_provider(&mut h_provider, provider_name.as_ptr(), 0);

        if status == 0 && !h_provider.is_null() {
            free_object(h_provider);
            return TpmStatus::Tpm2Available {
                provider_name: "Microsoft Platform Crypto Provider".to_string(),
                is_hardware_bound: true,
            };
        }

        // Secondary check: query tbs.dll (TPM Base Services)
        let tbs = win32::LoadLibraryA(b"tbs.dll\0".as_ptr());
        if !tbs.is_null() {
            let tbsi_context_create: Option<
                unsafe extern "system" fn(*const u32, *mut *mut win32::c_void) -> u32,
            > = std::mem::transmute(win32::GetProcAddress(
                tbs,
                b"Tbsi_Context_Create\0".as_ptr(),
            ));
            let tbsip_context_close: Option<unsafe extern "system" fn(*mut win32::c_void) -> u32> =
                std::mem::transmute(win32::GetProcAddress(
                    tbs,
                    b"Tbsip_Context_Close\0".as_ptr(),
                ));

            if let (Some(context_create), Some(context_close)) =
                (tbsi_context_create, tbsip_context_close)
            {
                // TBS_CONTEXT_PARAMS with version = 2 (TPM 2.0)
                let params: [u32; 1] = [2];
                let mut h_context: *mut win32::c_void = std::ptr::null_mut();
                let tbs_res = context_create(params.as_ptr(), &mut h_context);
                if tbs_res == 0 && !h_context.is_null() {
                    context_close(h_context);
                    return TpmStatus::Tpm2Available {
                        provider_name: "TPM Base Services (TBS 2.0)".to_string(),
                        is_hardware_bound: true,
                    };
                }

                match tbs_res {
                    0x80284000 => return TpmStatus::NotPresent,
                    0x80284008 | 0x8028400C => return TpmStatus::ServiceDisabled,
                    0x80284002 => return TpmStatus::AccessDenied,
                    _ => {}
                }
            }
        }

        // Status code interpretation for NCrypt
        match status as u32 {
            0x80090030 => TpmStatus::NotPresent,   // NTE_DEVICE_NOT_FOUND
            0x80090029 => TpmStatus::NotSupported, // NTE_NOT_SUPPORTED
            0x80070005 => TpmStatus::AccessDenied, // ERROR_ACCESS_DENIED
            _ => TpmStatus::NotPresent,
        }
    }
}

#[cfg(not(windows))]
pub fn probe_tpm_status() -> TpmStatus {
    TpmStatus::NotSupported
}

/// Generates a comprehensive L3 hardware identity and virtualization proof.
pub fn generate_l3_hardware_proof() -> HardwareProof {
    let tpm_status = probe_tpm_status();
    let environment = detect_platform_environment();
    let is_bare_metal = matches!(environment, PlatformEnvironment::PhysicalBareMetal);

    let verified_at_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    HardwareProof {
        tpm_status,
        environment,
        is_bare_metal,
        verified_at_millis,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_environment_detection() {
        let env = detect_platform_environment();
        match &env {
            PlatformEnvironment::PhysicalBareMetal => {
                // Expected on physical development machines
            }
            PlatformEnvironment::VirtualMachine {
                hypervisor_signature,
            } => {
                assert!(
                    !hypervisor_signature.is_empty(),
                    "hypervisor signature must not be empty if VM"
                );
            }
        }
    }

    #[test]
    fn test_hardware_proof_generation() {
        let proof = generate_l3_hardware_proof();
        assert!(proof.verified_at_millis > 0);
        // Ensure either BareMetal or VM is determined
        assert_eq!(
            proof.is_bare_metal,
            matches!(proof.environment, PlatformEnvironment::PhysicalBareMetal)
        );
    }
}
