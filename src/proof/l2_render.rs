//! L2: GPU 3D rendering pipeline and graphics device proof.
//!
//! Provides verifiable hardware and OS-level evidence that a mapped game process
//! is actively engaging a real 3D graphics pipeline (DirectX 11/12, Vulkan, OpenGL),
//! preventing mock scripts or minimal non-rendering console processes from accumulating play time:
//! - Enumerates loaded runtime modules for target process via Win32 PSAPI / kernel32.
//! - Identifies active 3D graphics presentation backends (D3D12, D3D11, DXGI, Vulkan, OpenGL).
//! - Inspects foreground window focus and ties rendering context to the active desktop session.
//! - Samples physical working set size (RAM footprint) to detect asset memory residency.
//! - Classifies render engagement into verifiable tiers (`ActiveForeground3D`, `BackgroundInWorld3D`, `HeadlessOrMock`).

#![allow(unsafe_code)]

use std::collections::BTreeSet;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::os_support::detect_process_working_set_bytes;
use crate::proof::ProofError;

/// Recognized 3D graphics rendering API backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum GraphicsBackend {
    /// Modern low-level DirectX 12 runtime (d3d12.dll)
    DirectX12 = 120,
    /// Industry standard DirectX 11 runtime (d3d11.dll)
    DirectX11 = 110,
    /// Cross-platform low-level Vulkan runtime (vulkan-1.dll)
    Vulkan = 100,
    /// Desktop OpenGL runtime (opengl32.dll)
    OpenGL = 80,
    /// DXGI swapchain / presentation infrastructure (dxgi.dll)
    DxgiOnly = 60,
    /// Legacy DirectX 9 runtime (d3d9.dll)
    DirectX9 = 50,
    /// No 3D graphics rendering module detected (console, headless, or mock stub)
    None = 0,
}

/// Evaluated engagement level of the process graphics pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RenderEngagementLevel {
    /// Process has loaded 3D graphics runtime AND is the active foreground window
    ActiveForeground3D = 100,
    /// Process has loaded 3D graphics runtime, running in background (valid AFK in-world gaming)
    BackgroundInWorld3D = 70,
    /// Process has loaded no 3D graphics modules (likely mock script or non-3D placeholder)
    HeadlessOrMock = 0,
}

/// Comprehensive L2 graphics rendering proof for a target process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderProof {
    pub pid: u32,
    pub process_name: String,
    pub is_foreground: bool,
    pub primary_backend: GraphicsBackend,
    pub loaded_render_modules: Vec<String>,
    pub engagement_level: RenderEngagementLevel,
    pub working_set_bytes: u64,
    pub verified_at_millis: u64,
}

