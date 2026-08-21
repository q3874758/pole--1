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
    let program_path = env::args().next().map(PathBuf::from).unwrap();
    let program_name = program_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("pole");

    let mode = if program_name == "pole-client" || program_name == "pole-client.exe" {
        "client"
    } else if program_name == "pole-node" || program_name == "pole-node.exe" {
        "node"
    } else if program_name == "pole-genesis" || program_name == "pole-genesis.exe" {
        "genesis"
    } else if program_name == "pole-sbom" || program_name == "pole-sbom.exe" {
        "sbom"
    } else if program_name == "pole" || program_name == "pole.exe" {
        if args.len() > 1 {
            match args[1].as_str() {
                "client" => "client",
                "node" => "node",
                "genesis" => "genesis",
                "sbom" => "sbom",
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
            eprintln!("Missing mode: pole [client|node|genesis|sbom|help] <command> [args...]");
            print_usage();
            return;
        }
    } else {
        print_usage();
        return;
    };

    // Rebase argv so the selected mode's dispatcher sees the command /
    // flags after the mode token:
    //
    //   pole client init ...     -> [pole, init, ...]        (strip "client")
    //   pole genesis --chain-id  -> [pole, --chain-id, ...]  (strip "genesis")
    //   pole-client  init ...    -> [pole-client, init, ...] (no mode token)
    let forwarded: Vec<String> = rebase_args(&args, mode);

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
    eprintln!("  pole [client|node|genesis|sbom|help] <command> [args...]");
    eprintln!();
    eprintln!("Modes:");
    eprintln!("  pole client <cmd>   - Run client commands");
    eprintln!("  pole node <cmd>     - Run node commands");
    eprintln!("  pole genesis <flags> - Generate a PoLE genesis.json");
    eprintln!("  pole sbom <flags>   - Generate a SBOM / license audit");
    eprintln!("  pole help           - Show this help");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  pole client init  - Initialize client config");
    eprintln!("  pole node status  - Check node status");
    eprintln!("  pole genesis --chain-id pole_7776-1 --allocations allocations.csv --out genesis.json");
    eprintln!("  pole sbom --out sbom.cdx.json");
}