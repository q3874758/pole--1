# Changelog

All notable changes to PoLE V1 are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
once a stable version is published.

## [Unreleased]

### Changed — Mutual-proof rewards and uncapped on-demand issuance

This batch re-centres the protocol on the whitepaper's mutual-proof model:
a node proves its own play, other nodes prove it independently, and the
chain decides validity before any reward weight is counted.

- `src/mutual_proof.rs` (new) — `PlaySession` / `PlayHeartbeat` /
  `WitnessAttestation` construction, deterministic `session_id_from_parts`
  (`b"pole-play-session-v1"` domain prefix), wire projections and file
  persistence. The five CLI commands `play-session`, `play-heartbeat`,
  `attest-session`, `settle-session` and `submit-reward-records` landed in
  `src/cli_node.rs`.
- `chain/x/pole/keeper/session.go` (new) — `validateWitnessIndependence`
  (a witness may not be the player, may not have collected the session, must
  hold the `collect` capability, must have its own observation for the
  epoch, may not reuse the session payload, and must stay within
  `min_witness_observation_tolerance_ppm`), `heartbeatCoverageBPS`,
  `SettlePlaySession`, `ValidSessionWeightUnitsForEpoch` and
  `WitnessCreditsForEpoch`.
- `chain/x/pole/types/witness_reward.go` (new) — `AllocateWitnessRewards`
  splits the verify pool by adopted-attestation credit, exhausts the pool
  exactly via largest-remainder with an address-lexicographic tie-break
  (consensus-critical determinism), and returns zeroes for degenerate
  inputs.
- **D1 — the player is the node.** No separate `player_address` is
  introduced; `RewardRecord.recipient` remains the node identity.
- **D2 — no issuance ceiling.** The yearly reference curve keeps minting,
  and `ClaimReward` tops the pool up by the exact shortfall when confirmed
  rewards exceed it. There is no monthly quota and no leftover-budget
  truncation (`MintedThisMonth` is observational only). Long-run
  constraints are the ±cap square-root feedback, the monotonically decaying
  curve, and activity-linked burns.
- **D3 — witness rewards reuse `verify_reward_bps`** (1500 bps = 15% of the
  service split) shared by adopted attestations, which works out to roughly
  1.5% of issuance against ~80% for players; this is a flagged,
  governance-tunable parameter-sensitivity point.
- Five burn channels, all implemented in `chain/x/pole/keeper/burn.go` and
  bounded by module balance: `fee_burn_bps` (new `chain/app/ante.go` fee
  decorator — an outer wrapper is required because the SDK's
  `ante.NewAnteHandler` decorator slice is hardcoded), `reward_burn_bps`,
  `governance_burn_bps`, `challenge_bond_burn_bps` and `session_slash_bps`.
- **`chain_bridge` removed.** `submit-batch` / `submit-epoch` / `export-tx`
  now build real proto3 `Any` messages via `src/cosmos/pole_msgs.rs` and
  broadcast with `CosmosClient::submit` instead of base64-wrapping
  `serde_json`; `src/chain_bridge.rs` and its `lib.rs` export are gone.
  The broadcast signer is derived from the identity pubkey
  (`cosmos_account_from_pubkey` → bech32), matching the `GetSigners()`
  field of each message (`NodeId` is a hash and is not an account).
- Cosmetic sign-doc fix: `src/cosmos/proto.rs` now serializes a real
  `SignDoc` proto message (SDK v0.54.0 `direct.GetSignBytes`) instead of
  concatenating and hashing fields, which was producing `code 4`
  signature-verification failures. `CosmosClient::submit` also re-reads the
  account and re-signs on sequence races (`code 19/32`) instead of
  rebroadcasting identical bytes.
- Tests: `chain/app/session_test.go` (12 cases),
  `chain/app/burn_test.go`, `chain/app/supply_simulation_test.go` (20/30-year
  supply and inflation simulations), `chain/x/pole/types/witness_reward_test.go`;
  new Rust coverage for peer batch ingestion in `src/node_daemon.rs`.

### Changed — Unified `pole` executable (maintenance)

- Merged the `pole-genesis` and `pole-sbom` command logic into the
  unified `pole.exe` dispatcher (in addition to the earlier
  `pole-client` / `pole-node` merge): all command logic now lives in
  shared library modules (`cli_client` / `cli_node` / `cli_genesis` /
  `cli_sbom`), and `pole [client|node|genesis|sbom] <cmd>` dispatches
  in-process. The standalone `pole-genesis` / `pole-sbom` binaries
  remain as thin shims for compatibility.
