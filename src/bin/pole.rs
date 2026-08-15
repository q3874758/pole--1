#![windows_subsystem = "windows"]

use std::env;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

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
    } else if program_name == "pole" || program_name == "pole.exe" {
        if args.len() > 1 {
            match args[1].as_str() {
                "client" => "client",
                "node" => "node",
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
            eprintln!("Missing mode: pole [client|node|help] <command> [args...]");
            print_usage();
            return;
        }
    } else {
        print_usage();
        return;
    };

    let binary_dir = program_path.parent().unwrap();

    let exit_code = match mode {
        "client" => run_forward(binary_dir, "pole-client.exe", &args),
        "node" => run_forward(binary_dir, "pole-node.exe", &args),
        _ => unreachable!(),
    };

    std::process::exit(exit_code);
}

fn run_forward(binary_dir: &Path, exe_name: &str, args: &[String]) -> i32 {
    let result = Command::new(binary_dir.join(exe_name))
        .args(&args[1..])
        .creation_flags(0x08000000)
        .spawn();
    match result {
        Ok(mut c) => c.wait().map(|s| s.code().unwrap_or(1)).unwrap_or(1),
        Err(e) => {
            eprintln!("Failed to run {}: {}", exe_name, e);
            1
        }
    }
}

fn print_usage() {
    eprintln!("PoLE V1 - Unified Client");
    eprintln!();
    eprintln!("Usage:");
    eprintln!("  pole [client|node|help] <command> [args...]");
    eprintln!();
    eprintln!("Modes:");
    eprintln!("  pole client <cmd> - Run client commands");
    eprintln!("  pole node <cmd>   - Run node commands");
    eprintln!("  pole help         - Show this help");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  pole client init  - Initialize client config");
    eprintln!("  pole node status  - Check node status");
}
