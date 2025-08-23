package hashutil

import (
	"crypto/sha256"
	"fmt"
	"io"
	"os"
)

// CalculateFileChecksum calculates the SHA256 checksum of a file
func CalculateFileChecksum(path string) (string, error) {
	file, err := os.Open(path)
	if err != nil {
		return "", err
	}
	defer func() {
		_ = file.Close()
	}()

	hash := sha256.New()
	if _, err := io.Copy(hash, file); err != nil {
		return "", err
	}

	return fmt.Sprintf("sha256:%x", hash.Sum(nil)), nil
}
