package core

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestSymlinkPowerUp_Integration verifies that the symlink powerup creates actual symlinks
func TestSymlinkPowerUp_Integration(t *testing.T) {
	// Setup test environment
	testEnv := testutil.NewTestEnvironment(t, "symlink-integration")
	defer testEnv.Cleanup()

	// Create vim pack
	vimPack := testEnv.CreatePack("vim")

	// Create a .vimrc file in the pack
	vimrcContent := "\" Test vimrc\nset number"
	testutil.CreateFile(t, vimPack, ".vimrc", vimrcContent)

	// Deploy the pack
	result, err := DeployPacks(DeployPacksOptions{
		DotfilesRoot: testEnv.DotfilesRoot(),
		PackNames:    []string{"vim"},
		DryRun:       false,
	})
	require.NoError(t, err)
	assert.Len(t, result.Packs, 1)
	assert.NotEmpty(t, result.Operations)

	// Execute operations (this is what the CLI does)
	executor := NewSynthfsExecutor(false)
	executor.EnableHomeSymlinks(true)
	err = executor.ExecuteOperations(result.Operations)
	require.NoError(t, err)

	// Verify the symlink was created in home directory
	homeVimrc := filepath.Join(testEnv.Home(), ".vimrc")

	// Check that ~/.vimrc exists and is a symlink
	info, err := os.Lstat(homeVimrc)
	require.NoError(t, err, "Expected ~/.vimrc to exist")
	assert.True(t, info.Mode()&os.ModeSymlink != 0, "Expected ~/.vimrc to be a symlink")

	// Verify we can read the content through the symlink
	content, err := os.ReadFile(homeVimrc)
	require.NoError(t, err)
	assert.Equal(t, vimrcContent, string(content))

	// Verify the double-symlink structure
	// ~/.vimrc should point to deployed/symlink/.vimrc
	firstTarget, err := os.Readlink(homeVimrc)
	require.NoError(t, err)
	assert.Contains(t, firstTarget, "deployed/symlink/.vimrc")

	// The deployed symlink should point to the source
	deployedSymlink := filepath.Join(testEnv.DataDir(), "deployed", "symlink", ".vimrc")
	secondTarget, err := os.Readlink(deployedSymlink)
	require.NoError(t, err)
	assert.Contains(t, secondTarget, "dotfiles/vim/.vimrc")
}
