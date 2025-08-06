package homebrew_test

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/commands"
	"github.com/arthur-debert/dodot/pkg/synthfs"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	// Import to register triggers and powerups
	_ "github.com/arthur-debert/dodot/pkg/powerups"
	_ "github.com/arthur-debert/dodot/pkg/triggers"
)

// TestBrewfilePowerUp_Integration verifies that the brewfile powerup creates sentinel files
func TestBrewfilePowerUp_Integration(t *testing.T) {
	// Setup test environment
	testEnv := testutil.NewTestEnvironment(t, "brewfile-integration")
	defer testEnv.Cleanup()

	// Create tools pack
	toolsPack := testEnv.CreatePack("tools")

	// Create a Brewfile
	brewfileContent := `# Development tools
brew 'git'
brew 'tmux'
brew 'neovim'
cask 'visual-studio-code'
`
	testutil.CreateFile(t, toolsPack, "Brewfile", brewfileContent)

	// First install should create operations
	result, err := commands.InstallPacks(commands.InstallPacksOptions{
		DotfilesRoot: testEnv.DotfilesRoot(),
		PackNames:    []string{"tools"},
		DryRun:       false,
		Force:        false,
	})
	require.NoError(t, err)
	assert.Len(t, result.Packs, 1)
	assert.NotEmpty(t, result.Operations)

	// Should have operations for creating sentinel file
	hasBrewOps := false
	for _, op := range result.Operations {
		if op.Type == "write_file" && filepath.Base(op.Target) == "tools" {
			hasBrewOps = true
			break
		}
	}
	assert.True(t, hasBrewOps, "Expected Brewfile operations")

	// Execute operations
	executor := synthfs.NewSynthfsExecutor(false)
	executor.EnableHomeSymlinks(true)
	_, err = executor.ExecuteOperations(result.Operations)
	require.NoError(t, err)

	// Verify sentinel file was created
	sentinelPath := filepath.Join(testEnv.DataDir(), "homebrew", "tools")
	info, err := os.Stat(sentinelPath)
	require.NoError(t, err, "Expected Brewfile sentinel to exist")
	assert.True(t, info.Mode().IsRegular(), "Expected sentinel to be a regular file")

	// Read sentinel content (should be checksum)
	content, err := os.ReadFile(sentinelPath)
	require.NoError(t, err)
	assert.NotEmpty(t, string(content), "Expected sentinel to contain checksum")

	// Second install should not generate Brewfile operations (already installed)
	result2, err := commands.InstallPacks(commands.InstallPacksOptions{
		DotfilesRoot: testEnv.DotfilesRoot(),
		PackNames:    []string{"tools"},
		DryRun:       false,
		Force:        false,
	})
	require.NoError(t, err)

	// Should not have any Brewfile operations (only checksum should run)
	hasBrewOps2 := false
	for _, op := range result2.Operations {
		if op.Type == "write_file" && filepath.Base(op.Target) == "tools" {
			hasBrewOps2 = true
			break
		}
	}
	assert.False(t, hasBrewOps2, "Should not have Brewfile operations on second run")
}
