package status

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestPathChecker_CheckStatus_NotDeployed(t *testing.T) {
	fs := newTestFileSystem()
	checker := NewPathChecker()

	op := &types.Operation{
		Type:    types.OperationCreateSymlink,
		Source:  "/packs/mypack/bin",
		Target:  "/deployed/path/mypack",
		PowerUp: "add_path",
	}

	status, err := checker.CheckStatus(op, fs)
	require.NoError(t, err)
	require.NotNil(t, status)

	assert.Equal(t, "/deployed/path/mypack", status.Path)
	assert.Equal(t, "add_path", status.PowerUp)
	assert.Equal(t, types.StatusReady, status.Status)
	assert.Equal(t, "Directory not deployed to PATH", status.Message)
	assert.Equal(t, false, status.Metadata["in_path"])
}

func TestPathChecker_CheckStatus_ProperlyDeployed(t *testing.T) {
	fs := newTestFileSystem()
	checker := NewPathChecker()

	// Create source directory
	err := fs.MkdirAll("/packs/mypack/bin", 0755)
	require.NoError(t, err)

	// Create deployment directory and symlink
	err = fs.MkdirAll("/deployed/path", 0755)
	require.NoError(t, err)
	err = fs.Symlink("/packs/mypack/bin", "/deployed/path/mypack")
	require.NoError(t, err)

	op := &types.Operation{
		Type:    types.OperationCreateSymlink,
		Source:  "/packs/mypack/bin",
		Target:  "/deployed/path/mypack",
		PowerUp: "add_path",
	}

	status, err := checker.CheckStatus(op, fs)
	require.NoError(t, err)
	require.NotNil(t, status)

	assert.Equal(t, types.StatusSkipped, status.Status)
	assert.Equal(t, "/packs/mypack/bin", status.Metadata["source_directory"])
	assert.Equal(t, "/deployed/path/mypack", status.Metadata["deployed_symlink"])
	assert.Equal(t, "/packs/mypack/bin", status.Metadata["actual_target"])
	assert.Equal(t, true, status.Metadata["source_exists"])
	assert.False(t, status.LastApplied.IsZero())

	// Note: in_current_path will be false unless the PATH env var contains the deployed path
	assert.Equal(t, false, status.Metadata["in_current_path"])
	assert.Contains(t, status.Message, "Directory deployed but not in current PATH")
}

func TestPathChecker_CheckStatus_InCurrentPath(t *testing.T) {
	fs := newTestFileSystem()
	checker := NewPathChecker()

	// Create source directory
	err := fs.MkdirAll("/packs/mypack/bin", 0755)
	require.NoError(t, err)

	// Create deployment directory and symlink
	deployedPath := "/tmp/test-deployed/path/mypack"
	err = fs.MkdirAll(filepath.Dir(deployedPath), 0755)
	require.NoError(t, err)
	err = fs.Symlink("/packs/mypack/bin", deployedPath)
	require.NoError(t, err)

	// Set PATH to include the deployed directory
	absDeployedPath, _ := filepath.Abs(deployedPath)
	oldPath := os.Getenv("PATH")
	defer func() {
		_ = os.Setenv("PATH", oldPath)
	}()
	_ = os.Setenv("PATH", oldPath+string(os.PathListSeparator)+absDeployedPath)

	op := &types.Operation{
		Type:    types.OperationCreateSymlink,
		Source:  "/packs/mypack/bin",
		Target:  deployedPath,
		PowerUp: "add_path",
	}

	status, err := checker.CheckStatus(op, fs)
	require.NoError(t, err)
	require.NotNil(t, status)

	assert.Equal(t, types.StatusSkipped, status.Status)
	assert.Equal(t, "Directory already deployed to PATH", status.Message)
	assert.Equal(t, true, status.Metadata["in_current_path"])
}

func TestPathChecker_CheckStatus_WrongTarget(t *testing.T) {
	fs := newTestFileSystem()
	checker := NewPathChecker()

	// Create deployment directory and symlink pointing to wrong directory
	err := fs.MkdirAll("/deployed/path", 0755)
	require.NoError(t, err)
	err = fs.Symlink("/wrong/directory", "/deployed/path/mypack")
	require.NoError(t, err)

	op := &types.Operation{
		Type:    types.OperationCreateSymlink,
		Source:  "/packs/mypack/bin",
		Target:  "/deployed/path/mypack",
		PowerUp: "add_path",
	}

	status, err := checker.CheckStatus(op, fs)
	require.NoError(t, err)
	require.NotNil(t, status)

	assert.Equal(t, types.StatusConflict, status.Status)
	assert.Equal(t, "PATH symlink points to wrong directory", status.Message)
	assert.Equal(t, "/wrong/directory", status.Metadata["actual_target"])
}

func TestPathChecker_CheckStatus_FileExistsNotSymlink(t *testing.T) {
	fs := newTestFileSystem()
	checker := NewPathChecker()

	// Create regular file where symlink should be
	err := fs.MkdirAll("/deployed/path", 0755)
	require.NoError(t, err)
	err = fs.WriteFile("/deployed/path/mypack", []byte("not a symlink"), 0644)
	require.NoError(t, err)

	op := &types.Operation{
		Type:    types.OperationCreateSymlink,
		Source:  "/packs/mypack/bin",
		Target:  "/deployed/path/mypack",
		PowerUp: "add_path",
	}

	status, err := checker.CheckStatus(op, fs)
	require.NoError(t, err)
	require.NotNil(t, status)

	assert.Equal(t, types.StatusConflict, status.Status)
	assert.Equal(t, "Deployment path exists but is not a symlink", status.Message)
}

func TestPathChecker_isDirectoryInPath(t *testing.T) {
	checker := &PathChecker{}

	tests := []struct {
		name     string
		dir      string
		pathEnv  string
		expected bool
	}{
		{
			name:     "directory in PATH",
			dir:      "/usr/local/bin",
			pathEnv:  "/usr/bin:/usr/local/bin:/bin",
			expected: true,
		},
		{
			name:     "directory not in PATH",
			dir:      "/opt/bin",
			pathEnv:  "/usr/bin:/usr/local/bin:/bin",
			expected: false,
		},
		{
			name:     "relative directory in PATH",
			dir:      "./bin",
			pathEnv:  "/usr/bin:./bin:/bin",
			expected: false, // Relative paths won't match after abs conversion
		},
		{
			name:     "empty PATH",
			dir:      "/usr/bin",
			pathEnv:  "",
			expected: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := checker.isDirectoryInPath(tt.dir, tt.pathEnv)
			assert.Equal(t, tt.expected, result)
		})
	}
}
