//! HTTP endpoints for metrics scraping and health checks.
//!
//! Endpoints (all under one TCP port, configurable):
//!   - `GET /healthz`  — liveness: returns 200 if the process is up
//!   - `GET /readyz`   — readiness: returns 200 if the chain RPC is reachable
//!   - `GET /metrics`  — Prometheus text format
//!
//! The skeleton uses the blocking `std::net::TcpListener` directly to
//! avoid pulling in a heavyweight HTTP framework. A production
//! deployment can swap the body of [`ObservabilityServer::serve`] for
//! a real `axum` or `hyper` server without changing the API.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::observability::metrics::Metrics;

/// State surfaced by `/readyz`. The bridge layer can flip `chain_ready`
/// based on the result of the most recent `latest_height()` call.
#[derive(Debug, Clone)]
pub struct HealthState {
    pub chain_ready: Arc<AtomicBool>,
    pub last_chain_height: Arc<AtomicU64>,
    pub last_check_unix_millis: Arc<AtomicU64>,
}

impl HealthState {
    pub fn new() -> Self {
        Self {
            chain_ready: Arc::new(AtomicBool::new(false)),
            last_chain_height: Arc::new(AtomicU64::new(0)),
            last_check_unix_millis: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn mark_chain_ready(&self, height: u64) {
        self.chain_ready.store(true, Ordering::Relaxed);
        self.last_chain_height.store(height, Ordering::Relaxed);
        self.last_check_unix_millis
            .store(unix_millis_now(), Ordering::Relaxed);
    }

    pub fn mark_chain_unreachable(&self) {
        self.chain_ready.store(false, Ordering::Relaxed);
    }

    pub fn is_ready(&self) -> bool {
        self.chain_ready.load(Ordering::Relaxed)
    }
}

impl Default for HealthState {
    fn default() -> Self {
        Self::new()
    }
}

/// Tiny HTTP server. Spawn one per process; bind to `127.0.0.1:0` for
/// tests to let the OS pick a free port.
pub struct ObservabilityServer {
    listener: TcpListener,
    metrics: &'static Metrics,
    health: HealthState,
}

impl ObservabilityServer {
    pub fn bind(
        addr: &str,
        metrics: &'static Metrics,
        health: HealthState,
    ) -> std::io::Result<Self> {
        let listener = TcpListener::bind(addr)?;
        Ok(Self {
            listener,
            metrics,
            health,
        })
    }

    /// Local address (useful for tests that bind to port 0).
    pub fn local_addr(&self) -> std::io::Result<std::net::SocketAddr> {
        self.listener.local_addr()
    }

    /// Accept a single request and return the response. Tests use
    /// this to drive the server without spawning a thread.
    pub fn handle_one(&self, _timeout: Duration) -> std::io::Result<HttpResponse> {
        let (mut stream, _) = self.listener.accept()?;
        let mut buf = [0u8; 4096];
        let n = stream.read(&mut buf)?;
        let req = std::str::from_utf8(&buf[..n]).unwrap_or("");
        let response = route(req, self.metrics, &self.health);
        stream.write_all(&response.to_bytes())?;
        stream.flush()?;
        Ok(response)
    }

    /// Run the server in a loop on the current thread. Returns when
    /// the listener is closed (e.g. by `drop`).
    pub fn serve(self) -> std::io::Result<()> {
        for stream in self.listener.incoming() {
            match stream {
                Ok(mut s) => {
                    let mut buf = [0u8; 4096];
                    if let Ok(n) = s.read(&mut buf) {
                        let req = std::str::from_utf8(&buf[..n]).unwrap_or("");
                        let response = route(req, self.metrics, &self.health);
                        let _ = s.write_all(&response.to_bytes());
                        let _ = s.flush();
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "observability accept failed");
                }
            }
        }
        Ok(())
    }
}

impl Drop for ObservabilityServer {
    fn drop(&mut self) {
        // Closing the listener unblocks `incoming()`.
        let _ = self.listener.set_nonblocking(true);
    }
}

/// Minimal HTTP response representation.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub status_text: &'static str,
    pub content_type: &'static str,
    pub body: String,
}

impl HttpResponse {
    fn ok_json(body: String) -> Self {
        Self {
            status: 200,
            status_text: "OK",
            content_type: "application/json",
            body,
        }
    }

    fn ok_text(content_type: &'static str, body: String) -> Self {
        Self {
            status: 200,
            status_text: "OK",
            content_type,
            body,
        }
    }

