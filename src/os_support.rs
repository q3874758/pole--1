//! OS-level support for lightweight, zero-overhead background operations on Windows.
//!
//! Designed specifically to eliminate game performance impacts:
//! - Direct Win32 FFI for process priority (Idle / Background mode) and EcoQoS (Efficiency Mode on E-cores)
//! - Zero external process spawning during monitoring loops (no powershell.exe or cmd.exe)
//! - Direct Win32 FFI for foreground window and process detection (< 0.01ms overhead)
//! - Direct Win32 FFI for active process enumeration via Toolhelp32 snapshot (< 1ms overhead)
//! - Working set memory trimming to keep RAM usage minimal (< 15MB)

#![allow(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

mod win32 {
    pub use std::ffi::c_void;

    pub const STILL_ACTIVE: u32 = 259;
    pub const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    pub const PROCESS_QUERY_INFORMATION: u32 = 0x0400;
    pub const PROCESS_TERMINATE: u32 = 0x0001;
    pub const TH32CS_SNAPPROCESS: u32 = 0x00000002;
    pub const IDLE_PRIORITY_CLASS: u32 = 0x00000040;
    pub const INVALID_HANDLE_VALUE: *mut c_void = -1isize as *mut c_void;

    pub const PROCESS_POWER_THROTTLING: i32 = 4;
    pub const PROCESS_POWER_THROTTLING_CURRENT_VERSION: u32 = 1;
    pub const PROCESS_POWER_THROTTLING_EXECUTION_SPEED: u32 = 0x1;

    #[repr(C)]
    pub struct PROCESS_POWER_THROTTLING_STATE {
        pub version: u32,
        pub control_mask: u32,
        pub state_mask: u32,
    }

    #[repr(C)]
    pub struct PROCESS_MEMORY_COUNTERS {
        pub cb: u32,
        pub page_fault_count: u32,
        pub peak_working_set_size: usize,
        pub working_set_size: usize,
        pub quota_peak_paged_pool_usage: usize,
        pub quota_paged_pool_usage: usize,
        pub quota_peak_non_paged_pool_usage: usize,
        pub quota_non_paged_pool_usage: usize,
        pub pagefile_usage: usize,
        pub peak_pagefile_usage: usize,
    }

    #[repr(C)]
    pub struct PROCESSENTRY32W {
        pub dw_size: u32,
        pub cnt_usage: u32,
        pub th32_process_id: u32,
        pub th32_default_heap_id: usize,
        pub th32_module_id: u32,
        pub cnt_threads: u32,
        pub th32_parent_process_id: u32,
        pub pc_pri_class_base: i32,
        pub dw_flags: u32,
        pub sz_exe_file: [u16; 260],
    }

    #[link(name = "kernel32")]
    extern "system" {
        pub fn GetCurrentProcess() -> *mut c_void;
        pub fn SetPriorityClass(h_process: *mut c_void, dw_priority_class: u32) -> i32;
        pub fn SetProcessInformation(
            h_process: *mut c_void,
            process_information_class: i32,
            process_information: *const c_void,
            process_information_size: u32,
        ) -> i32;
        pub fn SetProcessWorkingSetSize(
            h_process: *mut c_void,
            dw_minimum_working_set_size: usize,
            dw_maximum_working_set_size: usize,
        ) -> i32;
        pub fn OpenProcess(
            dw_desired_access: u32,
            b_inherit_handle: i32,
            dw_process_id: u32,
        ) -> *mut c_void;
        pub fn GetExitCodeProcess(h_process: *mut c_void, lp_exit_code: *mut u32) -> i32;
        pub fn TerminateProcess(h_process: *mut c_void, u_exit_code: u32) -> i32;
        pub fn CloseHandle(h_object: *mut c_void) -> i32;
        pub fn CreateToolhelp32Snapshot(dw_flags: u32, th32_process_id: u32) -> *mut c_void;
        pub fn Process32FirstW(h_snapshot: *mut c_void, lppe: *mut PROCESSENTRY32W) -> i32;
        pub fn Process32NextW(h_snapshot: *mut c_void, lppe: *mut PROCESSENTRY32W) -> i32;
        pub fn QueryFullProcessImageNameW(
            h_process: *mut c_void,
            dw_flags: u32,
            lp_exe_name: *mut u16,
            lpdw_size: *mut u32,
        ) -> i32;
        pub fn LoadLibraryA(lp_lib_file_name: *const u8) -> *mut c_void;
        pub fn GetProcAddress(h_module: *mut c_void, lp_proc_name: *const u8) -> *mut c_void;
    }
}

