package testutil

import (
	"crypto/sha256"
	"fmt"
)

// GetTestChecksum calculates a SHA256 checksum for test content
// This is used in tests to generate predictable checksums
func GetTestChecksum(content string) string {
	hash := sha256.Sum256([]byte(content))
	return fmt.Sprintf("%x", hash)
}
