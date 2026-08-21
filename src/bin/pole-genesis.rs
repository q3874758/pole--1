//! `pole-genesis` — generate a PoLE `genesis.json`.
//!
//! Thin shim over `pole_protocol_draft::cli_genesis` so the unified
//! `pole` binary can run the same logic in-process.

use std::process::ExitCode;

fn main() -> ExitCode {
    match pole_protocol_draft::cli_genesis::run(&std::env::args().collect::<Vec<_>>()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("pole-genesis: {e}");
            ExitCode::FAILURE
        }
    }
}