/// Applies background OS priority hints to ensure zero interference with games:
/// - Sets `IDLE_PRIORITY_CLASS` for lowest CPU and Disk I/O priority.
/// - Sets `PROCESS_POWER_THROTTLING_EXECUTION_SPEED` (EcoQoS) to route execution strictly to E-cores.
/// - Trims working set to minimize physical RAM consumption.
pub fn apply_process_background_priority() {
    unsafe {
        let handle = win32::GetCurrentProcess();

        // 1. Lower priority class to IDLE_PRIORITY_CLASS (0x00000040)
        // Ensures the process yields CPU to foreground games while still allowing clean child process spawning.
        win32::SetPriorityClass(handle, win32::IDLE_PRIORITY_CLASS);

        // 2. Set EcoQoS (Power Throttling - schedule exclusively on E-cores on hybrid architecture CPUs)
        let throttling = win32::PROCESS_POWER_THROTTLING_STATE {
            version: win32::PROCESS_POWER_THROTTLING_CURRENT_VERSION,
            control_mask: win32::PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
            state_mask: win32::PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
        };
        let _ = win32::SetProcessInformation(
            handle,
            win32::PROCESS_POWER_THROTTLING,
            &throttling as *const _ as *const win32::c_void,
            std::mem::size_of::<win32::PROCESS_POWER_THROTTLING_STATE>() as u32,
        );

        // 3. Trim working set
        trim_process_working_set();
    }
}

/// Trims unused pages from the process working set, keeping physical memory footprint minimal.
pub fn trim_process_working_set() {
    unsafe {
        let handle = win32::GetCurrentProcess();
        win32::SetProcessWorkingSetSize(handle, usize::MAX, usize::MAX);
    }
}

/// Checks whether a process with the given PID is currently active.
/// Uses zero-overhead native OS APIs without launching child processes.
pub fn is_process_running(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }

    unsafe {
        let handle = win32::OpenProcess(win32::PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return false;
        }
        let mut exit_code: u32 = 0;
        let ok = win32::GetExitCodeProcess(handle, &mut exit_code);
        win32::CloseHandle(handle);
        ok != 0 && exit_code == win32::STILL_ACTIVE
    }
}

/// Terminates a process by PID using native OS APIs.
pub fn kill_process(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }

    unsafe {
        let handle = win32::OpenProcess(win32::PROCESS_TERMINATE, 0, pid);
        if !handle.is_null() {
            let res = win32::TerminateProcess(handle, 1);
            win32::CloseHandle(handle);
            if res != 0 {
                return true;
            }
        }
    }
    // Fallback to taskkill if TerminateProcess was refused (e.g. cross-session or elevation)
    std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/F"])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Detects the executable name of the active foreground window without child process spawning.
pub fn detect_foreground_process_name() -> Option<String> {
    if let Ok(override_name) = std::env::var("POLE_CLIENT_FOREGROUND_PROCESS_OVERRIDE") {
        return display_process_name(&override_name);
    }

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

        let handle = win32::OpenProcess(win32::PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        let handle = if handle.is_null() {
            win32::OpenProcess(win32::PROCESS_QUERY_INFORMATION, 0, pid)
        } else {
            handle
        };

        if handle.is_null() {
            return None;
        }

        let mut buf = [0u16; 1024];
        let mut size = buf.len() as u32;
        let res = win32::QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size);
        win32::CloseHandle(handle);

        if res == 0 || size == 0 {
            return None;
        }

        let raw_path = String::from_utf16_lossy(&buf[..size as usize]);
        let file_name = std::path::Path::new(&raw_path)
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or(&raw_path);

        display_process_name(file_name)
    }
}

