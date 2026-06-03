use std::path::Path;

use crate::genesis_builder::{Allocation, GenesisError, Result, ValidatorSpec};

/// Load a CSV file with two columns: `address,amount_upole`. Comments
/// (lines starting with `#`) and empty lines are skipped.
pub fn load_allocations_csv(path: &Path) -> Result<Vec<Allocation>> {
    let raw = std::fs::read_to_string(path)?;
    let mut out = Vec::new();
    for (i, line) in raw.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split(',');
        let address = parts.next().ok_or_else(|| GenesisError::Csv {
            line: i + 1,
            message: "missing address column".into(),
        })?;
        let amount_str = parts.next().ok_or_else(|| GenesisError::Csv {
            line: i + 1,
            message: "missing amount column".into(),
        })?;
        if parts.next().is_some() {
            return Err(GenesisError::Csv {
                line: i + 1,
                message: "too many columns (expected 2)".into(),
            });
        }
        let amount_upole: u128 = amount_str.trim().parse().map_err(|e| {
            GenesisError::Csv {
                line: i + 1,
                message: format!("invalid amount `{amount_str}`: {e}"),
            }
        })?;
        out.push(Allocation {
            address: address.trim().to_string(),
            amount_upole,
        });
    }
    Ok(out)
}

/// Load a JSON array of [`ValidatorSpec`].
pub fn load_validators_json(path: &Path) -> Result<Vec<ValidatorSpec>> {
    let raw = std::fs::read_to_string(path)?;
    let v: Vec<ValidatorSpec> = serde_json::from_str(&raw)?;
    Ok(v)
}