    fn err(status: u16, status_text: &'static str, body: String) -> Self {
        Self {
            status,
            status_text,
            content_type: "text/plain",
            body,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            self.status,
            self.status_text,
            self.content_type,
            self.body.len(),
            self.body,
        )
        .into_bytes()
    }
}

fn route(req: &str, metrics: &Metrics, health: &HealthState) -> HttpResponse {
    // Parse the request line: "METHOD PATH HTTP/1.1"
    let first_line = req.lines().next().unwrap_or("");
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("");

    if method != "GET" {
        return HttpResponse::err(405, "Method Not Allowed", "method not allowed".into());
    }

    match path {
        "/healthz" => HttpResponse::ok_text("text/plain", "ok".into()),
        "/readyz" => {
            if health.is_ready() {
                let view = health_view(health);
                HttpResponse::ok_json(serde_json::to_string(&view).unwrap_or_default())
            } else {
                HttpResponse::err(503, "Service Unavailable", "chain not ready".into())
            }
        }
        "/metrics" => HttpResponse::ok_text(
            "text/plain; version=0.0.4; charset=utf-8",
            metrics.render_prometheus(),
        ),
        _ => HttpResponse::err(404, "Not Found", "no such endpoint".into()),
    }
}

fn health_view(h: &HealthState) -> serde_json::Value {
    serde_json::json!({
        "chain_ready": h.is_ready(),
        "last_chain_height": h.last_chain_height.load(Ordering::Relaxed),
        "last_check_unix_millis": h.last_check_unix_millis.load(Ordering::Relaxed),
    })
}

fn unix_millis_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpStream;
    use std::thread;
    use std::time::Duration;

    fn start_test_server() -> (std::net::SocketAddr, HealthState) {
        let server = ObservabilityServer::bind(
            "127.0.0.1:0",
            crate::observability::metrics::metrics(),
            HealthState::new(),
        )
        .expect("bind");
        let addr = server.local_addr().unwrap();
        // Run the server in a background thread that handles a few
        // requests and then exits.
        thread::spawn(move || {
            for _ in 0..8 {
                let _ = server.handle_one(Duration::from_secs(1));
            }
        });
        (addr, HealthState::new())
    }

    fn http_get(addr: std::net::SocketAddr, path: &str) -> String {
        let mut s = TcpStream::connect(addr).unwrap();
        let req = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
        s.write_all(req.as_bytes()).unwrap();
        let mut buf = String::new();
        s.read_to_string(&mut buf).unwrap();
        buf
    }

    #[test]
    fn healthz_returns_200() {
        let (addr, _h) = start_test_server();
        let resp = http_get(addr, "/healthz");
        assert!(resp.starts_with("HTTP/1.1 200"));
    }

    #[test]
    fn readyz_returns_503_when_chain_unreachable() {
        let (addr, _h) = start_test_server();
        let resp = http_get(addr, "/readyz");
        assert!(resp.starts_with("HTTP/1.1 503"));
    }

    #[test]
    fn metrics_endpoint_exposes_prometheus_text() {
        let (addr, _h) = start_test_server();
        let resp = http_get(addr, "/metrics");
        assert!(resp.contains("pole_bridge_finalize_epoch_ok_total"));
        assert!(resp.contains("# TYPE pole_bridge_rpc_retry_total counter"));
    }

    #[test]
    fn unknown_path_returns_404() {
        let (addr, _h) = start_test_server();
        let resp = http_get(addr, "/wat");
        assert!(resp.starts_with("HTTP/1.1 404"));
    }

    #[test]
    fn post_returns_405() {
        let (addr, _h) = start_test_server();
        let mut s = TcpStream::connect(addr).unwrap();
        s.write_all(
            b"POST /healthz HTTP/1.1\r\nHost: x\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        )
        .unwrap();
        let mut buf = String::new();
        s.read_to_string(&mut buf).unwrap();
        assert!(buf.starts_with("HTTP/1.1 405"));
    }

    #[test]
    fn health_state_marks_ready() {
        let h = HealthState::new();
        assert!(!h.is_ready());
        h.mark_chain_ready(12345);
        assert!(h.is_ready());
        assert_eq!(h.last_chain_height.load(Ordering::Relaxed), 12345);
        h.mark_chain_unreachable();
        assert!(!h.is_ready());
    }
}
