//! Schema versioning + migration framework for PoLE's local ledger.
//!
//! Every persisted JSON file (storage book, settlement state, config,
//! artifacts) goes through this module. Files are wrapped in a
//! [`version::Versioned`] envelope:
//!
//! ```json
//! { "schema_version": 1, "data": { ... } }
//! ```
//!
//! When a file is loaded:
//!  1. The top-level `schema_version` is read.
//!  2. If it is below [`version::CURRENT`], the registered
//!     [`migration::MigrationRegistry`] for that file type runs a
//!     chain of [`migration::Step`] functions to bring it up.
//!  3. The result is deserialised into the strongly-typed model.
//!
//! When a file is written, it is always written at
//! [`version::CURRENT`].

pub mod loader;
pub mod migration;
pub mod registries;
pub mod version;

pub use loader::{load_versioned, load_with_migrations, save_versioned, LoadError, SaveError};
pub use migration::{MigrationError, MigrationRegistry, Step};
pub use registries::{local_chain_runtime_registry, node_config_registry, storage_book_registry};
pub use version::{SchemaVersion, Versioned, CURRENT};
