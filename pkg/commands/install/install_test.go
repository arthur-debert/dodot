package install

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestInstallPacks_BothPhases(t *testing.T) {
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
echo "Tools installed" > /tmp/install-tools-output
`
	testutil.CreateFile(t, dotfilesDir, "tools/install.sh", installScript)
	err := os.Chmod(filepath.Join(dotfilesDir, "tools/install.sh"), 0755)
	testutil.AssertNoError(t, err)

	// Install the pack (should run both phases)
	ctx, err := InstallPacks(InstallPacksOptions{
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
	testutil.AssertEqual(t, "install", ctx.Command)
	testutil.AssertFalse(t, ctx.DryRun, "Should not be dry run")

	// Verify pack results
	packResult, ok := ctx.GetPackResult("tools")
	testutil.AssertTrue(t, ok, "Should have tools pack result")
	testutil.AssertNotNil(t, packResult)
	testutil.AssertEqual(t, "tools", packResult.Pack.Name)
	testutil.AssertEqual(t, types.ExecutionStatusSuccess, packResult.Status)

	// Should have both install_script and symlink power-up results
	testutil.AssertTrue(t, len(packResult.PowerUpResults) >= 2, "Should have multiple power-up results")

	var hasInstallScript, hasSymlink bool
	for _, pur := range packResult.PowerUpResults {
		if pur.PowerUpName == "install_script" {
			hasInstallScript = true
			testutil.AssertEqual(t, types.StatusReady, pur.Status)
		}
		if pur.PowerUpName == "symlink" {
			hasSymlink = true
			testutil.AssertEqual(t, types.StatusReady, pur.Status)
		}
	}

	testutil.AssertTrue(t, hasInstallScript, "Should have install_script power-up")
	testutil.AssertTrue(t, hasSymlink, "Should have symlink power-up")

	// Verify both install script power-up processed AND symlink was created
	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(homeDir, "aliases")), "aliases symlink should exist")

	// Check that install script power-up was processed (should create sentinel and copy script)
	installDir := filepath.Join(homeDir, ".local", "share", "dodot", "install")
	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(installDir, "sentinels", "tools")), "Install sentinel file should exist")
	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(installDir, "tools", "install.sh")), "Install script should be copied")
}

func TestInstallPacks_DryRun(t *testing.T) {
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
	ctx, err := InstallPacks(InstallPacksOptions{
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
	testutil.AssertEqual(t, "install", ctx.Command)

	// Should have pack results with both power-ups planned
	packResult, ok := ctx.GetPackResult("dev")
	testutil.AssertTrue(t, ok, "Should have dev pack result")
	testutil.AssertEqual(t, types.ExecutionStatusSuccess, packResult.Status)

	// Verify no actual files were created (dry run)
	testutil.AssertFalse(t, testutil.FileExists(t, filepath.Join(homeDir, "gitconfig")), "gitconfig symlink should not exist in dry run")
}

func TestInstallPacks_ForceFlag(t *testing.T) {
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
	ctx1, err := InstallPacks(InstallPacksOptions{
		DotfilesRoot:       dotfilesDir,
		PackNames:          []string{"force-test"},
		DryRun:             false,
		Force:              false, // No force first time
		EnableHomeSymlinks: true,
	})

	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, ctx1)

	// Verify install script was processed first time
	installDir := filepath.Join(homeDir, ".local", "share", "dodot", "install")
	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(installDir, "sentinels", "force-test")), "First run sentinel should exist")
	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(installDir, "force-test", "install.sh")), "First run script should be copied")

	// Second install with force flag (should run and update files)
	ctx2, err := InstallPacks(InstallPacksOptions{
		DotfilesRoot:       dotfilesDir,
		PackNames:          []string{"force-test"},
		DryRun:             false,
		Force:              true, // Force flag should override sentinel
		EnableHomeSymlinks: true,
	})

	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, ctx2)

	// Verify force run was processed (files still exist)
	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(installDir, "sentinels", "force-test")), "Force run sentinel should exist")
	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(installDir, "force-test", "install.sh")), "Force run script should be copied")
}

func TestInstallPacks_OnlySymlinks(t *testing.T) {
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
	ctx, err := InstallPacks(InstallPacksOptions{
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

	var hasInstallScript, hasSymlink bool
	for _, pur := range packResult.PowerUpResults {
		if pur.PowerUpName == "install_script" {
			hasInstallScript = true
		}
		if pur.PowerUpName == "symlink" {
			hasSymlink = true
		}
	}

	testutil.AssertFalse(t, hasInstallScript, "Should NOT have install_script power-up")
	testutil.AssertTrue(t, hasSymlink, "Should have symlink power-up")

	// Verify symlink was created
	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(homeDir, "vimrc")), "vimrc symlink should exist")
}

func TestInstallPacks_OnlyInstallScript(t *testing.T) {
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
echo "Setup complete" > /tmp/setup-output
`
	testutil.CreateFile(t, dotfilesDir, "setup/install.sh", installScript)
	err := os.Chmod(filepath.Join(dotfilesDir, "setup/install.sh"), 0755)
	testutil.AssertNoError(t, err)

	// Install the pack
	ctx, err := InstallPacks(InstallPacksOptions{
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

	var hasInstallScript, hasSymlink bool
	for _, pur := range packResult.PowerUpResults {
		if pur.PowerUpName == "install_script" {
			hasInstallScript = true
		}
		if pur.PowerUpName == "symlink" {
			hasSymlink = true
		}
	}

	testutil.AssertTrue(t, hasInstallScript, "Should have install_script power-up")
	testutil.AssertFalse(t, hasSymlink, "Should NOT have symlink power-up")

	// Verify install script was processed (copied and sentinel created)
	installDir := filepath.Join(homeDir, ".local", "share", "dodot", "install")
	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(installDir, "sentinels", "setup")), "Install sentinel should exist")
	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(installDir, "setup", "install.sh")), "Install script should be copied")
}

func TestInstallPacks_MultiplePacksAllTypes(t *testing.T) {
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
	ctx, err := InstallPacks(InstallPacksOptions{
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

	// Verify symlinks were created
	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(homeDir, "gitconfig")), "gitconfig symlink should exist")
	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(homeDir, "bashrc")), "bashrc symlink should exist")

	// Verify install scripts were processed (copied and sentinels created)
	installDir := filepath.Join(homeDir, ".local", "share", "dodot", "install")
	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(installDir, "sentinels", "langs")), "langs install sentinel should exist")
	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(installDir, "sentinels", "shell")), "shell install sentinel should exist")
	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(installDir, "langs", "install.sh")), "langs script should be copied")
	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(installDir, "shell", "install.sh")), "shell script should be copied")
}

func TestInstallPacks_InvalidPack(t *testing.T) {
	// Create test environment
	tempDir := testutil.TempDir(t, "install-invalid")
	dotfilesDir := filepath.Join(tempDir, "dotfiles")

	testutil.CreateDir(t, tempDir, "dotfiles")

	t.Setenv("HOME", tempDir)
	t.Setenv("DOTFILES_ROOT", dotfilesDir)

	// Try to install non-existent pack
	ctx, err := InstallPacks(InstallPacksOptions{
		DotfilesRoot:       dotfilesDir,
		PackNames:          []string{"nonexistent"},
		DryRun:             false,
		Force:              false,
		EnableHomeSymlinks: true,
	})

	// Should get an error from the first phase
	testutil.AssertError(t, err)

	// Context might still be returned with partial results
	if ctx != nil {
		testutil.AssertTrue(t, ctx.EndTime.After(ctx.StartTime), "End time should be set")
	}
}
