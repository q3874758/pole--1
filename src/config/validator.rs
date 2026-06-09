//! Strong validation of [`crate::node_config::NodeConfig`] against
//! a JSON Schema (Draft 2020-12).
//!
//! The schema lives at `config/node_config.schema.json` and is
//! embedded into the binary at compile time via
//! [`include_str!`]. Validating happens in two layers:
//!
//!  1. **Schema check** — ensures the file is structurally valid
//!     (required fields, types, ranges, patterns).
//!  2. **Semantic check** — [`validate_semantic`] performs the
//!     invariants that JSON Schema cannot express (e.g. hex byte
//!     length cross-checks against the address type, reward BPS
//!     sums to 10000).
//!
//! Both layers should be called when loading a config from disk.

use std::fmt;

use serde::Serialize;

use crate::node_config::NodeConfig;

const SCHEMA_TEXT: &str = include_str!("../../config/node_config.schema.json");

/// Errors produced by [`validate_config`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConfigValidationError {
    /// Dotted JSON path, e.g. `runtime.poll_interval_secs`.
    pub path: String,
    /// Human-readable reason.
    pub message: String,
}

impl fmt::Display for ConfigValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path, self.message)
    }
}

impl std::error::Error for ConfigValidationError {}

#[derive(Debug)]
pub enum ConfigValidatorError {
    Schema(String),
    Semantic(Vec<ConfigValidationError>),
    Json(serde_json::Error),
}

impl fmt::Display for ConfigValidatorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Schema(s) => write!(f, "schema error: {s}"),
            Self::Semantic(errs) => {
                write!(f, "{} semantic error(s):", errs.len())?;
                for e in errs {
                    write!(f, "\n  - {e}")?;
                }
                Ok(())
            }
            Self::Json(e) => write!(f, "json: {e}"),
        }
    }
}

impl std::error::Error for ConfigValidatorError {}

impl From<serde_json::Error> for ConfigValidatorError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

/// Validate a serialised config value against the embedded JSON
/// Schema. Returns the list of schema violations.
pub fn validate_schema(value: &serde_json::Value) -> Result<(), ConfigValidatorError> {
    let schema_value: serde_json::Value = serde_json::from_str(SCHEMA_TEXT)?;
    let validator = jsonschema::validator_for(&schema_value)
        .map_err(|e| ConfigValidatorError::Schema(e.to_string()))?;
    let mut errors: Vec<ConfigValidationError> = validator
        .iter_errors(value)
        .map(|e| ConfigValidationError {
            path: e.instance_path().to_string(),
            message: e.to_string(),
        })
        .collect();
    errors.sort_by(|a, b| a.path.cmp(&b.path).then(a.message.cmp(&b.message)));
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ConfigValidatorError::Semantic(errors))
    }
}

/// Semantic invariants that JSON Schema cannot easily express.
pub fn validate_semantic(cfg: &NodeConfig) -> Result<(), ConfigValidatorError> {
    let mut errors = Vec::new();
    if cfg.node_id_hex.len() != 64 {
        errors.push(ConfigValidationError {
            path: "node_id_hex".into(),
            message: format!("expected 64 hex chars, got {}", cfg.node_id_hex.len()),
        });
    }
    if cfg.reward_address_hex.len() != 40 {
        errors.push(ConfigValidationError {
            path: "reward_address_hex".into(),
            message: format!(
                "expected 40 hex chars, got {}",
                cfg.reward_address_hex.len()
            ),
        });
    }
    if cfg.runtime.target_app_ids.is_empty() {
        errors.push(ConfigValidationError {
            path: "runtime.target_app_ids".into(),
            message: "must contain at least one app id".into(),
        });
    }
    if cfg.runtime.poll_interval_secs == 0 {
        errors.push(ConfigValidationError {
            path: "runtime.poll_interval_secs".into(),
            message: "must be > 0".into(),
        });
    }
    if cfg.runtime.slots_per_epoch == 0 {
        errors.push(ConfigValidationError {
            path: "runtime.slots_per_epoch".into(),
            message: "must be > 0".into(),
        });
    }
    if cfg.storage.quota_gb == 0 {
        errors.push(ConfigValidationError {
            path: "storage.quota_gb".into(),
            message: "must be > 0".into(),
        });
    }
    if cfg.storage.retention_epochs == 0 {
        errors.push(ConfigValidationError {
            path: "storage.retention_epochs".into(),
            message: "must be > 0".into(),
        });
    }
    // Reward BPS must sum to 10000. JSON Schema enforces the
    // 0..=10000 range; this catches the "all zero" or
    // "over-allocated" misconfigurations.
    let bps_sum: u32 = cfg.reward.collect_reward_bps as u32
        + cfg.reward.store_reward_bps as u32
        + cfg.reward.verify_reward_bps as u32
        + cfg.reward.propose_reward_bps as u32;
    if bps_sum != 10_000 {
        errors.push(ConfigValidationError {
            path: "reward.{collect,store,verify,propose}_reward_bps".into(),
            message: format!("reward BPS sum is {bps_sum}, expected 10000"),
        });
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ConfigValidatorError::Semantic(errors))
    }
}

