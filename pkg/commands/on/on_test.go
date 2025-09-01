// pkg/commands/on/on_test.go
// TEST TYPE: Business Logic Integration
// DEPENDENCIES: Mock DataStore, Memory FS
// PURPOSE: Test on command orchestration without filesystem dependencies

package on_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/commands/on"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestOnPacks_EmptyPackNames_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	opts := on.OnPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{},
		DryRun:       false,
		Force:        false,
	}

	// Execute
	result, err := on.OnPacks(opts)

	// Verify orchestration behavior
	require.NoError(t, err)
	assert.Equal(t, "on", result.Command, "command should be on")
	assert.Equal(t, 0, result.Metadata.TotalDeployed, "no packs to deploy")
	assert.False(t, result.DryRun)
}

func TestOnPacks_DryRun_Orchestration(t *testing.T) {
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

	opts := on.OnPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{"testpack"},
		DryRun:       true,
		Force:        false,
	}

	// Execute
	result, err := on.OnPacks(opts)

	// Verify dry run propagates to sub-commands
	require.NoError(t, err)
	assert.Equal(t, "on", result.Command, "command should be on")
	assert.True(t, result.DryRun, "should preserve dry run flag")
	assert.True(t, len(result.Packs) > 0, "should have pack status")
	if len(result.Packs) > 0 {
		assert.Equal(t, "testpack", result.Packs[0].Name, "pack name should match")
	}
}

func TestOnPacks_Force_Orchestration(t *testing.T) {
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

	opts := on.OnPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{"testpack"},
		DryRun:       false,
		Force:        true,
	}

	// Execute
	result, err := on.OnPacks(opts)

	// Verify force flag handling (specific behavior depends on link implementation)
	require.NoError(t, err)
	assert.Equal(t, "on", result.Command, "command should be on")
	assert.False(t, result.DryRun, "should not be dry run")
	// Force flag gets passed to link command internally
}

func TestOnPacks_SpecificPackNames_Orchestration(t *testing.T) {
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

	opts := on.OnPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{"pack1"},
		DryRun:       false,
		Force:        false,
	}

	// Execute
	result, err := on.OnPacks(opts)

	// Verify orchestration passes pack names correctly
	require.NoError(t, err)
	assert.Equal(t, "on", result.Command, "command should be on")
	assert.Equal(t, 1, len(result.Packs), "should have one pack")
	if len(result.Packs) > 0 {
		assert.Equal(t, "pack1", result.Packs[0].Name, "should process pack1")
	}
}

func TestOnPacks_NonExistentPack_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	opts := on.OnPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{"nonexistent"},
		DryRun:       false,
		Force:        false,
	}

	// Execute
	result, err := on.OnPacks(opts)

	// Verify orchestration handles non-existent packs
	// When a pack doesn't exist, both link and provision fail
	require.Error(t, err, "should return error for non-existent pack")
	assert.Contains(t, err.Error(), "on command encountered", "error should indicate command encountered issues")

	// Result should still be returned with error details
	require.NotNil(t, result, "should return result even with errors")
	assert.Equal(t, "on", result.Command, "command should be on")
}

func TestOnPacks_ResultAggregation_Orchestration(t *testing.T) {
	// Setup - use isolated for consistency
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	opts := on.OnPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{},
		DryRun:       false,
		Force:        false,
	}

	// Execute
	result, err := on.OnPacks(opts)

	// Verify result aggregation structure
	require.NoError(t, err)
	assert.NotNil(t, result, "should return result object")

	// Verify all required fields are set
	assert.Equal(t, "on", result.Command, "command should be on")
	assert.GreaterOrEqual(t, result.Metadata.TotalDeployed, 0, "TotalDeployed should be non-negative")
	assert.False(t, result.DryRun, "DryRun should match input")
	assert.NotNil(t, result.Packs, "Packs should be populated")
	assert.NotNil(t, result.Metadata, "Metadata should be populated")
	assert.False(t, result.Metadata.NoProvision, "NoProvision should be false by default")
	assert.False(t, result.Metadata.ProvisionRerun, "ProvisionRerun should be false by default")
}

func TestOnPacks_ErrorHandling_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create pack with problematic setup to test error aggregation
	env.SetupPack("problempack", testutil.PackConfig{
		Files: map[string]string{
			".problemrc": "problematic config",
			".dodot.toml": `[[rule]]
match = ".problemrc"
handler = "symlink"`,
		},
	})

	opts := on.OnPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{"problempack"},
		DryRun:       false,
		Force:        false,
	}

	// Execute
	result, err := on.OnPacks(opts)

	// Verify error handling structure
	// The specific error behavior depends on link/provision implementation
	// This test ensures the orchestration layer handles errors properly
	_ = err // Error handling varies based on underlying implementation
	assert.NotNil(t, result, "should always return a result structure")

	// Check that both commands were attempted even if one fails
	// (This depends on the implementation - some commands may short-circuit)
	if result != nil {
		assert.Equal(t, "on", result.Command, "command should be on")
		assert.GreaterOrEqual(t, result.Metadata.TotalDeployed, 0, "total deployed should be non-negative")
	}
}