/// Classifies the primary 3D graphics backend and extracts recognized rendering modules from loaded DLLs.
pub fn classify_graphics_backend(modules: &[String]) -> (GraphicsBackend, Vec<String>) {
    let mut detected = BTreeSet::new();
    let mut primary = GraphicsBackend::None;

    for module in modules {
        let lower = module.to_ascii_lowercase();
        if lower.starts_with("d3d12") {
            detected.insert("d3d12.dll".to_string());
            if primary < GraphicsBackend::DirectX12 {
                primary = GraphicsBackend::DirectX12;
            }
        } else if lower.starts_with("d3d11") {
            detected.insert("d3d11.dll".to_string());
            if primary < GraphicsBackend::DirectX11 {
                primary = GraphicsBackend::DirectX11;
            }
        } else if lower.starts_with("vulkan-1") || lower.starts_with("vulkan") {
            detected.insert("vulkan-1.dll".to_string());
            if primary < GraphicsBackend::Vulkan {
                primary = GraphicsBackend::Vulkan;
            }
        } else if lower.starts_with("opengl32") {
            detected.insert("opengl32.dll".to_string());
            if primary < GraphicsBackend::OpenGL {
                primary = GraphicsBackend::OpenGL;
            }
        } else if lower.starts_with("dxgi") {
            detected.insert("dxgi.dll".to_string());
            if primary < GraphicsBackend::DxgiOnly {
                primary = GraphicsBackend::DxgiOnly;
            }
        } else if lower.starts_with("d3d9") {
            detected.insert("d3d9.dll".to_string());
            if primary < GraphicsBackend::DirectX9 {
                primary = GraphicsBackend::DirectX9;
            }
        }
    }

    (primary, detected.into_iter().collect())
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

/// Enumerates the base file names of all modules currently loaded into the target process.
#[cfg(windows)]
pub fn query_process_modules(pid: u32) -> Result<Vec<String>, ProofError> {
    unsafe {
        let kernel32 = win32::LoadLibraryA(b"kernel32.dll\0".as_ptr());
        if kernel32.is_null() {
            return Ok(Vec::new());
        }

        let open_process: Option<unsafe extern "system" fn(u32, i32, u32) -> *mut win32::c_void> =
            std::mem::transmute(win32::GetProcAddress(kernel32, b"OpenProcess\0".as_ptr()));
        let close_handle: Option<unsafe extern "system" fn(*mut win32::c_void) -> i32> =
            std::mem::transmute(win32::GetProcAddress(kernel32, b"CloseHandle\0".as_ptr()));

        let enum_modules: Option<
            unsafe extern "system" fn(
                *mut win32::c_void,
                *mut *mut win32::c_void,
                u32,
                *mut u32,
            ) -> i32,
        > = std::mem::transmute(win32::GetProcAddress(
            kernel32,
            b"K32EnumProcessModules\0".as_ptr(),
        ));
        let get_base_name: Option<
            unsafe extern "system" fn(*mut win32::c_void, *mut win32::c_void, *mut u16, u32) -> u32,
        > = std::mem::transmute(win32::GetProcAddress(
            kernel32,
            b"K32GetModuleBaseNameW\0".as_ptr(),
        ));

        let (Some(open_proc), Some(close_h)) = (open_process, close_handle) else {
            return Ok(Vec::new());
        };

        // PROCESS_QUERY_INFORMATION (0x0400) | PROCESS_VM_READ (0x0010) = 0x0410
        let handle = open_proc(0x0410, 0, pid);
        if handle.is_null() {
            return Err(ProofError::ProcessNotFound(pid));
        }

        let mut modules_out = Vec::new();

        if let (Some(enum_mod), Some(get_name)) = (enum_modules, get_base_name) {
            let mut h_modules = vec![std::ptr::null_mut(); 512];
            let mut cb_needed: u32 = 0;
            let cb_total = (h_modules.len() * std::mem::size_of::<*mut win32::c_void>()) as u32;

            if enum_mod(handle, h_modules.as_mut_ptr(), cb_total, &mut cb_needed) != 0
                && cb_needed > 0
            {
                let count = ((cb_needed as usize) / std::mem::size_of::<*mut win32::c_void>())
                    .min(h_modules.len());
                for &h_mod in &h_modules[..count] {
                    if !h_mod.is_null() {
                        let mut name_buf = [0u16; 256];
                        let len =
                            get_name(handle, h_mod, name_buf.as_mut_ptr(), name_buf.len() as u32);
                        if len > 0 {
                            let name = String::from_utf16_lossy(&name_buf[..len as usize]);
                            if !name.trim().is_empty() {
                                modules_out.push(name);
                            }
                        }
                    }
                }
            }
        }

        close_h(handle);
        Ok(modules_out)
    }
}

#[cfg(not(windows))]
pub fn query_process_modules(_pid: u32) -> Result<Vec<String>, ProofError> {
    Ok(Vec::new())
}

/// Checks whether the given PID currently owns the foreground active window.
#[cfg(windows)]
pub fn is_process_foreground(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    unsafe {
        let user32 = win32::LoadLibraryA(b"user32.dll\0".as_ptr());
        if user32.is_null() {
            return false;
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

        let (Some(get_fg), Some(get_thread_pid)) =
            (get_foreground_window, get_window_thread_process_id)
        else {
            return false;
        };

        let hwnd = get_fg();
        if hwnd.is_null() {
            return false;
        }
        let mut fg_pid: u32 = 0;
        get_thread_pid(hwnd, &mut fg_pid);
        fg_pid == pid
    }
}

#[cfg(not(windows))]
pub fn is_process_foreground(_pid: u32) -> bool {
    false
}

/// Generates a comprehensive L2 GPU graphics rendering proof for a specific process ID.
pub fn generate_l2_render_proof(pid: u32) -> Result<RenderProof, ProofError> {
    let modules = query_process_modules(pid)?;
    let (primary_backend, loaded_render_modules) = classify_graphics_backend(&modules);
    let is_foreground = is_process_foreground(pid);
    let working_set_bytes = detect_process_working_set_bytes(pid);

    let engagement_level = if primary_backend != GraphicsBackend::None {
        if is_foreground {
            RenderEngagementLevel::ActiveForeground3D
        } else {
            RenderEngagementLevel::BackgroundInWorld3D
        }
    } else {
        RenderEngagementLevel::HeadlessOrMock
    };

    let process_name = crate::proof::l1_binary::query_process_image_path(pid)
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| format!("pid_{pid}.exe"));

    let verified_at_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    Ok(RenderProof {
        pid,
        process_name,
        is_foreground,
        primary_backend,
        loaded_render_modules,
        engagement_level,
        working_set_bytes,
        verified_at_millis,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_graphics_backend() {
        let (backend, mods) = classify_graphics_backend(&[
            "ntdll.dll".to_string(),
            "kernel32.dll".to_string(),
            "D3D12.dll".to_string(),
            "dxgi.dll".to_string(),
        ]);
        assert_eq!(backend, GraphicsBackend::DirectX12);
        assert!(mods.contains(&"d3d12.dll".to_string()));
        assert!(mods.contains(&"dxgi.dll".to_string()));

        let (backend_vk, mods_vk) =
            classify_graphics_backend(&["vulkan-1.dll".to_string(), "user32.dll".to_string()]);
        assert_eq!(backend_vk, GraphicsBackend::Vulkan);
        assert_eq!(mods_vk, vec!["vulkan-1.dll".to_string()]);

        let (backend_none, mods_none) =
            classify_graphics_backend(&["cmd.exe".to_string(), "conhost.exe".to_string()]);
        assert_eq!(backend_none, GraphicsBackend::None);
        assert!(mods_none.is_empty());
    }

    #[test]
    fn test_current_process_l2_render_proof() {
        let current_pid = std::process::id();
        let proof = generate_l2_render_proof(current_pid).expect("current process l2 proof");
        assert_eq!(proof.pid, current_pid);
        assert!(proof.working_set_bytes > 0);
        assert!(proof.verified_at_millis > 0);
        assert!(!proof.process_name.is_empty());
    }
}
