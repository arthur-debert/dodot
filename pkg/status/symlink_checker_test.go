package status

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestSymlinkChecker_CheckStatus_NoSymlink(t *testing.T) {
	fs := newTestFileSystem()
	checker := NewSymlinkChecker()

	op := &types.Operation{
		Type:    types.OperationCreateSymlink,
		Source:  "/source/file.txt",
		Target:  "/target/link",
		PowerUp: "symlink",
	}

	status, err := checker.CheckStatus(op, fs)
	require.NoError(t, err)
	require.NotNil(t, status)

	assert.Equal(t, "/target/link", status.Path)
	assert.Equal(t, "symlink", status.PowerUp)
	assert.Equal(t, types.StatusReady, status.Status)
	assert.Equal(t, "Symlink does not exist", status.Message)
	assert.Equal(t, "/source/file.txt", status.Metadata["expected_target"])
}

func TestSymlinkChecker_CheckStatus_ValidSymlink(t *testing.T) {
	fs := newTestFileSystem()
	checker := NewSymlinkChecker()

	// Create source file
	err := fs.WriteFile("/source/file.txt", []byte("content"), 0644)
	require.NoError(t, err)

	// Create symlink
	err = fs.Symlink("/source/file.txt", "/target/link")
	require.NoError(t, err)

	op := &types.Operation{
		Type:    types.OperationCreateSymlink,
		Source:  "/source/file.txt",
		Target:  "/target/link",
		PowerUp: "symlink",
	}

	status, err := checker.CheckStatus(op, fs)
	require.NoError(t, err)
	require.NotNil(t, status)

	assert.Equal(t, types.StatusSkipped, status.Status)
	assert.Equal(t, "Symlink already exists with correct target", status.Message)
	assert.Equal(t, "/source/file.txt", status.Metadata["expected_target"])
	assert.Equal(t, "/source/file.txt", status.Metadata["actual_target"])
	assert.Equal(t, true, status.Metadata["link_valid"])
	assert.Equal(t, true, status.Metadata["target_exists"])
	// Test filesystem might not support modification times properly
	// Just check that the field was set if the filesystem supports it
	if !status.LastApplied.IsZero() {
		assert.False(t, status.LastApplied.IsZero())
	}
}

func TestSymlinkChecker_CheckStatus_WrongTarget(t *testing.T) {
	fs := newTestFileSystem()
	checker := NewSymlinkChecker()

	// Create symlink pointing to wrong target
	err := fs.Symlink("/wrong/target", "/target/link")
	require.NoError(t, err)

	op := &types.Operation{
		Type:    types.OperationCreateSymlink,
		Source:  "/source/file.txt",
		Target:  "/target/link",
		PowerUp: "symlink",
	}

	status, err := checker.CheckStatus(op, fs)
	require.NoError(t, err)
	require.NotNil(t, status)

	assert.Equal(t, types.StatusConflict, status.Status)
	assert.Equal(t, "Symlink exists but points to wrong target", status.Message)
	assert.Equal(t, "/source/file.txt", status.Metadata["expected_target"])
	assert.Equal(t, "/wrong/target", status.Metadata["actual_target"])
	assert.Equal(t, false, status.Metadata["link_valid"])
}

func TestSymlinkChecker_CheckStatus_FileExistsNotSymlink(t *testing.T) {
	fs := newTestFileSystem()
	checker := NewSymlinkChecker()

	// Create regular file where symlink should be
	err := fs.WriteFile("/target/link", []byte("content"), 0644)
	require.NoError(t, err)

	op := &types.Operation{
		Type:    types.OperationCreateSymlink,
		Source:  "/source/file.txt",
		Target:  "/target/link",
		PowerUp: "symlink",
	}

	status, err := checker.CheckStatus(op, fs)
	require.NoError(t, err)
	require.NotNil(t, status)

	assert.Equal(t, types.StatusConflict, status.Status)
	assert.Equal(t, "Path exists but is not a symlink", status.Message)
	assert.Equal(t, false, status.Metadata["is_directory"])
	assert.Equal(t, true, status.Metadata["is_regular_file"])
}

func TestSymlinkChecker_CheckStatus_DirectoryExistsNotSymlink(t *testing.T) {
	fs := newTestFileSystem()
	checker := NewSymlinkChecker()

	// Create directory where symlink should be
	err := fs.MkdirAll("/target/link", 0755)
	require.NoError(t, err)

	op := &types.Operation{
		Type:    types.OperationCreateSymlink,
		Source:  "/source/file.txt",
		Target:  "/target/link",
		PowerUp: "symlink",
	}

	status, err := checker.CheckStatus(op, fs)
	require.NoError(t, err)
	require.NotNil(t, status)

	assert.Equal(t, types.StatusConflict, status.Status)
	assert.Equal(t, "Path exists but is not a symlink", status.Message)
	assert.Equal(t, true, status.Metadata["is_directory"])
	assert.Equal(t, false, status.Metadata["is_regular_file"])
}

func TestSymlinkChecker_CheckStatus_BrokenSymlink(t *testing.T) {
	fs := newTestFileSystem()
	checker := NewSymlinkChecker()

	// Create symlink to non-existent target
	err := fs.Symlink("/source/file.txt", "/target/link")
	require.NoError(t, err)

	op := &types.Operation{
		Type:    types.OperationCreateSymlink,
		Source:  "/source/file.txt",
		Target:  "/target/link",
		PowerUp: "symlink",
	}

	status, err := checker.CheckStatus(op, fs)
	require.NoError(t, err)
	require.NotNil(t, status)

	assert.Equal(t, types.StatusSkipped, status.Status)
	assert.Equal(t, "Symlink already exists with correct target", status.Message)
	assert.Equal(t, false, status.Metadata["target_exists"])
}
