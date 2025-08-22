package dodot

import (
	"os"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
)

func TestDeployCmd(t *testing.T) {
	// Set up a test dotfiles directory
	dotfilesRoot := testutil.TempDir(t, "dotfiles")
	testutil.CreateDir(t, dotfilesRoot, "pack1")
	testutil.CreateDir(t, dotfilesRoot, "pack2")

	// Set the DOTFILES_ROOT environment variable
	t.Setenv("DOTFILES_ROOT", dotfilesRoot)

	// Create a new root command for testing
	rootCmd := NewRootCmd()

	// Execute the deploy command
	rootCmd.SetArgs([]string{"link"})
	err := rootCmd.Execute()

	// Assert no error occurred
	testutil.AssertNoError(t, err)
}

func TestDeployCmd_NoDotfilesRoot(t *testing.T) {
	// Unset the DOTFILES_ROOT environment variable
	t.Setenv("DOTFILES_ROOT", "")

	// Create a temporary directory to serve as current working directory
	// This will be used as the fallback dotfiles root
	tempDir := testutil.TempDir(t, "fallback-dotfiles")

	// Change working directory to the temp directory
	origWd, err := os.Getwd()
	testutil.AssertNoError(t, err)
	err = os.Chdir(tempDir)
	testutil.AssertNoError(t, err)
	t.Cleanup(func() {
		_ = os.Chdir(origWd)
	})

	// Create some basic pack structure for the fallback to work
	testutil.CreateDir(t, tempDir, "test-pack")

	// Create a new root command for testing
	rootCmd := NewRootCmd()

	// Execute the deploy command - should succeed with fallback warning
	rootCmd.SetArgs([]string{"link"})
	err = rootCmd.Execute()

	// The command should succeed but with a fallback warning
	// (it uses current working directory as fallback)
	testutil.AssertNoError(t, err)
}