/// Detects the window title of the current foreground active window.
pub fn detect_foreground_window_title() -> Option<String> {
    if let Ok(override_title) = std::env::var("POLE_CLIENT_FOREGROUND_TITLE_OVERRIDE") {
        let trimmed = override_title.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

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
        let get_window_text_w: Option<
            unsafe extern "system" fn(*mut win32::c_void, *mut u16, i32) -> i32,
        > = std::mem::transmute(win32::GetProcAddress(user32, b"GetWindowTextW\0".as_ptr()));

        let (Some(get_fg), Some(get_text)) = (get_foreground_window, get_window_text_w) else {
            return None;
        };

        let hwnd = get_fg();
        if hwnd.is_null() {
            return None;
        }

        let mut buf = [0u16; 512];
        let len = get_text(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        if len <= 0 {
            return None;
        }

        let title = String::from_utf16_lossy(&buf[..len as usize]);
        let trimmed = title.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }
}

/// Detects the physical working set size (in bytes) of a process by PID.
pub fn detect_process_working_set_bytes(pid: u32) -> u64 {
    if pid == 0 {
        return 0;
    }

    unsafe {
        let kernel32 = win32::LoadLibraryA(b"kernel32.dll\0".as_ptr());
        if kernel32.is_null() {
            return 0;
        }

        let get_mem_info: Option<
            unsafe extern "system" fn(
                *mut win32::c_void,
                *mut win32::PROCESS_MEMORY_COUNTERS,
                u32,
            ) -> i32,
        > = std::mem::transmute(win32::GetProcAddress(
            kernel32,
            b"K32GetProcessMemoryInfo\0".as_ptr(),
        ));

        let Some(get_info) = get_mem_info else {
            return 0;
        };

        let handle = win32::OpenProcess(win32::PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return 0;
        }

        let mut counters: win32::PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        counters.cb = std::mem::size_of::<win32::PROCESS_MEMORY_COUNTERS>() as u32;
        let ok = get_info(handle, &mut counters, counters.cb);
        win32::CloseHandle(handle);

        if ok != 0 {
            counters.working_set_size as u64
        } else {
            0
        }
    }
}

/// Represents the evaluated game engagement state of a player node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PlayEngagementState {
    /// Process is in launcher, login dialog, or main menu / title screen (not counted)
    MainMenu,
    /// Process has loaded game world / session (valid play, active or AFK in-world)
    #[default]
    InWorld,
}

/// Checks whether a window title indicates a launcher, updater, or main menu screen.
pub fn is_main_menu_or_launcher_title(title: &str) -> bool {
    let lower = title.to_ascii_lowercase();
    let trimmed = lower.trim();
    if trimmed.is_empty() {
        return false;
    }

    trimmed.contains("launcher")
        || trimmed.contains("updater")
        || trimmed.contains("splash screen")
        || trimmed.contains("press start")
        || trimmed.contains("press any key")
        || trimmed.contains("title screen")
        || trimmed.contains("main menu")
        || trimmed.contains("login")
        || trimmed.contains("sign in")
        || trimmed.ends_with(" - main menu")
        || trimmed.ends_with(" - title")
}

/// Checks whether an executable process name is an external launcher rather than the game itself.
pub fn is_launcher_process_name(process_name: &str) -> bool {
    let lower = process_name.to_ascii_lowercase();
    lower.contains("launcher")
        || lower.contains("bootstrap")
        || lower.contains("updater")
        || lower.contains("crashreport")
        || lower.contains("easyanticheat")
}

