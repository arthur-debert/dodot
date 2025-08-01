package dodot

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	// Import to register triggers and powerups
	_ "github.com/arthur-debert/dodot/pkg/powerups"
	_ "github.com/arthur-debert/dodot/pkg/triggers"
)

// TestDeployCommandActuallyExecutesOperations tests that the deploy command
// not only generates operations but actually executes them on the filesystem
func TestDeployCommandActuallyExecutesOperations(t *testing.T) {
	// Create a temporary directory for testing
	tmpDir := t.TempDir()
	dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
	homeDir := filepath.Join(tmpDir, "home")

	// Create directories
	require.NoError(t, os.MkdirAll(filepath.Join(dotfilesRoot, "vim"), 0755))
	require.NoError(t, os.MkdirAll(homeDir, 0755))

	// Create a simple pack with a file to symlink
	packConfig := `
name = "vim"

[[matchers]]
triggers = [
    { type = "FileName", pattern = ".vimrc" }
]
actions = [
    { type = "Symlink" }
]
`
	configPath := filepath.Join(dotfilesRoot, "vim", "pack.dodot.toml")
	require.NoError(t, os.WriteFile(configPath, []byte(packConfig), 0644))

	// Create the file to be symlinked
	vimrcContent := "\" Test vimrc"
	vimrcPath := filepath.Join(dotfilesRoot, "vim", ".vimrc")
	require.NoError(t, os.WriteFile(vimrcPath, []byte(vimrcContent), 0644))

	// Override home directory for the test
	origHome := os.Getenv("HOME")
	require.NoError(t, os.Setenv("HOME", homeDir))
	defer func() { _ = os.Setenv("HOME", origHome) }()

	// Also set DODOT_DATA_DIR to use a test-specific directory
	dataDir := filepath.Join(tmpDir, "dodot-data")
	require.NoError(t, os.Setenv("DODOT_DATA_DIR", dataDir))
	defer func() { _ = os.Unsetenv("DODOT_DATA_DIR") }()

	// Create and execute the deploy command
	rootCmd := NewRootCmd()
	rootCmd.SetArgs([]string{"deploy", "vim"})

	// Set the dotfiles root via environment variable
	require.NoError(t, os.Setenv("DOTFILES_ROOT", dotfilesRoot))
	defer func() { _ = os.Unsetenv("DOTFILES_ROOT") }()

	// Execute the command
	err := rootCmd.Execute()
	require.NoError(t, err)

	// CRITICAL TEST: Verify the symlink was actually created
	expectedSymlink := filepath.Join(homeDir, ".vimrc")

	// Check if the file exists
	info, err := os.Lstat(expectedSymlink)
	require.NoError(t, err, "Expected symlink %s to exist but it doesn't", expectedSymlink)

	// Verify it's a symlink
	assert.True(t, info.Mode()&os.ModeSymlink != 0, "Expected %s to be a symlink", expectedSymlink)

	// Verify the symlink target
	target, err := os.Readlink(expectedSymlink)
	require.NoError(t, err)

	// Debug output
	t.Logf("Expected symlink at: %s", expectedSymlink)
	t.Logf("Symlink target: %s", target)
	t.Logf("Expected target: %s", vimrcPath)

	// dodot uses a double-symlink approach:
	// ~/.vimrc -> deployed/symlink/.vimrc -> actual file
	// So we need to follow the symlink chain
	finalTarget, err := filepath.EvalSymlinks(expectedSymlink)
	require.NoError(t, err)

	// Also resolve the expected path for comparison (handles macOS /var -> /private/var)
	resolvedVimrcPath, err := filepath.EvalSymlinks(vimrcPath)
	require.NoError(t, err)

	assert.Equal(t, resolvedVimrcPath, finalTarget, "Symlink chain should ultimately point to the source file")

	// Verify we can read the file through the symlink
	content, err := os.ReadFile(expectedSymlink)
	require.NoError(t, err)
	assert.Equal(t, vimrcContent, string(content))
}

