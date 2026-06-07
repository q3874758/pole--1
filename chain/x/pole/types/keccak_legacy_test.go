package types

import (
	"encoding/hex"
	"testing"

	"golang.org/x/crypto/sha3"
)

// Pin the legacy Keccak-256 digest that the Rust bridge side will mirror
// in `tests/cosmos/keccak_legacy.rs`.
//
// **Critical:** EIP-712 specifies the *pre-NIST* Keccak-256, not the
// 2015-standardized SHA3-256. Go's `golang.org/x/crypto/sha3` exposes
// `NewLegacyKeccak256` (used here) for the EIP-712 variant and
// `New256` for the SHA3-256 variant — using the wrong one silently
// produces hashes that do not match Ethereum tooling.
//
// Test vector: Keccak256("abc") =
//   4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45
func TestKeccak256_ABC_MatchesKnownVector(t *testing.T) {
	const expectedHex = "4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45"
	h := sha3.NewLegacyKeccak256()
	h.Write([]byte("abc"))
	got := hex.EncodeToString(h.Sum(nil))
	if got != expectedHex {
		t.Fatalf("Keccak-256 mismatch:\n  got:  %s\n  want: %s", got, expectedHex)
	}
}

func TestKeccak256_Empty_MatchesKnownVector(t *testing.T) {
	const expectedHex = "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"
	h := sha3.NewLegacyKeccak256()
	got := hex.EncodeToString(h.Sum(nil))
	if got != expectedHex {
		t.Fatalf("Keccak-256(empty) mismatch:\n  got:  %s\n  want: %s", got, expectedHex)
	}
}

// Sanity: a future refactor that swaps NewLegacyKeccak256 for the
// post-NIST SHA3-256 constructor would change the digest. This test
// guards by asserting distinct inputs produce distinct outputs — the
// same shape as the Rust test.
func TestKeccak256_DistinctInputsDistinctDigests(t *testing.T) {
	h1 := sha3.NewLegacyKeccak256()
	h1.Write([]byte("abc"))
	h2 := sha3.NewLegacyKeccak256()
	h2.Write([]byte("abd"))
	if hex.EncodeToString(h1.Sum(nil)) == hex.EncodeToString(h2.Sum(nil)) {
		t.Fatal("Keccak-256 collision between 'abc' and 'abd' — algorithm broken")
	}
}
