package status

import (
	"errors"
	iosfs "io/fs"
	"testing"
	"time"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestBaseChecker_InitializeStatus(t *testing.T) {
	bc := &BaseChecker{PowerUpName: "test_powerup"}

	status := bc.InitializeStatus("/path/to/file", "default message")

	assert.Equal(t, "/path/to/file", status.Path)
	assert.Equal(t, "test_powerup", status.PowerUp)
	assert.Equal(t, types.StatusReady, status.Status)
	assert.Equal(t, "default message", status.Message)
	assert.True(t, status.LastApplied.IsZero())
	assert.NotNil(t, status.Metadata)
}

func TestBaseChecker_HandleStatError(t *testing.T) {
	bc := &BaseChecker{PowerUpName: "test_powerup"}

	tests := []struct {
		name            string
		err             error
		notExistMessage string
		expectedStatus  types.OperationStatus
		expectedMessage string
	}{
		{
			name:            "not_exist_error",
			err:             &iosfs.PathError{Op: "stat", Err: iosfs.ErrNotExist},
			notExistMessage: "File does not exist",
			expectedStatus:  types.StatusReady,
			expectedMessage: "File does not exist",
		},
		{
			name:            "other_error",
			err:             errors.New("permission denied"),
			notExistMessage: "File does not exist",
			expectedStatus:  types.StatusError,
			expectedMessage: "Failed to check test_powerup: permission denied",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			status := bc.InitializeStatus("/test", "default")

			result, err := bc.HandleStatError(status, tt.err, tt.notExistMessage)

			assert.NoError(t, err)
			assert.Equal(t, tt.expectedStatus, result.Status)
			assert.Equal(t, tt.expectedMessage, result.Message)
		})
	}
}

func TestBaseChecker_SetError(t *testing.T) {
	bc := &BaseChecker{PowerUpName: "test_powerup"}
	status := bc.InitializeStatus("/test", "default")

	testErr := errors.New("test error")
	bc.SetError(status, "perform action", testErr)

	assert.Equal(t, types.StatusError, status.Status)
	assert.Equal(t, "Failed to perform action: test error", status.Message)
}

func TestBaseChecker_CheckSymlink(t *testing.T) {
	bc := &BaseChecker{PowerUpName: "test_powerup"}
	fs := newTestFileSystem()

	// Create test files
	err := fs.WriteFile("/regular-file", []byte("content"), 0644)
	require.NoError(t, err)

	err = fs.MkdirAll("/test-dir", 0755)
	require.NoError(t, err)

	// Create a symlink
	err = fs.Symlink("/target", "/test-symlink")
	require.NoError(t, err)

	tests := []struct {
		name          string
		path          string
		wantExists    bool
		wantIsSymlink bool
		wantTarget    string
		wantErr       bool
	}{
		{
			name:          "non_existent_path",
			path:          "/not-exist",
			wantExists:    false,
			wantIsSymlink: false,
			wantTarget:    "",
			wantErr:       false,
		},
		{
			name:          "regular_file",
			path:          "/regular-file",
			wantExists:    true,
			wantIsSymlink: false,
			wantTarget:    "",
			wantErr:       false,
		},
		{
			name:          "directory",
			path:          "/test-dir",
			wantExists:    true,
			wantIsSymlink: false,
			wantTarget:    "",
			wantErr:       false,
		},
		{
			name:          "valid_symlink",
			path:          "/test-symlink",
			wantExists:    true,
			wantIsSymlink: true,
			wantTarget:    "target", // Test filesystem returns relative paths
			wantErr:       false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, err := bc.CheckSymlink(fs, tt.path)

			if tt.wantErr {
				assert.Error(t, err)
				return
			}

			require.NoError(t, err)
			assert.Equal(t, tt.wantExists, result.Exists)
			assert.Equal(t, tt.wantIsSymlink, result.IsSymlink)
			if tt.wantIsSymlink {
				assert.Equal(t, tt.wantTarget, result.ActualTarget)
			}
		})
	}
}

func TestBaseChecker_CompareSymlinkTargets(t *testing.T) {
	bc := &BaseChecker{PowerUpName: "test_powerup"}

	tests := []struct {
		name     string
		expected string
		actual   string
		want     bool
	}{
		{
			name:     "exact_match",
			expected: "/path/to/file",
			actual:   "/path/to/file",
			want:     true,
		},
		{
			name:     "relative_vs_absolute",
			expected: "/path/to/file",
			actual:   "path/to/file",
			want:     true,
		},
		{
			name:     "different_paths",
			expected: "/path/to/file1",
			actual:   "/path/to/file2",
			want:     false,
		},
		{
			name:     "both_relative",
			expected: "path/to/file",
			actual:   "path/to/file",
			want:     true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := bc.CompareSymlinkTargets(tt.expected, tt.actual)
			assert.Equal(t, tt.want, got)
		})
	}
}

func TestBaseChecker_SetSymlinkMetadata(t *testing.T) {
	bc := &BaseChecker{PowerUpName: "test_powerup"}

	t.Run("symlink_exists", func(t *testing.T) {
		status := bc.InitializeStatus("/test", "default")
		modTime := time.Now()

		result := &SymlinkCheckResult{
			Exists:       true,
			IsSymlink:    true,
			ActualTarget: "/actual/target",
			ModTime:      modTime,
		}

		bc.SetSymlinkMetadata(status, result, "/expected/target")

		assert.Equal(t, modTime, status.LastApplied)
		assert.Equal(t, "/actual/target", status.Metadata["actual_target"])
		assert.Equal(t, "/expected/target", status.Metadata["expected_target"])
		assert.Equal(t, true, status.Metadata["is_symlink"])
	})

	t.Run("file_exists_not_symlink", func(t *testing.T) {
		status := bc.InitializeStatus("/test", "default")
		modTime := time.Now()

		result := &SymlinkCheckResult{
			Exists:    true,
			IsSymlink: false,
			ModTime:   modTime,
		}

		bc.SetSymlinkMetadata(status, result, "/expected/target")

		assert.Equal(t, modTime, status.LastApplied)
		assert.Equal(t, false, status.Metadata["is_symlink"])
		assert.Nil(t, status.Metadata["actual_target"])
		assert.Nil(t, status.Metadata["expected_target"])
	})

	t.Run("not_exists", func(t *testing.T) {
		status := bc.InitializeStatus("/test", "default")

		result := &SymlinkCheckResult{
			Exists: false,
		}

		bc.SetSymlinkMetadata(status, result, "/expected/target")

		assert.True(t, status.LastApplied.IsZero())
		assert.Nil(t, status.Metadata["is_symlink"])
		assert.Nil(t, status.Metadata["actual_target"])
		assert.Nil(t, status.Metadata["expected_target"])
	})
}