- Fixed `src/lib.rs` not exporting `cli_sbom` (broke compilation of the
  unified dispatcher); fixed `clippy::drain_collect` in `src/p2p.rs`
  (`mem::take`); release packaging now includes the unified `pole`
  binary in the Windows portable zip and Linux DEB.

### Changed — CLI command-body unification (maintenance)

- Unified the duplicate `governance-show-*` / `reward-adjustment-show-*`
  command handlers between `pole-client` and `pole-node` into shared
  implementations in `src/cli_commands.rs`. Both binaries now use the
  same command body: optional leading `[config-path]` (default
  `./node.json`) plus a `PoLE <bin> <command>` header, so `pole-node`
  gains the header/optional-config behaviour already used by the client.
- `print_command_header` is now backed by a parameterized
  `print_command_header_for(bin, command, path)`; existing
  `pole-client` output is unchanged.
- Net reduction of ~210 duplicated lines across the two binaries;
  `cargo test`, `cargo clippy --all-targets --features integration`,
  and `cargo fmt` all stay green.
- Extended the same unification to `governance-vote` and
  `governance-show-proposal` (shared `resolve_config_and_header_with_known_first`
  matches a leading 64-hex proposal id before falling back to config-path).
  Combined, the shared command bodies cover 8 commands and remove ~230
  further duplicated lines.

### Removed — Codebase reduction (maintenance)

- Removed the production-dead `src/node_anomaly.rs` module
  (`detect_sample_anomalies` / `SampleAnomaly` / `SampleAnomalyKind`):
  it had zero references outside its own definitions and unit test —
  no pipeline, daemon, or CLI path ever called it. The whitepaper's
  signature-anomaly check is implemented separately in
  `node_verifier::verify_local_epoch`, so behaviour is unchanged.
- Removed the unused `with_client` constructors on `RestClient` and
  `TendermintRpc` (cosmos clients); both had no callers anywhere in
  `src/` or `tests/` — the active path always uses `new()`.
- Removed the unused `argon2` direct dependency (no reference anywhere
  in `src/`; the encrypted keystore uses `scrypt` for KDF, not Argon2)
  and the unused direct `prost` dependency (code only reaches protobuf
  types via `cosmos-sdk-proto`'s own re-export). `Cargo.lock` drops the
  `argon2` dep subtree; `cargo build`, `cargo test`, and
  `cargo clippy -D warnings` all stay green with behaviour unchanged.
- Corrected the stale `docs/wallet/SPEC.md` references from Argon2 to
  scrypt (the actual KDF), so docs match code and drop the removed dep.
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
- Removed earlier (commit `288874a`) the unused `src/observability/`
  and `src/schema/` modules (1,147 lines) — the now-stricken
  Observability / Schema / Config-validation items that used to be
  listed under `Added`. `src/observability/` + `src/config/` + the
  JSON schema are all gone; the surviving hardening items below
  (SBOM, crate metadata, release pipeline) remain live.

### Added — Production-Grade Hardening Pass

This batch adds a complete production-readiness layer without
changing protocol behaviour. Every change is backward compatible.
(NOTE: the Observability, Schema versioning + migration and Config
validation items that originally shipped here were later removed as
unused — see the `### Removed` section for the cleanup commits.)

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
    no proto regeneration) and `BeginBlockAnnualEmission`: activity = latest
    finalized epoch's `TotalNetworkWeightUnits`, time-proportional minting
    against the yearly reference curve into the module reward pool.
    (Superseded by the uncapped on-demand issuance entry above: the initial
    per-month quota cap was removed, `MintedThisMonth` is observational
    only, and a month-long clock jump is clamped to one month's worth of
    elapsed time.)
  - `PayoutClaimedReward` pays from the scheme-A pool and burns the excess
    above `RewardBurnThreshold` at `RewardBurnBps` (Net Supply = Emission −
    Burn); `bankKeeper` gained `BurnCoins`. Any confirmed-reward shortfall
    is now minted on demand (see the uncapped issuance entry above).
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
  (Both modules were later deleted in the `### Removed` cleanup above;
  this line is kept as a historical record of that pass.)
- Drift detector (`schema_and_rust_struct_do_not_drift`) caught a
  real `$ref` indirection issue during development; fixed in the
  same pass.
- Full suite: 433 tests collected, 432 passed, 1 ignored, 0 failures
  (`cargo test --all-targets`: lib 200 + integration binaries 233; the
  `--features integration` chain-backed scenarios are counted separately
  and require a built `poled` on `$PATH`).

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
