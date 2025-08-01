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

// TestFullInstallDeployWorkflow_Integration tests complete install+deploy flow for single pack
func TestFullInstallDeployWorkflow_Integration(t *testing.T) {
	// Setup test environment
	testEnv := testutil.NewTestEnvironment(t, "full-workflow-integration")
	defer testEnv.Cleanup()

	// Create development pack with both install and deploy powerups
	devPack := testEnv.CreatePack("development")

	// ===== INSTALL POWERUPS =====
	
	// Add a Brewfile for install
	brewfileContent := `# Development environment
brew 'git'
brew 'neovim'
brew 'tmux'
brew 'fzf'
`
	testutil.CreateFile(t, devPack, "Brewfile", brewfileContent)

	// Add an install.sh script
	installContent := `#!/bin/bash
# Development environment setup
set -euo pipefail

echo "Setting up development environment..."

# Create development directories
mkdir -p "$HOME/.config/dev"
echo "dev_configured=true" > "$HOME/.config/dev/setup.conf"

# Create a completion marker
echo "dev-install-complete" > "$HOME/.dev-install-marker"

echo "Development environment setup complete!"
`
	testutil.CreateFile(t, devPack, "install.sh", installContent)
	
	// Make the script executable
	installPath := filepath.Join(devPack, "install.sh")
	err := os.Chmod(installPath, 0755)
	require.NoError(t, err)

	// ===== DEPLOY POWERUPS =====
	
	// Add symlink files
	gitconfigContent := `[user]
	name = Test User
	email = test@example.com
[core]
	editor = nvim
`
	testutil.CreateFile(t, devPack, ".gitconfig", gitconfigContent)

	tmuxConfContent := `# Tmux configuration
set -g prefix C-a
bind-key C-a send-prefix
set -g mouse on
`
	testutil.CreateFile(t, devPack, ".tmux.conf", tmuxConfContent)

	// Add a profile file
	aliasesContent := `#!/bin/bash
# Development aliases
alias g='git'
alias gs='git status'
alias gd='git diff'
alias vim='nvim'
`
	testutil.CreateFile(t, devPack, "aliases.sh", aliasesContent)

	// ===== TEST FULL INSTALL FLOW =====
	
	// Run InstallPacks (should run install powerups first, then deploy powerups)
	result, err := InstallPacks(InstallPacksOptions{
		DotfilesRoot: testEnv.DotfilesRoot(),
		PackNames:    []string{"development"},
		DryRun:       false,
		Force:        false,
	})
	require.NoError(t, err)
	assert.Len(t, result.Packs, 1)
	assert.NotEmpty(t, result.Operations)

	// Should have operations for all powerups:
	// Install: Brewfile sentinel, install script sentinel
	// Deploy: .gitconfig symlinks (2), .tmux.conf symlinks (2), aliases.sh profile (1)
	brewOps := 0
	installOps := 0
	symlinkOps := 0
	profileOps := 0
	
	for _, op := range result.Operations {
		switch {
		case op.Type == "write_file" && filepath.Base(op.Target) == "development" && 
			 filepath.Dir(op.Target) == testEnv.DataDir()+"/brewfile":
			brewOps++
		case op.Type == "write_file" && filepath.Base(op.Target) == "development" &&
			 filepath.Dir(op.Target) == testEnv.DataDir()+"/install":
			installOps++
		case op.Type == "create_symlink" && 
			 (filepath.Base(op.Target) == ".gitconfig" || filepath.Base(op.Target) == ".tmux.conf"):
			symlinkOps++
		case op.Type == "create_symlink" && 
			 filepath.Dir(op.Target) == testEnv.DataDir()+"/deployed/shell_profile":
			profileOps++
		}
	}

	assert.True(t, brewOps >= 1, "Expected at least 1 Brewfile operation")
	assert.True(t, installOps >= 1, "Expected at least 1 install script operation")
	assert.True(t, symlinkOps >= 4, "Expected at least 4 symlink operations (2 files x 2 ops each)")
	assert.True(t, profileOps >= 1, "Expected at least 1 profile operation")

	// Execute operations
	executor := NewSynthfsExecutor(false)
	executor.EnableHomeSymlinks(true)
	err = executor.ExecuteOperations(result.Operations)
	require.NoError(t, err)

	// ===== VERIFY INSTALL POWERUPS EXECUTED =====
	
	// Verify Brewfile sentinel
	brewSentinel := filepath.Join(testEnv.DataDir(), "brewfile", "development")
	info, err := os.Stat(brewSentinel)
	require.NoError(t, err, "Expected Brewfile sentinel to exist")
	assert.True(t, info.Mode().IsRegular(), "Expected Brewfile sentinel to be a regular file")

	// Verify install script sentinel
	installSentinel := filepath.Join(testEnv.DataDir(), "install", "development")
	info, err = os.Stat(installSentinel)
	require.NoError(t, err, "Expected install script sentinel to exist")
	assert.True(t, info.Mode().IsRegular(), "Expected install script sentinel to be a regular file")

	// ===== VERIFY DEPLOY POWERUPS EXECUTED =====
	
	// Verify .gitconfig symlink
	gitconfigPath := filepath.Join(testEnv.Home(), ".gitconfig")
	info, err = os.Lstat(gitconfigPath)
	require.NoError(t, err, "Expected .gitconfig symlink to exist")
	assert.True(t, info.Mode()&os.ModeSymlink != 0, "Expected .gitconfig to be a symlink")

	// Verify .tmux.conf symlink
	tmuxConfPath := filepath.Join(testEnv.Home(), ".tmux.conf")
	info, err = os.Lstat(tmuxConfPath)
	require.NoError(t, err, "Expected .tmux.conf symlink to exist")
	assert.True(t, info.Mode()&os.ModeSymlink != 0, "Expected .tmux.conf to be a symlink")

	// Verify shell profile script was deployed
	shellScript := filepath.Join(testEnv.DataDir(), "deployed", "shell_profile", "development.sh")
	info, err = os.Lstat(shellScript)
	require.NoError(t, err, "Expected development.sh profile script to exist")
	assert.True(t, info.Mode()&os.ModeSymlink != 0, "Expected development.sh to be a symlink")

	// ===== VERIFY CONTENT ACCESSIBLE =====
	
	// Verify symlinked file content
	gitconfigFileContent, err := os.ReadFile(gitconfigPath)
	require.NoError(t, err)
	assert.Contains(t, string(gitconfigFileContent), "editor = nvim", "Expected .gitconfig content to be accessible")

	tmuxConfFileContent, err := os.ReadFile(tmuxConfPath)
	require.NoError(t, err)
	assert.Contains(t, string(tmuxConfFileContent), "set -g prefix C-a", "Expected .tmux.conf content to be accessible")

	// Verify shell profile content
	shellProfileContent, err := os.ReadFile(shellScript)
	require.NoError(t, err)
	assert.Contains(t, string(shellProfileContent), "alias g='git'", "Expected aliases.sh content to be accessible")

	// ===== TEST IDEMPOTENCY =====
	
	// Second install should not run install powerups again, but should run deploy powerups
	result2, err := InstallPacks(InstallPacksOptions{
		DotfilesRoot: testEnv.DotfilesRoot(),
		PackNames:    []string{"development"},
		DryRun:       false,
		Force:        false,
	})
	require.NoError(t, err)

	// Count operations in second run
	brewOps2 := 0
	installOps2 := 0
	symlinkOps2 := 0
	profileOps2 := 0
	
	for _, op := range result2.Operations {
		switch {
		case op.Type == "write_file" && filepath.Base(op.Target) == "development" && 
			 filepath.Dir(op.Target) == testEnv.DataDir()+"/brewfile":
			brewOps2++
		case op.Type == "write_file" && filepath.Base(op.Target) == "development" &&
			 filepath.Dir(op.Target) == testEnv.DataDir()+"/install":
			installOps2++
		case op.Type == "create_symlink" && 
			 (filepath.Base(op.Target) == ".gitconfig" || filepath.Base(op.Target) == ".tmux.conf"):
			symlinkOps2++
		case op.Type == "create_symlink" && 
			 filepath.Dir(op.Target) == testEnv.DataDir()+"/deployed/shell_profile":
			profileOps2++
		}
	}

	// Install powerups should not run again
	assert.False(t, brewOps2 > 0, "Should not have Brewfile operations on second run")
	assert.False(t, installOps2 > 0, "Should not have install operations on second run")
	
	// Deploy powerups should still run (RunModeMany)
	assert.True(t, symlinkOps2 >= 4, "Should still have symlink operations on second run")
	assert.True(t, profileOps2 >= 1, "Should still have profile operations on second run")
}