/// Run both layers of validation against the deserialised config.
pub fn validate_config(cfg: &NodeConfig) -> Result<(), ConfigValidatorError> {
    let value = serde_json::to_value(cfg)?;
    validate_schema(&value)?;
    validate_semantic(cfg)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_config::{
        CapabilityConfig, CollectConfig, NodeConfig, RewardConfig, RuntimeConfig, StorageConfig,
    };

    fn minimal_config() -> NodeConfig {
        NodeConfig {
            chain_id: "pole-test-1".into(),
            node_id_hex: "11".repeat(32),
            reward_address_hex: "22".repeat(20),
            capabilities: CapabilityConfig {
                collect: true,
                store: true,
                verify: true,
                propose: false,
                archive: false,
            },
            collect: CollectConfig {
                enabled: true,
                default_epoch_id: 0,
                default_slot_id: 0,
            },
            runtime: RuntimeConfig {
                data_dir: "/var/lib/pole".into(),
                poll_interval_secs: 60,
                slots_per_epoch: 60,
                challenge_window_blocks: 100,
                low_impact_mode: false,
                os_background_priority: true,
                game_active_poll_interval_secs: 5,
                game_process_names: vec![],
                target_app_ids: vec![730u32],
                p2p_simulation: Default::default(),
                p2p_socket: Default::default(),
                p2p_libp2p: Default::default(),
                activity_sources: vec![],
            },
            storage: StorageConfig {
                quota_gb: 10,
                retention_epochs: 24,
            },
            reward: RewardConfig::default(),
        }
    }

    #[test]
    fn minimal_config_validates() {
        let cfg = minimal_config();
        match validate_config(&cfg) {
            Ok(()) => {}
            Err(e) => panic!("config should validate, got: {e}"),
        }
    }

    #[test]
    fn empty_target_app_ids_is_rejected_semantically() {
        let mut cfg = minimal_config();
        cfg.runtime.target_app_ids.clear();
        let err = validate_config(&cfg).unwrap_err();
        let s = err.to_string();
        assert!(s.contains("target_app_ids"), "got: {s}");
    }

    #[test]
    fn reward_bps_must_sum_to_10k() {
        let mut cfg = minimal_config();
        cfg.reward.collect_reward_bps = 2500;
        cfg.reward.store_reward_bps = 2500;
        cfg.reward.verify_reward_bps = 2500;
        cfg.reward.propose_reward_bps = 2500; // sum = 10000, ok
        assert!(validate_config(&cfg).is_ok());
        cfg.reward.propose_reward_bps = 0; // sum = 7500
        let err = validate_config(&cfg).unwrap_err();
        let s = err.to_string();
        assert!(s.contains("BPS sum"), "got: {s}");
    }

    #[test]
    fn schema_rejects_missing_required_field() {
        let cfg = minimal_config();
        let mut v = serde_json::to_value(&cfg).unwrap();
        v.as_object_mut().unwrap().remove("chain_id");
        let err = validate_schema(&v).unwrap_err();
        let s = err.to_string();
        assert!(s.contains("chain_id"), "got: {s}");
    }

    #[test]
    fn schema_rejects_out_of_range_poll_interval() {
        let cfg = minimal_config();
        let mut v = serde_json::to_value(&cfg).unwrap();
        v["runtime"]["poll_interval_secs"] = serde_json::json!(0);
        let err = validate_schema(&v).unwrap_err();
        let s = err.to_string();
        assert!(s.contains("poll_interval_secs"), "got: {s}");
    }

    #[test]
    fn schema_rejects_bad_node_id_hex_length() {
        let cfg = minimal_config();
        let mut v = serde_json::to_value(&cfg).unwrap();
        v["node_id_hex"] = serde_json::json!("abcd");
        let err = validate_schema(&v).unwrap_err();
        let s = err.to_string();
        assert!(s.contains("node_id_hex"), "got: {s}");
    }

    /// Resolve a JSON Schema fragment, following a single level of
    /// `$ref` indirection. Refs in this schema look like
    /// `"#/$defs/foo"` — we only support local refs.
    fn resolve_ref<'a>(
        schema: &'a serde_json::Value,
        root: &'a serde_json::Value,
    ) -> &'a serde_json::Value {
        if let Some(r) = schema.get("$ref").and_then(|v| v.as_str()) {
            if let Some(rest) = r.strip_prefix("#/") {
                let mut cur = root;
                for segment in rest.split('/') {
                    cur = cur.get(segment).unwrap_or(&serde_json::Value::Null);
                }
                return cur;
            }
        }
        schema
    }

    /// Collect property names from a JSON Schema fragment. Walks one
    /// level of `properties`, following a single `$ref`.
    fn collect_schema_properties<'a>(
        schema: &'a serde_json::Value,
        root: &'a serde_json::Value,
    ) -> std::collections::BTreeSet<String> {
        let resolved = resolve_ref(schema, root);
        let mut out = std::collections::BTreeSet::new();
        if let Some(props) = resolved.get("properties").and_then(|p| p.as_object()) {
            for k in props.keys() {
                out.insert(k.clone());
            }
        }
        out
    }

    fn collect_value_keys(value: &serde_json::Value) -> std::collections::BTreeSet<String> {
        let mut out = std::collections::BTreeSet::new();
        if let Some(obj) = value.as_object() {
            for k in obj.keys() {
                out.insert(k.clone());
            }
        }
        out
    }

    /// Drift detector: the top-level keys of the JSON schema's
    /// `properties` object must match the top-level keys of a
    /// serialised `NodeConfig` (and a serialised `RewardConfig`,
    /// and the `runtime` sub-object, and the `storage` sub-object).
    ///
    /// If you add a field to `NodeConfig` in Rust and forget to add
    /// it to `node_config.schema.json`, this test fails. Conversely
    /// if you add a property to the schema but no Rust struct
    /// change is wired up, this test fails. Either way you get
    /// told exactly which key drifted.
    #[test]
    fn schema_and_rust_struct_do_not_drift() {
        let schema: serde_json::Value = serde_json::from_str(SCHEMA_TEXT).expect("schema parses");
        let cfg = NodeConfig::default();
        let value = serde_json::to_value(&cfg).expect("config serialises");

        let top_schema = collect_schema_properties(&schema, &schema);
        let top_rust = collect_value_keys(&value);
        assert_eq!(
            top_schema, top_rust,
            "top-level NodeConfig keys drift: schema={top_schema:?}, rust={top_rust:?}"
        );

        // runtime sub-object
        let runtime_schema = schema
            .get("properties")
            .and_then(|p| p.get("runtime"))
            .cloned()
            .unwrap_or_default();
        let runtime_value = value.get("runtime").cloned().unwrap_or_default();
        let rs = collect_schema_properties(&runtime_schema, &schema);
        let rv = collect_value_keys(&runtime_value);
        assert_eq!(
            rs, rv,
            "runtime sub-object keys drift: schema={rs:?}, rust={rv:?}"
        );

        // storage sub-object
        let storage_schema = schema
            .get("properties")
            .and_then(|p| p.get("storage"))
            .cloned()
            .unwrap_or_default();
        let storage_value = value.get("storage").cloned().unwrap_or_default();
        let ss = collect_schema_properties(&storage_schema, &schema);
        let sv = collect_value_keys(&storage_value);
        assert_eq!(
            ss, sv,
            "storage sub-object keys drift: schema={ss:?}, rust={sv:?}"
        );

        // reward sub-object (defaulted via `#[serde(default)]`,
        // schema uses `$ref: #/$defs/reward`).
        let reward_schema = schema
            .get("properties")
            .and_then(|p| p.get("reward"))
            .cloned()
            .unwrap_or_default();
        let reward_value = value.get("reward").cloned().unwrap_or_default();
        let rws = collect_schema_properties(&reward_schema, &schema);
        let rwv = collect_value_keys(&reward_value);
        assert_eq!(
            rws, rwv,
            "reward sub-object keys drift: schema={rws:?}, rust={rwv:?}"
        );
    }

    /// Negative drift: if a Rust field is removed but the schema
    /// still references it, the schema rejects the missing key
    /// (we already have `schema_rejects_missing_required_field`).
    /// This test inverts it: mutate a config to drop a required
    /// field, schema must reject.
    #[test]
    fn drift_negative_required_field_drop_is_detected() {
        let cfg = minimal_config();
        let mut v = serde_json::to_value(&cfg).unwrap();
        v.as_object_mut().unwrap().remove("chain_id");
        let err = validate_schema(&v).unwrap_err();
        let s = err.to_string();
        assert!(
            s.contains("chain_id"),
            "missing chain_id should be flagged, got: {s}"
        );
    }

    /// Drift check for additionalProperties: the schema marks
    /// `additionalProperties: false` at the top level and on the
    /// nested objects. Verify the schema rejects an unknown top
    /// key, so adding one to the schema and not to Rust will be
    /// caught symmetrically.
    #[test]
    fn schema_rejects_unknown_top_level_key() {
        let cfg = minimal_config();
        let mut v = serde_json::to_value(&cfg).unwrap();
        v.as_object_mut()
            .unwrap()
            .insert("not_a_real_field".into(), serde_json::json!(true));
        let err = validate_schema(&v).unwrap_err();
        let s = err.to_string();
        assert!(
            s.contains("not_a_real_field") || s.contains("additional"),
            "got: {s}"
        );
    }
}
