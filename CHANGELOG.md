# Changelog

All notable changes to PoLE V1 are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
once a stable version is published.

## [Unreleased]

### Removed — Codebase reduction (maintenance)

- Removed the libp2p diagnostic skeleton (`src/p2p_libp2p.rs`) and its
  6 `libp2p-*` CLI commands; the active P2P runtime path is
  `src/p2p.rs` (socket / filesystem / in-memory). Dropped the
  `real-libp2p` feature and the `libp2p`, `libp2p-identity`,
  `multiaddr` dependencies.
- Removed the production-dead config validation module
  (`src/config/`), its JSON schema, and the `jsonschema` dependency;
  validation is covered by `NodeConfig::validate` /
  `ProtocolParams::validate`.
- Removed the now-unused `vendor/core2` patch and its integrity test.
- Removed committed AI-tool work artifacts (`.mavis`, `.omx`,
  `.harness`) and archived `deliverable-*.md` milestone reports.
- Deduplicated three near-identical `NodeConfig` test fixtures.
  Overall ~3,700 lines removed; `cargo test`, `cargo clippy -D
  warnings`, and `cargo fmt` all stay green.

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

### Release pipeline (ch.5)
- `src/update_manifest.rs` — `resolve_release_manifest_dir` resolves the
  manifest directory from `POLE_RELEASE_MANIFEST_DIR`, the installed
  layout's `release-manifests`, the in-tree dist (dev, no network), or a
  GitHub Releases pull (`latest/download/{channel}.json` + `.sig`/`.cert`
  sidecars cached under the update dir). `control_api.rs` update
  endpoints now use it instead of the compile-time source path; cosign
  verification is unchanged.
- `src/updater.rs` — Windows default install root is now per-user
  `%LOCALAPPDATA%\PoLE` (falls back to `C:\Program Files\PoLE`).
- `packaging/windows/{layout.json,install-service.cmd,pole-node-service.json}`
  — aligned to the per-user layout (`POLE_INSTALL_ROOT` override for
  LocalSystem installs).
- `.github/workflows/release.yml` — RELEASE_NOTES heredoc unquoted so
  `${VERSION}` expands; DEB package now ships `conffiles` and drops the
  leading `v` from the artifact name (`pole-node_0.1.0_amd64.deb`);
  `build-package.sh` copies `conffiles` too.
- `dist/release-manifests/stable.json` — removed the leftover
  `"signature": "dev-signature"` inline placeholder (signing is cosign
  keyless via sidecars only).
- `tests/control_api.rs` — update-flow tests seed a dev-signed
  `stable.json` into each test's own `release-manifests` dir so they
  exercise the real resolver instead of the repo manifest.

### Testing & CI
- `src/transitions.rs` — 36 unit tests covering all ten `apply_*`
  transitions plus boundary/error paths (signer binding, signatures,
  capabilities, stale epochs, duplicates, windows, bonds, balances,
  nonces, voting power, governance quorum scheduling, challenge
  response window/responder), and `process_mature_unbonds`. New
  `ProtocolParams::default()`.
- `tests/harness/mod.rs` — real `MsgCommitEpoch` / `MsgFinalizeEpoch` /
  `MsgOpenChallenge` / `MsgUpsertAggregateRecord` helpers replace the
  `Unsupported`/`Unimplemented` stubs; `finalize_epoch` polls and retries
  until the chain accepts.
- `tests/integration.rs` — new real-chain scenarios (serially, one
  `poled` per test): full epoch lifecycle
  (register → submit → commit → aggregate → finalize) and challenge
  opening against a committed epoch.
- `.github/workflows/ci.yml` — new `chain` job: setup-go 1.26,
  `go vet` + `go test ./...`, builds `poled`, then runs
  `cargo test --features integration`.

### Scheme A — activity-linked annual emission
- `src/tokenomics.rs` — `annual_emission(year, target, current, cap)` and
  `annual_emission_activity_factor`: nominal annual issuance scaled by
  `sqrt(target/current)` clamped to ±cap (default 10%,
  `ANNUAL_EMISSION_ADJUSTMENT_CAP_BPS`). `integer_sqrt` is now shared
  (`tokenomics::integer_sqrt`, reused by `node_rewards`). Cross-language
  fixtures lock Rust and chain values to the same rows.
- `chain/x/pole` — scheme A on-chain execution:
  - `types/emission.go` — `AnnualEmissionRateBps` / `AnnualEmissionAmount`
    (mirror of the Rust curve) and `AnnualAdjustedEmission`, which calls
    `AdjustedHourlyReward` (its first real call site).
  - `keeper/emission.go` — `annualEmissionState` (collections.Item[[]byte],
    no proto regeneration) and `BeginBlockAnnualEmission`: the yearly
    budget is split into 12 monthly quotas (30-day periods, 360-day
    protocol year), activity = latest finalized epoch's
    `TotalNetworkWeightUnits`, time-proportional minting of the monthly
    quota into the module reward pool with a hard per-month cap.
  - `PayoutClaimedReward` now pays from the scheme-A pool (rewards no
    longer mint on demand) and burns the excess above
    `RewardBurnThreshold` at `RewardBurnBps` (Net Supply = Emission −
    Burn); `bankKeeper` gained `BurnCoins`.
  - `module.go BeginBlock` wired to the annual mint.
- `docs_PoLE_Whitepaper.md` §4.4.5 — activity-linked issuance formula,
  anchor (`TargetNetworkWeightUnits`) and 10% cap, on-chain execution
  semantics.

### Security
- `src/wallet/keystore.rs` + `src/node_config.rs` — node identity
  (`identity.json`) is now stored as an AES-256-GCM + scrypt encrypted
  keystore instead of a plaintext private key. Password comes from the
  `POLE_IDENTITY_PASSWORD` environment variable or an interactive prompt
  during `pole-client init` / `repair-identity` (empty passwords are
  rejected). Legacy plaintext identity files remain readable for a smooth
  upgrade; the plaintext buffer and password strings are zeroized after use.
- `src/wallet/keys.rs` — `KeyPair` now implements `Drop` and zeroizes its
  secret on drop.
- `src/node_verifier.rs` — collector-signature audit is now a hard gate of
  `all_valid` for own batches: every observation in a locally collected
  batch must carry a valid Ed25519 signature (empty / dev / invalid /
  unverifiable signatures fail the epoch). Non-own batches (no collector
  key available) keep reporting-only semantics. `BatchVerificationReport`
  gains `own_batch` / `signatures_audit_valid` (serde defaults keep old
  reports readable); `node_daemon` verification credentials use the same
  bar.
- `src/node_pipeline.rs` — the 32-byte dev-placeholder signature shortcut
  is now compiled only in debug builds; release builds treat any
  non-64-byte signature as invalid, closing the bypass.

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
