#![windows_subsystem = "windows"]

use std::env;
use std::path::PathBuf;

/// Unified `pole` executable.
///
/// Dispatches in-process to the shared `cli_client` / `cli_node` library
/// modules. No longer spawns `pole-client.exe` / `pole-node.exe` — all
/// command logic runs inside this single binary.
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

    // Rebase argv so the selected mode's dispatcher sees `<cmd>` at index 1.
    //
    //   pole client init ...  -> [pole, init, ...]   (strip the "client" token)
    //   pole-client  init ... -> [pole-client, init, ...]  (no mode token)
    let forwarded: Vec<String> = match mode {
        "client" => rebase_args(&args, mode),
        "node" => rebase_args(&args, mode),
        _ => unreachable!(),
    };

    let result = match mode {
        "client" => pole_protocol_draft::cli_client::run(&forwarded),
        "node" => pole_protocol_draft::cli_node::run(&forwarded),
        _ => unreachable!(),
    };

    if let Err(err) = result {
        eprintln!("pole error: {err}");
        std::process::exit(1);
    }
}

/// Build the argument vector handed to the mode's dispatcher.
///
/// When invoked as `pole client|node <cmd> ...`, the mode token at index 1
/// is stripped so the underlying `dispatch_command` still reads `<cmd>` from
/// `args[1]`. When invoked via argv0 (`pole-client <cmd> ...`) no stripping
/// happens.
fn rebase_args(args: &[String], mode: &str) -> Vec<String> {
    let program_name_stripped_mode = matches!(args.get(1), Some(first) if first == mode);
    if program_name_stripped_mode {
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
