package status

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestSentinelChecker_ComputeSentinelPath(t *testing.T) {
	sc := NewSentinelChecker("homebrew")

	tests := []struct {
		name     string
		op       *types.Operation
		expected string
	}{
		{
			name: "extract_pack_from_metadata",
			op: &types.Operation{
				Source: "/dotfiles/mypack/Brewfile",
				Target: "/data/homebrew/sentinel",
				Metadata: map[string]interface{}{
					"pack": "mypack",
				},
			},
			expected: "/data/homebrew/mypack",
		},
		{
			name: "extract_pack_from_source_path",
			op: &types.Operation{
				Source: "/dotfiles/otherpack/Brewfile",
				Target: "/data/homebrew/sentinel",
			},
			expected: "/data/homebrew/otherpack",
		},
		{
			name: "sentinel_write_operation",
			op: &types.Operation{
				Type:   types.OperationWriteFile,
				Source: "/dotfiles/mypack/Brewfile",
				Target: "/data/homebrew/mypack",
			},
			expected: "/data/homebrew/mypack",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := sc.ComputeSentinelPath(tt.op)
			assert.Equal(t, tt.expected, got)
		})
	}
}

func TestSentinelChecker_CheckSentinel(t *testing.T) {
	sc := NewSentinelChecker("homebrew")
	fs := newTestFileSystem()

	// Create a sentinel file with checksum
	err := fs.MkdirAll("/data/homebrew", 0755)
	require.NoError(t, err)
	err = fs.WriteFile("/data/homebrew/mypack", []byte("abc123checksum"), 0644)
	require.NoError(t, err)

	tests := []struct {
		name         string
		sentinelPath string
		op           *types.Operation
		wantExists   bool
		wantStored   string
		wantCurrent  string
		wantErr      bool
	}{
		{
			name:         "sentinel_not_exists",
			sentinelPath: "/data/homebrew/notexist",
			op: &types.Operation{
				Metadata: map[string]interface{}{
					"checksum": "newchecksum",
				},
			},
			wantExists:  false,
			wantStored:  "",
			wantCurrent: "newchecksum",
			wantErr:     false,
		},
		{
			name:         "sentinel_exists_with_checksum",
			sentinelPath: "/data/homebrew/mypack",
			op: &types.Operation{
				Content: "abc123checksum",
			},
			wantExists:  true,
			wantStored:  "abc123checksum",
			wantCurrent: "abc123checksum",
			wantErr:     false,
		},
		{
			name:         "sentinel_exists_different_checksum",
			sentinelPath: "/data/homebrew/mypack",
			op: &types.Operation{
				Metadata: map[string]interface{}{
					"checksum": "differentchecksum",
				},
			},
			wantExists:  true,
			wantStored:  "abc123checksum",
			wantCurrent: "differentchecksum",
			wantErr:     false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := sc.CheckSentinel(fs, tt.sentinelPath, tt.op)

			if tt.wantErr {
				assert.NotNil(t, result.Error)
				return
			}

			assert.Nil(t, result.Error)
			assert.Equal(t, tt.wantExists, result.Exists)
			assert.Equal(t, tt.wantStored, result.StoredChecksum)
			assert.Equal(t, tt.wantCurrent, result.CurrentChecksum)
		})
	}
}

func TestSentinelChecker_GetCurrentChecksum(t *testing.T) {
	sc := NewSentinelChecker("homebrew")

	tests := []struct {
		name     string
		op       *types.Operation
		expected string
	}{
		{
			name: "checksum_from_content",
			op: &types.Operation{
				Content: "checksum123",
			},
			expected: "checksum123",
		},
		{
			name: "checksum_from_metadata",
			op: &types.Operation{
				Metadata: map[string]interface{}{
					"checksum": "metachecksum",
				},
			},
			expected: "metachecksum",
		},
		{
			name: "content_takes_precedence",
			op: &types.Operation{
				Content: "contentchecksum",
				Metadata: map[string]interface{}{
					"checksum": "metachecksum",
				},
			},
			expected: "contentchecksum",
		},
		{
			name:     "no_checksum",
			op:       &types.Operation{},
			expected: "",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := sc.GetCurrentChecksum(tt.op)
			assert.Equal(t, tt.expected, got)
		})
	}
}

func TestSentinelChecker_SetSentinelMetadata(t *testing.T) {
	sc := NewSentinelChecker("homebrew")

	t.Run("sentinel_exists_matching_checksum", func(t *testing.T) {
		status := &types.FileStatus{
			Metadata: make(map[string]interface{}),
		}
		result := &CheckSentinelResult{
			Exists:          true,
			StoredChecksum:  "abc123",
			CurrentChecksum: "abc123",
		}

		sc.SetSentinelMetadata(status, result, "mypack")

		assert.Equal(t, "mypack", status.Metadata["pack"])
		assert.Equal(t, true, status.Metadata["sentinel_exists"])
		assert.Equal(t, "abc123", status.Metadata["stored_checksum"])
		assert.Equal(t, "abc123", status.Metadata["current_checksum"])
		assert.Equal(t, true, status.Metadata["checksum_match"])
	})

	t.Run("sentinel_exists_different_checksum", func(t *testing.T) {
		status := &types.FileStatus{
			Metadata: make(map[string]interface{}),
		}
		result := &CheckSentinelResult{
			Exists:          true,
			StoredChecksum:  "old123",
			CurrentChecksum: "new456",
		}

		sc.SetSentinelMetadata(status, result, "mypack")

		assert.Equal(t, "mypack", status.Metadata["pack"])
		assert.Equal(t, true, status.Metadata["sentinel_exists"])
		assert.Equal(t, "old123", status.Metadata["stored_checksum"])
		assert.Equal(t, "new456", status.Metadata["current_checksum"])
		assert.Equal(t, false, status.Metadata["checksum_match"])
	})

	t.Run("sentinel_not_exists", func(t *testing.T) {
		status := &types.FileStatus{
			Metadata: make(map[string]interface{}),
		}
		result := &CheckSentinelResult{
			Exists:          false,
			CurrentChecksum: "new123",
		}

		sc.SetSentinelMetadata(status, result, "mypack")

		assert.Equal(t, "mypack", status.Metadata["pack"])
		assert.Equal(t, false, status.Metadata["sentinel_exists"])
		assert.Equal(t, "new123", status.Metadata["current_checksum"])
		assert.Nil(t, status.Metadata["stored_checksum"])
		assert.Nil(t, status.Metadata["checksum_match"])
	})
}
