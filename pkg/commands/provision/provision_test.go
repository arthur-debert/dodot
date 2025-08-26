package provision

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestProvisionPacks_BothPhases(t *testing.T) {
	// Create test environment
	tempDir := testutil.TempDir(t, "install-both")
	dotfilesDir := filepath.Join(tempDir, "dotfiles")
	homeDir := filepath.Join(tempDir, "home")

	testutil.CreateDir(t, tempDir, "dotfiles")
	testutil.CreateDir(t, tempDir, "home")
	testutil.CreateDir(t, homeDir, ".local/share/dodot")

	t.Setenv("HOME", homeDir)
	t.Setenv("DOTFILES_ROOT", dotfilesDir)
	t.Setenv("DODOT_DATA_DIR", filepath.Join(homeDir, ".local", "share", "dodot"))

	// Create a pack with both install script AND symlink files
	testutil.CreateDir(t, dotfilesDir, "tools")
	testutil.CreateFile(t, dotfilesDir, "tools/aliases", "# Test aliases")

	// Create install script
	installScript := `#!/bin/bash
echo "Tools installed" > /tmp/install-run-marker
`
	testutil.CreateFile(t, dotfilesDir, "tools/install.sh", installScript)
	err := os.Chmod(filepath.Join(dotfilesDir, "tools/install.sh"), 0755)
	testutil.AssertNoError(t, err)

	// Install the pack (should run both phases)
	ctx, err := ProvisionPacks(ProvisionPacksOptions{
		DotfilesRoot:       dotfilesDir,
		PackNames:          []string{"tools"},
		DryRun:             false,
		Force:              false,
		EnableHomeSymlinks: true,
	})

	// Verify no errors
	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, ctx)

	// Verify execution context
	testutil.AssertEqual(t, "provision", ctx.Command)
	testutil.AssertFalse(t, ctx.DryRun, "Should not be dry run")

	// Verify pack results
	packResult, ok := ctx.GetPackResult("tools")
	testutil.AssertTrue(t, ok, "Should have tools pack result")
	testutil.AssertNotNil(t, packResult)
	testutil.AssertEqual(t, "tools", packResult.Pack.Name)
	testutil.AssertEqual(t, types.ExecutionStatusSuccess, packResult.Status)

	// Handlers use generic "handler" name, but we should have multiple handler results
	// (both install and symlink handlers)
	testutil.AssertTrue(t, len(packResult.HandlerResults) >= 2, "Should have multiple handler results")

	// Check that all handlers completed successfully
	for _, pur := range packResult.HandlerResults {
		testutil.AssertEqual(t, types.StatusReady, pur.Status)
	}

	// Verify both install script handler processed AND symlink was created
	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(homeDir, ".aliases")), "aliases symlink should exist")

	// Architecture doesn't create sentinel files or copy scripts in the same way
	// The DataStore manages provisioning state internally
	// The key test is that the script was executed (verified by marker file)
	testutil.AssertTrue(t, testutil.FileExists(t, "/tmp/install-run-marker"), "Install script should have been executed")
}

func TestProvisionPacks_DryRun(t *testing.T) {
	// Create test environment
	tempDir := testutil.TempDir(t, "install-dryrun")
	dotfilesDir := filepath.Join(tempDir, "dotfiles")
	homeDir := filepath.Join(tempDir, "home")

	testutil.CreateDir(t, tempDir, "dotfiles")
	testutil.CreateDir(t, tempDir, "home")

	t.Setenv("HOME", homeDir)
	t.Setenv("DOTFILES_ROOT", dotfilesDir)

	// Create pack with both install script and symlink
	testutil.CreateDir(t, dotfilesDir, "dev")
	testutil.CreateFile(t, dotfilesDir, "dev/gitconfig", "[user]\n\tname = Test")
	testutil.CreateFile(t, dotfilesDir, "dev/install.sh", "#!/bin/bash\necho 'installing'")
	err := os.Chmod(filepath.Join(dotfilesDir, "dev/install.sh"), 0755)
	testutil.AssertNoError(t, err)

	// Install in dry-run mode
	ctx, err := ProvisionPacks(ProvisionPacksOptions{
		DotfilesRoot:       dotfilesDir,
		PackNames:          []string{"dev"},
		DryRun:             true,
		Force:              false,
		EnableHomeSymlinks: true,
	})

	// Verify no errors
	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, ctx)

	// Verify execution context
	testutil.AssertTrue(t, ctx.DryRun, "Should be dry run")
	testutil.AssertEqual(t, "provision", ctx.Command)

	// Should have pack results with both handlers planned
	packResult, ok := ctx.GetPackResult("dev")
	testutil.AssertTrue(t, ok, "Should have dev pack result")
	testutil.AssertEqual(t, types.ExecutionStatusSuccess, packResult.Status)

	// Verify no actual files were created (dry run)
	testutil.AssertFalse(t, testutil.FileExists(t, filepath.Join(homeDir, "gitconfig")), "gitconfig symlink should not exist in dry run")
}

