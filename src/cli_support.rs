use std::path::{Path, PathBuf};

use crate::{
    effective_challenge_window_blocks, portable_layout_for_config, progress_path,
    runtime_layout_for_config, suggested_settlement_height, LocalNodeProgress, NodeConfig,
};

pub fn print_path_entry(label: &str, path: impl AsRef<Path>) {
    println!("{label}={}", path.as_ref().to_string_lossy());
}

pub fn print_data_dir_path(label: &str, data_dir: &Path, child: &str) {
    print_path_entry(label, data_dir.join(child));
}

pub fn print_command_header(command_name: &str, config_path: &Path) {
    println!("PoLE client {command_name}");
    print_path_entry("config_path", config_path);
}

pub fn default_data_dir_for_config(config_path: &Path) -> String {
    portable_layout_for_config(config_path)
        .data_dir
        .to_string_lossy()
        .into_owned()
}

pub fn effective_install_layout(config_path: &Path, config: &NodeConfig) -> crate::InstallLayout {
    runtime_layout_for_config(config_path, &config.runtime.data_dir)
}

pub fn parse_optional_u64_arg(
    args: &[String],
    index: usize,
) -> Result<Option<u64>, Box<dyn std::error::Error>> {
    args.get(index)
        .map(|value| value.parse::<u64>().map_err(Into::into))
        .transpose()
}

pub fn parse_optional_u32_arg(
    args: &[String],
    index: usize,
) -> Result<Option<u32>, Box<dyn std::error::Error>> {
    args.get(index)
        .map(|value| value.parse::<u32>().map_err(Into::into))
        .transpose()
}

pub fn resolve_epoch_id_arg(
    args: &[String],
    index: usize,
    config: &NodeConfig,
) -> Result<u64, Box<dyn std::error::Error>> {
    Ok(parse_optional_u64_arg(args, index)?.unwrap_or(latest_local_epoch(config)?))
}

pub fn resolve_current_height_arg(
    args: &[String],
    index: usize,
    progress: &LocalNodeProgress,
) -> Result<u64, Box<dyn std::error::Error>> {
    Ok(parse_optional_u64_arg(args, index)?.unwrap_or(progress.ticks_completed.max(1)))
}

pub fn resolve_submission_height_arg(
    args: &[String],
    index: usize,
    config: &NodeConfig,
) -> Result<u64, Box<dyn std::error::Error>> {
    Ok(parse_optional_u64_arg(args, index)?.unwrap_or(suggested_settlement_height(config)?))
}

pub fn resolve_challenge_window_blocks_arg(
    args: &[String],
    index: usize,
    config: &NodeConfig,
) -> Result<u32, Box<dyn std::error::Error>> {
    Ok(parse_optional_u32_arg(args, index)?.unwrap_or(effective_challenge_window_blocks(config)))
}

pub fn load_config_and_epoch_arg(
    args: &[String],
    config_arg_index: usize,
    epoch_arg_index: usize,
    default_config_path: &str,
) -> Result<(PathBuf, NodeConfig, u64), Box<dyn std::error::Error>> {
    let (config_path_arg, start_index) =
        parse_config_path_and_rest(args, config_arg_index, default_config_path);
    let (config_path, config) = NodeConfig::load_json_with_runtime_paths(config_path_arg)?;
    let epoch_id = resolve_epoch_id_arg(args, start_index + epoch_arg_index, &config)?;
    Ok((config_path, config, epoch_id))
}

pub fn parse_config_path_and_rest<'a>(
    args: &'a [String],
    start_index: usize,
    default_config_path: &'a str,
) -> (&'a str, usize) {
    match args.get(start_index) {
        Some(value) if value.parse::<u64>().is_err() => (value.as_str(), start_index + 1),
        _ => (default_config_path, start_index),
    }
}

pub fn parse_config_path_and_rest_with_known_first_arg<'a>(
    args: &'a [String],
    start_index: usize,
    default_config_path: &'a str,
    is_first_arg: impl FnOnce(&str) -> bool,
) -> (&'a str, usize) {
    match args.get(start_index) {
        Some(value) if is_first_arg(value) => (default_config_path, start_index),
        Some(value) => (value.as_str(), start_index + 1),
        None => (default_config_path, start_index),
    }
}

pub fn looks_like_hex_32_arg(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub fn is_reward_config_subcommand(value: &str) -> bool {
    matches!(
        value,
        "mode" | "emission-year" | "tail-policy" | "service-split"
    )
}

pub fn latest_local_epoch(config: &NodeConfig) -> Result<u64, Box<dyn std::error::Error>> {
    let progress = LocalNodeProgress::load_or_default(progress_path(config), config)?;
    if progress.ticks_completed == 0 {
        return Err(
            "no local collection history exists yet; run `pole-client collect` first".into(),
        );
    }

    if progress.next_slot_id == 1 {
        Ok(progress.next_epoch_id.saturating_sub(1))
    } else {
        Ok(progress.next_epoch_id)
    }
}