// TestDeployCommandDryRunDoesNotExecute tests that dry-run mode doesn't create files
func TestDeployCommandDryRunDoesNotExecute(t *testing.T) {
	// Create a temporary directory for testing
	tmpDir := t.TempDir()
	dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
	homeDir := filepath.Join(tmpDir, "home")

	// Create directories
	require.NoError(t, os.MkdirAll(filepath.Join(dotfilesRoot, "vim"), 0755))
	require.NoError(t, os.MkdirAll(homeDir, 0755))

	// Create a simple pack
	packConfig := `
name = "vim"

[[matchers]]
triggers = [
    { type = "FileName", pattern = ".vimrc" }
]
actions = [
    { type = "Symlink" }
]
`
	configPath := filepath.Join(dotfilesRoot, "vim", "pack.dodot.toml")
	require.NoError(t, os.WriteFile(configPath, []byte(packConfig), 0644))

	vimrcPath := filepath.Join(dotfilesRoot, "vim", ".vimrc")
	require.NoError(t, os.WriteFile(vimrcPath, []byte("test"), 0644))

	// Override home directory
	origHome := os.Getenv("HOME")
	require.NoError(t, os.Setenv("HOME", homeDir))
	defer func() { _ = os.Setenv("HOME", origHome) }()

	// Create and execute the deploy command with dry-run
	rootCmd := NewRootCmd()
	rootCmd.SetArgs([]string{"deploy", "vim", "--dry-run"})

	require.NoError(t, os.Setenv("DOTFILES_ROOT", dotfilesRoot))
	defer func() { _ = os.Unsetenv("DOTFILES_ROOT") }()

	// Execute the command
	err := rootCmd.Execute()
	require.NoError(t, err)

	// Verify NO symlink was created
	expectedSymlink := filepath.Join(homeDir, ".vimrc")
	_, err = os.Lstat(expectedSymlink)
	assert.True(t, os.IsNotExist(err), "Expected no symlink in dry-run mode but file exists")
}

// TestCoreDeployPacksReturnsOperations verifies that core.DeployPacks generates operations
// This test should continue to pass - it tests operation generation, not execution
func TestCoreDeployPacksReturnsOperations(t *testing.T) {
	tmpDir := t.TempDir()

	// Set a test-specific data directory to avoid conflicts
	require.NoError(t, os.Setenv("DODOT_DATA_DIR", filepath.Join(tmpDir, "dodot-data")))
	defer func() { _ = os.Unsetenv("DODOT_DATA_DIR") }()

	// Create a mock pack
	packPath := filepath.Join(tmpDir, "test-pack")
	require.NoError(t, os.MkdirAll(packPath, 0755))

	// Create a file that will trigger the symlink
	// Use .vimrc which matches the default matchers
	testFile := filepath.Join(packPath, ".vimrc")
	require.NoError(t, os.WriteFile(testFile, []byte("\" vim config content"), 0644))

	// Call DeployPacks
	opts := core.DeployPacksOptions{
		DotfilesRoot: tmpDir,
		PackNames:    []string{"test-pack"},
		DryRun:       false,
	}

	result, err := core.DeployPacks(opts)
	require.NoError(t, err)

	// Debug: log what we got
	t.Logf("Result operations: %d", len(result.Operations))
	for i, op := range result.Operations {
		t.Logf("Operation %d: Type=%s, Description=%s", i, op.Type, op.Description)
	}

	// Verify operations were generated
	assert.NotEmpty(t, result.Operations)

	// Find the symlink operation
	var foundSymlinkOp bool
	for _, op := range result.Operations {
		if op.Type == types.OperationCreateSymlink {
			foundSymlinkOp = true
			assert.Contains(t, op.Description, ".vimrc")
		}
	}
	assert.True(t, foundSymlinkOp, "Expected to find a symlink operation")
}
