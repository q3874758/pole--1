// Tests for EIP-712 helper. Mirrors the Rust tests in
// `src/cosmos/eip712.rs` so the two sides can never drift.
package types

import (
	"encoding/hex"
	"testing"
)

// Mail to CEO test vector from EIP-712 spec:
// https://eips.ethereum.org/EIPS/eip-712#example
//
// Reference values (computed independently with PyCryptodome's
// legacy Keccak-256, cross-checked against the spec):
//
//	domainSeparator : f2cee375fa42b42143804025fc449deafd50cc031ca257e0b194a650a912090f
//	mail_type_hash  : a0cedeb2dc280ba39b857546d74f5549c3a1d7bdc2dd96bf881f76108e23dac2
//	mail_hash       : 1be177f7195c2412e97c6a288029793ca07b7713a7a82b7731dad07a3efe4e0d
//	final digest    : 402e171069a6cd61bdee2ef100f90701ece65c39caf59ae30eaee87c74289604
const (
	mailExpectedDomain    = "f2cee375fa42b42143804025fc449deafd50cc031ca257e0b194a650a912090f"
	mailExpectedMailType  = "a0cedeb2dc280ba39b857546d74f5549c3a1d7bdc2dd96bf881f76108e23dac2"
	mailExpectedMailHash  = "1be177f7195c2412e97c6a288029793ca07b7713a7a82b7731dad07a3efe4e0d"
	mailExpectedDigest    = "402e171069a6cd61bdee2ef100f90701ece65c39caf59ae30eaee87c74289604"
)

func mustHex(t *testing.T, s string) []byte {
	t.Helper()
	b, err := hex.DecodeString(s)
	if err != nil {
		t.Fatalf("invalid hex %q: %v", s, err)
	}
	return b
}

func hexOf(b []byte) string { return hex.EncodeToString(b) }

// TestKeccak256_ABC_MatchesKnownVector is a thin wrapper over the
// legacy-keccak pin test in keccak_legacy_test.go: the new helper
// must produce the same digest when called with a byte slice.
func TestKeccak256_Helper_ProducesSameAsLegacy(t *testing.T) {
	h := Keccak256([]byte("abc"))
	if got := hexOf(h[:]); got != "4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45" {
		t.Fatalf("Keccak256(\"abc\") = %s, want 4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45", got)
	}
}

func TestDomainSeparator_Deterministic(t *testing.T) {
	domain := &DomainSeparator{
		Name:              "PoLE",
		Version:           "1",
		ChainID:           0,
		VerifyingContract: [20]byte{},
	}
	h1 := domain.Hash()
	h2 := domain.Hash()
	if h1 != h2 {
		t.Fatalf("DomainSeparator.Hash() not deterministic: %x vs %x", h1, h2)
	}
}

func TestDomainSeparator_SaltPresenceChangesTypeString(t *testing.T) {
	empty := [32]byte{}
	nonzero := [32]byte{}
	nonzero[31] = 0x42
	with := &DomainSeparator{
		Name: "PoLE", Version: "1", ChainID: 0,
		VerifyingContract: [20]byte{}, Salt: &nonzero,
	}
	without := &DomainSeparator{
		Name: "PoLE", Version: "1", ChainID: 0,
		VerifyingContract: [20]byte{}, Salt: nil,
	}
	_ = empty
	if with.Hash() == without.Hash() {
		t.Fatal("salt presence must change the domain separator")
	}
}

func TestEncodeUint256_One_BePadded(t *testing.T) {
	got := EncodeUint256(1)
	want := mustHex(t, "0000000000000000000000000000000000000000000000000000000000000001")
	if hexOf(got) != hexOf(want) {
		t.Fatalf("EncodeUint256(1) = %s, want %s", hexOf(got), hexOf(want))
	}
}

func TestEncodeAddress_LeftPaddedTo32(t *testing.T) {
	addr, _ := hex.DecodeString("cccccccccccccccccccccccccccccccccccccccc")
	if len(addr) != 20 {
		t.Fatalf("test fixture must be 20 bytes, got %d", len(addr))
	}
	var a [20]byte
	copy(a[:], addr)
	got := EncodeAddress(a)
	want := mustHex(t, "000000000000000000000000cccccccccccccccccccccccccccccccccccccccc")
	if hexOf(got) != hexOf(want) {
		t.Fatalf("EncodeAddress mismatch: %s vs %s", hexOf(got), hexOf(want))
	}
}

