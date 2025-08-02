package commands

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/synthfs"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestProfilePowerUp_Integration verifies that the profile powerup deploys shell scripts
func TestProfilePowerUp_Integration(t *testing.T) {
	// Setup test environment
	testEnv := testutil.NewTestEnvironment(t, "profile-integration")
	defer testEnv.Cleanup()

	// Create bash pack
	bashPack := testEnv.CreatePack("bash")

	// Create an aliases script
	aliasContent := `#!/usr/bin/env bash
# Bash aliases
alias ll='ls -la'
alias gs='git status'
`
	testutil.CreateFile(t, bashPack, "aliases.sh", aliasContent)

	// Make it executable
	require.NoError(t, os.Chmod(filepath.Join(bashPack, "aliases.sh"), 0755))

	// Deploy the pack
	result, err := DeployPacks(DeployPacksOptions{
		DotfilesRoot: testEnv.DotfilesRoot(),
		PackNames:    []string{"bash"},
		DryRun:       false,
	})
	require.NoError(t, err)
	assert.Len(t, result.Packs, 1)
	assert.NotEmpty(t, result.Operations)

	// Execute operations
	executor := synthfs.NewSynthfsExecutor(false)
	executor.EnableHomeSymlinks(true)
	err = executor.ExecuteOperations(result.Operations)
	require.NoError(t, err)

	// Verify the shell script was deployed
	shellScript := filepath.Join(testEnv.DataDir(), "deployed", "shell_profile", "bash.sh")

	// Check that the script exists and is a symlink
	info, err := os.Lstat(shellScript)
	require.NoError(t, err, "Expected shell script to exist")
	assert.True(t, info.Mode()&os.ModeSymlink != 0, "Expected shell script to be a symlink")

	// Verify it points to the source
	target, err := os.Readlink(shellScript)
	require.NoError(t, err)
	assert.Contains(t, target, "dotfiles/bash/aliases.sh")

	// Verify we can read the content through the symlink
	content, err := os.ReadFile(shellScript)
	require.NoError(t, err)
	assert.Equal(t, aliasContent, string(content))
}
