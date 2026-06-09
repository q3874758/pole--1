#![windows_subsystem = "windows"]

use std::env;
use std::net::TcpStream;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

const POLED_HTTP_PORT: u16 = 1317;

fn main() {
    let args: Vec<String> = env::args().collect();
    let program_path = env::args().next().map(PathBuf::from).unwrap();
    let program_name = program_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("pole");

    let mode = if program_name == "pole-client" || program_name == "pole-client.exe" {
        "client"
    } else if program_name == "pole-node" || program_name == "pole-node.exe" {
        "node"
    } else if program_name == "pole-gui" || program_name == "pole-gui.exe" {
        "gui"
    } else if program_name == "pole" || program_name == "pole.exe" {
        if args.len() > 1 {
            match args[1].as_str() {
                "client" => "client",
                "node" => "node",
                "gui" => "gui",
                "full" => "full",
                "help" | "-h" | "--help" => {
                    print_usage();
                    return;
                }
                _ => {
                    eprintln!("Unknown mode: {}", args[1]);
                    print_usage();
                    return;
                }
            }
        } else {
            "full"
        }
    } else {
        print_usage();
        return;
    };

    let binary_dir = program_path.parent().unwrap();

    let exit_code = match mode {
        "client" => run_client(binary_dir, &args),
        "node" => run_node(binary_dir, &args),
        "gui" => run_gui(binary_dir, &args),
        "full" => run_full(binary_dir, &args),
        _ => unreachable!(),
    };

    std::process::exit(exit_code);
}

fn run_client(binary_dir: &Path, args: &[String]) -> i32 {
    let result = Command::new(binary_dir.join("pole-client.exe"))
        .args(&args[1..])
        .creation_flags(0x08000000)
        .spawn();
    match result {
        Ok(mut c) => c.wait().map(|s| s.code().unwrap_or(1)).unwrap_or(1),
        Err(e) => {
            eprintln!("Failed to run pole-client: {}", e);
            1
        }
    }
}

fn run_node(binary_dir: &Path, args: &[String]) -> i32 {
    let result = Command::new(binary_dir.join("pole-node.exe"))
        .args(&args[1..])
        .creation_flags(0x08000000)
        .spawn();
    match result {
        Ok(mut c) => c.wait().map(|s| s.code().unwrap_or(1)).unwrap_or(1),
        Err(e) => {
            eprintln!("Failed to run pole-node: {}", e);
            1
        }
    }
}

fn run_gui(binary_dir: &Path, args: &[String]) -> i32 {
    let result = Command::new(binary_dir.join("pole-gui.exe"))
        .args(&args[1..])
        .creation_flags(0x08000000)
        .spawn();
    match result {
        Ok(mut c) => c.wait().map(|s| s.code().unwrap_or(1)).unwrap_or(1),
        Err(e) => {
            eprintln!("Failed to run pole-gui: {}", e);
            1
        }
    }
}

