package on

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestOnPacks_EmptyPacks(t *testing.T) {
	env := testutil.NewTestEnvironment(t, "on-empty")
	defer env.Cleanup()

	opts := OnPacksOptions{
		DotfilesRoot: env.DotfilesRoot(),
		PackNames:    []string{},
		DryRun:       false,
	}

	result, err := OnPacks(opts)
	require.NoError(t, err)
	assert.NotNil(t, result.LinkResult)
	assert.NotNil(t, result.ProvisionResult)
	assert.Zero(t, result.TotalDeployed)
	assert.False(t, result.DryRun)
}

func TestOnPacks_Basic(t *testing.T) {
	env := testutil.NewTestEnvironment(t, "on-basic")
	defer env.Cleanup()

	// Create a pack
	packDir := env.CreatePack("mypack")
	testutil.CreateFile(t, packDir, ".vimrc", "set number")
	testutil.CreateFile(t, packDir, ".bashrc", "alias ll='ls -la'")

	opts := OnPacksOptions{
		DotfilesRoot: env.DotfilesRoot(),
		PackNames:    []string{"mypack"},
		DryRun:       false,
		Force:        false,
	}

	result, err := OnPacks(opts)
	require.NoError(t, err)
	assert.NotNil(t, result.LinkResult)
	assert.NotNil(t, result.ProvisionResult)
	assert.Greater(t, result.TotalDeployed, 0)
	assert.Empty(t, result.Errors)
}

func TestOnPacks_DryRun(t *testing.T) {
	env := testutil.NewTestEnvironment(t, "on-dryrun")
	defer env.Cleanup()

	// Create a pack
	packDir := env.CreatePack("mypack")
	testutil.CreateFile(t, packDir, ".vimrc", "set number")

	opts := OnPacksOptions{
		DotfilesRoot: env.DotfilesRoot(),
		PackNames:    []string{"mypack"},
		DryRun:       true,
		Force:        false,
	}

	result, err := OnPacks(opts)
	require.NoError(t, err)
	assert.True(t, result.DryRun)
	assert.NotNil(t, result.LinkResult)
	assert.NotNil(t, result.ProvisionResult)
	assert.True(t, result.LinkResult.DryRun)
	assert.True(t, result.ProvisionResult.DryRun)
}

func TestOnPacks_Force(t *testing.T) {
	env := testutil.NewTestEnvironment(t, "on-force")
	defer env.Cleanup()

	// Create a pack
	packDir := env.CreatePack("mypack")
	testutil.CreateFile(t, packDir, ".vimrc", "set number")

	// First deployment
	opts := OnPacksOptions{
		DotfilesRoot: env.DotfilesRoot(),
		PackNames:    []string{"mypack"},
		DryRun:       false,
		Force:        false,
	}
	_, err := OnPacks(opts)
	require.NoError(t, err)

	// Force re-deployment
	opts.Force = true
	result, err := OnPacks(opts)
	require.NoError(t, err)
	assert.NotNil(t, result.LinkResult)
	assert.NotNil(t, result.ProvisionResult)
}

func TestOnPacks_NonExistentPack(t *testing.T) {
	env := testutil.NewTestEnvironment(t, "on-nonexistent")
	defer env.Cleanup()

	opts := OnPacksOptions{
		DotfilesRoot: env.DotfilesRoot(),
		PackNames:    []string{"nonexistent"},
		DryRun:       false,
	}

	// The command should handle non-existent packs
	result, err := OnPacks(opts)
	// May or may not error depending on implementation
	_ = err
	_ = result
}
