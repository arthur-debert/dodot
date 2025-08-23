package link

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestLinkPacks_SymlinkHandler(t *testing.T) {
	// Create test environment
	tempDir := testutil.TempDir(t, "deploy-symlink")
	dotfilesDir := filepath.Join(tempDir, "dotfiles")
	homeDir := filepath.Join(tempDir, "home")

	testutil.CreateDir(t, tempDir, "dotfiles")
	testutil.CreateDir(t, tempDir, "home")
	testutil.CreateDir(t, homeDir, ".local/share/dodot")

	t.Setenv("HOME", homeDir)
	t.Setenv("DOTFILES_ROOT", dotfilesDir)
	t.Setenv("DODOT_DATA_DIR", filepath.Join(homeDir, ".local", "share", "dodot"))

	// Create a test pack with files that should be symlinked
	testutil.CreateDir(t, dotfilesDir, "vim")
	testutil.CreateFile(t, dotfilesDir, "vim/vimrc", "\" Test vimrc configuration")
	testutil.CreateFile(t, dotfilesDir, "vim/gvimrc", "\" Test gvimrc configuration")

	// Deploy the vim pack
	ctx, err := LinkPacks(LinkPacksOptions{
		DotfilesRoot:       dotfilesDir,
		PackNames:          []string{"vim"},
		DryRun:             false,
		EnableHomeSymlinks: true,
	})

	// Verify no errors
	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, ctx)

	// Verify execution context
	testutil.AssertEqual(t, "link", ctx.Command)
	testutil.AssertFalse(t, ctx.DryRun, "Should not be dry run")

	// Verify pack results
	packResult, ok := ctx.GetPackResult("vim")
	testutil.AssertTrue(t, ok, "Should have vim pack result")
	testutil.AssertNotNil(t, packResult)
	testutil.AssertEqual(t, "vim", packResult.Pack.Name)
	testutil.AssertEqual(t, types.ExecutionStatusSuccess, packResult.Status)

	// Should have symlink handler results
	testutil.AssertTrue(t, len(packResult.HandlerResults) > 0, "Should have handler results")

	// Find handler result (actions use generic "handler" name)
	var handlerResult *types.HandlerResult
	for _, pur := range packResult.HandlerResults {
		if pur.HandlerName == "handler" {
			handlerResult = pur
			break
		}
	}
	testutil.AssertNotNil(t, handlerResult, "Should have handler result")
	testutil.AssertEqual(t, types.StatusReady, handlerResult.Status)

	// Verify actual symlinks were created (Layer 1: top-level files get dot prefix)
	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(homeDir, ".vimrc")), "vimrc symlink should exist")
	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(homeDir, ".gvimrc")), "gvimrc symlink should exist")
}

func TestLinkPacks_DryRun(t *testing.T) {
	// Create test environment
	tempDir := testutil.TempDir(t, "deploy-dryrun")
	dotfilesDir := filepath.Join(tempDir, "dotfiles")
	homeDir := filepath.Join(tempDir, "home")

	testutil.CreateDir(t, tempDir, "dotfiles")
	testutil.CreateDir(t, tempDir, "home")

	t.Setenv("HOME", homeDir)
	t.Setenv("DOTFILES_ROOT", dotfilesDir)

	// Create a test pack
	testutil.CreateDir(t, dotfilesDir, "bash")
	testutil.CreateFile(t, dotfilesDir, "bash/bashrc", "# Test bashrc")

	// Deploy in dry-run mode
	ctx, err := LinkPacks(LinkPacksOptions{
		DotfilesRoot:       dotfilesDir,
		PackNames:          []string{"bash"},
		DryRun:             true,
		EnableHomeSymlinks: true,
	})

	// Verify no errors
	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, ctx)

	// Verify execution context
	testutil.AssertTrue(t, ctx.DryRun, "Should be dry run")
	testutil.AssertEqual(t, "link", ctx.Command)

	// Verify pack results exist
	packResult, ok := ctx.GetPackResult("bash")
	testutil.AssertTrue(t, ok, "Should have bash pack result")
	testutil.AssertEqual(t, types.ExecutionStatusSuccess, packResult.Status)

	// Verify no actual files were created (dry run)
	testutil.AssertFalse(t, testutil.FileExists(t, filepath.Join(homeDir, ".bashrc")), "bashrc symlink should not exist in dry run")
}

