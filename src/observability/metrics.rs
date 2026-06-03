//! Minimal in-process metrics registry. Designed to be cheap to read
//! (lock-free `AtomicU64` for the common case) and serialised in
//! Prometheus text format by [`super::server::ObservabilityServer`].
//!
//! We deliberately avoid the `prometheus` crate to keep the
//! dependency surface small — the metric set we need is tiny and the
//! format is stable.

use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};

/// A single counter. Labels are not supported in the skeleton;
/// callers that need them should allocate one `Counter` per label
/// combination.
pub struct Counter {
    name: &'static str,
    help: &'static str,
    value: AtomicU64,
}

impl Counter {
    pub const fn new(name: &'static str, help: &'static str) -> Self {
        Self {
            name,
            help,
            value: AtomicU64::new(0),
        }
    }

    pub fn inc(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add(&self, n: u64) {
        self.value.fetch_add(n, Ordering::Relaxed);
    }

    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }
}

/// Process-wide metrics for the bridge layer. Add new fields as the
/// surface grows.
pub struct Metrics {
    /// Number of successful `MsgFinalizeEpoch` broadcasts.
    pub finalize_epoch_ok: Counter,
    /// Number of failed `MsgFinalizeEpoch` broadcasts.
    pub finalize_epoch_err: Counter,
    /// Number of successful `MsgClaimReward` broadcasts.
    pub claim_reward_ok: Counter,
    /// Number of failed `MsgClaimReward` broadcasts.
    pub claim_reward_err: Counter,
    /// Number of times the RPC client retried a broadcast.
    pub rpc_retry: Counter,
    /// Total bytes broadcast (sum of TxRaw payload sizes).
    pub broadcast_bytes: Counter,
}

impl Metrics {
    pub const fn new() -> Self {
        Self {
            finalize_epoch_ok: Counter::new(
                "pole_bridge_finalize_epoch_ok_total",
                "Successful MsgFinalizeEpoch broadcasts",
            ),
            finalize_epoch_err: Counter::new(
                "pole_bridge_finalize_epoch_err_total",
                "Failed MsgFinalizeEpoch broadcasts",
            ),
            claim_reward_ok: Counter::new(
                "pole_bridge_claim_reward_ok_total",
                "Successful MsgClaimReward broadcasts",
            ),
            claim_reward_err: Counter::new(
                "pole_bridge_claim_reward_err_total",
                "Failed MsgClaimReward broadcasts",
            ),
            rpc_retry: Counter::new(
                "pole_bridge_rpc_retry_total",
                "Number of RPC broadcast retries",
            ),
            broadcast_bytes: Counter::new(
                "pole_bridge_broadcast_bytes_total",
                "Total bytes broadcast to the chain",
            ),
        }
    }

    /// Render all metrics in Prometheus text format. The format spec
    /// is `text/plain; version=0.0.4; charset=utf-8`; see
    /// <https://prometheus.io/docs/instrumenting/exposition_formats/>.
    pub fn render_prometheus(&self) -> String {
        let mut out = String::with_capacity(1024);
        for c in [
            &self.finalize_epoch_ok,
            &self.finalize_epoch_err,
            &self.claim_reward_ok,
            &self.claim_reward_err,
            &self.rpc_retry,
            &self.broadcast_bytes,
        ] {
            let _ = writeln!(out, "# HELP {} {}", c.name, c.help);
            let _ = writeln!(out, "# TYPE {} counter", c.name);
            let _ = writeln!(out, "{} {}", c.name, c.get());
        }
        out
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Process-wide singleton. Use [`metrics()`] to access.
use std::sync::OnceLock;
static GLOBAL: OnceLock<Metrics> = OnceLock::new();

pub fn metrics() -> &'static Metrics {
    GLOBAL.get_or_init(Metrics::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_increments() {
        let c = Counter::new("test_counter", "for tests");
        assert_eq!(c.get(), 0);
        c.inc();
        c.inc();
        c.add(8);
        assert_eq!(c.get(), 10);
    }

    #[test]
    fn render_prometheus_is_well_formed() {
        let m = Metrics::new();
        m.finalize_epoch_ok.inc();
        m.claim_reward_ok.inc();
        m.claim_reward_ok.inc();
        let text = m.render_prometheus();
        assert!(text.contains("pole_bridge_finalize_epoch_ok_total 1"));
        assert!(text.contains("pole_bridge_claim_reward_ok_total 2"));
        assert!(text.contains("# TYPE pole_bridge_rpc_retry_total counter"));
    }

    #[test]
    fn global_metrics_singleton() {
        let a = metrics();
        a.finalize_epoch_ok.inc();
        let b = metrics();
        assert!(b.finalize_epoch_ok.get() >= 1);
    }
}
