//! `pole-genesis` — generate a PoLE `genesis.json`.
//!
//! Usage:
//!     pole-genesis \
//!         --chain-id pole_7776-1 \
//!         --allocations allocations.csv \
//!         --validators  validators.json \
//!         --params      params-overrides.json \
//!         --out         genesis.json
//!
//! All flags except `--chain-id` and `--allocations` are optional.
//! `--validators` defaults to an empty list (validation will fail).
//! `--params` is a partial JSON object whose keys overwrite the
//! defaults produced by `default_pole_params`.

use std::path::PathBuf;
use std::process::ExitCode;

use pole_protocol_draft::genesis_builder::{
    GenesisBuilder, GenesisError, GenesisInputs, ValidatorSpec,
};

#[derive(Debug)]
struct Cli {
    chain_id: String,
    allocations: Option<PathBuf>,
    validators: Option<PathBuf>,
    params: Option<PathBuf>,
    out: Option<PathBuf>,
}

impl Cli {
    fn from_env() -> Result<Self, String> {
        let mut chain_id = None;
        let mut allocations = None;
        let mut validators = None;
        let mut params = None;
        let mut out = None;
        let mut args = std::env::args().skip(1);
        while let Some(a) = args.next() {
            match a.as_str() {
                "--chain-id" => {
                    chain_id = args
                        .next()
                        .map(PathBuf::from)
                        .map(|p| p.to_string_lossy().to_string())
                }
                "--allocations" => allocations = args.next().map(PathBuf::from),
                "--validators" => validators = args.next().map(PathBuf::from),
                "--params" => params = args.next().map(PathBuf::from),
                "--out" => out = args.next().map(PathBuf::from),
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                other => return Err(format!("unknown flag: {other}")),
            }
        }
        let chain_id = chain_id.ok_or_else(|| "--chain-id is required".to_string())?;
        Ok(Self {
            chain_id,
            allocations,
            validators,
            params,
            out,
        })
    }
}

fn print_help() {
    println!("pole-genesis — generate a PoLE genesis.json");
    println!();
    println!("USAGE:");
    println!("    pole-genesis --chain-id <id> --allocations <csv> [--validators <json>] [--params <json>] --out <path>");
    println!();
    println!("ARGS:");
    println!("    --chain-id       Cosmos chain id, e.g. pole_7776-1");
    println!("    --allocations    CSV with rows `address,amount_upole`");
    println!("    --validators     JSON array of validator specs (optional)");
    println!("    --params         JSON object whose keys override defaults");
    println!("    --out            Output path (default: ./genesis.json)");
}

fn run() -> Result<(), GenesisError> {
    let cli = Cli::from_env().map_err(|e| GenesisError::Validation(e))?;
    let builder = if let Some(alloc) = cli.allocations {
        GenesisBuilder::from_paths(
            cli.chain_id,
            alloc,
            cli.validators
                .unwrap_or_else(|| PathBuf::from("validators.json")),
            cli.params,
        )?
    } else {
        // `--allocations` omitted: build an empty-inputs builder that
        // will fail validation. Callers can plug in a struct via a
        // programmatic API in the future.
        GenesisBuilder::new(GenesisInputs {
            chain_id: cli.chain_id,
            allocations: Vec::new(),
            validators: Vec::<ValidatorSpec>::new(),
            params_overrides: serde_json::Value::Null,
        })
    };
    let out = cli.out.unwrap_or_else(|| PathBuf::from("genesis.json"));
    builder.write(&out)?;
    println!("wrote {out:?}");
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("pole-genesis: {e}");
            ExitCode::FAILURE
        }
    }
}
