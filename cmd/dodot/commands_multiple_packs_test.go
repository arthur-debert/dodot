package dodot

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/require"

	// Import to register triggers and powerups
	_ "github.com/arthur-debert/dodot/pkg/powerups"
	_ "github.com/arthur-debert/dodot/pkg/triggers"
)

// TestDeployCommandMultiplePacksExecutesOperations tests that the deploy command
// executes operations when deploying multiple packs
func TestDeployCommandMultiplePacksExecutesOperations(t *testing.T) {
	// Create a temporary directory for testing
	tmpDir := t.TempDir()
	dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
	homeDir := filepath.Join(tmpDir, "home")

	// Create directories
	require.NoError(t, os.MkdirAll(filepath.Join(dotfilesRoot, "vim"), 0755))
	require.NoError(t, os.MkdirAll(filepath.Join(dotfilesRoot, "bash"), 0755))
	require.NoError(t, os.MkdirAll(homeDir, 0755))

	// Create pack configs (no pack config needed for default matchers)

	// Create files to be symlinked
	vimrcContent := "\" Test vimrc"
	vimrcPath := filepath.Join(dotfilesRoot, "vim", ".vimrc")
	require.NoError(t, os.WriteFile(vimrcPath, []byte(vimrcContent), 0644))

	bashrcContent := "# Test bashrc"
	bashrcPath := filepath.Join(dotfilesRoot, "bash", ".bashrc")
	require.NoError(t, os.WriteFile(bashrcPath, []byte(bashrcContent), 0644))

	// Override home directory for the test
	origHome := os.Getenv("HOME")
	require.NoError(t, os.Setenv("HOME", homeDir))
	defer func() { _ = os.Setenv("HOME", origHome) }()

	// Also set DODOT_DATA_DIR to use a test-specific directory
	dataDir := filepath.Join(tmpDir, "dodot-data")
	require.NoError(t, os.Setenv("DODOT_DATA_DIR", dataDir))
	defer func() { _ = os.Unsetenv("DODOT_DATA_DIR") }()

	// Create and execute the deploy command with multiple packs
	rootCmd := NewRootCmd()
	rootCmd.SetArgs([]string{"deploy", "vim", "bash"})

	// Set the dotfiles root via environment variable
	require.NoError(t, os.Setenv("DOTFILES_ROOT", dotfilesRoot))
	defer func() { _ = os.Unsetenv("DOTFILES_ROOT") }()

	// Execute the command
	err := rootCmd.Execute()
	require.NoError(t, err)

	// CRITICAL TEST: Verify BOTH symlinks were actually created
	expectedVimrc := filepath.Join(homeDir, ".vimrc")
	expectedBashrc := filepath.Join(homeDir, ".bashrc")

	// Check if .vimrc exists
	vimrcInfo, err := os.Lstat(expectedVimrc)
	require.NoError(t, err, "Expected symlink %s to exist but it doesn't", expectedVimrc)
	require.True(t, vimrcInfo.Mode()&os.ModeSymlink != 0, "Expected %s to be a symlink", expectedVimrc)

	// Check if .bashrc exists
	bashrcInfo, err := os.Lstat(expectedBashrc)
	require.NoError(t, err, "Expected symlink %s to exist but it doesn't", expectedBashrc)
	require.True(t, bashrcInfo.Mode()&os.ModeSymlink != 0, "Expected %s to be a symlink", expectedBashrc)

	// Verify we can read the files through the symlinks
	vimrcReadContent, err := os.ReadFile(expectedVimrc)
	require.NoError(t, err)
	require.Equal(t, vimrcContent, string(vimrcReadContent))

	bashrcReadContent, err := os.ReadFile(expectedBashrc)
	require.NoError(t, err)
	require.Equal(t, bashrcContent, string(bashrcReadContent))
}