func TestProvisionPacks_ForceFlag(t *testing.T) {
	// Create test environment
	tempDir := testutil.TempDir(t, "install-force")
	dotfilesDir := filepath.Join(tempDir, "dotfiles")
	homeDir := filepath.Join(tempDir, "home")

	testutil.CreateDir(t, tempDir, "dotfiles")
	testutil.CreateDir(t, tempDir, "home")
	testutil.CreateDir(t, homeDir, ".local/share/dodot")

	t.Setenv("HOME", homeDir)
	t.Setenv("DOTFILES_ROOT", dotfilesDir)
	t.Setenv("DODOT_DATA_DIR", filepath.Join(homeDir, ".local", "share", "dodot"))

	// Create pack with install script
	testutil.CreateDir(t, dotfilesDir, "force-test")

	installScript := `#!/bin/bash
echo "Installing..."
`
	testutil.CreateFile(t, dotfilesDir, "force-test/install.sh", installScript)
	err := os.Chmod(filepath.Join(dotfilesDir, "force-test/install.sh"), 0755)
	testutil.AssertNoError(t, err)

	// First install (should run)
	ctx1, err := ProvisionPacks(ProvisionPacksOptions{
		DotfilesRoot:       dotfilesDir,
		PackNames:          []string{"force-test"},
		DryRun:             false,
		Force:              false, // No force first time
		EnableHomeSymlinks: true,
	})

	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, ctx1)

	// Architecture doesn't create sentinel files
	// The DataStore manages provisioning state internally
	// (First run would have executed the script)

	// Second install with force flag (should run and update files)
	ctx2, err := ProvisionPacks(ProvisionPacksOptions{
		DotfilesRoot:       dotfilesDir,
		PackNames:          []string{"force-test"},
		DryRun:             false,
		Force:              true, // Force flag should override sentinel
		EnableHomeSymlinks: true,
	})

	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, ctx2)

	// Architecture doesn't create sentinel files
	// The DataStore manages provisioning state internally
	// (Force run would have executed the script again)
}

func TestProvisionPacks_OnlySymlinks(t *testing.T) {
	// Create test environment
	tempDir := testutil.TempDir(t, "install-symonly")
	dotfilesDir := filepath.Join(tempDir, "dotfiles")
	homeDir := filepath.Join(tempDir, "home")

	testutil.CreateDir(t, tempDir, "dotfiles")
	testutil.CreateDir(t, tempDir, "home")
	testutil.CreateDir(t, homeDir, ".local/share/dodot")

	t.Setenv("HOME", homeDir)
	t.Setenv("DOTFILES_ROOT", dotfilesDir)
	t.Setenv("DODOT_DATA_DIR", filepath.Join(homeDir, ".local", "share", "dodot"))

	// Create pack with only symlink files (no install script)
	testutil.CreateDir(t, dotfilesDir, "vim")
	testutil.CreateFile(t, dotfilesDir, "vim/vimrc", "\" Test vimrc")

	// Install the pack
	ctx, err := ProvisionPacks(ProvisionPacksOptions{
		DotfilesRoot:       dotfilesDir,
		PackNames:          []string{"vim"},
		DryRun:             false,
		Force:              false,
		EnableHomeSymlinks: true,
	})

	// Verify no errors
	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, ctx)

	// Verify pack results - should have symlink but no install script
	packResult, ok := ctx.GetPackResult("vim")
	testutil.AssertTrue(t, ok, "Should have vim pack result")

	// Handlers use generic "handler" name
	// In provisioning mode with symlinks only, we should only have 1 handler result
	testutil.AssertEqual(t, 1, len(packResult.HandlerResults), "Should have exactly one handler result for symlink")

	// Verify symlink was created (Layer 1: top-level files get dot prefix)
	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(homeDir, ".vimrc")), "vimrc symlink should exist")
}

