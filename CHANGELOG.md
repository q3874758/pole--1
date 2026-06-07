# Changelog

All notable changes to PoLE V1 are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
once a stable version is published.

## [Unreleased]

### Added — Production-Grade Hardening Pass

This batch adds a complete production-readiness layer without
changing protocol behaviour. Every change is backward compatible.

#### Observability
- `src/observability/metrics.rs` — in-process Prometheus registry
  with 6 counters (`finalize_epoch_ok/err`, `claim_reward_ok/err`,
  `rpc_retry`, `broadcast_bytes`). Lock-free `AtomicU64`, no
  external dep.
- `src/observability/server.rs` — blocking HTTP server on a single
  TCP port exposing `GET /healthz` (liveness), `GET /readyz`
  (chain-RPC reachability), and `GET /metrics` (Prometheus text
  format 0.0.4).
- `src/observability/mod.rs` — `init_tracing()` (pretty) and
  `init_tracing_json()` (machine-readable) with `RUST_LOG` support
  and idempotent guards.

#### Schema versioning + migration
- `src/schema/version.rs` — `Versioned<T>` envelope
  (`{schema_version, data}`), `SchemaVersion` newtype, `CURRENT`
  version constant.
- `src/schema/migration.rs` — `MigrationRegistry` with chained
  step functions, missing-path / step-failed error reporting,
  step-to-immediate-next-version guard.
- `src/schema/loader.rs` — `load_with_migrations` /
  `save_versioned` file I/O with version auto-detection, "too new"
  rejection, and a permissive default for legacy v0 raw payloads.
- `src/schema/registries.rs` — concrete registries for
  `LocalRetentionBook`, `NodeConfig`, and `LocalChainRuntimeState`.
  Adding a new file type is three lines.

#### Config validation
- `config/node_config.schema.json` — Draft 2020-12 schema covering
  every field of `NodeConfig` with patterns, ranges, and
  `additionalProperties: false` on every object.
- `src/config/validator.rs` — two-layer validation: schema check
  via the embedded schema, plus semantic invariants (BPS sum ==
  10000, target_app_ids non-empty, hex length cross-checks).
- `src/config/validator.rs::schema_and_rust_struct_do_not_drift` —
  drift detector that walks both the schema and a serialised
  `NodeConfig::default()` and asserts the key sets match for the
  top level plus `runtime`, `storage`, and `reward` (with
  `$ref` resolution). Adding a field to Rust but not the schema
  fails the test, and vice versa.

#### SBOM + license compliance
- `src/bin/pole-sbom.rs` — `pole-sbom` binary emitting
  **CycloneDX 1.5** (default) or **SPDX 2.3** JSON for the
  resolved workspace dependency tree, plus a license audit
  (`--deny-licenses`, `--warn-licenses`) that exits 2 on denial.
- `deny.toml` — `cargo-deny` configuration: explicit allow list,
  hard denials for GPL / AGPL / SSPL / Commons-Clause /
  Elastic-2.0, and `clarify` blocks for `ring` / `webpki` /
  `core2` (whose license expressions are non-trivial).
- `.github/workflows/ci.yml` — extended with two new jobs:
  - `license`: builds `pole-sbom`, fails the build on
    GPL-2.0/3.0, AGPL, or SSPL dependencies; warns on MPL/BSL.
  - `sbom`: emits CycloneDX + SPDX, uploads both as build
    artifacts (30-day retention).

#### Crate metadata
- `Cargo.toml` — added `rust-version`, `license = "MIT OR
  Apache-2.0"`, `authors`, `homepage`, `repository`, `readme`,
  `keywords`, `categories`, and an `exclude` block for build
  artifacts and runtime data.
- `LICENSE-MIT` and `LICENSE-APACHE` — dual-license texts at the
  repo root.

### Fixed
- `src/observability/server.rs` — replaced a broken
  `UnixMillis::default_or_now()` reference with a direct
  `SystemTime::now()` helper; removed conflicting `Default`
  impl; fixed `serde_json::to_string` borrow on the readiness
  view; replaced unstable `TcpListener::set_read_timeout` with a
  test driver that uses a per-request accept loop.
- `tests/harness/mod.rs` — updated `BridgeMessage` callsites to
  the current enum shape (the harness used pre-refactor
  `UpsertNode` and `SubmitReplicaReceipt` variants that no
  longer exist). The `ClaimReward` call now also passes
  `claimer`.

### Tests
- 14 new unit tests across `schema` (10) and `config` (4) modules.
- Drift detector (`schema_and_rust_struct_do_not_drift`) caught a
  real `$ref` indirection issue during development; fixed in the
  same pass.
- Full suite: 327 tests, 0 failures.

### Notes
- `core2` is the only dependency without a declared license
  expression. It is a vendored path dep declared in
  `[patch.crates-io]`; the `deny.toml` `clarify` block
  documents this. Upstream license: MIT (tiernano).
- `pole` itself now declares `MIT OR Apache-2.0` in
  `Cargo.toml`; the warning from the previous run is therefore
  resolved.

### Added — Phase 0.3: EIP-712 typed-data signing helper

- `src/cosmos/eip712.rs` — spec-compliant EIP-712 primitives
  (`DomainSeparator`, `hash_struct`, `typed_data_hash`,
  `encode_uint256`/`encode_string`/`encode_bytes32`/`encode_address`).
  Wraps `sha3::Keccak256` (pre-NIST variant — the EIP-712 spec
  uses the original Keccak padding, not the 2015 SHA3-256
  padding). The `eip712_sign` helper is curve-agnostic: it
  accepts any closure that signs the 32-byte digest, so the
  chain can stay on Ed25519 today and swap in secp256k1
  without touching the helper.
- `src/cosmos/mod.rs` — re-exports `keccak256`, `DomainSeparator`,
  `hash_struct`, `typed_data_hash`, `eip712_sign`.
- `chain/x/pole/types/eip712.go` — Go mirror of the Rust helper
  using `golang.org/x/crypto/sha3.NewLegacyKeccak256`. The two
  sides are pinned together by the shared EIP-712 spec test
  vector (Mail to CEO): Rust and Go produce byte-identical
  digests for the same input.
- `chain/x/pole/types/eip712_test.go` — 9 tests covering the
  Mail to CEO reference vector, salt-presence domain separator
  distinction, encoding helpers, and the `EIP712Sign` glue
  function.
- `chain/docs/adr/0003-eip712-keccak-variant.md` — ADR for the
  Keccak-256 vs SHA3-256 decision (pre-NIST Keccak is required
  by EIP-712; using the SHA3-256 constructor would silently
  produce digests the chain would reject).
