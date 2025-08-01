package core

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	// Import to register triggers and powerups
	_ "github.com/arthur-debert/dodot/pkg/powerups"
	_ "github.com/arthur-debert/dodot/pkg/triggers"
)

// TestMultipleInstallPowerUps_Integration tests a pack with multiple install powerups
func TestMultipleInstallPowerUps_Integration(t *testing.T) {
	// Setup test environment
	testEnv := testutil.NewTestEnvironment(t, "multi-install-integration")
	defer testEnv.Cleanup()

	// Create tools pack with multiple install powerups
	toolsPack := testEnv.CreatePack("tools")

	// Add a Brewfile
	brewfileContent := `# Development tools
brew 'git'
brew 'gh'
brew 'tmux'
brew 'ripgrep'
cask 'docker'
`
	testutil.CreateFile(t, toolsPack, "Brewfile", brewfileContent)

	// Add an install.sh script
	installContent := `#!/bin/bash
# Tools installation script
set -euo pipefail

echo "Installing additional tools..."

# Create a marker file to verify the script ran
echo "tools-install-complete" > "$HOME/.tools-setup-marker"

# Install some additional configuration
mkdir -p "$HOME/.config/tools"
echo "configured=true" > "$HOME/.config/tools/config.ini"

echo "Tools installation complete!"
`
	testutil.CreateFile(t, toolsPack, "install.sh", installContent)

	// Make the script executable
	installPath := filepath.Join(toolsPack, "install.sh")
	err := os.Chmod(installPath, 0755)
	require.NoError(t, err)

	// Install the pack - this runs install powerups (RunModeOnce)
	result, err := InstallPacks(InstallPacksOptions{
		DotfilesRoot: testEnv.DotfilesRoot(),
		PackNames:    []string{"tools"},
		DryRun:       false,
		Force:        false,
	})
	require.NoError(t, err)
	assert.Len(t, result.Packs, 1)
	assert.NotEmpty(t, result.Operations)

	// Should have operations for both Brewfile and install script sentinel files
	brewOps := 0
	installOps := 0
	for _, op := range result.Operations {
		switch {
		case op.Type == "write_file" && filepath.Base(op.Target) == "tools" &&
			filepath.Dir(op.Target) == testEnv.DataDir()+"/brewfile":
			brewOps++
		case op.Type == "write_file" && filepath.Base(op.Target) == "tools" &&
			filepath.Dir(op.Target) == testEnv.DataDir()+"/install":
			installOps++
		}
	}

	assert.True(t, brewOps >= 1, "Expected at least 1 Brewfile operation")
	assert.True(t, installOps >= 1, "Expected at least 1 install script operation")

	// Execute operations
	executor := NewSynthfsExecutor(false)
	executor.EnableHomeSymlinks(true)
	err = executor.ExecuteOperations(result.Operations)
	require.NoError(t, err)

	// Verify Brewfile sentinel file was created
	brewSentinel := filepath.Join(testEnv.DataDir(), "brewfile", "tools")
	info, err := os.Stat(brewSentinel)
	require.NoError(t, err, "Expected Brewfile sentinel to exist")
	assert.True(t, info.Mode().IsRegular(), "Expected Brewfile sentinel to be a regular file")

	// Read Brewfile sentinel content (should be checksum)
	brewContent, err := os.ReadFile(brewSentinel)
	require.NoError(t, err)
	assert.NotEmpty(t, string(brewContent), "Expected Brewfile sentinel to contain checksum")

	// Verify install script sentinel file was created
	installSentinel := filepath.Join(testEnv.DataDir(), "install", "tools")
	info, err = os.Stat(installSentinel)
	require.NoError(t, err, "Expected install script sentinel to exist")
	assert.True(t, info.Mode().IsRegular(), "Expected install script sentinel to be a regular file")

	// Read install script sentinel content (should be checksum)
	installSentinelContent, err := os.ReadFile(installSentinel)
	require.NoError(t, err)
	assert.NotEmpty(t, string(installSentinelContent), "Expected install script sentinel to contain checksum")

	// Verify the checksums are different (different file contents)
	assert.NotEqual(t, string(brewContent), string(installSentinelContent),
		"Brewfile and install script should have different checksums")

	// Second install should not generate install operations (already installed)
	result2, err := InstallPacks(InstallPacksOptions{
		DotfilesRoot: testEnv.DotfilesRoot(),
		PackNames:    []string{"tools"},
		DryRun:       false,
		Force:        false,
	})
	require.NoError(t, err)

	// Should not have any install operations (only checksum should run)
	brewOps2 := 0
	installOps2 := 0
	for _, op := range result2.Operations {
		switch {
		case op.Type == "write_file" && filepath.Base(op.Target) == "tools" &&
			filepath.Dir(op.Target) == testEnv.DataDir()+"/brewfile":
			brewOps2++
		case op.Type == "write_file" && filepath.Base(op.Target) == "tools" &&
			filepath.Dir(op.Target) == testEnv.DataDir()+"/install":
			installOps2++
		}
	}

	assert.False(t, brewOps2 > 0, "Should not have Brewfile operations on second run")
	assert.False(t, installOps2 > 0, "Should not have install operations on second run")
}
