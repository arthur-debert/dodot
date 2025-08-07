package commands

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
)

func TestDeployPacksDirect(t *testing.T) {
	// Setup test environment
	tempDir := testutil.TempDir(t, "deploy-direct")
	dotfilesDir := filepath.Join(tempDir, "dotfiles")
	packDir := filepath.Join(dotfilesDir, "test-pack")
	homeDir := filepath.Join(tempDir, "home")

	// Create directories
	testutil.CreateDir(t, tempDir, "dotfiles/test-pack")
	testutil.CreateDir(t, tempDir, "home")
	testutil.CreateDir(t, homeDir, ".local/share/dodot")

	// Set environment
	t.Setenv("HOME", homeDir)
	t.Setenv("DOTFILES_ROOT", dotfilesDir)
	t.Setenv("DODOT_DATA_DIR", filepath.Join(homeDir, ".local", "share", "dodot"))

	// Create test files
	testutil.CreateFile(t, packDir, "test.txt", "test content")

	// Create pack config for symlink power-up
	testutil.CreateFile(t, packDir, ".dodot.toml", `
[[overrides]]
pattern = "*.txt"
powerup = "symlink"
`)

	// Test DeployPacksDirect
	opts := DeployPacksOptions{
		DotfilesRoot:       dotfilesDir,
		PackNames:          []string{"test-pack"},
		DryRun:             false,
		EnableHomeSymlinks: true,
	}

	context, err := DeployPacks(opts)
	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, context)

	// Verify context has results
	testutil.AssertTrue(t, len(context.PackResults) > 0, "Should have pack results")

	packResult, exists := context.GetPackResult("test-pack")
	testutil.AssertTrue(t, exists, "Should have test-pack results")
	testutil.AssertNotNil(t, packResult)

	// Verify operations were executed (symlinks created)
	testutil.AssertTrue(t, len(packResult.Operations) > 0, "Should have operation results")
}

func TestInstallPacksDirect(t *testing.T) {
	// Setup test environment
	tempDir := testutil.TempDir(t, "install-direct")
	dotfilesDir := filepath.Join(tempDir, "dotfiles")
	packDir := filepath.Join(dotfilesDir, "test-pack")
	homeDir := filepath.Join(tempDir, "home")

	// Create directories
	testutil.CreateDir(t, tempDir, "dotfiles/test-pack")
	testutil.CreateDir(t, tempDir, "home")
	testutil.CreateDir(t, homeDir, ".local/share/dodot")

	// Set environment
	t.Setenv("HOME", homeDir)
	t.Setenv("DOTFILES_ROOT", dotfilesDir)
	t.Setenv("DODOT_DATA_DIR", filepath.Join(homeDir, ".local", "share", "dodot"))

	// Create test files
	testutil.CreateFile(t, packDir, "test.txt", "test content")
	testutil.CreateFile(t, packDir, "install.sh", "#!/bin/bash\necho 'installing'")

	// Make install.sh executable
	err := os.Chmod(filepath.Join(packDir, "install.sh"), 0755)
	testutil.AssertNoError(t, err)

	// Create pack config for mixed power-ups
	testutil.CreateFile(t, packDir, ".dodot.toml", `
[[overrides]]
pattern = "*.txt"
powerup = "symlink"

[[overrides]]  
pattern = "install.sh"
powerup = "install"
`)

	// Test InstallPacksDirect
	opts := InstallPacksOptions{
		DotfilesRoot:       dotfilesDir,
		PackNames:          []string{"test-pack"},
		DryRun:             false,
		Force:              false,
		EnableHomeSymlinks: true,
	}

	context, err := InstallPacks(opts)
	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, context)

	// Verify context has results
	testutil.AssertTrue(t, len(context.PackResults) > 0, "Should have pack results")

	packResult, exists := context.GetPackResult("test-pack")
	testutil.AssertTrue(t, exists, "Should have test-pack results")
	testutil.AssertNotNil(t, packResult)

	// Verify operations were executed
	testutil.AssertTrue(t, len(packResult.Operations) > 0, "Should have operation results")
}
