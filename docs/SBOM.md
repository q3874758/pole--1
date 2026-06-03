# SBOM and cosign verification

Every release of PoLE ships with two Software Bills of Materials
(SBOMs) and a cosign signature for each.

## What is in the release?

| File | Format | Tool |
|---|---|---|
| `sbom.cyclonedx.json` | CycloneDX 1.5 | `pole-sbom --format cyclonedx` |
| `sbom.spdx.json` | SPDX 2.3 | `pole-sbom --format spdx` |
| `sbom.cyclonedx.json.sig` | cosign signature | `cosign sign-blob` |
| `sbom.cyclonedx.json.pem` | signing certificate (x509) | `cosign sign-blob` |
| `sbom.spdx.json.sig` | cosign signature | `cosign sign-blob` |
| `sbom.spdx.json.pem` | signing certificate (x509) | `cosign sign-blob` |

The signatures are produced **keylessly** by GitHub Actions
OIDC during the `release-github` job. The signing identity is
the workflow's OIDC token, scoped to this repository.

## Verifying a signature

Requirements: `cosign` ≥ 2.0. Install from
<https://docs.sigstore.dev/cosign/system_config/installation/>.

```bash
# 1. Download the SBOM and its signature from the release.
#    (gh release download v0.1.0 -p 'sbom.*' is the easiest path.)

# 2. Verify the CycloneDX SBOM.
cosign verify-blob \
  --signature sbom.cyclonedx.json.sig \
  --certificate sbom.cyclonedx.json.pem \
  --certificate-identity-regexp 'https://github.com/q3874758/pole--1' \
  --certificate-oidc-issuer 'https://token.actions.githubusercontent.com' \
  sbom.cyclonedx.json

# 3. Same for SPDX.
cosign verify-blob \
  --signature sbom.spdx.json.sig \
  --certificate sbom.spdx.json.pem \
  --certificate-identity-regexp 'https://github.com/q3874758/pole--1' \
  --certificate-oidc-issuer 'https://token.actions.githubusercontent.com' \
  sbom.spdx.json
```

A successful verification looks like:

```
Verified OK
```

## Why two formats?

- **CycloneDX 1.5** — the de-facto standard for security tooling
  (Grype, Dependency-Track, Anchore). Use this for
  vulnerability scanning and continuous compliance.
- **SPDX 2.3** — the Linux Foundation standard; required by many
  procurement and supply-chain compliance frameworks (NTIA
  minimum elements, OpenSSF SLSA, EU Cyber Resilience Act
  drafts).

If you only need one, CycloneDX is the better choice for
vulnerability scanning; SPDX is the better choice for legal /
license audits.

## Generating a local SBOM

You don't have to wait for a release. From the repo root:

```bash
cargo build --release --bin pole-sbom
./target/release/pole-sbom --format cyclonedx --out sbom.cdx.json
./target/release/pole-sbom --format spdx     --out sbom.spdx.json

# License audit
./target/release/pole-sbom \
  --deny-licenses GPL-3.0-only,AGPL-3.0-only,SSPL-1.0
```

## What's denied by default?

The CI license gate fails the build if any transitive
dependency has one of:

- `GPL-1.0-only`, `GPL-2.0-only`, `GPL-3.0-only`
- `AGPL-1.0-only`, `AGPL-3.0-only`
- `SSPL-1.0`

A wider allow-list and the corresponding denials live in
`deny.toml`. The same list is enforced by `cargo-deny` in CI.

## What's warned but not denied?

- `MPL-2.0` (file-level copyleft; usually fine for vendored
  code)
- `BSL-1.0` (Boost Software License; permissive but with
  naming clause)

## Re-vendoring the SBOM

If you fork the project and want your fork's CI to also
produce signed SBOMs:

1. Add `id-token: write` to the workflow's `permissions:`.
2. Update the `certificate-identity-regexp` to your repo URL.
3. Push a tag; the release job emits and signs automatically.

The signing key never leaves Sigstore's transparency log —
there is nothing to back up.
