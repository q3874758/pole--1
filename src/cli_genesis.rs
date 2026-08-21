//! `cli_genesis` — library module backing the `pole-genesis` CLI:
//! generate a PoLE `genesis.json`.
//!
//! Usage (as invoked through the `pole-genesis` / unified `pole` binaries):
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

use crate::genesis_builder::{
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
    /// Parse CLI arguments. `args` is the full argv (including the program
    /// name at index 0, which is skipped to match the old `args().skip(1)`
    /// behavior). Returns `Ok(None)` when `--help`/`-h` was requested; the
    /// caller is responsible for printing the help text and exiting
    /// successfully.
    fn from_args(args: &[String]) -> Result<Option<Self>, String> {
        let mut chain_id = None;
        let mut allocations = None;
        let mut validators = None;
        let mut params = None;
        let mut out = None;
        let mut args = args.get(1..).unwrap_or_default().iter();
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
                "--help" | "-h" => return Ok(None),
                other => return Err(format!("unknown flag: {other}")),
            }
        }
        let chain_id = chain_id.ok_or_else(|| "--chain-id is required".to_string())?;
        Ok(Some(Self {
            chain_id,
            allocations,
            validators,
            params,
            out,
        }))
    }
}

pub fn print_help() {
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

pub fn run(args: &[String]) -> Result<(), GenesisError> {
    let cli = match Cli::from_args(args) {
        Ok(Some(cli)) => cli,
        Ok(None) => {
            // Help requested: print it and return success.
            print_help();
            return Ok(());
        }
        Err(msg) => return Err(GenesisError::Validation(msg)),
    };
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