fn run_full(binary_dir: &Path, _args: &[String]) -> i32 {
    let chain_home = binary_dir.join("pole-chain-data");
    let poled_exe = binary_dir.join("poled.exe");

    if !poled_exe.exists() {
        eprintln!("Error: poled.exe not found in same directory as pole.exe");
        eprintln!("  Expected: {}", poled_exe.display());
        eprintln!();
        eprintln!("The PoLE blockchain node binary is missing.");
        eprintln!("Please place poled.exe next to pole.exe and try again.");
        return 1;
    }

    if !chain_home.join("config").join("genesis.json").exists() {
        println!("[PoLE] Initializing blockchain home directory...");
        let output = Command::new(&poled_exe)
            .args(["init", "--home", &chain_home.to_string_lossy()])
            .creation_flags(0x08000000)
            .output();
        match output {
            Ok(out) => {
                if !out.status.success() {
                    eprintln!(
                        "[PoLE] Failed to initialize chain (exit code: {:?})",
                        out.status.code()
                    );
                    return 1;
                }
            }
            Err(e) => {
                eprintln!("[PoLE] Failed to run poled init: {}", e);
                return 1;
            }
        }
        println!("[PoLE] Genesis initialized.");
    }

    println!("[PoLE] Starting blockchain node...");
    let poled_child = Command::new(&poled_exe)
        .args(["start", "--home", &chain_home.to_string_lossy()])
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .spawn();
    let mut poled = match poled_child {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[PoLE] Failed to start poled: {}", e);
            return 1;
        }
    };

    println!(
        "[PoLE] Waiting for blockchain RPC to be ready (port {})...",
        POLED_HTTP_PORT
    );
    let max_wait = 60;
    for i in 0..max_wait {
        if TcpStream::connect(("localhost", POLED_HTTP_PORT)).is_ok() {
            println!("[PoLE] Blockchain RPC is ready!");
            break;
        }
        match poled.try_wait() {
            Ok(Some(status)) if i > 2 => {
                let code = status.code().unwrap_or(-1);
                eprintln!(
                    "[PoLE] poled crashed (exit {}), cleaning up and retrying...",
                    code
                );
                drop(poled);
                let _ = std::fs::remove_dir_all(&chain_home);
                let init_out = Command::new(&poled_exe)
                    .args(["init", "--home", &chain_home.to_string_lossy()])
                    .creation_flags(0x08000000)
                    .output();
                let init_ok = init_out
                    .as_ref()
                    .map(|o| o.status.success())
                    .unwrap_or(false);
                if !init_ok {
                    eprintln!("[PoLE] Failed to reinitialize chain");
                    return 1;
                }
                let retry = Command::new(&poled_exe)
                    .args(["start", "--home", &chain_home.to_string_lossy()])
                    .creation_flags(0x08000000)
                    .spawn();
                match retry {
                    Ok(c) => {
                        poled = c;
                    }
                    Err(e) => {
                        eprintln!("[PoLE] Failed to restart poled: {}", e);
                        return 1;
                    }
                }
            }
            _ => {}
        }
        if i == max_wait - 1 {
            eprintln!(
                "[PoLE] Timeout waiting for blockchain RPC after {}s",
                max_wait
            );
            let _ = poled.kill();
            return 1;
        }
        thread::sleep(Duration::from_secs(1));
        print!(".");
    }
    println!();

    println!("[PoLE] Starting GUI...");
    let gui_result = Command::new(binary_dir.join("pole-gui.exe"))
        .creation_flags(0x08000000)
        .spawn();
    match gui_result {
        Ok(mut gui) => {
            let _ = gui.wait();
        }
        Err(e) => {
            eprintln!("[PoLE] Failed to start GUI: {}", e);
            let _ = poled.kill();
            return 1;
        }
    }

    println!("[PoLE] Shutting down blockchain node...");
    let _ = poled.kill();
    let _ = poled.wait();
    println!("[PoLE] Done.");
    0
}

fn print_usage() {
    eprintln!("PoLE V1 - Unified Client");
    eprintln!("");
    eprintln!("Usage:");
    eprintln!("  pole [client|node|gui|full|help] <command> [args...]");
    eprintln!("");
    eprintln!("Modes:");
    eprintln!("  pole              - Start everything (default)");
    eprintln!("  pole full         - Start blockchain + GUI");
    eprintln!("  pole gui          - Start GUI only");
    eprintln!("  pole client <cmd> - Run client commands");
    eprintln!("  pole node <cmd>   - Run node commands");
    eprintln!("  pole help         - Show this help");
    eprintln!("");
    eprintln!("One-click startup (pole or pole full):");
    eprintln!("  1. Starts embedded Cosmos blockchain (poled)");
    eprintln!("  2. Waits for RPC to be ready");
    eprintln!("  3. Opens the GUI dashboard");
    eprintln!("");
    eprintln!("Examples:");
    eprintln!("  pole              - Start everything (default)");
    eprintln!("  pole full         - Same as above, explicit");
    eprintln!("  pole client init  - Initialize client config");
    eprintln!("  pole node status  - Check node status");
}
