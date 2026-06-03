//! Migration registry.
//!
//! Each persisted file type owns a [`MigrationRegistry`] that maps
//! `(from_version, to_version)` to a step function. Steps operate on
//! [`serde_json::Value`] so they can reshape data without knowing the
//! final type — the registry only knows that step `N → N+1` adds a
//! field, removes a field, renames a key, or coerces a value.
//!
//! Once the data has been migrated up to [`super::version::CURRENT`],
//! the caller deserialises it into the strongly-typed model.

use std::collections::BTreeMap;
use std::fmt;

use serde_json::Value;

use super::version::SchemaVersion;

/// Reason a migration step failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationError {
    /// No path of registered steps connects `from` to `to`.
    NoPath { from: SchemaVersion, to: SchemaVersion },
    /// A registered step returned an error.
    StepFailed { from: SchemaVersion, to: SchemaVersion, reason: String },
    /// The step registered for a transition is missing.
    MissingStep { from: SchemaVersion, to: SchemaVersion },
}

impl fmt::Display for MigrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoPath { from, to } =>
                write!(f, "no migration path from {from} to {to}"),
            Self::MissingStep { from, to } =>
                write!(f, "missing migration step {from} -> {to}"),
            Self::StepFailed { from, to, reason } =>
                write!(f, "migration {from} -> {to} failed: {reason}"),
        }
    }
}

impl std::error::Error for MigrationError {}

/// A single migration step. Steps are version-to-version (never
/// skip a generation) so the registry can chain them up to
/// [`super::version::CURRENT`].
pub type Step = fn(Value) -> Result<Value, String>;

/// Registry of migration steps. Identified by a string so the loader
/// can report which file type the registry belongs to.
#[derive(Debug, Clone)]
pub struct MigrationRegistry {
    name: &'static str,
    /// Keyed by the source version. Each entry maps the next
    /// version to the step that performs the transition.
    steps: BTreeMap<u32, BTreeMap<u32, Step>>,
}

impl MigrationRegistry {
    pub fn new(name: &'static str) -> Self {
        Self { name, steps: BTreeMap::new() }
    }

    /// Register a single step. Panics if a step for the same
    /// `(from, to)` is already registered — that's a programmer
    /// error, not a runtime condition.
    pub fn register(mut self, from: u32, to: u32, step: Step) -> Self {
        assert!(
            to == from + 1,
            "{}: step must be to the immediate next version (got {from} -> {to})",
            self.name,
        );
        self.steps
            .entry(from)
            .or_default()
            .insert(to, step);
        self
    }

    /// Migrate `value` from `from` to `to`, following the chain of
    /// registered steps. If `from == to`, the value is returned
    /// unchanged.
    pub fn migrate(
        &self,
        from: SchemaVersion,
        to: SchemaVersion,
        mut value: Value,
    ) -> Result<Value, MigrationError> {
        let mut current = from.as_u32();
        let target = to.as_u32();
        if current == target {
            return Ok(value);
        }
        if current > target {
            return Err(MigrationError::NoPath { from, to });
        }
        while current < target {
            let next = current + 1;
            let step = self
                .steps
                .get(&current)
                .and_then(|m| m.get(&next))
                .copied()
                .ok_or(MigrationError::MissingStep {
                    from: SchemaVersion(current),
                    to: SchemaVersion(next),
                })?;
            value = step(value).map_err(|reason| MigrationError::StepFailed {
                from: SchemaVersion(current),
                to: SchemaVersion(next),
                reason,
            })?;
            current = next;
        }
        Ok(value)
    }

    /// Highest version this registry knows a step for. Used by the
    /// loader to assert the file is at most one step behind.
    pub fn head(&self) -> u32 {
        self.steps
            .keys()
            .next_back()
            .and_then(|from| self.steps.get(from).and_then(|m| m.keys().max()))
            .copied()
            .unwrap_or(super::version::CURRENT)
    }

    pub fn name(&self) -> &'static str {
        self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Bump a field `value` by one — used as a trivial step in tests.
    fn bump_value(v: Value) -> Result<Value, String> {
        let mut obj = v.as_object().cloned().ok_or("not an object")?;
        let cur = obj.get("v").and_then(|x| x.as_u64()).unwrap_or(0);
        obj.insert("v".into(), json!(cur + 1));
        Ok(Value::Object(obj))
    }

    #[test]
    fn empty_registry_is_a_noop_when_from_equals_to() {
        let reg = MigrationRegistry::new("test");
        let v = json!({});
        assert!(reg.migrate(SchemaVersion::V0_RAW, SchemaVersion::V0_RAW, v.clone()).is_ok());
    }

    #[test]
    fn no_path_when_target_is_behind() {
        let reg = MigrationRegistry::new("test");
        let v = json!({});
        let err = reg
            .migrate(SchemaVersion::V1_ENVELOPE, SchemaVersion::V0_RAW, v)
            .unwrap_err();
        assert!(matches!(err, MigrationError::NoPath { .. }));
    }

    #[test]
    fn chains_two_steps() {
        let reg = MigrationRegistry::new("test")
            .register(0, 1, bump_value)
            .register(1, 2, bump_value);
        let v = json!({ "v": 0 });
        let out = reg
            .migrate(SchemaVersion(0), SchemaVersion(2), v)
            .unwrap();
        assert_eq!(out["v"], json!(2));
    }

    #[test]
    fn missing_step_is_reported() {
        let reg = MigrationRegistry::new("test");
        let v = json!({});
        let err = reg
            .migrate(SchemaVersion(0), SchemaVersion(2), v)
            .unwrap_err();
        assert!(matches!(err, MigrationError::MissingStep { .. }));
    }

    #[test]
    fn step_must_target_immediate_next() {
        let result = std::panic::catch_unwind(|| {
            MigrationRegistry::new("test").register(0, 2, bump_value)
        });
        assert!(result.is_err());
    }
}
