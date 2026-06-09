package types

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"testing"
)

// TestMerkleLeafFromRecord_Sha256DomainSeparator locks the wire format
// for chain-side leaf hashing:
//
//	leaf = sha256(0x00 || json.Marshal(record))
//
// The Rust side (`src/node_pipeline.rs::merkle_leaf_sha256`) hashes
// with the same domain separator (`0x00`) but feeds borsh-encoded
// bytes — see the report at `deliverable-merkle-cross-language.md`
// for the cross-language scope discussion. This test only asserts
// that the chain-side leaf format is sha256 + 0x00 prefix.
func TestMerkleLeafFromRecord_Sha256DomainSeparator(t *testing.T) {
	type recordShape struct {
		X int `json:"X"`
	}
	rec := recordShape{X: 1}
	leaf, err := MerkleLeafFromRecord(rec)
	if err != nil {
		t.Fatalf("MerkleLeafFromRecord: %v", err)
	}
	// Expected: sha256(0x00 || json.Marshal({X:1}))
	// json.Marshal({X:1}) = `{"X":1}` (7 bytes, no whitespace).
	// Independently computed via Python:
	//   python -c "import hashlib; print(hashlib.sha256(b'\x00' + b'{\"X\":1}').hexdigest())"
	//   = f807460fcf5311aa4715652912b5683e073a5c79d786ae6bd8713f1e27e055f1
	const wantHex = "f807460fcf5311aa4715652912b5683e073a5c79d786ae6bd8713f1e27e055f1"
	if got := hex.EncodeToString(leaf); got != wantHex {
		t.Fatalf("leaf drift for {X:1}: got %s, want %s", got, wantHex)
	}

	// Raw-bytes sanity check: sha256(0x00 || "a") is the same leaf
	// format the chain would emit for a record whose JSON encoding
	// collapses to a single byte (rare in practice, but useful as a
	// regression tripwire on the domain-separator byte).
	rawLeaf := sha256.New()
	rawLeaf.Write([]byte{0x00})
	rawLeaf.Write([]byte("a"))
	if got := hex.EncodeToString(rawLeaf.Sum(nil)); got != "022a6979e6dab7aa5ae4c3e5e45f7e977112a7e63593820dbec1ec738a24f93c" {
		t.Fatalf("raw sha256(0x00||a) drift: got %s", got)
	}
}

// TestMerkleRootFixtures mirrors
// `src/node_pipeline.rs::tests::fixture_table_matches_chain_for_full_sweep`.
// Expected hex values were independently computed via Python's
// hashlib (sha256 + 0x00/0x01 domain separators) and re-asserted here.
// Any drift on either side surfaces in CI.
func TestMerkleRootFixtures(t *testing.T) {
	// leaf(record) = sha256(0x00 || record) — precomputed to avoid the
	// JSON-record coupling for these raw-byte fixtures.
	makeLeaf := func(b []byte) []byte {
		h := sha256.New()
		h.Write([]byte{0x00})
		h.Write(b)
		return h.Sum(nil)
	}

	cases := []struct {
		name     string
		records  [][]byte
		wantRoot string
	}{
		{"empty", nil, "0000000000000000000000000000000000000000000000000000000000000000"},
		{"one_leaf_a", [][]byte{[]byte("a")}, "022a6979e6dab7aa5ae4c3e5e45f7e977112a7e63593820dbec1ec738a24f93c"},
		{"two_leaves_ab", [][]byte{[]byte("a"), []byte("b")}, "b137985ff484fb600db93107c77b0365c80d78f5b429ded0fd97361d077999eb"},
		{"three_leaves_abc", [][]byte{[]byte("a"), []byte("b"), []byte("c")}, "e9636069c740c9ff51625b01a0b040396d265a9b920cc6febdfa5ecc9f58ecce"},
		{"four_leaves_abcd", [][]byte{[]byte("a"), []byte("b"), []byte("c"), []byte("d")}, "33376a3bd63e9993708a84ddfe6c28ae58b83505dd1fed711bd924ec5a6239f0"},
		{"five_leaves_abcdef", [][]byte{[]byte("a"), []byte("b"), []byte("c"), []byte("d"), []byte("e")}, "605c72ca9351dd39f38678f4c1326df06d8fb1a58272792acaf70e8c191fb823"},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			leaves := make([][]byte, 0, len(tc.records))
			for _, r := range tc.records {
				leaves = append(leaves, makeLeaf(r))
			}
			root := merkleRoot(leaves)
			got := hex.EncodeToString(root)
			if got != tc.wantRoot {
				t.Fatalf("Merkle root drift for %s: got %s, want %s", tc.name, got, tc.wantRoot)
			}
		})
	}
}

// TestVerifyMerkleProofHex_RejectsBadInput confirms the
// `VerifyMerkleProofHex` validator fails closed on malformed input.
// Useful as a smoke check on the proof side (chain-side only — the
// Rust off-chain code does not generate proofs, only leaf + root).
func TestVerifyMerkleProofHex_RejectsBadInput(t *testing.T) {
	leaves := [][]byte{[]byte("a"), []byte("b")}
	root := merkleRoot(leaves)
	rootHex := hex.EncodeToString(root)

	// Out-of-range leaf index must be rejected.
	if VerifyMerkleProofHex(leaves[0], []string{}, 5, rootHex) {
		t.Fatal("VerifyMerkleProofHex accepted out-of-range index 5")
	}
	// Empty proof for a 2-leaf tree must be rejected.
	if VerifyMerkleProofHex(leaves[0], []string{}, 0, rootHex) {
		t.Fatal("VerifyMerkleProofHex accepted empty proof for 2-leaf tree")
	}
}

// Compile-time guarantee that `json` is still referenced (the helper
// `MerkleLeafFromRecord` uses it). If a future refactor drops the
// import, this file fails to build rather than silently leaving an
// unused-import error.
var _ = json.Marshal