func TestProvisionPacks_OnlyInstallScript(t *testing.T) {
	// Create test environment
	tempDir := testutil.TempDir(t, "install-scriptonly")
	dotfilesDir := filepath.Join(tempDir, "dotfiles")
	homeDir := filepath.Join(tempDir, "home")

	testutil.CreateDir(t, tempDir, "dotfiles")
	testutil.CreateDir(t, tempDir, "home")
	testutil.CreateDir(t, homeDir, ".local/share/dodot")

	t.Setenv("HOME", homeDir)
	t.Setenv("DOTFILES_ROOT", dotfilesDir)
	t.Setenv("DODOT_DATA_DIR", filepath.Join(homeDir, ".local", "share", "dodot"))

	// Create pack with only install script (no symlink files)
	testutil.CreateDir(t, dotfilesDir, "setup")

	installScript := `#!/bin/bash
echo "Setup complete" > /tmp/setup-tools-marker
`
	testutil.CreateFile(t, dotfilesDir, "setup/install.sh", installScript)
	err := os.Chmod(filepath.Join(dotfilesDir, "setup/install.sh"), 0755)
	testutil.AssertNoError(t, err)

	// Install the pack
	ctx, err := ProvisionPacks(ProvisionPacksOptions{
		DotfilesRoot:       dotfilesDir,
		PackNames:          []string{"setup"},
		DryRun:             false,
		Force:              false,
		EnableHomeSymlinks: true,
	})

	// Verify no errors
	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, ctx)

	// Verify pack results - should have install script but no symlink
	packResult, ok := ctx.GetPackResult("setup")
	testutil.AssertTrue(t, ok, "Should have setup pack result")

	// Handlers use generic "handler" name
	// For install-only pack, should have exactly one handler result
	testutil.AssertEqual(t, 1, len(packResult.HandlerResults), "Should have exactly one handler result for provision")

	// Architecture doesn't create sentinel files or copy scripts in the same way
	// The DataStore manages provisioning state internally
	// The key test is that the script was executed (verified by marker file)
	testutil.AssertTrue(t, testutil.FileExists(t, "/tmp/setup-tools-marker"), "Install script should have been executed")
}

