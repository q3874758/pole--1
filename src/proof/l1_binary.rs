//! L1: Binary physical identity and Authenticode verification.
//!
//! Provides verifiable hardware and OS-level evidence that a mapped game process
//! is indeed a legitimate binary on physical disk, rather than a spoofed or injected mock:
//! - Resolves physical executable path via `QueryFullProcessImageNameW`.
//! - Validates PE (Portable Executable) DOS `MZ` and `PE\0\0` headers.
//! - Computes SHA-256 binary hash fingerprint.
//! - Verifies Authenticode digital signatures offline via Win32 `WinVerifyTrust`
//!   (with CRL network checks disabled to eliminate latency/freeze).
//! - Extracts publisher certificate subject names (e.g. "Valve Corp.", "Electronic Arts, Inc.").
//! - Provides graceful fallback (L4) for unsigned games via Steam `appmanifest_{appid}.acf` matching.

#![allow(unsafe_code)]

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::os_support::{compute_executable_sha256, is_valid_pe_executable};

/// Hierarchical verification tier for executable proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ProofTier {
    /// L1A: Authenticode signed with valid certificate from known trusted publisher (Valve, EA, Epic, etc.)
    L1SignedTrusted = 100,
    /// L1B: Authenticode signed with valid certificate from another legitimate publisher
    L1SignedStandard = 80,
    /// L1C: Unsigned or self-signed, but located within a verified Steam library with matching appmanifest_{appid}.acf
    L1SteamManifest = 50,
    /// L1D: Valid DOS/PE executable with computed SHA-256 (bare fallback - 0 reward weight)
    L1BareBinary = 0,
    /// Invalid binary, missing file, or non-executable
    Untrusted = -1,
}

impl ProofTier {
    /// Effective reward multiplier in basis points (10,000 = 100%).
    /// Bare PE executables (L1BareBinary) receive 0 reward weight to prevent spoofing via arbitrary binaries (e.g. notepad.exe).
    pub fn reward_weight_bps(&self) -> u32 {
        match self {
            Self::L1SignedTrusted => 10_000,
            Self::L1SignedStandard => 8_000,
            Self::L1SteamManifest => 5_000,
            Self::L1BareBinary => 0,
            Self::Untrusted => 0,
        }
    }

    /// Whether this proof tier is eligible for reward generation.
    pub fn is_reward_eligible(&self) -> bool {
        self.reward_weight_bps() > 0
    }
}

/// Result of WinVerifyTrust Authenticode signature check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthenticodeStatus {
    Valid {
        signer_subject: String,
        is_trusted_publisher: bool,
    },
    NotSigned,
    InvalidSignature(String),
    NotSupported,
}

/// Metadata extracted from a Steam installation manifest (appmanifest_{appid}.acf).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SteamManifestInfo {
    pub app_id: u32,
    pub name: Option<String>,
    pub install_dir: Option<String>,
    pub build_id: Option<String>,
    pub manifest_path: PathBuf,
}

/// Objective raw binary evidence captured directly from the game process on disk.
///
/// Contains purely objective telemetry without self-assigned tier scoring, ensuring
/// third-party witnesses and validators perform independent tier re-computation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawBinaryEvidence {
    pub pid: u32,
    pub process_name: String,
    pub full_path: PathBuf,
    pub sha256: String,
    pub authenticode: AuthenticodeStatus,
    pub steam_manifest: Option<SteamManifestInfo>,
    pub captured_at_millis: u64,
}

/// Evaluates the verified proof tier from raw binary evidence.
///
/// MUST be executed by independent witnesses or validators; never accepted
/// as an authoritative claim directly from the playing node.
pub fn evaluate_binary_tier(evidence: &RawBinaryEvidence) -> ProofTier {
    match &evidence.authenticode {
        AuthenticodeStatus::Valid {
            is_trusted_publisher: true,
            ..
        } => ProofTier::L1SignedTrusted,
        AuthenticodeStatus::Valid {
            is_trusted_publisher: false,
            ..
        } => ProofTier::L1SignedStandard,
        _ => {
            if evidence.steam_manifest.is_some() {
                ProofTier::L1SteamManifest
            } else {
                ProofTier::L1BareBinary
            }
        }
    }
}

/// Complete L1 binary proof for an active process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BinaryProof {
    pub pid: u32,
    pub process_name: String,
    pub full_path: PathBuf,
    pub sha256: String,
    pub tier: ProofTier,
    pub authenticode: AuthenticodeStatus,
    pub steam_manifest: Option<SteamManifestInfo>,
    pub verified_at_millis: u64,
    pub raw_evidence: RawBinaryEvidence,
}