func TestLinkPacks_AllPacks(t *testing.T) {
	// Create test environment
	tempDir := testutil.TempDir(t, "deploy-allpacks")
	dotfilesDir := filepath.Join(tempDir, "dotfiles")
	homeDir := filepath.Join(tempDir, "home")

	testutil.CreateDir(t, tempDir, "dotfiles")
	testutil.CreateDir(t, tempDir, "home")
	testutil.CreateDir(t, homeDir, ".local/share/dodot")

	t.Setenv("HOME", homeDir)
	t.Setenv("DOTFILES_ROOT", dotfilesDir)
	t.Setenv("DODOT_DATA_DIR", filepath.Join(homeDir, ".local", "share", "dodot"))

	// Create multiple test packs
	testutil.CreateDir(t, dotfilesDir, "vim")
	testutil.CreateFile(t, dotfilesDir, "vim/vimrc", "\" Vim config")

	testutil.CreateDir(t, dotfilesDir, "git")
	testutil.CreateFile(t, dotfilesDir, "git/gitconfig", "[user]\n\tname = Test")

	// Deploy all packs (empty PackNames means all)
	ctx, err := LinkPacks(LinkPacksOptions{
		DotfilesRoot:       dotfilesDir,
		PackNames:          []string{}, // All packs
		DryRun:             false,
		EnableHomeSymlinks: true,
	})

	// Verify no errors
	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, ctx)

	// Should have results for both packs
	vimResult, hasVim := ctx.GetPackResult("vim")
	gitResult, hasGit := ctx.GetPackResult("git")

	testutil.AssertTrue(t, hasVim, "Should have vim pack result")
	testutil.AssertTrue(t, hasGit, "Should have git pack result")
	testutil.AssertEqual(t, types.ExecutionStatusSuccess, vimResult.Status)
	testutil.AssertEqual(t, types.ExecutionStatusSuccess, gitResult.Status)

	// Verify files from both packs were deployed (Layer 1: top-level files get dot prefix)
	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(homeDir, ".vimrc")), "vimrc should exist")
	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(homeDir, ".gitconfig")), "gitconfig should exist")
}

func TestLinkPacks_SkipInstallScripts(t *testing.T) {
	// Create test environment
	tempDir := testutil.TempDir(t, "deploy-skip-install")
	dotfilesDir := filepath.Join(tempDir, "dotfiles")
	homeDir := filepath.Join(tempDir, "home")

	testutil.CreateDir(t, tempDir, "dotfiles")
	testutil.CreateDir(t, tempDir, "home")
	testutil.CreateDir(t, homeDir, ".local/share/dodot")

	t.Setenv("HOME", homeDir)
	t.Setenv("DOTFILES_ROOT", dotfilesDir)
	t.Setenv("DODOT_DATA_DIR", filepath.Join(homeDir, ".local", "share", "dodot"))

	// Create a pack with both symlink files and install script
	testutil.CreateDir(t, dotfilesDir, "tools")
	testutil.CreateFile(t, dotfilesDir, "tools/aliases", "# Test aliases")

	// Create install script (should be skipped in deploy mode)
	installScript := `#!/bin/bash
echo "Installing tools" > /tmp/install-was-run
`
	testutil.CreateFile(t, dotfilesDir, "tools/install.sh", installScript)
	err := os.Chmod(filepath.Join(dotfilesDir, "tools/install.sh"), 0755)
	testutil.AssertNoError(t, err)

	// Deploy the pack
	ctx, err := LinkPacks(LinkPacksOptions{
		DotfilesRoot:       dotfilesDir,
		PackNames:          []string{"tools"},
		DryRun:             false,
		EnableHomeSymlinks: true,
	})

	// Verify no errors
	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, ctx)

	// Verify pack results
	packResult, ok := ctx.GetPackResult("tools")
	testutil.AssertTrue(t, ok, "Should have tools pack result")
	testutil.AssertEqual(t, types.ExecutionStatusSuccess, packResult.Status)

	// Should only have handler results (uses generic "handler" name)
	// In link mode, we should not have any provisioning actions
	testutil.AssertTrue(t, len(packResult.HandlerResults) > 0, "Should have handler results")

	// The key test is that install script was not executed
	// (which is verified below)

	// Verify symlink was created but install script was not run
	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(homeDir, ".aliases")), "aliases symlink should exist")
	testutil.AssertFalse(t, testutil.FileExists(t, "/tmp/install-was-run"), "Install script should NOT have been executed")
}

// TestLinkPacks_InvalidPack and TestLinkPacks_EmptyPack were removed
// These scenarios are already tested in pkg/commands/internal/pipeline_test.go
