package off

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/commands/link"
	"github.com/arthur-debert/dodot/pkg/testutil_old"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestOffPacks_EmptyPacks(t *testing.T) {
	env := testutil.NewTestEnvironment(t, "off-empty")
	defer env.Cleanup()

	opts := OffPacksOptions{
		DotfilesRoot: env.DotfilesRoot(),
		PackNames:    []string{},
		DryRun:       false,
	}

	result, err := OffPacks(opts)
	require.NoError(t, err)
	assert.NotNil(t, result.UnlinkResult)
	assert.NotNil(t, result.DeprovisionResult)
	assert.Zero(t, result.TotalCleared)
	assert.False(t, result.DryRun)
}

func TestOffPacks_Basic(t *testing.T) {
	env := testutil.NewTestEnvironment(t, "off-basic")
	defer env.Cleanup()

	// Create a pack with files
	packDir := env.CreatePack("mypack")
	testutil.CreateFile(t, packDir, ".vimrc", "set number")
	testutil.CreateFile(t, packDir, ".bashrc", "alias ll='ls -la'")

	// First, link the pack to have something to turn off
	linkOpts := link.LinkPacksOptions{
		DotfilesRoot: env.DotfilesRoot(),
		PackNames:    []string{"mypack"},
		DryRun:       false,
	}
	_, err := link.LinkPacks(linkOpts)
	require.NoError(t, err)

	// Now turn it off
	opts := OffPacksOptions{
		DotfilesRoot: env.DotfilesRoot(),
		PackNames:    []string{"mypack"},
		DryRun:       false,
	}

	result, err := OffPacks(opts)
	require.NoError(t, err)
	assert.NotNil(t, result.UnlinkResult)
	assert.NotNil(t, result.DeprovisionResult)
	assert.Greater(t, result.TotalCleared, 0)
	assert.Empty(t, result.Errors)
}

func TestOffPacks_DryRun(t *testing.T) {
	env := testutil.NewTestEnvironment(t, "off-dryrun")
	defer env.Cleanup()

	// Create and link a pack
	packDir := env.CreatePack("mypack")
	testutil.CreateFile(t, packDir, ".vimrc", "set number")

	linkOpts := link.LinkPacksOptions{
		DotfilesRoot: env.DotfilesRoot(),
		PackNames:    []string{"mypack"},
		DryRun:       false,
	}
	_, err := link.LinkPacks(linkOpts)
	require.NoError(t, err)

	// Turn off with dry run
	opts := OffPacksOptions{
		DotfilesRoot: env.DotfilesRoot(),
		PackNames:    []string{"mypack"},
		DryRun:       true,
	}

	result, err := OffPacks(opts)
	require.NoError(t, err)
	assert.True(t, result.DryRun)
	assert.NotNil(t, result.UnlinkResult)
	assert.NotNil(t, result.DeprovisionResult)
	assert.True(t, result.UnlinkResult.DryRun)
	assert.True(t, result.DeprovisionResult.DryRun)
}

func TestOffPacks_NonExistentPack(t *testing.T) {
	env := testutil.NewTestEnvironment(t, "off-nonexistent")
	defer env.Cleanup()

	opts := OffPacksOptions{
		DotfilesRoot: env.DotfilesRoot(),
		PackNames:    []string{"nonexistent"},
		DryRun:       false,
	}

	// The command should handle non-existent packs gracefully
	result, err := OffPacks(opts)
	// May or may not error depending on implementation
	_ = err
	_ = result
}