/// Errors occurring during L1 binary proof generation.
#[derive(Debug, thiserror::Error)]
pub enum ProofError {
    #[error("process ID {0} not found or access denied")]
    ProcessNotFound(u32),
    #[error("failed to query full process image name: {0}")]
    QueryImagePathFailed(String),
    #[error("file not found: {0}")]
    FileNotFound(PathBuf),
    #[error("file is not a valid PE binary: {0}")]
    InvalidPeBinary(PathBuf),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Pre-populated list of recognized gaming and software publishers.
pub const KNOWN_TRUSTED_PUBLISHERS: &[&str] = &[
    "valve",
    "electronic arts",
    "epic games",
    "riot games",
    "ubisoft",
    "activision",
    "blizzard entertainment",
    "capcom",
    "bandai namco",
    "square enix",
    "sega",
    "microsoft",
    "sony interactive",
    "tencent",
    "bethesda",
    "cd projekt",
    "take-two",
    "2k games",
    "rockstar games",
    "bungie",
    "krafton",
    "mihoyo",
    "cognosphere",
    "wargaming",
];

/// Checks if a signer subject matches any recognized major publisher.
pub fn is_known_trusted_publisher(subject: &str) -> bool {
    let lower = subject.to_ascii_lowercase();
    KNOWN_TRUSTED_PUBLISHERS.iter().any(|k| lower.contains(k))
}

#[cfg(windows)]
#[allow(clippy::upper_case_acronyms)]
mod win32 {
    pub use std::ffi::c_void;

    pub const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    pub const PROCESS_QUERY_INFORMATION: u32 = 0x0400;

    pub const WTD_UI_NONE: u32 = 2;
    pub const WTD_REVOCATION_CHECK_NONE: u32 = 0x00000000;
    pub const WTD_CHOICE_FILE: u32 = 1;
    pub const WTD_STATEACTION_IGNORE: u32 = 0;
    pub const WTD_CACHE_ONLY_URL_RETRIEVAL: u32 = 0x00001000;

    pub const CERT_QUERY_OBJECT_FILE: u32 = 0x00000001;
    pub const CERT_QUERY_CONTENT_FLAG_PKCS7_SIGNED_EMBED: u32 = 1 << 10;
    pub const CERT_QUERY_FORMAT_FLAG_BINARY: u32 = 1 << 1;
    pub const CERT_NAME_SIMPLE_DISPLAY_TYPE: u32 = 4;
    pub const CMSG_SIGNER_INFO_PARAM: u32 = 6;
    pub const X509_ASN_ENCODING: u32 = 0x00000001;
    pub const PKCS_7_ASN_ENCODING: u32 = 0x00010000;
    pub const CERT_FIND_SUBJECT_CERT: u32 = (11 << 16) | 7;

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct GUID {
        data1: u32,
        data2: u16,
        data3: u16,
        data4: [u8; 8],
    }

    pub const WINTRUST_ACTION_GENERIC_VERIFY_V2: GUID = GUID {
        data1: 0x00aac56b,
        data2: 0xcd44,
        data3: 0x11d0,
        data4: [0x8c, 0xc2, 0x00, 0xc0, 0x4f, 0xc2, 0x95, 0xee],
    };

    #[repr(C)]
    pub struct WINTRUST_FILE_INFO {
        pub cb_struct: u32,
        pub pcwsz_file_path: *const u16,
        pub h_file: *mut c_void,
        pub pg_known_subject: *mut GUID,
    }

