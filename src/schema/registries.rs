//! Migration registries for specific PoLE file types.
//!
//! Each file type owns a [`MigrationRegistry`] built once and
//! exported. The loaders in the relevant modules call
//! [`super::loader::load_with_migrations`] with the right registry
//! to upgrade older files transparently.
//!
//! Adding a new migration step (e.g. when introducing a new
//! required field) is a three-step process:
//!  1. Bump [`super::version::CURRENT`] (or keep it and add a new
//!     `register(N, N+1, ...)` call).
//!  2. Register a step on the relevant `*_registry()`.
//!  3. Re-emit any on-disk files in the new shape on next save.

use super::migration::MigrationRegistry;
use serde_json::{json, Map, Value};

/// Registry for `LocalRetentionBook` files. The on-disk envelope
/// was introduced at v1; the v0 → v1 step promotes a bare book
/// object to the wrapped shape.
pub fn storage_book_registry() -> MigrationRegistry {
    MigrationRegistry::new("LocalRetentionBook").register(0, 1, v0_to_v1_envelope)
}

/// Registry for `NodeConfig` files. The runtime config lives at
/// `<data_dir>/config/node.json` and historically was written
/// without an envelope. The v0 → v1 step wraps it so future
/// migration steps (e.g. when a new required field is introduced)
/// have somewhere to anchor.
pub fn node_config_registry() -> MigrationRegistry {
    MigrationRegistry::new("NodeConfig").register(0, 1, v0_to_v1_envelope)
}

/// Registry for `LocalChainRuntimeState` files. The runtime state
/// (`<data_dir>/state/runtime.json`) was a bare `{height,
/// current_epoch}` object in v0; v1 wraps it in the standard
/// envelope.
pub fn local_chain_runtime_registry() -> MigrationRegistry {
    MigrationRegistry::new("LocalChainRuntimeState").register(0, 1, v0_to_v1_envelope)
}

/// Wrap a v0 raw payload in the v1 envelope: prepend
/// `schema_version: 1` and move the rest of the document under
/// `data`.
fn v0_to_v1_envelope(v: Value) -> Result<Value, String> {
    let data = v;
    let mut obj = Map::new();
    obj.insert("schema_version".into(), json!(1u32));
    obj.insert("data".into(), data);
    Ok(Value::Object(obj))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::loader::{load_with_migrations, save_versioned};
    use crate::schema::version::{SchemaVersion, CURRENT};
    use crate::storage_book::LocalRetentionBook;
    use serde::{Deserialize, Serialize};
    use std::fs;
    use tempfile::tempdir;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Tiny {
        quota_bytes: u64,
        used_bytes: u64,
    }

    /// Demonstrates the v0 raw → v1 envelope migration path for
    /// every registry we ship. A new `*_registry()` added below
    /// should also be added to this list.
    #[test]
    fn all_registries_upgrade_v0_raw_files() {
        let cases: &[(&str, fn() -> MigrationRegistry, &str)] = &[
            (
                "LocalRetentionBook",
                storage_book_registry,
                r#"{ "quota_bytes": 1024, "used_bytes": 0 }"#,
            ),
            (
                "NodeConfig",
                node_config_registry,
                r#"{ "chain_id": "pole-test-1", "data_dir": "/tmp" }"#,
            ),
            (
                "LocalChainRuntimeState",
                local_chain_runtime_registry,
                r#"{ "height": 0, "current_epoch": 0 }"#,
            ),
        ];

        for (label, make_reg, raw) in cases {
            let dir = tempdir().unwrap();
            let path = dir.path().join(format!("{label}.json"));
            fs::write(&path, raw).unwrap();
            let reg = make_reg();
            // Pre-conditions: file has no schema_version, the
            // registry knows how to get it to CURRENT.
            let value: Value = serde_json::from_str(raw).unwrap();
            assert_eq!(
                crate::schema::loader::read_schema_version(&value),
                SchemaVersion::V0_RAW,
                "{label}: expected V0_RAW"
            );
            assert_eq!(
                reg.head(),
                CURRENT,
                "{label}: registry head should be CURRENT"
            );
            // Post-conditions: after migration, the data is loadable
            // as a generic JSON object.
            let _: Value = load_with_migrations(&path, &reg).unwrap().unwrap();
        }
    }

    #[test]
    fn storage_book_registry_upgrades_v0_to_v1() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("book.json");
        let raw = r#"{ "quota_bytes": 1024, "used_bytes": 0 }"#;
        fs::write(&path, raw).unwrap();

        let reg = storage_book_registry();
        let loaded: Tiny = load_with_migrations(&path, &reg).unwrap().unwrap();
        assert_eq!(loaded.quota_bytes, 1024);
    }

    #[test]
    fn storage_book_round_trip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("book.json");
        let book = LocalRetentionBook::with_quota_gb(1);
        save_versioned(&book, &path).unwrap();
        let reg = storage_book_registry();
        let back: LocalRetentionBook = load_with_migrations(&path, &reg).unwrap().unwrap();
        assert_eq!(back.quota_bytes, book.quota_bytes);
    }
}