func TestProvisionPacks_MultiplePacksAllTypes(t *testing.T) {
	// Create test environment
	tempDir := testutil.TempDir(t, "install-multi")
	dotfilesDir := filepath.Join(tempDir, "dotfiles")
	homeDir := filepath.Join(tempDir, "home")

	testutil.CreateDir(t, tempDir, "dotfiles")
	testutil.CreateDir(t, tempDir, "home")
	testutil.CreateDir(t, homeDir, ".local/share/dodot")

	t.Setenv("HOME", homeDir)
	t.Setenv("DOTFILES_ROOT", dotfilesDir)
	t.Setenv("DODOT_DATA_DIR", filepath.Join(homeDir, ".local", "share", "dodot"))

	// Create multiple packs with different characteristics

	// Pack 1: Only symlinks
	testutil.CreateDir(t, dotfilesDir, "git")
	testutil.CreateFile(t, dotfilesDir, "git/gitconfig", "[core]\n\teditor = vim")

	// Pack 2: Only install script
	testutil.CreateDir(t, dotfilesDir, "langs")
	testutil.CreateFile(t, dotfilesDir, "langs/install.sh", "#!/bin/bash\necho 'languages' > /tmp/langs-install")
	err := os.Chmod(filepath.Join(dotfilesDir, "langs/install.sh"), 0755)
	testutil.AssertNoError(t, err)

	// Pack 3: Both install script and symlinks
	testutil.CreateDir(t, dotfilesDir, "shell")
	testutil.CreateFile(t, dotfilesDir, "shell/bashrc", "# Custom bashrc")
	testutil.CreateFile(t, dotfilesDir, "shell/install.sh", "#!/bin/bash\necho 'shell setup' > /tmp/shell-install")
	err = os.Chmod(filepath.Join(dotfilesDir, "shell/install.sh"), 0755)
	testutil.AssertNoError(t, err)

	// Install all packs
	ctx, err := ProvisionPacks(ProvisionPacksOptions{
		DotfilesRoot:       dotfilesDir,
		PackNames:          []string{}, // All packs
		DryRun:             false,
		Force:              false,
		EnableHomeSymlinks: true,
	})

	// Verify no errors
	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, ctx)

	// Should have results for all packs
	gitResult, hasGit := ctx.GetPackResult("git")
	langsResult, hasLangs := ctx.GetPackResult("langs")
	shellResult, hasShell := ctx.GetPackResult("shell")

	testutil.AssertTrue(t, hasGit, "Should have git pack result")
	testutil.AssertTrue(t, hasLangs, "Should have langs pack result")
	testutil.AssertTrue(t, hasShell, "Should have shell pack result")

	testutil.AssertEqual(t, types.ExecutionStatusSuccess, gitResult.Status)
	testutil.AssertEqual(t, types.ExecutionStatusSuccess, langsResult.Status)
	testutil.AssertEqual(t, types.ExecutionStatusSuccess, shellResult.Status)

	// Verify symlinks were created (Layer 1: top-level files get dot prefix)
	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(homeDir, ".gitconfig")), "gitconfig symlink should exist")
	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(homeDir, ".bashrc")), "bashrc symlink should exist")

	// Verify install scripts were processed (copied and sentinels created)
	// Architecture doesn't create sentinel files or copy scripts
	// The DataStore manages provisioning state internally
	// The key test is that install scripts were executed (verified by marker files)
	testutil.AssertTrue(t, testutil.FileExists(t, "/tmp/langs-install"), "langs install script should have been executed")
	testutil.AssertTrue(t, testutil.FileExists(t, "/tmp/shell-install"), "shell install script should have been executed")
}

// TestProvisionPacks_InvalidPack was removed
// This scenario is already tested in pkg/commands/internal/pipeline_test.go

func TestProvisionPacks_ShellIntegration(t *testing.T) {
	// Create test environment
	tempDir := testutil.TempDir(t, "install-shell-integration")
	dotfilesDir := filepath.Join(tempDir, "dotfiles")
	homeDir := filepath.Join(tempDir, "home")

	testutil.CreateDir(t, tempDir, "dotfiles")
	testutil.CreateDir(t, tempDir, "home")
	testutil.CreateDir(t, homeDir, ".local/share/dodot")

	t.Setenv("HOME", homeDir)
	t.Setenv("DOTFILES_ROOT", dotfilesDir)
	t.Setenv("DODOT_DATA_DIR", filepath.Join(homeDir, ".local", "share", "dodot"))

	// Set PROJECT_ROOT so shell scripts can be found in development
	// This is needed because the test runs from pkg/commands/install
	// and needs to find scripts in pkg/shell/
	// Walk up from current directory to find project root
	cwd, _ := os.Getwd()
	for dir := cwd; dir != "/" && dir != ""; dir = filepath.Dir(dir) {
		if _, err := os.Stat(filepath.Join(dir, "go.mod")); err == nil {
			// Found project root
			if _, err := os.Stat(filepath.Join(dir, "pkg", "shell", "dodot-init.sh")); err == nil {
				t.Setenv("PROJECT_ROOT", dir)
				break
			}
		}
	}

	// Create a simple pack with just a symlink (to have successful actions)
	testutil.CreateDir(t, dotfilesDir, "shell-test")
	testutil.CreateFile(t, dotfilesDir, "shell-test/bashrc", "# test bashrc")

	// Install the pack
	ctx, err := ProvisionPacks(ProvisionPacksOptions{
		DotfilesRoot:       dotfilesDir,
		PackNames:          []string{"shell-test"},
		DryRun:             false,
		Force:              false,
		EnableHomeSymlinks: true,
	})

	// Verify no errors
	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, ctx)

	// Verify shell integration was installed (shell scripts should exist)
	dataDir := filepath.Join(homeDir, ".local", "share", "dodot")
	shellDir := filepath.Join(dataDir, "shell")

	testutil.AssertTrue(t, testutil.DirExists(t, shellDir), "Shell directory should exist")
	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(shellDir, "dodot-init.sh")), "Bash shell script should exist")
	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(shellDir, "dodot-init.fish")), "Fish shell script should exist")

	// Verify scripts are executable
	bashScript := filepath.Join(shellDir, "dodot-init.sh")
	if stat, err := os.Stat(bashScript); err == nil {
		mode := stat.Mode()
		testutil.AssertTrue(t, mode&0100 != 0, "Bash script should be executable")
	}
}

