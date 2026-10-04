#![windows_subsystem = "windows"]

use std::env;
use std::path::PathBuf;

/// Unified `pole` executable.
///
/// Dispatches in-process to the shared `cli_client` / `cli_node` /
/// `cli_genesis` / `cli_sbom` library modules. No longer spawns
/// `pole-client.exe` / `pole-node.exe` / `pole-genesis.exe` /
/// `pole-sbom.exe` — all command logic runs inside this single binary.
fn main() {
    let args: Vec<String> = env::args().collect();
    let is_background_daemon = args.iter().any(|arg| {
        arg == "watch"
            || arg == "watch-p2p-sim"
            || arg == "watch-p2p-fs"
            || arg == "watch-p2p-socket"
            || arg == "control-api-serve"
    });

    #[cfg(windows)]
    if !is_background_daemon {
        unsafe {
            extern "system" {
                fn AttachConsole(dwProcessId: u32) -> i32;
            }
            const ATTACH_PARENT_PROCESS: u32 = 0xFFFF_FFFF;
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }

    let _ = pole_protocol_draft::ensure_default_identity_password();
    let program_path = env::args().next().map(PathBuf::from).unwrap();
    let program_name = program_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("pole");

    let (mode, is_subcommand_dispatch) =
        if program_name == "pole-client" || program_name == "pole-client.exe" {
            ("client", false)
        } else if program_name == "pole-node" || program_name == "pole-node.exe" {
            ("node", false)
        } else if program_name == "pole-genesis" || program_name == "pole-genesis.exe" {
            ("genesis", false)
        } else if program_name == "pole-sbom" || program_name == "pole-sbom.exe" {
            ("sbom", false)
        } else if program_name == "pole" || program_name == "pole.exe" {
            if args.len() > 1 {
                match args[1].as_str() {
                    "client" => ("client", true),
                    "node" => ("node", true),
                    "genesis" => ("genesis", true),
                    "sbom" => ("sbom", true),
                    "help" | "-h" | "--help" => {
                        print_usage();
                        return;
                    }
                    cmd if pole_protocol_draft::cli_client::CLIENT_COMMANDS
                        .iter()
                        .any(|(name, _)| *name == cmd) =>
                    {
                        ("client", false)
                    }
                    cmd if pole_protocol_draft::cli_node::NODE_COMMANDS
                        .iter()
                        .any(|(name, _)| *name == cmd) =>
                    {
                        ("node", false)
                    }
                    _ => {
                        eprintln!("Unknown mode or command: {}", args[1]);
                        print_usage();
                        return;
                    }
                }
            } else {
                // Running pole with 0 arguments (e.g. double-clicked pole.exe)
                // Launches one-click player mode and opens the web dashboard!
                ("client", false)
            }
        } else {
            print_usage();
            return;
        };

    let forwarded: Vec<String> = if args.len() <= 1 {
        vec![args[0].clone(), "player-start".to_string()]
    } else if is_subcommand_dispatch {
        rebase_args(&args, mode)
    } else {
        args.clone()
    };

    match mode {
        "client" => {
            if let Err(err) = pole_protocol_draft::cli_client::run(&forwarded) {
                eprintln!("pole error: {err}");
                std::process::exit(1);
            }
        }
        "node" => {
            if let Err(err) = pole_protocol_draft::cli_node::run(&forwarded) {
                eprintln!("pole error: {err}");
                std::process::exit(1);
            }
        }
        "genesis" => {
            if let Err(err) = pole_protocol_draft::cli_genesis::run(&forwarded) {
                eprintln!("pole-genesis: {err}");
                std::process::exit(1);
            }
        }
        "sbom" => match pole_protocol_draft::cli_sbom::run(&forwarded) {
            Ok(0) => {}
            Ok(code) => std::process::exit(code),
            Err(err) => {
                eprintln!("error: {err}");
                std::process::exit(1);
            }
        },
        _ => unreachable!(),
    }
}

/// Build the argument vector handed to the mode's dispatcher.
///
/// When invoked as `pole <mode> ...`, the mode token at index 1 is
/// stripped so the underlying handler reads its command/flags starting at
/// index 1. When invoked via argv0 (`pole-client ...` etc.) no stripping
/// happens.
fn rebase_args(args: &[String], mode: &str) -> Vec<String> {
    let mode_token_stripped = matches!(args.get(1), Some(first) if first == mode);
    if mode_token_stripped {
        let mut rebased = Vec::with_capacity(args.len() - 1);
        rebased.push(args[0].clone());
        rebased.extend_from_slice(&args[2..]);
        rebased
    } else {
        args.to_vec()
    }
}

fn print_usage() {
    eprintln!("PoLE V1 - Unified Client");
    eprintln!();
    eprintln!("Usage:");
    eprintln!("  pole [command] [args...]");
    eprintln!("  pole [client|node|genesis|sbom|help] <command> [args...]");
    eprintln!();
    eprintln!("Quick Start:");
    eprintln!("  pole               - One-click launch player mode and open dashboard");
    eprintln!("  pole player-start  - Start player node and open dashboard");
    eprintln!("  pole player-stop   - Stop player node");
    eprintln!("  pole status        - Show node status");
    eprintln!();
    eprintln!("Modes:");
    eprintln!("  pole client <cmd>    - Run client commands");
    eprintln!("  pole node <cmd>      - Run node commands");
    eprintln!("  pole genesis <flags> - Generate a PoLE genesis.json");
    eprintln!("  pole sbom <flags>    - Generate a SBOM / license audit");
    eprintln!("  pole help            - Show this help");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  pole               - Double-click or run directly to start playing");
    eprintln!("  pole status        - Check player node status");
    eprintln!("  pole client init   - Initialize client config");
    eprintln!("  pole node status   - Check node status");
}
