# ADR 0003: Use pre-NIST Keccak-256 (not SHA3-256) for EIP-712

- Status: Accepted
- Date: 2026-06-05
- Phase: 0.3
- Risk register entry: R3

## Context

EIP-712 specifies a typed-data signing format that produces a
deterministic 32-byte digest. The chain (Phase 3 session-key
receipts, Phase 4 withdrawal authorizations, Phase 7 PNT-20
meta-tx permits) will accept EIP-712 digests signed with the
operator's key, and operators will sign those digests with
off-chain tooling (Ethereum wallets, hardware devices,
infrastructure scripts).

The digest function is Keccak-256 — but Keccak-256 has **two
incompatible variants**:

1. **Pre-NIST Keccak-256** — the original algorithm finalized
   before NIST standardized SHA3. The padding rule is a single
   `0x01` byte after the message (followed by zero bytes and
   the message length). EIP-712 specifies this variant.
2. **NIST SHA3-256** — standardized in FIPS 202. The padding
   rule is a single `0x06` byte after the message. This is
   what "SHA3" colloquially means in most modern libraries.

The two algorithms produce **different digests for the same
input**. A single-byte difference in the padding rule cascades
through the full 24-round sponge. For example:

- Keccak-256(`""`) = `c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470`
- SHA3-256(`""`) = `a7ffc6f8bf1ed76651c14756a061d662f580ff4de43b49fa82d80a4b80f8434a`

If we picked the wrong constructor here, every typed-data
receipt the chain emits would carry a digest that **no other
EIP-712 implementation could verify** — silent interoperability
break.

The risk surfaces in two crates/libraries our code depends on:

| Language | Library | EIP-712 variant | SHA3 variant |
|----------|---------|-----------------|--------------|
| Rust | `sha3 = "0.10"` | `Keccak256` (used) | `Sha3_256` |
| Go | `golang.org/x/crypto/sha3` | `NewLegacyKeccak256` (used) | `New256` |

Both libraries deliberately expose both constructors side by
side — and the names are similar enough that a refactor or
copy-paste can swap them without anyone noticing until the
chain rejects signed receipts in production.

## Decision

Use **pre-NIST Keccak-256** for EIP-712 in both languages:

- Rust: `sha3::Keccak256::new()` / `sha3::Keccak256::digest(...)`
- Go: `sha3.NewLegacyKeccak256()`

Pin the choice in two places:

1. A single `keccak256(...)` / `Keccak256(...)` helper at the
   lib root (`src/cosmos/eip712.rs`, `chain/x/pole/types/eip712.go`)
   so a future grep finds the variant selection in one spot.
2. A round-trip test against the canonical EIP-712 example
   (Mail to CEO from <https://eips.ethereum.org/EIPS/eip-712#example>)
   that fails closed with a clear message if the algorithm
   ever changes. The test pins four reference values
   (domain type hash, domain separator, mail struct hash, final
   digest) so a regression localizes to one encoder.

The chosen names — `keccak256` / `Keccak256` — are spec-correct
("Keccak-256" is what EIP-712 names the algorithm) and contain
no "Sha3" / "NIST" tokens, so a refactor that reaches for
"the SHA3 constructor" will produce a name mismatch that
fails the build.

## Consequences

**Positive**

- EIP-712 digests are interoperable with every Ethereum
  wallet, hardware signer, and standard library that
  follows the spec.
- The lib-root helper makes the variant selection easy to
  audit (`grep -r 'sha3::\|New256\|NewLegacyKeccak256' src/ chain/`).
- Both languages have the same constraint; the cross-language
  round-trip test catches drift between the two sides.

**Negative**

- The `sha3` crate name is a slight misnomer (the constructor
  is actually Keccak). New contributors may reach for
  `Sha3_256` first. The doc comment at the top of
  `src/cosmos/eip712.rs` and `chain/x/pole/types/eip712.go`
  calls this out explicitly.
- The two test vectors in `tests/keccak_legacy.rs` and
  `chain/x/pole/types/keccak_legacy_test.go` are redundant
  with the EIP-712 tests but exist as belt-and-suspenders.
  The cost is three tests; the benefit is that the most
  common foot-gun (a constructor swap) is caught by the
  first suite a contributor runs.

## Verification

```bash
# Rust
cargo test -p pole --lib cosmos::eip712::tests::mail_to_ceo_typed_data_hash_matches_spec_vector
cargo test -p pole --lib tests::keccak_legacy

# Go
cd chain && go test ./x/pole/types/ -run 'MailToCEO|FullTypedDataHash|Keccak256'
```

If either side's digest of the EIP-712 Mail to CEO example
diverges from the published reference (`402e171069a6cd61bdee2ef100f90701ece65c39caf59ae30eaee87c74289604`),
this ADR is being violated and the fix is to revert the
constructor swap.

## References

- EIP-712: <https://eips.ethereum.org/EIPS/eip-712>
- Keccak padding distinction: <https://keccak.team/keccak.html>
- FIPS 202 (SHA3-256): <https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.202.pdf>
- Original Keccak-256 test vectors: <https://github.com/ethereum/EIPs/blob/master/assets/eip-712/Example.js>
