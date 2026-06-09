//! File I/O with versioned envelopes.
//!
//! Use [`load_versioned`] / [`save_versioned`] for files that are
//! already wrapped in a [`Versioned`] envelope, and
//! [`load_with_migrations`] / [`save_versioned`] for files that need
//! version auto-detection plus a migration step-up.

use std::fs;
use std::io;
use std::path::Path;

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

use super::migration::{MigrationError, MigrationRegistry};
use super::version::{SchemaVersion, Versioned, CURRENT};

/// Errors that can occur when loading a versioned file.
#[derive(Debug)]
pub enum LoadError {
    Io(io::Error),
    Json(serde_json::Error),
    Migration(MigrationError),
    /// The file's `schema_version` is newer than this build supports.
    TooNew {
        found: u32,
        current: u32,
    },
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io: {e}"),
            Self::Json(e) => write!(f, "json: {e}"),
            Self::Migration(e) => write!(f, "migration: {e}"),
            Self::TooNew { found, current } => write!(
                f,
                "file schema_version={found} is newer than supported ({current})"
            ),
        }
    }
}

impl std::error::Error for LoadError {}

impl From<io::Error> for LoadError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for LoadError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl From<MigrationError> for LoadError {
    fn from(value: MigrationError) -> Self {
        Self::Migration(value)
    }
}

/// Read the top-level `schema_version` from a JSON object. Returns
/// `V0_RAW` if the key is missing (legacy files written before the
/// envelope was introduced).
pub fn read_schema_version(value: &Value) -> SchemaVersion {
    value
        .get("schema_version")
        .and_then(|v| v.as_u64())
        .map(|n| SchemaVersion(n as u32))
        .unwrap_or(SchemaVersion::V0_RAW)
}

/// Load a JSON file, run it through the migration registry up to
/// `CURRENT`, and deserialise the result into `T`. Missing files
/// return `Ok(None)` so callers can fall back to a default.
pub fn load_with_migrations<T, P>(
    path: P,
    registry: &MigrationRegistry,
) -> Result<Option<T>, LoadError>
where
    T: DeserializeOwned,
    P: AsRef<Path>,
{
    let path = path.as_ref();
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path)?;
    let value: Value = serde_json::from_str(&raw)?;
    let found = read_schema_version(&value);
    if found.as_u32() > CURRENT {
        return Err(LoadError::TooNew {
            found: found.as_u32(),
            current: CURRENT,
        });
    }
    let migrated = registry.migrate(found, SchemaVersion::new(CURRENT), value)?;
    // After migration, the data is wrapped in the V1 envelope shape:
    // { "schema_version": N, "data": <payload> }.
    let data = migrated.get("data").cloned().unwrap_or(Value::Null);
    let typed: T = serde_json::from_value(data)?;
    Ok(Some(typed))
}

/// Load a file that is *already* in the V1 envelope shape (no
/// migration needed).
pub fn load_versioned<T, P>(path: P) -> Result<Option<T>, LoadError>
where
    T: DeserializeOwned,
    P: AsRef<Path>,
{
    let path = path.as_ref();
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path)?;
    let v: Versioned<T> = serde_json::from_str(&raw)?;
    if v.schema_version.as_u32() > CURRENT {
        return Err(LoadError::TooNew {
            found: v.schema_version.as_u32(),
            current: CURRENT,
        });
    }
    Ok(Some(v.data))
}

/// Write `value` wrapped in the V1 envelope.
pub fn save_versioned<T, P>(value: &T, path: P) -> Result<(), SaveError>
where
    T: Serialize,
    P: AsRef<Path>,
{
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let envelope = Versioned::new(value);
    let text = serde_json::to_string_pretty(&envelope)?;
    fs::write(path, text)?;
    Ok(())
}

/// Errors that can occur when saving a versioned file.
#[derive(Debug)]
pub enum SaveError {
    Io(io::Error),
    Json(serde_json::Error),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io: {e}"),
            Self::Json(e) => write!(f, "json: {e}"),
        }
    }
}

impl std::error::Error for SaveError {}

impl From<io::Error> for SaveError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for SaveError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use tempfile::tempdir;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Sample {
        n: u32,
        label: String,
    }

    #[test]
    fn save_then_load_round_trip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("sample.json");
        let original = Sample {
            n: 7,
            label: "ok".into(),
        };
        save_versioned(&original, &path).unwrap();
        let loaded: Sample = load_versioned(&path).unwrap().unwrap();
        assert_eq!(loaded, original);
    }

    #[test]
    fn missing_file_returns_none() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("missing.json");
        let loaded: Option<Sample> = load_versioned(&path).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn rejects_newer_version() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("future.json");
        let raw = r#"{ "schema_version": 9999, "data": { "n": 1, "label": "x" } }"#;
        fs::write(&path, raw).unwrap();
        let err = load_versioned::<Sample, _>(&path).unwrap_err();
        assert!(matches!(
            err,
            LoadError::TooNew {
                found: 9999,
                current: 1
            }
        ));
    }

    #[test]
    fn migration_step_zero_to_one_promotes_payload() {
        // v0: raw payload at the top level. v1: payload nested under "data".
        fn wrap(v: Value) -> Result<Value, String> {
            let mut obj = serde_json::Map::new();
            obj.insert("schema_version".into(), Value::from(1u32));
            obj.insert("data".into(), v);
            Ok(Value::Object(obj))
        }
        let reg = MigrationRegistry::new("sample").register(0, 1, wrap);

        let dir = tempdir().unwrap();
        let path = dir.path().join("legacy.json");
        let raw = r#"{ "n": 4, "label": "old" }"#;
        fs::write(&path, raw).unwrap();

        let value: Value = serde_json::from_str(raw).unwrap();
        let found = read_schema_version(&value);
        assert_eq!(found, SchemaVersion::V0_RAW);

        let migrated = reg
            .migrate(found, SchemaVersion::new(CURRENT), value)
            .unwrap();
        let loaded: Sample = serde_json::from_value(migrated["data"].clone()).unwrap();
        assert_eq!(
            loaded,
            Sample {
                n: 4,
                label: "old".into()
            }
        );
    }
}