func TestMailToCEO_DomainSeparator_MatchesReference(t *testing.T) {
	vc, _ := hex.DecodeString("cccccccccccccccccccccccccccccccccccccccc")
	var vcArr [20]byte
	copy(vcArr[:], vc)
	domain := &DomainSeparator{
		Name: "Ether Mail", Version: "1", ChainID: 1,
		VerifyingContract: vcArr, Salt: nil,
	}
	ds := domain.Hash()
	if got := hexOf(ds[:]); got != mailExpectedDomain {
		t.Fatalf("domainSeparator = %s, want %s", got, mailExpectedDomain)
	}
}

func TestMailToCEO_FullTypedDataHash_MatchesReference(t *testing.T) {
	vc, _ := hex.DecodeString("cccccccccccccccccccccccccccccccccccccccc")
	var vcArr [20]byte
	copy(vcArr[:], vc)
	domain := &DomainSeparator{
		Name: "Ether Mail", Version: "1", ChainID: 1,
		VerifyingContract: vcArr, Salt: nil,
	}

	mailTypeHash := Keccak256([]byte("Mail(Person from,Person to,string contents)Person(string name,address wallet)"))
	if got := hexOf(mailTypeHash[:]); got != mailExpectedMailType {
		t.Fatalf("mail type hash = %s, want %s", got, mailExpectedMailType)
	}

	personTypeHash := Keccak256([]byte("Person(string name,address wallet)"))

	fromWallet, _ := hex.DecodeString("cd2a3d9f938e13cd947ec05abc7fe734df8dd826")
	var fromArr [20]byte
	copy(fromArr[:], fromWallet)
	fromFields := append(EncodeString("Cow"), EncodeAddress(fromArr)...)
	fromPerson := HashStruct(personTypeHash, fromFields)

	toWallet, _ := hex.DecodeString("bbfa62c1571b141684e8cb46be5c5b48b0e89e5c")
	var toArr [20]byte
	copy(toArr[:], toWallet)
	toFields := append(EncodeString("Bob"), EncodeAddress(toArr)...)
	toPerson := HashStruct(personTypeHash, toFields)

	mailFields := append(append(fromPerson[:], toPerson[:]...), EncodeString("Hello, Bob!")...)
	mailHash := HashStruct(mailTypeHash, mailFields)
	if got := hexOf(mailHash[:]); got != mailExpectedMailHash {
		t.Fatalf("mail struct hash = %s, want %s", got, mailExpectedMailHash)
	}

	digest := TypedDataHash(domain.Hash(), mailHash)
	if got := hexOf(digest[:]); got != mailExpectedDigest {
		t.Fatalf("typed-data digest = %s, want %s", got, mailExpectedDigest)
	}
}

func TestEIP712Sign_DelegatesToSignFuncWithCorrectDigest(t *testing.T) {
	domain := &DomainSeparator{
		Name: "PoLE Test", Version: "1", ChainID: 7,
		VerifyingContract: [20]byte{}, Salt: nil,
	}
	var captured [32]byte
	sig, err := EIP712Sign(domain, [32]byte{0xAB}, func(d [32]byte) ([]byte, error) {
		captured = d
		return []byte{0xDE, 0xAD, 0xBE, 0xEF}, nil
	})
	if err != nil {
		t.Fatalf("EIP712Sign returned error: %v", err)
	}
	if hexOf(sig) != "deadbeef" {
		t.Fatalf("signature = %s, want deadbeef", hexOf(sig))
	}
	expected := TypedDataHash(domain.Hash(), [32]byte{0xAB})
	if captured != expected {
		t.Fatalf("signer received digest %x, want %x", captured, expected)
	}
}

func TestEIP712Sign_NilSignFuncRejected(t *testing.T) {
	domain := &DomainSeparator{Name: "PoLE", Version: "1"}
	if _, err := EIP712Sign(domain, [32]byte{}, nil); err == nil {
		t.Fatal("expected error for nil SignFunc, got nil")
	}
}
