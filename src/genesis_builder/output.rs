use std::path::Path;

use crate::genesis_builder::Result;

pub fn write_genesis(path: &Path, doc: &serde_json::Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, serde_json::to_string_pretty(doc)?)?;
    Ok(())
}
