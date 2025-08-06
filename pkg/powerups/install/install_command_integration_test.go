package install_test

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

// TestInstallScriptPowerUp_Integration verifies that the install script powerup creates sentinel files
// WARNING: This test has a false positive - it only verifies sentinel creation, not script execution!
// The test uses SynthfsExecutor which cannot execute commands, so the install script never runs.
// See TestInstallScriptActuallyExecutes for a proper test that verifies actual execution.
func TestInstallScriptPowerUp_Integration(t *testing.T) {
	// Setup test environment
	testEnv := testutil.NewTestEnvironment(t, "install-integration")
	defer testEnv.Cleanup()

	// Create dev pack
	devPack := testEnv.CreatePack("dev")

	// Create an install.sh script
	installContent := `#!/bin/bash
# Development environment setup
set -eou pipefail

echo "Setting up development environment..."

# Create a test file to verify the script ran
echo "dev-setup-complete" > "$HOME/.dev-setup-marker"

echo "Development setup complete!"
`
	testutil.CreateFile(t, devPack, "install.sh", installContent)

	// Make the script executable
	installPath := filepath.Join(devPack, "install.sh")
	err := os.Chmod(installPath, 0755)
	require.NoError(t, err)

	// First install should create operations
	result, err := commands.InstallPacks(commands.InstallPacksOptions{
		DotfilesRoot: testEnv.DotfilesRoot(),
		PackNames:    []string{"dev"},
		DryRun:       false,
		Force:        false,
	})
	require.NoError(t, err)
	assert.Len(t, result.Packs, 1)
	assert.NotEmpty(t, result.Operations)

	// Should have operations for creating sentinel file
	hasInstallOps := false
	for _, op := range result.Operations {
		if op.Type == "write_file" && filepath.Base(op.Target) == "dev" {
			hasInstallOps = true
			break
		}
	}
	assert.True(t, hasInstallOps, "Expected install script operations")

	// Execute operations
	// BUG: Using SynthfsExecutor which CANNOT execute commands!
	// This only creates directories and writes files, but never runs the install script
	executor := synthfs.NewSynthfsExecutor(false)
	executor.EnableHomeSymlinks(true)
	_, err = executor.ExecuteOperations(result.Operations)
	require.NoError(t, err)

	// Verify sentinel file was created
	sentinelPath := filepath.Join(testEnv.DataDir(), "install", "dev")
	info, err := os.Stat(sentinelPath)
	require.NoError(t, err, "Expected install script sentinel to exist")
	assert.True(t, info.Mode().IsRegular(), "Expected sentinel to be a regular file")

	// Read sentinel content (should be checksum)
	content, err := os.ReadFile(sentinelPath)
	require.NoError(t, err)
	assert.NotEmpty(t, string(content), "Expected sentinel to contain checksum")

	// Second install should not generate install operations (already installed)
	result2, err := commands.InstallPacks(commands.InstallPacksOptions{
		DotfilesRoot: testEnv.DotfilesRoot(),
		PackNames:    []string{"dev"},
		DryRun:       false,
		Force:        false,
	})
	require.NoError(t, err)

	// Should not have any install operations (only checksum should run)
	hasInstallOps2 := false
	for _, op := range result2.Operations {
		if op.Type == "write_file" && filepath.Base(op.Target) == "dev" {
			hasInstallOps2 = true
			break
		}
	}
	assert.False(t, hasInstallOps2, "Should not have install operations on second run")
}
