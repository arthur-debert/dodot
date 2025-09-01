// Test Type: Unit Test
// Description: Tests for the hashutil package - checksum calculation with minimal filesystem dependency

package utils_test

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/utils"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestCalculateFileChecksum_Success(t *testing.T) {
	// Create a temporary directory for test files
	tempDir := t.TempDir()

	tests := []struct {
		name           string
		content        string
		expectedPrefix string
		expectedLength int
	}{
		{
			name:           "file_with_content",
			content:        "Hello, World!\nThis is a test file.\n",
			expectedPrefix: "sha256:",
			expectedLength: 71, // "sha256:" + 64 hex chars
		},
		{
			name:           "empty_file",
			content:        "",
			expectedPrefix: "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
			expectedLength: 71,
		},
		{
			name:           "single_character",
			content:        "a",
			expectedPrefix: "sha256:",
			expectedLength: 71,
		},
		{
			name:           "binary_content",
			content:        "\x00\x01\x02\x03\x04\x05",
			expectedPrefix: "sha256:",
			expectedLength: 71,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Create test file
			testFile := filepath.Join(tempDir, tt.name)
			err := os.WriteFile(testFile, []byte(tt.content), 0644)
			require.NoError(t, err)

			// Calculate checksum
			checksum, err := utils.CalculateFileChecksum(testFile)
			require.NoError(t, err)

			// Verify checksum format
			assert.Contains(t, checksum, tt.expectedPrefix)
			assert.Len(t, checksum, tt.expectedLength)

			// If we know the exact checksum (like for empty file), verify it
			if len(tt.expectedPrefix) == tt.expectedLength {
				assert.Equal(t, tt.expectedPrefix, checksum)
			}
		})
	}
}

func TestCalculateFileChecksum_Consistency(t *testing.T) {
	// Create a test file
	tempDir := t.TempDir()
	testFile := filepath.Join(tempDir, "consistent.txt")
	testContent := "This content should always produce the same checksum"

	err := os.WriteFile(testFile, []byte(testContent), 0644)
	require.NoError(t, err)

	// Calculate checksum multiple times
	checksums := make([]string, 5)
	for i := 0; i < 5; i++ {
		checksum, err := utils.CalculateFileChecksum(testFile)
		require.NoError(t, err)
		checksums[i] = checksum
	}

	// All checksums should be identical
	for i := 1; i < 5; i++ {
		assert.Equal(t, checksums[0], checksums[i], "Checksum should be consistent across multiple calculations")
	}
}

func TestCalculateFileChecksum_Errors(t *testing.T) {
	tests := []struct {
		name        string
		path        string
		setupFunc   func(t *testing.T, dir string) string
		expectError bool
		errorMsg    string
	}{
		{
			name:        "non_existent_file",
			path:        "/non/existent/file",
			expectError: true,
			errorMsg:    "no such file or directory",
		},
		{
			name: "directory_instead_of_file",
			setupFunc: func(t *testing.T, dir string) string {
				subDir := filepath.Join(dir, "subdir")
				err := os.Mkdir(subDir, 0755)
				require.NoError(t, err)
				return subDir
			},
			expectError: true,
			errorMsg:    "is a directory",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			path := tt.path
			if tt.setupFunc != nil {
				tempDir := t.TempDir()
				path = tt.setupFunc(t, tempDir)
			}

			// Attempt to calculate checksum
			_, err := utils.CalculateFileChecksum(path)

			if tt.expectError {
				require.Error(t, err)
				assert.Contains(t, err.Error(), tt.errorMsg)
			} else {
				require.NoError(t, err)
			}
		})
	}
}

func TestCalculateFileChecksum_LargeFile(t *testing.T) {
	// Skip in short mode as this creates a larger file
	if testing.Short() {
		t.Skip("Skipping large file test in short mode")
	}

	tempDir := t.TempDir()
	largeFile := filepath.Join(tempDir, "large.bin")

	// Create a 1MB file with repeating pattern
	size := 1024 * 1024 // 1MB
	data := make([]byte, size)
	for i := range data {
		data[i] = byte(i % 256)
	}

	err := os.WriteFile(largeFile, data, 0644)
	require.NoError(t, err)

	// Calculate checksum
	checksum, err := utils.CalculateFileChecksum(largeFile)
	require.NoError(t, err)

	// Verify format
	assert.Contains(t, checksum, "sha256:")
	assert.Len(t, checksum, 71)
}
