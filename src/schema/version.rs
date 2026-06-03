//! Schema version type and helpers.
//!
//! Every persisted JSON file in PoLE embeds a top-level
//! `schema_version` field. The [`CURRENT`] constant is the version
//! that this build reads and writes. Older files are upgraded
//! through the [`super::migration`] registry before being handed
//! to the deserialiser.
//!
//! The on-disk envelope is:
//!
//! ```json
//! { "schema_version": 1, "data": { ... } }
//! ```

use std::fmt;

/// Current schema version for files written by this build.
pub const CURRENT: u32 = 1;

/// A schema version. `0` is reserved for the pre-versioning era
/// (raw type dump, no envelope).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SchemaVersion(pub u32);

impl SchemaVersion {
    pub const V0_RAW: Self = Self(0);
    pub const V1_ENVELOPE: Self = Self(1);

    pub const fn new(v: u32) -> Self {
        Self(v)
    }

    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

impl fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "v{}", self.0)
    }
}

impl From<u32> for SchemaVersion {
    fn from(v: u32) -> Self {
        Self(v)
    }
}

/// The on-disk envelope. Wrap the payload in this when persisting,
/// and unwrap it when loading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Versioned<T> {
    pub schema_version: SchemaVersion,
    pub data: T,
}

impl<T> Versioned<T> {
    pub fn new(data: T) -> Self {
        Self {
            schema_version: CURRENT.into(),
            data,
        }
    }

    pub fn with_version(mut self, v: SchemaVersion) -> Self {
        self.schema_version = v;
        self
    }
}

impl<T: serde::Serialize> serde::Serialize for Versioned<T> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("Versioned", 2)?;
        st.serialize_field("schema_version", &self.schema_version.as_u32())?;
        st.serialize_field("data", &self.data)?;
        st.end()
    }
}

impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for Versioned<T> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Raw<T> {
            #[serde(default = "default_version")]
            schema_version: u32,
            data: T,
        }
        let raw = Raw::deserialize(d)?;
        Ok(Versioned {
            schema_version: SchemaVersion(raw.schema_version),
            data: raw.data,
        })
    }
}

fn default_version() -> u32 {
    CURRENT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_is_human_readable() {
        assert_eq!(SchemaVersion::V0_RAW.to_string(), "v0");
        assert_eq!(SchemaVersion::V1_ENVELOPE.to_string(), "v1");
        assert_eq!(SchemaVersion::new(7).to_string(), "v7");
    }

    #[test]
    fn current_is_stable() {
        assert_eq!(CURRENT, 1);
    }

    #[test]
    fn round_trip_envelope() {
        let v: Versioned<u32> = Versioned::new(42);
        let s = serde_json::to_string(&v).unwrap();
        let back: Versioned<u32> = serde_json::from_str(&s).unwrap();
        assert_eq!(back.data, 42);
        assert_eq!(back.schema_version, SchemaVersion::V1_ENVELOPE);
    }
}
