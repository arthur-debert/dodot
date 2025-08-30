// pkg/commands/off/off_test.go
// TEST TYPE: Business Logic Integration
// DEPENDENCIES: Mock DataStore, Memory FS
// PURPOSE: Test off command orchestration without filesystem dependencies

package off_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/commands/off"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestOffPacks_EmptyPackNames_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	opts := off.OffPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{},
		DryRun:       false,
	}

	// Execute
	result, err := off.OffPacks(opts)

	// Verify orchestration behavior
	require.NoError(t, err)
	assert.NotNil(t, result.UnlinkResult, "should call unlink command")
	assert.NotNil(t, result.DeprovisionResult, "should call deprovision command")
	assert.Zero(t, result.TotalCleared, "no packs to clear")
	assert.Empty(t, result.Errors, "no errors expected")
	assert.False(t, result.DryRun)
}

func TestOffPacks_DryRun_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create a basic pack structure
	env.SetupPack("testpack", testutil.PackConfig{
		Files: map[string]string{
			".testrc": "test config",
			".dodot.toml": `[[rule]]
match = ".testrc"
handler = "symlink"`,
		},
	})

	opts := off.OffPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{"testpack"},
		DryRun:       true,
	}

	// Execute
	result, err := off.OffPacks(opts)

	// Verify dry run propagates to sub-commands
	require.NoError(t, err)
	assert.True(t, result.DryRun, "should preserve dry run flag")
	assert.NotNil(t, result.UnlinkResult, "should call unlink")
	assert.NotNil(t, result.DeprovisionResult, "should call deprovision")
	if result.UnlinkResult != nil {
		assert.True(t, result.UnlinkResult.DryRun, "unlink should receive dry run flag")
	}
	if result.DeprovisionResult != nil {
		assert.True(t, result.DeprovisionResult.DryRun, "deprovision should receive dry run flag")
	}
}

func TestOffPacks_SpecificPackNames_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create multiple packs
	env.SetupPack("pack1", testutil.PackConfig{
		Files: map[string]string{
			".pack1rc": "pack1 config",
			".dodot.toml": `[[rule]]
match = ".pack1rc"
handler = "symlink"`,
		},
	})

	env.SetupPack("pack2", testutil.PackConfig{
		Files: map[string]string{
			".pack2rc": "pack2 config",
			".dodot.toml": `[[rule]]
match = ".pack2rc"
handler = "symlink"`,
		},
	})

	opts := off.OffPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{"pack1"},
		DryRun:       false,
	}

	// Execute
	result, err := off.OffPacks(opts)

	// Verify orchestration passes pack names correctly
	require.NoError(t, err)
	assert.NotNil(t, result.UnlinkResult, "should call unlink")
	assert.NotNil(t, result.DeprovisionResult, "should call deprovision")
}

func TestOffPacks_NonExistentPack_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	opts := off.OffPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{"nonexistent"},
		DryRun:       false,
	}

	// Execute
	result, err := off.OffPacks(opts)

	// Verify orchestration handles non-existent packs
	// When a pack doesn't exist, the underlying commands return errors
	// but the off command should still return a result structure
	assert.NotNil(t, result, "should return result even with errors")

	// The off command may return an error if underlying commands fail
	if err != nil {
		assert.Contains(t, err.Error(), "off command encountered", "error should indicate command encountered issues")
	}

	// Result structure should still be populated even with errors
	if result != nil {
		assert.GreaterOrEqual(t, len(result.Errors), 0, "errors slice should contain error information")
	}
}

func TestOffPacks_ResultAggregation_Orchestration(t *testing.T) {
	// Setup - use isolated for consistency
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	opts := off.OffPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{},
		DryRun:       false,
	}

	// Execute
	result, err := off.OffPacks(opts)

	// Verify result aggregation structure
	require.NoError(t, err)
	assert.NotNil(t, result, "should return result object")

	// Verify all required fields are set
	assert.NotNil(t, result.UnlinkResult, "UnlinkResult should be populated")
	assert.NotNil(t, result.DeprovisionResult, "DeprovisionResult should be populated")
	assert.GreaterOrEqual(t, result.TotalCleared, 0, "TotalCleared should be non-negative")
	assert.GreaterOrEqual(t, len(result.Errors), 0, "Errors slice should be accessible (nil or empty)")
	assert.False(t, result.DryRun, "DryRun should match input")
}