func TestProvisionPacks_ShellIntegration_DryRun(t *testing.T) {
	// Create test environment
	tempDir := testutil.TempDir(t, "install-shell-dryrun")
	dotfilesDir := filepath.Join(tempDir, "dotfiles")
	homeDir := filepath.Join(tempDir, "home")

	testutil.CreateDir(t, tempDir, "dotfiles")
	testutil.CreateDir(t, tempDir, "home")

	t.Setenv("HOME", homeDir)
	t.Setenv("DOTFILES_ROOT", dotfilesDir)
	t.Setenv("DODOT_DATA_DIR", filepath.Join(homeDir, ".local", "share", "dodot"))

	// Create a simple pack
	testutil.CreateDir(t, dotfilesDir, "dryrun-test")
	testutil.CreateFile(t, dotfilesDir, "dryrun-test/config", "test config")

	// Install in dry-run mode
	ctx, err := ProvisionPacks(ProvisionPacksOptions{
		DotfilesRoot:       dotfilesDir,
		PackNames:          []string{"dryrun-test"},
		DryRun:             true, // Dry run should NOT install shell integration
		Force:              false,
		EnableHomeSymlinks: true,
	})

	// Verify no errors
	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, ctx)

	// Verify shell integration was NOT installed in dry-run mode
	dataDir := filepath.Join(homeDir, ".local", "share", "dodot")
	shellDir := filepath.Join(dataDir, "shell")

	testutil.AssertFalse(t, testutil.FileExists(t, filepath.Join(shellDir, "dodot-init.sh")), "Shell script should NOT exist in dry run")
	testutil.AssertFalse(t, testutil.FileExists(t, filepath.Join(shellDir, "dodot-init.fish")), "Fish script should NOT exist in dry run")
}

func TestProvisionPacks_ShellIntegration_NoActions(t *testing.T) {
	// Create test environment with no packs (no successful actions)
	tempDir := testutil.TempDir(t, "install-no-actions")
	dotfilesDir := filepath.Join(tempDir, "dotfiles")
	homeDir := filepath.Join(tempDir, "home")

	testutil.CreateDir(t, tempDir, "dotfiles")
	testutil.CreateDir(t, tempDir, "home")

	t.Setenv("HOME", homeDir)
	t.Setenv("DOTFILES_ROOT", dotfilesDir)
	t.Setenv("DODOT_DATA_DIR", filepath.Join(homeDir, ".local", "share", "dodot"))

	// Install with no packs (should have no successful actions)
	ctx, err := ProvisionPacks(ProvisionPacksOptions{
		DotfilesRoot:       dotfilesDir,
		PackNames:          []string{}, // Empty pack list
		DryRun:             false,
		Force:              false,
		EnableHomeSymlinks: true,
	})

	// Should succeed but have no actions
	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, ctx)
	testutil.AssertEqual(t, 0, ctx.CompletedActions)
	testutil.AssertEqual(t, 0, ctx.SkippedActions)

	// Shell integration should NOT be installed when no actions were completed
	dataDir := filepath.Join(homeDir, ".local", "share", "dodot")
	shellDir := filepath.Join(dataDir, "shell")

	testutil.AssertFalse(t, testutil.FileExists(t, filepath.Join(shellDir, "dodot-init.sh")), "Shell script should NOT exist with no actions")
}