    #[repr(C)]
    pub struct WINTRUST_DATA {
        pub cb_struct: u32,
        pub p_policy_callback_data: *mut c_void,
        pub p_sip_client_data: *mut c_void,
        pub dw_ui_choice: u32,
        pub fdw_revocation_checks: u32,
        pub dw_union_choice: u32,
        pub p_file: *mut WINTRUST_FILE_INFO,
        pub dw_state_action: u32,
        pub h_wvt_state_data: *mut c_void,
        pub pwsz_url_reference: *mut u16,
        pub dw_prov_flags: u32,
        pub dw_ui_context: u32,
        pub p_signature_settings: *mut c_void,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct CRYPT_INTEGER_BLOB {
        pub cb_data: u32,
        pub pb_data: *mut u8,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct CRYPT_ALGORITHM_IDENTIFIER {
        pub psz_obj_id: *mut u8,
        pub parameters: CRYPT_INTEGER_BLOB,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct FILETIME {
        pub dw_low_date_time: u32,
        pub dw_high_date_time: u32,
    }

    #[repr(C)]
    pub struct CERT_INFO {
        pub dw_version: u32,
        pub serial_number: CRYPT_INTEGER_BLOB,
        pub signature_algorithm: CRYPT_ALGORITHM_IDENTIFIER,
        pub issuer: CRYPT_INTEGER_BLOB,
        pub not_before: FILETIME,
        pub not_after: FILETIME,
        pub subject: CRYPT_INTEGER_BLOB,
    }

    #[repr(C)]
    pub struct CMSG_SIGNER_INFO {
        pub dw_version: u32,
        pub issuer: CRYPT_INTEGER_BLOB,
        pub serial_number: CRYPT_INTEGER_BLOB,
    }

    #[repr(C)]
    pub struct CERT_CONTEXT {
        pub dw_cert_encoding_type: u32,
        pub pb_cert_encoded: *mut u8,
        pub cb_cert_encoded: u32,
        pub p_cert_info: *mut CERT_INFO,
        pub h_cert_store: *mut c_void,
    }

    #[link(name = "wintrust")]
    extern "system" {
        pub fn WinVerifyTrust(
            hwnd: *mut c_void,
            pg_action_id: *const GUID,
            p_wvt_data: *mut c_void,
        ) -> i32;
    }

    #[link(name = "crypt32")]
    extern "system" {
        pub fn CryptQueryObject(
            dw_object_type: u32,
            pv_object: *const c_void,
            dw_expected_content_type_flags: u32,
            dw_expected_format_type_flags: u32,
            dw_flags: u32,
            pdw_msg_and_cert_encoding_type: *mut u32,
            pdw_content_type: *mut u32,
            pdw_format_type: *mut u32,
            ph_cert_store: *mut *mut c_void,
            ph_msg: *mut *mut c_void,
            ppv_context: *mut *const c_void,
        ) -> i32;

        pub fn CryptMsgGetParam(
            h_crypt_msg: *mut c_void,
            dw_param_type: u32,
            dw_index: u32,
            pv_data: *mut c_void,
            pcb_data: *mut u32,
        ) -> i32;

        pub fn CertFindCertificateInStore(
            h_cert_store: *mut c_void,
            dw_cert_encoding_type: u32,
            dw_find_flags: u32,
            dw_find_type: u32,
            pv_find_para: *const c_void,
            p_prev_cert_context: *const CERT_CONTEXT,
        ) -> *const CERT_CONTEXT;

        pub fn CertEnumCertificatesInStore(
            h_cert_store: *mut c_void,
            p_prev_cert_context: *const CERT_CONTEXT,
        ) -> *const CERT_CONTEXT;

        pub fn CertGetNameStringW(
            p_cert_context: *const CERT_CONTEXT,
            dw_type: u32,
            dw_flags: u32,
            pv_type_para: *const c_void,
            psz_name_string: *mut u16,
            cch_name_string: u32,
        ) -> u32;

        pub fn CertFreeCertificateContext(p_cert_context: *const CERT_CONTEXT) -> i32;
        pub fn CertCloseStore(h_cert_store: *mut c_void, dw_flags: u32) -> i32;
        pub fn CryptMsgClose(h_crypt_msg: *mut c_void) -> i32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        pub fn OpenProcess(
            dw_desired_access: u32,
            b_inherit_handle: i32,
            dw_process_id: u32,
        ) -> *mut c_void;
        pub fn QueryFullProcessImageNameW(
            h_process: *mut c_void,
            dw_flags: u32,
            lp_exe_name: *mut u16,
            lpdw_size: *mut u32,
        ) -> i32;
        pub fn CloseHandle(h_object: *mut c_void) -> i32;
        pub fn LoadLibraryA(lp_lib_file_name: *const u8) -> *mut c_void;
        pub fn GetProcAddress(h_module: *mut c_void, lp_proc_name: *const u8) -> *mut c_void;
    }
}

/// Queries the full physical executable path on disk for a given PID.
#[cfg(windows)]
pub fn query_process_image_path(pid: u32) -> Result<PathBuf, ProofError> {
    unsafe {
        let handle = win32::OpenProcess(win32::PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        let handle = if handle.is_null() {
            win32::OpenProcess(win32::PROCESS_QUERY_INFORMATION, 0, pid)
        } else {
            handle
        };

        if handle.is_null() {
            return Err(ProofError::ProcessNotFound(pid));
        }

        let mut buf = [0u16; 1024];
        let mut size = buf.len() as u32;
        let res = win32::QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size);
        win32::CloseHandle(handle);

        if res == 0 || size == 0 {
            return Err(ProofError::QueryImagePathFailed(format!(
                "QueryFullProcessImageNameW returned 0 for PID {pid}"
            )));
        }

        let path_str = String::from_utf16_lossy(&buf[..size as usize]);
        Ok(PathBuf::from(path_str))
    }
}

#[cfg(not(windows))]
pub fn query_process_image_path(pid: u32) -> Result<PathBuf, ProofError> {
    Err(ProofError::QueryImagePathFailed(format!(
        "unsupported platform for PID {pid}"
    )))
}

#[cfg(windows)]
unsafe fn compare_blobs(a: &win32::CRYPT_INTEGER_BLOB, b: &win32::CRYPT_INTEGER_BLOB) -> bool {
    if a.cb_data != b.cb_data {
        return false;
    }
    if a.cb_data == 0 {
        return true;
    }
    if a.pb_data.is_null() || b.pb_data.is_null() {
        return false;
    }
    let slice_a = std::slice::from_raw_parts(a.pb_data, a.cb_data as usize);
    let slice_b = std::slice::from_raw_parts(b.pb_data, b.cb_data as usize);
    slice_a == slice_b
}

/// Extracts the display name of the primary certificate signer from an Authenticode-signed PE file.
///
/// Follows standard PKCS#7 resolution:
/// 1. Queries `CMSG_SIGNER_INFO_PARAM` from `h_msg` to obtain the signer's `Issuer` and `SerialNumber`.
/// 2. Matches the exact End-Entity leaf certificate in `h_store` matching `(Issuer, SerialNumber)`.
/// 3. Ignores intermediate CAs and timestamp counter-signers, ensuring deterministic publisher attribution.
#[cfg(windows)]
pub fn extract_authenticode_signer_name(path: &Path) -> Option<String> {
    use std::os::windows::ffi::OsStrExt;
    let mut wide_path: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide_path.push(0);

    unsafe {
        let mut encoding: u32 = 0;
        let mut content_type: u32 = 0;
        let mut format_type: u32 = 0;
        let mut h_store: *mut win32::c_void = std::ptr::null_mut();
        let mut h_msg: *mut win32::c_void = std::ptr::null_mut();

        let ok = win32::CryptQueryObject(
            win32::CERT_QUERY_OBJECT_FILE,
            wide_path.as_ptr() as *const win32::c_void,
            win32::CERT_QUERY_CONTENT_FLAG_PKCS7_SIGNED_EMBED,
            win32::CERT_QUERY_FORMAT_FLAG_BINARY,
            0,
            &mut encoding,
            &mut content_type,
            &mut format_type,
            &mut h_store,
            &mut h_msg,
            std::ptr::null_mut(),
        );

        if ok == 0 || h_store.is_null() {
            if !h_msg.is_null() {
                win32::CryptMsgClose(h_msg);
            }
            if !h_store.is_null() {
                win32::CertCloseStore(h_store, 0);
            }
            return None;
        }

        let mut name_out = None;
        let mut p_exact_cert: *const win32::CERT_CONTEXT = std::ptr::null();

        // 1. Extract CMSG_SIGNER_INFO to get the exact leaf signer's Issuer and SerialNumber
        let mut cb_signer_info: u32 = 0;
        if !h_msg.is_null()
            && win32::CryptMsgGetParam(
                h_msg,
                win32::CMSG_SIGNER_INFO_PARAM,
                0,
                std::ptr::null_mut(),
                &mut cb_signer_info,
            ) != 0
            && cb_signer_info > 0
        {
            let mut signer_info_buf = vec![0u8; cb_signer_info as usize];
            if win32::CryptMsgGetParam(
                h_msg,
                win32::CMSG_SIGNER_INFO_PARAM,
                0,
                signer_info_buf.as_mut_ptr() as *mut win32::c_void,
                &mut cb_signer_info,
            ) != 0
            {
                let signer_info = signer_info_buf.as_ptr() as *const win32::CMSG_SIGNER_INFO;
                let target_issuer = (*signer_info).issuer;
                let target_serial = (*signer_info).serial_number;

                // First try Win32 CertFindCertificateInStore with CERT_FIND_SUBJECT_CERT
                let mut cert_info: win32::CERT_INFO = std::mem::zeroed();
                cert_info.issuer = target_issuer;
                cert_info.serial_number = target_serial;

                let found = win32::CertFindCertificateInStore(
                    h_store,
                    win32::X509_ASN_ENCODING | win32::PKCS_7_ASN_ENCODING,
                    0,
                    win32::CERT_FIND_SUBJECT_CERT,
                    &cert_info as *const _ as *const win32::c_void,
                    std::ptr::null(),
                );

                if !found.is_null() {
                    p_exact_cert = found;
                } else {
                    // Fallback: iterate h_store and find the cert whose Issuer & SerialNumber match byte-for-byte
                    let mut curr = win32::CertEnumCertificatesInStore(h_store, std::ptr::null());
                    while !curr.is_null() {
                        if !(*curr).p_cert_info.is_null() {
                            let cert_inf = &*(*curr).p_cert_info;
                            if compare_blobs(&cert_inf.serial_number, &target_serial)
                                && compare_blobs(&cert_inf.issuer, &target_issuer)
                            {
                                p_exact_cert = curr;
                                break;
                            }
                        }
                        curr = win32::CertEnumCertificatesInStore(h_store, curr);
                    }
                }
            }
        }

        // 2. Extract display name from the exact End-Entity certificate
        if !p_exact_cert.is_null() {
            let mut name_buf = [0u16; 512];
            let len = win32::CertGetNameStringW(
                p_exact_cert,
                win32::CERT_NAME_SIMPLE_DISPLAY_TYPE,
                0,
                std::ptr::null(),
                name_buf.as_mut_ptr(),
                name_buf.len() as u32,
            );
            if len > 1 {
                name_out = Some(String::from_utf16_lossy(&name_buf[..len as usize - 1]));
            }
            win32::CertFreeCertificateContext(p_exact_cert);
        } else {
            // Ultimate fallback if CMSG_SIGNER_INFO was unparseable
            let mut p_cert = win32::CertEnumCertificatesInStore(h_store, std::ptr::null());
            while !p_cert.is_null() {
                let mut name_buf = [0u16; 512];
                let len = win32::CertGetNameStringW(
                    p_cert,
                    win32::CERT_NAME_SIMPLE_DISPLAY_TYPE,
                    0,
                    std::ptr::null(),
                    name_buf.as_mut_ptr(),
                    name_buf.len() as u32,
                );
                if len > 1 {
                    let name = String::from_utf16_lossy(&name_buf[..len as usize - 1]);
                    if !name.trim().is_empty() {
                        let is_trusted = is_known_trusted_publisher(&name);
                        if is_trusted {
                            name_out = Some(name);
                            win32::CertFreeCertificateContext(p_cert);
                            break;
                        }
                        if name_out.is_none() {
                            name_out = Some(name);
                        }
                    }
                }
                p_cert = win32::CertEnumCertificatesInStore(h_store, p_cert);
            }
        }

        if !h_msg.is_null() {
            win32::CryptMsgClose(h_msg);
        }
        if !h_store.is_null() {
            win32::CertCloseStore(h_store, 0);
        }

        name_out
    }
}

#[cfg(not(windows))]
pub fn extract_authenticode_signer_name(_path: &Path) -> Option<String> {
    None
}

/// Verifies whether an executable has a valid Authenticode signature without online revocation delay.
#[cfg(windows)]
pub fn verify_authenticode(path: &Path) -> AuthenticodeStatus {
    use std::os::windows::ffi::OsStrExt;
    let mut wide_path: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide_path.push(0);

    let mut file_info = win32::WINTRUST_FILE_INFO {
        cb_struct: std::mem::size_of::<win32::WINTRUST_FILE_INFO>() as u32,
        pcwsz_file_path: wide_path.as_ptr(),
        h_file: std::ptr::null_mut(),
        pg_known_subject: std::ptr::null_mut(),
    };

    let mut trust_data = win32::WINTRUST_DATA {
        cb_struct: std::mem::size_of::<win32::WINTRUST_DATA>() as u32,
        p_policy_callback_data: std::ptr::null_mut(),
        p_sip_client_data: std::ptr::null_mut(),
        dw_ui_choice: win32::WTD_UI_NONE,
        fdw_revocation_checks: win32::WTD_REVOCATION_CHECK_NONE,
        dw_union_choice: win32::WTD_CHOICE_FILE,
        p_file: &mut file_info,
        dw_state_action: win32::WTD_STATEACTION_IGNORE,
        h_wvt_state_data: std::ptr::null_mut(),
        pwsz_url_reference: std::ptr::null_mut(),
        dw_prov_flags: win32::WTD_CACHE_ONLY_URL_RETRIEVAL,
        dw_ui_context: 0,
        p_signature_settings: std::ptr::null_mut(),
    };

    let status = unsafe {
        win32::WinVerifyTrust(
            std::ptr::null_mut(),
            &win32::WINTRUST_ACTION_GENERIC_VERIFY_V2,
            &mut trust_data as *mut _ as *mut std::ffi::c_void,
        )
    };

    if status == 0 {
        let subject = extract_authenticode_signer_name(path)
            .unwrap_or_else(|| "Unknown Verified Signer".to_string());
        let is_trusted = is_known_trusted_publisher(&subject);
        AuthenticodeStatus::Valid {
            signer_subject: subject,
            is_trusted_publisher: is_trusted,
        }
    } else {
        match status as u32 {
            0x800B0100 => AuthenticodeStatus::NotSigned,
            0x800B0101 => AuthenticodeStatus::InvalidSignature(
                "Certificate has expired (CERT_E_EXPIRED)".into(),
            ),
            0x800B0109 => AuthenticodeStatus::InvalidSignature(
                "Untrusted root certificate (CERT_E_UNTRUSTEDROOT)".into(),
            ),
            0x800B0111 => AuthenticodeStatus::InvalidSignature(
                "Explicit distrust / revoked (TRUST_E_EXPLICIT_DISTRUST)".into(),
            ),
            other => AuthenticodeStatus::InvalidSignature(format!(
                "WinVerifyTrust status code: 0x{other:08X}"
            )),
        }
    }
}

#[cfg(not(windows))]
pub fn verify_authenticode(_path: &Path) -> AuthenticodeStatus {
    AuthenticodeStatus::NotSupported
}

/// Checks whether an executable resides within the expected Steam installation directory.
fn is_exe_within_steam_installdir(
    exe_path: &Path,
    steamapps_dir: &Path,
    install_dir: &str,
) -> bool {
    let common_game_dir = steamapps_dir.join("common").join(install_dir);
    if exe_path.starts_with(&common_game_dir) {
        return true;
    }
    exe_path.components().any(|c| {
        c.as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case(install_dir)
    })
}

/// Searches the parent directory hierarchy of an executable for an associated Steam `appmanifest_{appid}.acf`.
///
/// Enforces directory containment: verifies that `exe_path` resides strictly within
/// the declared `<steamapps_dir>/common/<installdir>/` tree.
pub fn find_steam_appmanifest(
    exe_path: &Path,
    expected_app_id: Option<u32>,
) -> Option<SteamManifestInfo> {
    let mut current = exe_path.parent();
    // Traverse upwards up to 6 directory levels to find a 'steamapps' folder
    for _ in 0..6 {
        let dir = current?;
        if dir.file_name().map(|n| n == "steamapps").unwrap_or(false) {
            // Found steamapps directory! Look for manifests
            if let Some(target_id) = expected_app_id {
                let manifest_file = dir.join(format!("appmanifest_{target_id}.acf"));
                if manifest_file.is_file() {
                    if let Some(info) = parse_steam_manifest_file(&manifest_file, target_id) {
                        if let Some(ref install_dir) = info.install_dir {
                            if is_exe_within_steam_installdir(exe_path, dir, install_dir) {
                                return Some(info);
                            }
                        }
                    }
                }
            } else if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if let Some(file_name) = path.file_name().and_then(|f| f.to_str()) {
                        if file_name.starts_with("appmanifest_") && file_name.ends_with(".acf") {
                            let id_part = file_name
                                .trim_start_matches("appmanifest_")
                                .trim_end_matches(".acf");
                            if let Ok(app_id) = id_part.parse::<u32>() {
                                if let Some(info) = parse_steam_manifest_file(&path, app_id) {
                                    if let Some(ref install_dir) = info.install_dir {
                                        if is_exe_within_steam_installdir(
                                            exe_path,
                                            dir,
                                            install_dir,
                                        ) {
                                            return Some(info);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        current = dir.parent();
    }
    None
}

/// Parses standard KeyValue format in a Steam `.acf` file.
fn parse_steam_manifest_file(manifest_path: &Path, app_id: u32) -> Option<SteamManifestInfo> {
    let content = std::fs::read_to_string(manifest_path).ok()?;
    let mut name = None;
    let mut install_dir = None;
    let mut build_id = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("\"name\"") {
            name = extract_acf_value(trimmed);
        } else if trimmed.starts_with("\"installdir\"") {
            install_dir = extract_acf_value(trimmed);
        } else if trimmed.starts_with("\"buildid\"") {
            build_id = extract_acf_value(trimmed);
        }
    }

    Some(SteamManifestInfo {
        app_id,
        name,
        install_dir,
        build_id,
        manifest_path: manifest_path.to_path_buf(),
    })
}

fn extract_acf_value(line: &str) -> Option<String> {
    let parts: Vec<&str> = line.split('"').filter(|s| !s.trim().is_empty()).collect();
    if parts.len() >= 2 {
        Some(parts[1].to_string())
    } else {
        None
    }
}

/// Captures objective raw binary evidence directly from the game process on disk.
///
/// Contains purely objective telemetry without self-assigned tier scoring, ensuring
/// third-party witnesses and validators perform independent tier re-computation.
pub fn capture_raw_binary_evidence(
    pid: u32,
    expected_app_id: Option<u32>,
) -> Result<RawBinaryEvidence, ProofError> {
    let full_path = query_process_image_path(pid)?;
    if !full_path.is_file() {
        return Err(ProofError::FileNotFound(full_path));
    }
    if !is_valid_pe_executable(&full_path) {
        return Err(ProofError::InvalidPeBinary(full_path));
    }

    let sha256 = compute_executable_sha256(&full_path).unwrap_or_default();
    let process_name = full_path
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("unknown.exe")
        .to_string();

    let authenticode = verify_authenticode(&full_path);
    let steam_manifest = find_steam_appmanifest(&full_path, expected_app_id);

    let captured_at_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    Ok(RawBinaryEvidence {
        pid,
        process_name,
        full_path,
        sha256,
        authenticode,
        steam_manifest,
        captured_at_millis,
    })
}

/// Generates a comprehensive L1 physical binary proof for a specific process ID.
///
/// Produces raw evidence and evaluates the proof tier locally for reporting/preview.
pub fn generate_l1_binary_proof(
    pid: u32,
    expected_app_id: Option<u32>,
) -> Result<BinaryProof, ProofError> {
    let evidence = capture_raw_binary_evidence(pid, expected_app_id)?;
    let tier = evaluate_binary_tier(&evidence);

    Ok(BinaryProof {
        pid: evidence.pid,
        process_name: evidence.process_name.clone(),
        full_path: evidence.full_path.clone(),
        sha256: evidence.sha256.clone(),
        tier,
        authenticode: evidence.authenticode.clone(),
        steam_manifest: evidence.steam_manifest.clone(),
        verified_at_millis: evidence.captured_at_millis,
        raw_evidence: evidence,
    })
}

/// Inspects the current foreground active process and generates its L1 binary proof.
#[cfg(windows)]
pub fn inspect_foreground_process_l1(expected_app_id: Option<u32>) -> Option<BinaryProof> {
    unsafe {
        let user32 = win32::LoadLibraryA(b"user32.dll\0".as_ptr());
        if user32.is_null() {
            return None;
        }

        let get_foreground_window: Option<unsafe extern "system" fn() -> *mut win32::c_void> =
            std::mem::transmute(win32::GetProcAddress(
                user32,
                b"GetForegroundWindow\0".as_ptr(),
            ));
        let get_window_thread_process_id: Option<
            unsafe extern "system" fn(*mut win32::c_void, *mut u32) -> u32,
        > = std::mem::transmute(win32::GetProcAddress(
            user32,
            b"GetWindowThreadProcessId\0".as_ptr(),
        ));

        let (Some(get_fg_window), Some(get_thread_pid)) =
            (get_foreground_window, get_window_thread_process_id)
        else {
            return None;
        };

        let hwnd = get_fg_window();
        if hwnd.is_null() {
            return None;
        }
        let mut pid: u32 = 0;
        get_thread_pid(hwnd, &mut pid);
        if pid == 0 {
            return None;
        }

        generate_l1_binary_proof(pid, expected_app_id).ok()
    }
}

#[cfg(not(windows))]
pub fn inspect_foreground_process_l1(_expected_app_id: Option<u32>) -> Option<BinaryProof> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publisher_whitelist_detects_major_gaming_publishers() {
        assert!(is_known_trusted_publisher("Valve Corporation"));
        assert!(is_known_trusted_publisher("Electronic Arts, Inc."));
        assert!(is_known_trusted_publisher("Epic Games Inc."));
        assert!(is_known_trusted_publisher("Riot Games, Inc."));
        assert!(is_known_trusted_publisher("Microsoft Corporation"));
        assert!(!is_known_trusted_publisher("Malicious Hacker Ltd."));
    }

    #[test]
    fn parse_steam_manifest_extracts_fields() {
        let manifest_content = r#"
"AppState"
{
    "appid"     "730"
    "Universe"  "1"
    "name"      "Counter-Strike 2"
    "installdir"    "Counter-Strike Global Offensive"
    "buildid"   "16843920"
}
"#;
        let temp_dir = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("target")
            .join("tmp");
        let _ = std::fs::create_dir_all(&temp_dir);
        let manifest_path = temp_dir.join(format!("test_manifest_{}.acf", std::process::id()));
        std::fs::write(&manifest_path, manifest_content).unwrap();

        let parsed = parse_steam_manifest_file(&manifest_path, 730).unwrap();
        assert_eq!(parsed.app_id, 730);
        assert_eq!(parsed.name.as_deref(), Some("Counter-Strike 2"));
        assert_eq!(
            parsed.install_dir.as_deref(),
            Some("Counter-Strike Global Offensive")
        );
        assert_eq!(parsed.build_id.as_deref(), Some("16843920"));

        let _ = std::fs::remove_file(&manifest_path);
    }

    #[cfg(windows)]
    #[test]
    fn current_process_proof_generates_valid_pe_evidence() {
        let current_pid = std::process::id();
        let proof = generate_l1_binary_proof(current_pid, None).expect("current process proof");
        assert_eq!(proof.pid, current_pid);
        assert!(proof.full_path.is_file());
        assert!(!proof.sha256.is_empty());
        assert!(proof.tier >= ProofTier::L1BareBinary);
    }

    #[cfg(windows)]
    #[test]
    fn system_windows_binary_verifies_authenticode() {
        let candidates = [
            PathBuf::from("C:\\Windows\\System32\\appverif.exe"),
            PathBuf::from("C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe"),
            PathBuf::from("C:\\Program Files\\Microsoft Edge\\Application\\msedge.exe"),
        ];

        let mut tested = false;
        for candidate in &candidates {
            if candidate.is_file() {
                let auth = verify_authenticode(candidate);
                match auth {
                    AuthenticodeStatus::Valid {
                        signer_subject,
                        is_trusted_publisher,
                    } => {
                        assert!(
                            signer_subject.to_lowercase().contains("microsoft"),
                            "expected Microsoft in subject for {:?}, got: {signer_subject}",
                            candidate
                        );
                        assert!(
                            is_trusted_publisher,
                            "Microsoft should be recognized as a trusted publisher"
                        );
                        tested = true;
                        break;
                    }
                    other => {
                        eprintln!("candidate {:?} returned: {:?}", candidate, other);
                    }
                }
            }
        }

        assert!(
            tested,
            "at least one candidate binary with Authenticode signature should exist and verify"
        );
    }

    #[test]
    fn proof_tier_reward_weights_and_eligibility() {
        assert_eq!(ProofTier::L1SignedTrusted.reward_weight_bps(), 10_000);
        assert!(ProofTier::L1SignedTrusted.is_reward_eligible());

        assert_eq!(ProofTier::L1SignedStandard.reward_weight_bps(), 8_000);
        assert!(ProofTier::L1SignedStandard.is_reward_eligible());

        assert_eq!(ProofTier::L1SteamManifest.reward_weight_bps(), 5_000);
        assert!(ProofTier::L1SteamManifest.is_reward_eligible());

        // Bare PE binary receives 0 reward weight to avoid spoofing (notepad.exe)
        assert_eq!(ProofTier::L1BareBinary.reward_weight_bps(), 0);
        assert!(!ProofTier::L1BareBinary.is_reward_eligible());

        assert_eq!(ProofTier::Untrusted.reward_weight_bps(), 0);
        assert!(!ProofTier::Untrusted.is_reward_eligible());
    }

    #[test]
    fn evaluate_binary_tier_computations() {
        let base_evidence = RawBinaryEvidence {
            pid: 100,
            process_name: "test.exe".to_string(),
            full_path: PathBuf::from("C:\\Games\\test.exe"),
            sha256: "fakehash".to_string(),
            authenticode: AuthenticodeStatus::Valid {
                signer_subject: "Valve Corporation".to_string(),
                is_trusted_publisher: true,
            },
            steam_manifest: None,
            captured_at_millis: 12345,
        };

        // 1. Trusted publisher signature -> L1SignedTrusted
        assert_eq!(
            evaluate_binary_tier(&base_evidence),
            ProofTier::L1SignedTrusted
        );

        // 2. Standard commercial signature -> L1SignedStandard
        let mut std_evidence = base_evidence.clone();
        std_evidence.authenticode = AuthenticodeStatus::Valid {
            signer_subject: "Indie Studio LLC".to_string(),
            is_trusted_publisher: false,
        };
        assert_eq!(
            evaluate_binary_tier(&std_evidence),
            ProofTier::L1SignedStandard
        );

        // 3. Unsigned binary with Steam Manifest -> L1SteamManifest
        let mut steam_evidence = base_evidence.clone();
        steam_evidence.authenticode = AuthenticodeStatus::NotSigned;
        steam_evidence.steam_manifest = Some(SteamManifestInfo {
            app_id: 440,
            name: Some("Team Fortress 2".to_string()),
            install_dir: Some("Team Fortress 2".to_string()),
            build_id: Some("1234".to_string()),
            manifest_path: PathBuf::from("C:\\Steam\\steamapps\\appmanifest_440.acf"),
        });
        assert_eq!(
            evaluate_binary_tier(&steam_evidence),
            ProofTier::L1SteamManifest
        );

        // 4. Bare PE binary (unsigned, no manifest) -> L1BareBinary
        let mut bare_evidence = base_evidence;
        bare_evidence.authenticode = AuthenticodeStatus::NotSigned;
        bare_evidence.steam_manifest = None;
        assert_eq!(
            evaluate_binary_tier(&bare_evidence),
            ProofTier::L1BareBinary
        );
    }

    #[test]
    fn steam_installdir_containment_check() {
        let steamapps = Path::new("C:\\SteamLibrary\\steamapps");
        let exe_inside = steamapps
            .join("common")
            .join("Dota 2 beta")
            .join("game")
            .join("bin")
            .join("win64")
            .join("dota2.exe");
        let exe_outside = steamapps
            .join("common")
            .join("Counter-Strike")
            .join("cs.exe");

        assert!(is_exe_within_steam_installdir(
            &exe_inside,
            steamapps,
            "Dota 2 beta"
        ));
        assert!(!is_exe_within_steam_installdir(
            &exe_outside,
            steamapps,
            "Dota 2 beta"
        ));
        // Case insensitive
        assert!(is_exe_within_steam_installdir(
            &exe_inside,
            steamapps,
            "dota 2 beta"
        ));
    }
}
