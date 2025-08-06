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

// TestMultiplePacksWithMultiplePowerUps_Integration tests multiple packs with different powerup combinations
func TestMultiplePacksWithMultiplePowerUps_Integration(t *testing.T) {
	// Setup test environment
	testEnv := testutil.NewTestEnvironment(t, "multi-packs-integration")
	defer testEnv.Cleanup()

	// ===== CREATE PACK 1: VIM (Deploy powerups only) =====
	vimPack := testEnv.CreatePack("vim")

	vimrcContent := `" Vim configuration
set number
set syntax=on
set autoindent
`
	testutil.CreateFile(t, vimPack, ".vimrc", vimrcContent)

	vimAliasesContent := `#!/bin/bash
# Vim aliases
alias vi='vim'
alias vim='nvim'
`
	testutil.CreateFile(t, vimPack, "aliases.sh", vimAliasesContent)

	// ===== CREATE PACK 2: DEV (Install powerups only) =====
	devPack := testEnv.CreatePack("dev")

	devBrewfileContent := `# Development tools
brew 'git'
brew 'fzf'
brew 'ripgrep'
`
	testutil.CreateFile(t, devPack, "Brewfile", devBrewfileContent)

	devInstallContent := `#!/bin/bash
# Development setup
echo "dev-setup-complete" > "$HOME/.dev-marker"
`
	testutil.CreateFile(t, devPack, "install.sh", devInstallContent)

	// Make install script executable
	devInstallPath := filepath.Join(devPack, "install.sh")
	err := os.Chmod(devInstallPath, 0755)
	require.NoError(t, err)

	// ===== CREATE PACK 3: SHELL (Mixed install + deploy powerups) =====
	shellPack := testEnv.CreatePack("shell")

	// Deploy powerup - symlink
	bashrcContent := `# Bash configuration
export PS1='\\u@\\h:\\w\\$ '
export EDITOR=vim
`
	testutil.CreateFile(t, shellPack, ".bashrc", bashrcContent)

	// Deploy powerup - profile
	shellAliasesContent := `#!/bin/bash
# Shell aliases
alias ll='ls -la'
alias grep='grep --color=auto'
`
	testutil.CreateFile(t, shellPack, "aliases.sh", shellAliasesContent)

	// Install powerup - Brewfile
	shellBrewfileContent := `# Shell tools
brew 'zsh'
brew 'bash-completion'
`
	testutil.CreateFile(t, shellPack, "Brewfile", shellBrewfileContent)

	// ===== TEST INSTALL ALL PACKS =====

	// Run InstallPacks for all packs
	result, err := InstallPacks(InstallPacksOptions{
		DotfilesRoot: testEnv.DotfilesRoot(),
		PackNames:    []string{"vim", "dev", "shell"}, // Explicit pack list
		DryRun:       false,
		Force:        false,
	})
	require.NoError(t, err)
	assert.Len(t, result.Packs, 3) // Should have processed all 3 packs
	assert.NotEmpty(t, result.Operations)

	// Count operations by type and pack
	symlinkOps := make(map[string]int)  // pack -> count
	profileOps := make(map[string]int)  // pack -> count
	brewfileOps := make(map[string]int) // pack -> count
	installOps := make(map[string]int)  // pack -> count

	for _, op := range result.Operations {
		switch {
		case op.Type == "create_symlink" &&
			filepath.Dir(op.Target) == testEnv.DataDir()+"/deployed/shell_profile":
			// Determine pack from target filename
			targetFile := filepath.Base(op.Target)
			switch targetFile {
			case "vim.sh":
				profileOps["vim"]++
			case "shell.sh":
				profileOps["shell"]++
			}
		case op.Type == "create_symlink":
			if filepath.Base(op.Target) == ".vimrc" {
				symlinkOps["vim"]++
			} else if filepath.Base(op.Target) == ".bashrc" {
				symlinkOps["shell"]++
			}
		case op.Type == "write_file" &&
			filepath.Dir(op.Target) == testEnv.DataDir()+"/homebrew":
			packName := filepath.Base(op.Target)
			brewfileOps[packName]++
		case op.Type == "write_file" &&
			filepath.Dir(op.Target) == testEnv.DataDir()+"/install":
			packName := filepath.Base(op.Target)
			installOps[packName]++
		}
	}

	// Verify expected operations per pack
	assert.True(t, symlinkOps["vim"] >= 2, "Expected vim pack to have symlink operations")
	assert.True(t, symlinkOps["shell"] >= 2, "Expected shell pack to have symlink operations")
	assert.True(t, profileOps["vim"] >= 1, "Expected vim pack to have profile operations")
	assert.True(t, profileOps["shell"] >= 1, "Expected shell pack to have profile operations")
	assert.True(t, brewfileOps["dev"] >= 1, "Expected dev pack to have Brewfile operations")
	assert.True(t, brewfileOps["shell"] >= 1, "Expected shell pack to have Brewfile operations")
	assert.True(t, installOps["dev"] >= 1, "Expected dev pack to have install operations")

	// Execute operations
	executor := synthfs.NewSynthfsExecutor(false)
	executor.EnableHomeSymlinks(true)
	_, err = executor.ExecuteOperations(result.Operations)
	require.NoError(t, err)

	// ===== VERIFY PACK 1 (VIM) - Deploy powerups =====

	// Check symlink
	vimrcPath := filepath.Join(testEnv.Home(), ".vimrc")
	info, err := os.Lstat(vimrcPath)
	require.NoError(t, err, "Expected .vimrc symlink to exist")
	assert.True(t, info.Mode()&os.ModeSymlink != 0, "Expected .vimrc to be a symlink")

	// Check profile
	vimProfilePath := filepath.Join(testEnv.DataDir(), "deployed", "shell_profile", "vim.sh")
	info, err = os.Lstat(vimProfilePath)
	require.NoError(t, err, "Expected vim profile to exist")
	assert.True(t, info.Mode()&os.ModeSymlink != 0, "Expected vim profile to be a symlink")

	// ===== VERIFY PACK 2 (DEV) - Install powerups =====

	// Check Brewfile sentinel
	devBrewSentinel := filepath.Join(testEnv.DataDir(), "homebrew", "dev")
	info, err = os.Stat(devBrewSentinel)
	require.NoError(t, err, "Expected dev Brewfile sentinel to exist")
	assert.True(t, info.Mode().IsRegular(), "Expected dev Brewfile sentinel to be a regular file")

	// Check install script sentinel
	devInstallSentinel := filepath.Join(testEnv.DataDir(), "install", "dev")
	info, err = os.Stat(devInstallSentinel)
	require.NoError(t, err, "Expected dev install sentinel to exist")
	assert.True(t, info.Mode().IsRegular(), "Expected dev install sentinel to be a regular file")

	// ===== VERIFY PACK 3 (SHELL) - Mixed powerups =====

	// Check deploy - symlink
	bashrcPath := filepath.Join(testEnv.Home(), ".bashrc")
	info, err = os.Lstat(bashrcPath)
	require.NoError(t, err, "Expected .bashrc symlink to exist")
	assert.True(t, info.Mode()&os.ModeSymlink != 0, "Expected .bashrc to be a symlink")

	// Check deploy - profile
	shellProfilePath := filepath.Join(testEnv.DataDir(), "deployed", "shell_profile", "shell.sh")
	info, err = os.Lstat(shellProfilePath)
	require.NoError(t, err, "Expected shell profile to exist")
	assert.True(t, info.Mode()&os.ModeSymlink != 0, "Expected shell profile to be a symlink")

	// Check install - Brewfile sentinel
	shellBrewSentinel := filepath.Join(testEnv.DataDir(), "homebrew", "shell")
	info, err = os.Stat(shellBrewSentinel)
	require.NoError(t, err, "Expected shell Brewfile sentinel to exist")
	assert.True(t, info.Mode().IsRegular(), "Expected shell Brewfile sentinel to be a regular file")

	// ===== VERIFY CONTENT ACCESSIBLE =====

	// Check vim content
	vimrcFileContent, err := os.ReadFile(vimrcPath)
	require.NoError(t, err)
	assert.Contains(t, string(vimrcFileContent), "set number", "Expected .vimrc content to be accessible")

	vimProfileContent, err := os.ReadFile(vimProfilePath)
	require.NoError(t, err)
	assert.Contains(t, string(vimProfileContent), "alias vi='vim'", "Expected vim profile content to be accessible")

	// Check shell content
	bashrcFileContent, err := os.ReadFile(bashrcPath)
	require.NoError(t, err)
	assert.Contains(t, string(bashrcFileContent), "export EDITOR=vim", "Expected .bashrc content to be accessible")

	shellProfileContent, err := os.ReadFile(shellProfilePath)
	require.NoError(t, err)
	assert.Contains(t, string(shellProfileContent), "alias ll='ls -la'", "Expected shell profile content to be accessible")

	// ===== TEST PACK ISOLATION =====

	// Verify that each pack's sentinels have different checksums
	devBrewChecksum, err := os.ReadFile(devBrewSentinel)
	require.NoError(t, err)

	shellBrewChecksum, err := os.ReadFile(shellBrewSentinel)
	require.NoError(t, err)

	devInstallChecksum, err := os.ReadFile(devInstallSentinel)
	require.NoError(t, err)

	assert.NotEqual(t, string(devBrewChecksum), string(shellBrewChecksum),
		"Different packs should have different Brewfile checksums")
	assert.NotEqual(t, string(devBrewChecksum), string(devInstallChecksum),
		"Different files should have different checksums")

	// ===== TEST IDEMPOTENCY ACROSS PACKS =====

	// Second install should only run deploy powerups, not install powerups
	result2, err := InstallPacks(InstallPacksOptions{
		DotfilesRoot: testEnv.DotfilesRoot(),
		PackNames:    []string{"vim", "dev", "shell"},
		DryRun:       false,
		Force:        false,
	})
	require.NoError(t, err)

	// Count second run operations
	brewfileOps2 := 0
	installOps2 := 0
	symlinkOps2 := 0
	profileOps2 := 0

	for _, op := range result2.Operations {
		switch {
		case op.Type == "write_file" &&
			filepath.Dir(op.Target) == testEnv.DataDir()+"/homebrew":
			brewfileOps2++
		case op.Type == "write_file" &&
			filepath.Dir(op.Target) == testEnv.DataDir()+"/install":
			installOps2++
		case op.Type == "create_symlink" &&
			(filepath.Base(op.Target) == ".vimrc" || filepath.Base(op.Target) == ".bashrc"):
			symlinkOps2++
		case op.Type == "create_symlink" &&
			filepath.Dir(op.Target) == testEnv.DataDir()+"/deployed/shell_profile":
			profileOps2++
		}
	}

	// Install powerups should not run again
	assert.Equal(t, 0, brewfileOps2, "Should not have Brewfile operations on second run")
	assert.Equal(t, 0, installOps2, "Should not have install operations on second run")

	// Deploy powerups should still run (RunModeMany)
	assert.True(t, symlinkOps2 >= 2, "Should still have symlink operations on second run")
	assert.True(t, profileOps2 >= 2, "Should still have profile operations on second run")
}
