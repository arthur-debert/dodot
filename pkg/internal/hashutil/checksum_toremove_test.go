package hashutil

import (
	"os"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestCalculateFileChecksum(t *testing.T) {
	// Create a temporary file with known content
	tempFile, err := os.CreateTemp("", "checksum-test-*")
	require.NoError(t, err)
	defer func() {
		_ = os.Remove(tempFile.Name())
	}()

	testContent := "Hello, World!\nThis is a test file.\n"
	_, err = tempFile.WriteString(testContent)
	require.NoError(t, err)
	require.NoError(t, tempFile.Close())

	// Calculate checksum
	checksum, err := CalculateFileChecksum(tempFile.Name())
	require.NoError(t, err)

	// Verify checksum format
	assert.Contains(t, checksum, "sha256:")
	assert.Len(t, checksum, 71) // "sha256:" + 64 hex chars

	// Calculate again to ensure consistency
	checksum2, err := CalculateFileChecksum(tempFile.Name())
	require.NoError(t, err)
	assert.Equal(t, checksum, checksum2)

	// Test with non-existent file
	_, err = CalculateFileChecksum("/non/existent/file")
	assert.Error(t, err)
}

func TestCalculateFileChecksumWithEmptyFile(t *testing.T) {
	// Create an empty file
	tempFile, err := os.CreateTemp("", "empty-checksum-test-*")
	require.NoError(t, err)
	defer func() {
		_ = os.Remove(tempFile.Name())
	}()
	require.NoError(t, tempFile.Close())

	// Calculate checksum of empty file
	checksum, err := CalculateFileChecksum(tempFile.Name())
	require.NoError(t, err)

	// Empty file should have a specific SHA256 hash
	expectedEmptyFileHash := "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
	assert.Equal(t, expectedEmptyFileHash, checksum)
}
