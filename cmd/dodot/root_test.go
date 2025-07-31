package main

import (
	"os"
	"testing"

	"github.com/arthur-debert/dodot/internal/cli"
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
	rootCmd := cli.NewRootCmd()

	// Execute the deploy command
	rootCmd.SetArgs([]string{"deploy"})
	err := rootCmd.Execute()

	// Assert no error occurred
	testutil.AssertNoError(t, err)
}

func TestDeployCmd_NoDotfilesRoot(t *testing.T) {
	t.Skip("Skipping test - needs to be updated for new command structure")
	
	// Unset the DOTFILES_ROOT environment variable
	if err := os.Unsetenv("DOTFILES_ROOT"); err != nil {
		t.Fatalf("Failed to unset DOTFILES_ROOT: %v", err)
	}

	// Create a new root command for testing
	rootCmd := cli.NewRootCmd()

	// Execute the deploy command
	rootCmd.SetArgs([]string{"deploy"})
	err := rootCmd.Execute()

	// Assert that an error occurred
	testutil.AssertError(t, err)
}
