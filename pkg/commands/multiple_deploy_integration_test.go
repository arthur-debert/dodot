package commands

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/synthfs"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	// Import to register triggers and powerups
	_ "github.com/arthur-debert/dodot/pkg/powerups"
	_ "github.com/arthur-debert/dodot/pkg/triggers"
)

// TestMultipleDeployPowerUps_Integration tests a pack with multiple deploy powerups
func TestMultipleDeployPowerUps_Integration(t *testing.T) {
	// Setup test environment
	testEnv := testutil.NewTestEnvironment(t, "multi-deploy-integration")
	defer testEnv.Cleanup()

	// Create vim pack with multiple deploy powerups
	vimPack := testEnv.CreatePack("vim")

	// Add a symlink file (.vimrc)
	vimrcContent := `" My vim configuration
set number
set autoindent
syntax on
`
	testutil.CreateFile(t, vimPack, ".vimrc", vimrcContent)

	// Add a profile file (aliases.sh)
	aliasesContent := `#!/bin/bash
# Vim-related aliases
alias vi='nvim'
alias vim='nvim'
alias vimdiff='nvim -d'
`
	testutil.CreateFile(t, vimPack, "aliases.sh", aliasesContent)

	// Add another symlink file (.bashrc)
	bashrcContent := `# Bash configuration for vim
export EDITOR=nvim
set -o vi
`
	testutil.CreateFile(t, vimPack, ".bashrc", bashrcContent)

	// Deploy the pack
	result, err := DeployPacks(DeployPacksOptions{
		DotfilesRoot: testEnv.DotfilesRoot(),
		PackNames:    []string{"vim"},
		DryRun:       false,
	})
	require.NoError(t, err)
	assert.Len(t, result.Packs, 1)
	assert.NotEmpty(t, result.Operations)

	// Should have operations for:
	// - Symlink .vimrc (2 operations: deploy + user)
	// - Symlink .bashrc (2 operations: deploy + user)
	// - Deploy aliases.sh to shell profile (1 operation)
	symlinkOps := 0
	profileOps := 0
	for _, op := range result.Operations {
		switch {
		case op.Type == "create_symlink" &&
			(filepath.Base(op.Target) == ".vimrc" || filepath.Base(op.Target) == ".bashrc"):
			symlinkOps++
		case op.Type == "create_symlink" &&
			filepath.Dir(op.Target) == testEnv.DataDir()+"/deployed/shell_profile":
			profileOps++
		}
	}

	assert.True(t, symlinkOps >= 4, "Expected at least 4 symlink operations (2 files x 2 ops each)")
	assert.True(t, profileOps >= 1, "Expected at least 1 profile operation")

	// Execute operations
	executor := synthfs.NewSynthfsExecutor(false)
	executor.EnableHomeSymlinks(true)
	_, err = executor.ExecuteOperations(result.Operations)
	require.NoError(t, err)

	// Verify .vimrc symlink was created
	vimrcPath := filepath.Join(testEnv.Home(), ".vimrc")
	info, err := os.Lstat(vimrcPath)
	require.NoError(t, err, "Expected .vimrc symlink to exist")
	assert.True(t, info.Mode()&os.ModeSymlink != 0, "Expected .vimrc to be a symlink")

	// Verify shell profile script was deployed
	shellScript := filepath.Join(testEnv.DataDir(), "deployed", "shell_profile", "vim.sh")
	info, err = os.Lstat(shellScript)
	require.NoError(t, err, "Expected vim.sh profile script to exist")
	assert.True(t, info.Mode()&os.ModeSymlink != 0, "Expected vim.sh to be a symlink")

	// Verify .bashrc symlink was created
	bashrcPath := filepath.Join(testEnv.Home(), ".bashrc")
	info, err = os.Lstat(bashrcPath)
	require.NoError(t, err, "Expected .bashrc symlink to exist")
	assert.True(t, info.Mode()&os.ModeSymlink != 0, "Expected .bashrc to be a symlink")

	// Verify content is accessible through symlinks
	// Read .vimrc content through symlink
	content, err := os.ReadFile(vimrcPath)
	require.NoError(t, err)
	assert.Contains(t, string(content), "set number", "Expected .vimrc content to be accessible")

	// Read .bashrc content through symlink
	bashrcFileContent, err := os.ReadFile(bashrcPath)
	require.NoError(t, err)
	assert.Contains(t, string(bashrcFileContent), "export EDITOR=nvim", "Expected .bashrc content to be accessible")

	// Verify shell profile content through symlink
	shellProfileContent, err := os.ReadFile(shellScript)
	require.NoError(t, err)
	assert.Contains(t, string(shellProfileContent), "alias vi=", "Expected aliases.sh content to be accessible")
}
