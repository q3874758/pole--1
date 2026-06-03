//! Observability primitives: structured logging, Prometheus metrics,
//! and health-check endpoints.
//!
//! The bridge layer and node daemon should use [`tracing`] directly:
//!     use tracing::{info, warn, error};
//!     info!(tx_hash = %hash, "broadcast succeeded");
//!
//! For metrics, register your counters in [`metrics::Metrics`] (a thin
//! in-process registry) and expose them via the [`server`] module.

pub mod metrics;
pub mod server;

pub use server::{HealthState, ObservabilityServer};

/// Initialise the global tracing subscriber. Safe to call multiple
/// times — the second call is a no-op. Honours `RUST_LOG` for the
/// env filter, falling back to `info` for `pole` crates and `warn`
/// for everything else.
pub fn init_tracing() {
    use std::sync::Once;
    static START: Once = Once::new();
    START.call_once(|| {
        let filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,libp2p=warn"));
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(false)
            .try_init();
    });
}

/// Initialise the global tracing subscriber in JSON mode. Used by
/// the daemon and the GUI when stdout is being captured by a log
/// shipper rather than read by a human.
pub fn init_tracing_json() {
    use std::sync::Once;
    static START: Once = Once::new();
    START.call_once(|| {
        let filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,libp2p=warn"));
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(false)
            .json()
            .try_init();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_is_idempotent() {
        // Multiple calls shouldn't panic.
        init_tracing();
        init_tracing();
        init_tracing_json();
    }
}
