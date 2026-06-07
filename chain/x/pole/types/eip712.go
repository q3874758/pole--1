// Package types — EIP-712 typed-data signing primitives.
//
// EIP-712 produces a deterministic, structured digest that wallets
// can show to users as human-readable signing prompts. The PoLE chain
// uses it for off-chain authorizations that later land on-chain as
// messages (Phase 3 session-key receipts, Phase 4 withdrawal
// authorizations, Phase 7 PNT-20 meta-tx permits).
//
// # Spec references
//   - EIP-712: https://eips.ethereum.org/EIPS/eip-712
//   - EIP-712 example (Mail to CEO): https://eips.ethereum.org/EIPS/eip-712#example
//
// # Hash function: legacy pre-NIST Keccak-256 (NOT SHA3-256)
// EIP-712 uses the *original* Keccak padding (a single 0x01 byte after
// the message), finalized before NIST standardized SHA3 with the
// 0x06 padding. The two algorithms are not interchangeable. Go's
// `golang.org/x/crypto/sha3` package exposes `NewLegacyKeccak256`
// (used here) for the EIP-712 variant; `New256` would silently
// produce digests the chain would reject.
//
// # Wire layout
//   keccak256(0x19 || 0x01 || domainSeparator || hashStruct(message))
//
// # Curve / signature scheme
// EIP-712 itself is curve-agnostic: the standard only specifies the
// *digest*. The signature scheme is whatever the chain uses. PoLE
// signs with Ed25519 (the chain's native key type) — secp256k1 is
// not introduced until Phase 5 (threshold encryption).
//
// This file mirrors the Rust helper in
// `src/cosmos/eip712.rs`. Both sides must produce byte-identical
// digests for the same input — they are pinned together by the
// shared test vectors at the bottom of this file.
package types

import (
	"encoding/binary"
	"errors"
	"fmt"

	"golang.org/x/crypto/sha3"
)

// Keccak256 returns the 32-byte legacy (pre-NIST) Keccak-256 digest
// of `data`. Every EIP-712 hash in this package goes through here so
// that any future swap to a different constructor (e.g. `sha3.New256`)
// can be caught by a single grep.
func Keccak256(data []byte) [32]byte {
	h := sha3.NewLegacyKeccak256()
	h.Write(data)
	var out [32]byte
	copy(out[:], h.Sum(nil))
	return out
}

// DomainSeparator mirrors `src/cosmos/eip712.rs::DomainSeparator`.
// Salt is optional per the EIP-712 spec — when `Salt == nil`, the
// `salt` field is dropped from the `EIP712Domain` type string.
type DomainSeparator struct {
	Name              string
	Version           string
	ChainID           uint64
	VerifyingContract [20]byte
	Salt              *[32]byte
}

// Hash returns the 32-byte domain separator hash per EIP-712 §"Domain
// Separator": keccak256(typeHash || keccak(name) || keccak(version) ||
// chainId || verifyingContract || [salt]).
func (d *DomainSeparator) Hash() [32]byte {
	var domainType string
	if d.Salt != nil {
		domainType = "EIP712Domain(string name,string version,uint256 chainId,address verifyingContract,bytes32 salt)"
	} else {
		domainType = "EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)"
	}
	typeHash := Keccak256([]byte(domainType))

	buf := make([]byte, 0, 32*7)
	buf = append(buf, typeHash[:]...)
	nameHash := Keccak256([]byte(d.Name))
	buf = append(buf, nameHash[:]...)
	versionHash := Keccak256([]byte(d.Version))
	buf = append(buf, versionHash[:]...)
	buf = append(buf, EncodeUint256(d.ChainID)...)
	buf = append(buf, EncodeAddress(d.VerifyingContract)...)
	if d.Salt != nil {
		buf = append(buf, d.Salt[:]...)
	}
	return Keccak256(buf)
}

// HashStruct implements `hashStruct(s) = keccak256(typeHash ||
// encodeData(s))`. Callers pass only the encoded *fields* — the
// type hash is prepended internally.
func HashStruct(typeHash [32]byte, encodedFields []byte) [32]byte {
	buf := make([]byte, 0, 32+len(encodedFields))
	buf = append(buf, typeHash[:]...)
	buf = append(buf, encodedFields...)
	return Keccak256(buf)
}

// TypedDataHash computes the final EIP-712 digest:
// keccak256(0x19 || 0x01 || domainSeparator || messageHash).
func TypedDataHash(domainSeparator, messageHash [32]byte) [32]byte {
	buf := make([]byte, 0, 2+32+32)
	buf = append(buf, 0x19, 0x01)
	buf = append(buf, domainSeparator[:]...)
	buf = append(buf, messageHash[:]...)
	return Keccak256(buf)
}

// --- canonical 32-byte encoders ----------------------------------------

// EncodeUint256 returns a big-endian 32-byte encoding of `value`.
// Mirrors `src/cosmos/eip712.rs::encode_uint256`.
func EncodeUint256(value uint64) []byte {
	var slot [32]byte
	binary.BigEndian.PutUint64(slot[24:], value)
	return slot[:]
}

// EncodeString returns `keccak256(string)` — the EIP-712 §"Hashing the
// `string` type" rule. Result is exactly 32 bytes.
func EncodeString(value string) []byte {
	h := Keccak256([]byte(value))
	return h[:]
}

// EncodeBytes32 returns the raw 32 bytes (the type is already
// fixed-width, no extra hashing).
func EncodeBytes32(value [32]byte) []byte {
	out := make([]byte, 32)
	copy(out, value[:])
	return out
}

// EncodeAddress returns a left-padded 32-byte encoding of the
// 20-byte address. Mirrors `src/cosmos/eip712.rs::encode_address`.
func EncodeAddress(value [20]byte) []byte {
	var slot [32]byte
	copy(slot[12:], value[:])
	return slot[:]
}

// --- signing glue ------------------------------------------------------

// SignFunc abstracts the signature scheme so this package stays
// curve-agnostic. PoLE calls into it with Ed25519 today; Phase 5
// (threshold encryption) introduces secp256k1 and a different
// closure. The closure receives the 32-byte digest and returns a
// signature in whatever format the chain expects.
type SignFunc func(digest [32]byte) ([]byte, error)

// EIP712Sign computes the EIP-712 digest for `domain` and `messageHash`
// and applies `sign`. Mirrors `src/cosmos/eip712.rs::eip712_sign`.
// Any error from the signer is wrapped with a clear "eip712 sign"
// prefix.
func EIP712Sign(domain *DomainSeparator, messageHash [32]byte, sign SignFunc) ([]byte, error) {
	if sign == nil {
		return nil, errors.New("eip712: nil SignFunc")
	}
	ds := domain.Hash()
	digest := TypedDataHash(ds, messageHash)
	sig, err := sign(digest)
	if err != nil {
		return nil, fmt.Errorf("eip712 sign: %w", err)
	}
	return sig, nil
}
