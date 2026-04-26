package types

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
)

func MerkleLeafFromRecord[T any](record T) ([]byte, error) {
	bz, err := json.Marshal(record)
	if err != nil {
		return nil, err
	}
	hash := sha256.Sum256(append([]byte{0}, bz...))
	return hash[:], nil
}

func MerkleRootHexForRecords[T any](records []T) (string, uint32, error) {
	if len(records) == 0 {
		return hex.EncodeToString(make([]byte, 32)), 0, nil
	}
	leaves := make([][]byte, 0, len(records))
	for _, record := range records {
		leaf, err := MerkleLeafFromRecord(record)
		if err != nil {
			return "", 0, err
		}
		leaves = append(leaves, leaf)
	}
	root := merkleRoot(leaves)
	return hex.EncodeToString(root), uint32(len(records)), nil
}

func MerkleProofHexForRecords[T any](records []T, index int) ([]string, error) {
	if index < 0 || index >= len(records) {
		return nil, fmt.Errorf("merkle proof index out of range")
	}
	leaves := make([][]byte, 0, len(records))
	for _, record := range records {
		leaf, err := MerkleLeafFromRecord(record)
		if err != nil {
			return nil, err
		}
		leaves = append(leaves, leaf)
	}
	proof := merkleProof(leaves, index)
	result := make([]string, 0, len(proof))
	for _, step := range proof {
		result = append(result, hex.EncodeToString(step))
	}
	return result, nil
}

func VerifyMerkleProofHex(leaf []byte, proofHex []string, index int, expectedRootHex string) bool {
	proof := make([][]byte, 0, len(proofHex))
	for _, hexStep := range proofHex {
		step, err := hex.DecodeString(hexStep)
		if err != nil {
			return false
		}
		proof = append(proof, step)
	}
	computed := verifyMerkleProof(leaf, proof, index)
	return hex.EncodeToString(computed) == expectedRootHex
}

func merkleRoot(leaves [][]byte) []byte {
	if len(leaves) == 0 {
		return make([]byte, 32)
	}
	level := append([][]byte(nil), leaves...)
	for len(level) > 1 {
		next := make([][]byte, 0, (len(level)+1)/2)
		for i := 0; i < len(level); i += 2 {
			left := level[i]
			right := left
			if i+1 < len(level) {
				right = level[i+1]
			}
			next = append(next, merkleParent(left, right))
		}
		level = next
	}
	return level[0]
}

func merkleProof(leaves [][]byte, index int) [][]byte {
	proof := [][]byte{}
	level := append([][]byte(nil), leaves...)
	idx := index
	for len(level) > 1 {
		sibling := idx ^ 1
		if sibling >= len(level) {
			sibling = idx
		}
		proof = append(proof, level[sibling])
		next := make([][]byte, 0, (len(level)+1)/2)
		for i := 0; i < len(level); i += 2 {
			left := level[i]
			right := left
			if i+1 < len(level) {
				right = level[i+1]
			}
			next = append(next, merkleParent(left, right))
		}
		level = next
		idx /= 2
	}
	return proof
}

func verifyMerkleProof(leaf []byte, proof [][]byte, index int) []byte {
	current := append([]byte(nil), leaf...)
	idx := index
	for _, sibling := range proof {
		if idx%2 == 0 {
			current = merkleParent(current, sibling)
		} else {
			current = merkleParent(sibling, current)
		}
		idx /= 2
	}
	return current
}

func merkleParent(left, right []byte) []byte {
	payload := append([]byte{1}, append(append([]byte{}, left...), right...)...)
	hash := sha256.Sum256(payload)
	return hash[:]
}
