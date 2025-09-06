package dodot

import (
	"os"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestDeployCmd(t *testing.T) {
	// Set up test environment with isolated filesystem
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)
	defer env.Cleanup()

	// Create pack directories
	env.SetupPack("pack1", testutil.PackConfig{})
	env.SetupPack("pack2", testutil.PackConfig{})

	// Set the DOTFILES_ROOT environment variable
	t.Setenv("DOTFILES_ROOT", env.DotfilesRoot)

	// Create a new root command for testing
	rootCmd := NewRootCmd()

	// Execute the on command
	rootCmd.SetArgs([]string{"up"})
	err := rootCmd.Execute()

	// Assert no error occurred
	require.NoError(t, err)
}

func TestDeployCmd_NoDotfilesRoot(t *testing.T) {
	// Create test environment without setting DOTFILES_ROOT
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)
	defer env.Cleanup()

	// Unset the DOTFILES_ROOT environment variable
	t.Setenv("DOTFILES_ROOT", "")

	// Change working directory to the dotfiles root
	origWd, err := os.Getwd()
	require.NoError(t, err)
	err = os.Chdir(env.DotfilesRoot)
	require.NoError(t, err)
	t.Cleanup(func() {
		_ = os.Chdir(origWd)
	})

	// Create some basic pack structure for the fallback to work
	env.SetupPack("test-pack", testutil.PackConfig{})

	// Create a new root command for testing
	rootCmd := NewRootCmd()

	// Execute the on command - should succeed with fallback warning
	rootCmd.SetArgs([]string{"up"})
	err = rootCmd.Execute()

	// The command should succeed but with a fallback warning
	// (it uses current working directory as fallback)
	assert.NoError(t, err)
}