/// Evaluates whether the game process is in an active in-world session (including AFK)
/// or parked at the main menu / launcher.
///
/// Rules:
/// - In-world gaming (even when character is AFK / sleeping) is valid and fully rewarded.
/// - Parked at launcher, login screen, or main menu without loading the world is rejected.
pub fn evaluate_game_engagement(process_name: &str, pid: Option<u32>) -> PlayEngagementState {
    let _ = pid;
    if let Ok(override_val) = std::env::var("POLE_ENGAGEMENT_STATE_OVERRIDE") {
        let lower = override_val.trim().to_ascii_lowercase();
        if lower == "main_menu" || lower == "menu" {
            return PlayEngagementState::MainMenu;
        }
        if lower == "in_world" || lower == "world" {
            return PlayEngagementState::InWorld;
        }
    }

    // 1. Process name matches standalone launcher/bootstrap wrapper
    if is_launcher_process_name(process_name) {
        return PlayEngagementState::MainMenu;
    }

    // 2. Foreground window title detection
    if let Some(title) = detect_foreground_window_title() {
        if is_main_menu_or_launcher_title(&title) {
            return PlayEngagementState::MainMenu;
        }
    }

    // 3. Low working-set threshold check for 3D PC games
    if let Some(pid_val) = pid {
        let working_set = detect_process_working_set_bytes(pid_val);
        // Modern 3D PC games (cs2, elden ring, etc.) occupy multi-GB when world assets load;
        // Under 120MB strongly indicates initial launcher or title stub.
        if working_set > 0 && working_set < 120 * 1024 * 1024 {
            if let Some(title) = detect_foreground_window_title() {
                let lower_proc = normalize_process_name(process_name);
                if title.to_ascii_lowercase().contains(&lower_proc)
                    && is_main_menu_or_launcher_title(&title)
                {
                    return PlayEngagementState::MainMenu;
                }
            }
        }
    }

    // Default to InWorld: legitimate players AFK in-world are fully counted
    PlayEngagementState::InWorld
}

/// Detects which configured target process names are currently running on the system.
/// Uses native process snapshotting (< 1ms, zero child processes).
pub fn detect_active_process_names(process_names: &[String]) -> Vec<String> {
    let configured = normalize_process_names(process_names);
    if configured.is_empty() {
        return Vec::new();
    }

    let running = list_running_process_names();
    match_configured_process_names(&configured, &running)
}

fn list_running_process_names() -> Vec<String> {
    let mut names = Vec::new();
    unsafe {
        let snapshot = win32::CreateToolhelp32Snapshot(win32::TH32CS_SNAPPROCESS, 0);
        if snapshot.is_null() || snapshot == win32::INVALID_HANDLE_VALUE {
            return names;
        }

        let mut entry = std::mem::zeroed::<win32::PROCESSENTRY32W>();
        entry.dw_size = std::mem::size_of::<win32::PROCESSENTRY32W>() as u32;

        if win32::Process32FirstW(snapshot, &mut entry) != 0 {
            loop {
                let len = entry
                    .sz_exe_file
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.sz_exe_file.len());
                let exe_name = String::from_utf16_lossy(&entry.sz_exe_file[..len]);
                if !exe_name.is_empty() {
                    names.push(exe_name);
                }

                if win32::Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }
        win32::CloseHandle(snapshot);
    }
    names
}

pub fn normalize_process_name(input: &str) -> String {
    input
        .trim()
        .to_ascii_lowercase()
        .trim_end_matches(".exe")
        .to_string()
}

pub fn normalize_process_names(process_names: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut normalized = Vec::new();
    for name in process_names {
        let name = normalize_process_name(name);
        if !name.is_empty() && seen.insert(name.clone()) {
            normalized.push(name);
        }
    }
    normalized
}

pub fn display_process_name(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    let base = if trimmed.len() >= 4 && trimmed[trimmed.len() - 4..].eq_ignore_ascii_case(".exe") {
        &trimmed[..trimmed.len() - 4]
    } else {
        trimmed
    };
    Some(format!("{base}.exe"))
}

