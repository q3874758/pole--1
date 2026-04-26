use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{ActivitySourceKind, AssembledBatch, NodeConfig, P2pSimulationConfig};

pub fn current_unix_millis() -> Result<u64, Box<dyn std::error::Error>> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64)
}

pub fn source_kind_label(source_kind: ActivitySourceKind) -> &'static str {
    match source_kind {
        ActivitySourceKind::Steam => "steam",
        ActivitySourceKind::Epic => "epic",
        ActivitySourceKind::Ea => "ea",
        ActivitySourceKind::Gog => "gog",
        ActivitySourceKind::Community => "community",
    }
}

pub fn maybe_write_payload(
    assembled: &AssembledBatch,
    output_path: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(path) = output_path {
        let path = Path::new(path);
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        fs::write(path, &assembled.payload_bytes)?;
    }
    Ok(())
}

pub fn print_batch_summary(config: &NodeConfig, assembled: &AssembledBatch) {
    println!("collector_id={}", config.node_id_hex);
    println!("payload_cid={}", assembled.payload_cid);
    println!("payload_hash={}", crate::hex_32(assembled.payload_hash));
    println!(
        "batch_root={}",
        crate::hex_32(assembled.batch_commit.batch.root)
    );
    println!("obs_count={}", assembled.batch_commit.obs_count);
    println!(
        "slot_range={}-{}",
        assembled.batch_commit.slot_start, assembled.batch_commit.slot_end
    );
}

pub fn parse_simulation_topology_args(
    args: &[String],
    start_index: usize,
    usage: &str,
    defaults: P2pSimulationConfig,
) -> Result<P2pSimulationConfig, Box<dyn std::error::Error>> {
    let mut topology = defaults;
    let mut index = start_index;

    while index < args.len() {
        let flag = args[index].as_str();
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("{usage}: missing value for {flag}"))?;
        let parsed = value
            .parse::<usize>()
            .map_err(|_| format!("{usage}: invalid value for {flag}: {value}"))?;

        match flag {
            "--batch-listeners" => topology.batch_listener_count = parsed,
            "--receipt-listeners" => topology.receipt_listener_count = parsed,
            "--dual-listeners" => topology.dual_listener_count = parsed,
            _ => return Err(format!("{usage}: unknown argument {flag}").into()),
        }

        index += 2;
    }

    if topology.batch_listener_count
        + topology.receipt_listener_count
        + topology.dual_listener_count
        == 0
    {
        return Err(format!("{usage}: at least one listener is required").into());
    }

    Ok(topology)
}
