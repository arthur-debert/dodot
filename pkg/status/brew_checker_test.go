package status

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestBrewChecker_CheckStatus_NoSentinel(t *testing.T) {
	fs := newTestFileSystem()
	checker := NewBrewChecker()

	op := &types.Operation{
		Type:    types.OperationExecute,
		Source:  "/packs/mypack/Brewfile",
		Target:  "/deployed/homebrew/mypack",
		PowerUp: "homebrew",
		Metadata: map[string]interface{}{
			"pack": "mypack",
		},
	}

	status, err := checker.CheckStatus(op, fs)
	require.NoError(t, err)
	require.NotNil(t, status)

	assert.Equal(t, "/packs/mypack/Brewfile", status.Path)
	assert.Equal(t, "homebrew", status.PowerUp)
	assert.Equal(t, types.StatusReady, status.Status)
	assert.Equal(t, "Brewfile not processed (no sentinel file)", status.Message)
	assert.Equal(t, false, status.Metadata["sentinel_exists"])
	assert.Equal(t, "mypack", status.Metadata["pack"])
	assert.Equal(t, "/packs/mypack/Brewfile", status.Metadata["brewfile"])
}

func TestBrewChecker_CheckStatus_ValidSentinel(t *testing.T) {
	fs := newTestFileSystem()
	checker := NewBrewChecker()

	// Create sentinel file with checksum
	err := fs.MkdirAll("/deployed/homebrew", 0755)
	require.NoError(t, err)
	err = fs.WriteFile("/deployed/homebrew/mypack", []byte("abc123checksum"), 0644)
	require.NoError(t, err)

	op := &types.Operation{
		Type:    types.OperationWriteFile,
		Source:  "/packs/mypack/Brewfile",
		Target:  "/deployed/homebrew/mypack",
		Content: "abc123checksum",
		PowerUp: "homebrew",
		Metadata: map[string]interface{}{
			"pack":     "mypack",
			"checksum": "abc123checksum",
		},
	}

	status, err := checker.CheckStatus(op, fs)
	require.NoError(t, err)
	require.NotNil(t, status)

	assert.Equal(t, types.StatusSkipped, status.Status)
	assert.Equal(t, "Brewfile already processed (checksum matches)", status.Message)
	assert.Equal(t, true, status.Metadata["sentinel_exists"])
	assert.Equal(t, "abc123checksum", status.Metadata["stored_checksum"])
	assert.Equal(t, "abc123checksum", status.Metadata["current_checksum"])
	assert.False(t, status.LastApplied.IsZero())
}

func TestBrewChecker_CheckStatus_ChecksumMismatch(t *testing.T) {
	fs := newTestFileSystem()
	checker := NewBrewChecker()

	// Create sentinel file with old checksum
	err := fs.MkdirAll("/deployed/homebrew", 0755)
	require.NoError(t, err)
	err = fs.WriteFile("/deployed/homebrew/mypack", []byte("oldchecksum"), 0644)
	require.NoError(t, err)

	op := &types.Operation{
		Type:    types.OperationWriteFile,
		Source:  "/packs/mypack/Brewfile",
		Target:  "/deployed/homebrew/mypack",
		Content: "newchecksum",
		PowerUp: "homebrew",
		Metadata: map[string]interface{}{
			"pack":     "mypack",
			"checksum": "newchecksum",
		},
	}

	status, err := checker.CheckStatus(op, fs)
	require.NoError(t, err)
	require.NotNil(t, status)

	assert.Equal(t, types.StatusReady, status.Status)
	assert.Equal(t, "Brewfile has changed (checksum mismatch)", status.Message)
	assert.Equal(t, true, status.Metadata["sentinel_exists"])
	assert.Equal(t, "oldchecksum", status.Metadata["stored_checksum"])
	assert.Equal(t, "newchecksum", status.Metadata["current_checksum"])
	assert.Equal(t, false, status.Metadata["checksum_match"])
}

func TestBrewChecker_CheckStatus_ExecuteOperation(t *testing.T) {
	fs := newTestFileSystem()
	checker := NewBrewChecker()

	// Test with execute operation type
	op := &types.Operation{
		Type:    types.OperationExecute,
		Source:  "/packs/mypack/Brewfile",
		Target:  "/deployed/homebrew/mypack",
		PowerUp: "homebrew",
		Metadata: map[string]interface{}{
			"pack":     "mypack",
			"checksum": "abc123",
		},
	}

	status, err := checker.CheckStatus(op, fs)
	require.NoError(t, err)
	require.NotNil(t, status)

	assert.Equal(t, types.StatusReady, status.Status)
	assert.Equal(t, "Brewfile not processed (no sentinel file)", status.Message)
	assert.Equal(t, false, status.Metadata["sentinel_exists"])
}

func TestBrewChecker_CheckStatus_NoChecksumMetadata(t *testing.T) {
	fs := newTestFileSystem()
	checker := NewBrewChecker()

	// Create sentinel file
	err := fs.MkdirAll("/deployed/homebrew", 0755)
	require.NoError(t, err)
	err = fs.WriteFile("/deployed/homebrew/mypack", []byte("abc123"), 0644)
	require.NoError(t, err)

	op := &types.Operation{
		Type:    types.OperationExecute,
		Source:  "/packs/mypack/Brewfile",
		Target:  "/deployed/homebrew/mypack",
		PowerUp: "homebrew",
		Metadata: map[string]interface{}{
			"pack": "mypack",
			// No checksum in metadata
		},
	}

	status, err := checker.CheckStatus(op, fs)
	require.NoError(t, err)
	require.NotNil(t, status)

	assert.Equal(t, types.StatusUnknown, status.Status)
	assert.Equal(t, "Cannot determine current Brewfile checksum", status.Message)
	assert.Equal(t, true, status.Metadata["sentinel_exists"])
	assert.Equal(t, "abc123", status.Metadata["stored_checksum"])
}

func TestBrewChecker_CheckStatus_ExtractPackFromPath(t *testing.T) {
	fs := newTestFileSystem()
	checker := NewBrewChecker()

	op := &types.Operation{
		Type:    types.OperationExecute,
		Source:  "/packs/extracted-pack/Brewfile",
		Target:  "/deployed/homebrew/extracted-pack",
		PowerUp: "homebrew",
		// No metadata, pack should be extracted from path
	}

	status, err := checker.CheckStatus(op, fs)
	require.NoError(t, err)
	require.NotNil(t, status)

	assert.Equal(t, "extracted-pack", status.Metadata["pack"])
}