pub fn match_configured_process_names(configured: &[String], running: &[String]) -> Vec<String> {
    let running_set = normalize_process_names(running)
        .into_iter()
        .collect::<BTreeSet<_>>();
    configured
        .iter()
        .filter(|name| running_set.contains(*name))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_match_configured_process_names() {
        let configured = vec!["cs2".to_string(), "dota2".to_string()];
        let running = vec!["cs2.exe".to_string(), "explorer.exe".to_string()];
        let matched = match_configured_process_names(&configured, &running);
        assert_eq!(matched, vec!["cs2"]);
    }

    #[test]
    fn test_process_normalization() {
        assert_eq!(normalize_process_name("CS2.EXE"), "cs2");
        assert_eq!(normalize_process_name(" Cyberpunk2077 "), "cyberpunk2077");
        assert_eq!(display_process_name("dota2"), Some("dota2.exe".to_string()));
        assert_eq!(
            display_process_name("dota2.exe"),
            Some("dota2.exe".to_string())
        );
        assert_eq!(display_process_name("   "), None);
    }

    #[test]
    fn test_current_process_is_running() {
        let current_pid = std::process::id();
        assert!(is_process_running(current_pid));
        assert!(!is_process_running(0));
    }

    #[test]
    fn test_background_priority_does_not_panic() {
        apply_process_background_priority();
        trim_process_working_set();
    }

    #[test]
    fn test_active_process_detection() {
        // Current process executable should be detectable if in list
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_name) = exe_path.file_name().and_then(|f| f.to_str()) {
                let detected = detect_active_process_names(&[exe_name.to_string()]);
                assert!(
                    !detected.is_empty(),
                    "current process {} should be detected",
                    exe_name
                );
            }
        }
    }

    #[test]
    fn test_main_menu_and_launcher_detection() {
        assert!(is_main_menu_or_launcher_title(
            "Counter-Strike 2 - Main Menu"
        ));
        assert!(is_main_menu_or_launcher_title("Elden Ring - Title Screen"));
        assert!(is_main_menu_or_launcher_title("Game Launcher v1.2"));
        assert!(is_main_menu_or_launcher_title("Please Login to Continue"));
        assert!(is_main_menu_or_launcher_title("Press Start - Palworld"));
        assert!(!is_main_menu_or_launcher_title("Counter-Strike 2"));
        assert!(!is_main_menu_or_launcher_title("Playing Casual on Dust II"));
        assert!(!is_main_menu_or_launcher_title("Exploring Limgrave"));

        assert!(is_launcher_process_name("masseffectlauncher.exe"));
        assert!(is_launcher_process_name("EpicGamesLauncher.exe"));
        assert!(is_launcher_process_name("steam_updater.exe"));
        assert!(!is_launcher_process_name("cs2.exe"));
        assert!(!is_launcher_process_name("eldenring.exe"));
    }

    #[test]
    fn test_evaluate_game_engagement_state() {
        // Launcher process is evaluated as MainMenu
        assert_eq!(
            evaluate_game_engagement("masseffectlauncher.exe", None),
            PlayEngagementState::MainMenu
        );

        // Normal game process with no menu title evaluates as InWorld (even if AFK)
        assert_eq!(
            evaluate_game_engagement("cs2.exe", None),
            PlayEngagementState::InWorld
        );

        // Environment override works for integration/unit test isolation
        std::env::set_var("POLE_ENGAGEMENT_STATE_OVERRIDE", "main_menu");
        assert_eq!(
            evaluate_game_engagement("cs2.exe", None),
            PlayEngagementState::MainMenu
        );
        std::env::set_var("POLE_ENGAGEMENT_STATE_OVERRIDE", "in_world");
        assert_eq!(
            evaluate_game_engagement("masseffectlauncher.exe", None),
            PlayEngagementState::InWorld
        );
        std::env::remove_var("POLE_ENGAGEMENT_STATE_OVERRIDE");
    }
}
