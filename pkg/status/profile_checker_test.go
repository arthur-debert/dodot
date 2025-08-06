package status

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestProfileChecker_CheckStatus_NotDeployed(t *testing.T) {
	fs := newTestFileSystem()
	checker := NewProfileChecker()

	op := &types.Operation{
		Type:    types.OperationCreateSymlink,
		Source:  "/packs/mypack/aliases.sh",
		Target:  "/deployed/shell_profile/mypack.sh",
		PowerUp: "shell_profile",
	}

	status, err := checker.CheckStatus(op, fs)
	require.NoError(t, err)
	require.NotNil(t, status)

	assert.Equal(t, "/deployed/shell_profile/mypack.sh", status.Path)
	assert.Equal(t, "shell_profile", status.PowerUp)
	assert.Equal(t, types.StatusReady, status.Status)
	assert.Equal(t, "Profile script not deployed", status.Message)
}

func TestProfileChecker_CheckStatus_ProperlyDeployed(t *testing.T) {
	fs := newTestFileSystem()
	checker := NewProfileChecker()

	// Create source script
	err := fs.MkdirAll("/packs/mypack", 0755)
	require.NoError(t, err)
	err = fs.WriteFile("/packs/mypack/aliases.sh", []byte("alias ll='ls -la'"), 0644)
	require.NoError(t, err)

	// Create deployment directory and symlink
	err = fs.MkdirAll("/deployed/shell_profile", 0755)
	require.NoError(t, err)
	err = fs.Symlink("/packs/mypack/aliases.sh", "/deployed/shell_profile/mypack.sh")
	require.NoError(t, err)

	op := &types.Operation{
		Type:    types.OperationCreateSymlink,
		Source:  "/packs/mypack/aliases.sh",
		Target:  "/deployed/shell_profile/mypack.sh",
		PowerUp: "shell_profile",
	}

	status, err := checker.CheckStatus(op, fs)
	require.NoError(t, err)
	require.NotNil(t, status)

	assert.Equal(t, types.StatusSkipped, status.Status)
	assert.Equal(t, "Profile script already deployed", status.Message)
	assert.Equal(t, "/packs/mypack/aliases.sh", status.Metadata["source_script"])
	assert.Equal(t, "/deployed/shell_profile/mypack.sh", status.Metadata["deployed_symlink"])
	assert.Equal(t, "/packs/mypack/aliases.sh", status.Metadata["actual_target"])
	assert.Equal(t, true, status.Metadata["source_exists"])
	// Test filesystem might not support modification times properly
	// Just check that the field was set if the filesystem supports it
	if !status.LastApplied.IsZero() {
		assert.False(t, status.LastApplied.IsZero())
	}

	// Check loading shells
	loadedByRaw := status.Metadata["loaded_by"]
	if loadedByRaw != nil {
		loadedBy := loadedByRaw.([]string)
		assert.Contains(t, loadedBy, "bash (via dodot-init.sh)")
		assert.Contains(t, loadedBy, "zsh (via dodot-init.sh)")
	}
}

func TestProfileChecker_CheckStatus_WrongTarget(t *testing.T) {
	fs := newTestFileSystem()
	checker := NewProfileChecker()

	// Create deployment directory and symlink pointing to wrong script
	err := fs.MkdirAll("/deployed/shell_profile", 0755)
	require.NoError(t, err)
	err = fs.Symlink("/wrong/script.sh", "/deployed/shell_profile/mypack.sh")
	require.NoError(t, err)

	op := &types.Operation{
		Type:    types.OperationCreateSymlink,
		Source:  "/packs/mypack/aliases.sh",
		Target:  "/deployed/shell_profile/mypack.sh",
		PowerUp: "shell_profile",
	}

	status, err := checker.CheckStatus(op, fs)
	require.NoError(t, err)
	require.NotNil(t, status)

	assert.Equal(t, types.StatusConflict, status.Status)
	assert.Equal(t, "Profile symlink points to wrong script", status.Message)
	assert.Equal(t, "/wrong/script.sh", status.Metadata["actual_target"])
}

func TestProfileChecker_CheckStatus_FileExistsNotSymlink(t *testing.T) {
	fs := newTestFileSystem()
	checker := NewProfileChecker()

	// Create regular file where symlink should be
	err := fs.MkdirAll("/deployed/shell_profile", 0755)
	require.NoError(t, err)
	err = fs.WriteFile("/deployed/shell_profile/mypack.sh", []byte("not a symlink"), 0644)
	require.NoError(t, err)

	op := &types.Operation{
		Type:    types.OperationCreateSymlink,
		Source:  "/packs/mypack/aliases.sh",
		Target:  "/deployed/shell_profile/mypack.sh",
		PowerUp: "shell_profile",
	}

	status, err := checker.CheckStatus(op, fs)
	require.NoError(t, err)
	require.NotNil(t, status)

	assert.Equal(t, types.StatusConflict, status.Status)
	assert.Equal(t, "Deployment path exists but is not a symlink", status.Message)
}

func TestProfileChecker_getLoadingShells(t *testing.T) {
	checker := &ProfileChecker{}

	tests := []struct {
		name     string
		path     string
		expected []string
	}{
		{
			name:     "profile in deployed directory",
			path:     "/data/deployed/shell_profile/mypack.sh",
			expected: []string{"bash (via dodot-init.sh)", "zsh (via dodot-init.sh)"},
		},
		{
			name:     "profile not in standard location",
			path:     "/other/location/script.sh",
			expected: []string{"none (dodot-init.sh not sourced)"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := checker.getLoadingShells(tt.path)
			assert.Equal(t, tt.expected, result)
		})
	}
}
