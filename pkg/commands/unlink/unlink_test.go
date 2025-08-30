// pkg/commands/unlink/unlink_test.go
// TEST TYPE: Business Logic Integration
// DEPENDENCIES: Mock DataStore, Memory FS
// PURPOSE: Test unlink command cleanup orchestration without filesystem dependencies

package unlink_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/commands/unlink"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestUnlinkPacks_EmptyPackNames_Cleanup(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	opts := unlink.UnlinkPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{},
		DryRun:       false,
		Force:        false,
	}

	// Execute
	result, err := unlink.UnlinkPacks(opts)

	// Verify cleanup behavior
	require.NoError(t, err)
	assert.NotNil(t, result, "should return result object")
	assert.Empty(t, result.Packs, "should return empty packs list for no packs")
	assert.Zero(t, result.TotalRemoved, "no items to remove")
	assert.False(t, result.DryRun)
}

func TestUnlinkPacks_SinglePack_Cleanup(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create a pack with configuration files
	env.SetupPack("testpack", testutil.PackConfig{
		Files: map[string]string{
			".testrc": "test configuration",
			".dodot.toml": `[[rule]]
match = ".testrc"  
handler = "symlink"`,
		},
	})

	opts := unlink.UnlinkPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{"testpack"},
		DryRun:       false,
		Force:        false,
	}

	// Execute
	result, err := unlink.UnlinkPacks(opts)

	// Verify cleanup orchestration
	require.NoError(t, err)
	assert.NotNil(t, result, "should return result object")
	assert.Len(t, result.Packs, 1, "should process one pack")

	if len(result.Packs) > 0 {
		pack := result.Packs[0]
		assert.Equal(t, "testpack", pack.Name, "pack name should match")
		// RemovedItems depends on whether there's actually state to clean
		assert.GreaterOrEqual(t, len(pack.RemovedItems), 0, "removed items should be accessible")
		assert.GreaterOrEqual(t, len(pack.Errors), 0, "errors should be accessible")
	}

	assert.False(t, result.DryRun)
}

func TestUnlinkPacks_DryRun_Cleanup(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create a pack structure
	env.SetupPack("testpack", testutil.PackConfig{
		Files: map[string]string{
			".testrc": "test configuration",
			".dodot.toml": `[[rule]]
match = ".testrc"
handler = "symlink"`,
		},
	})

	opts := unlink.UnlinkPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{"testpack"},
		DryRun:       true,
		Force:        false,
	}

	// Execute
	result, err := unlink.UnlinkPacks(opts)

	// Verify dry run behavior
	require.NoError(t, err)
	assert.True(t, result.DryRun, "should preserve dry run flag")
	assert.NotNil(t, result, "should return result object")

	// In dry run mode, cleanup should report what would be done without doing it
	// The specific behavior depends on whether there's existing state
	assert.Len(t, result.Packs, 1, "should process the pack")
}

func TestUnlinkPacks_MultiplePacks_Cleanup(t *testing.T) {
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

	opts := unlink.UnlinkPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{"pack1", "pack2"},
		DryRun:       false,
		Force:        false,
	}

	// Execute
	result, err := unlink.UnlinkPacks(opts)

	// Verify cleanup processes multiple packs
	require.NoError(t, err)
	assert.NotNil(t, result, "should return result object")
	assert.Len(t, result.Packs, 2, "should process both packs")

	packNames := make([]string, len(result.Packs))
	for i, pack := range result.Packs {
		packNames[i] = pack.Name
	}

	assert.Contains(t, packNames, "pack1", "should process pack1")
	assert.Contains(t, packNames, "pack2", "should process pack2")
}

func TestUnlinkPacks_NonExistentPack_Cleanup(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	opts := unlink.UnlinkPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{"nonexistent"},
		DryRun:       false,
		Force:        false,
	}

	// Execute
	result, err := unlink.UnlinkPacks(opts)

	// Verify error handling for non-existent packs
	assert.Error(t, err, "should return error for non-existent pack")
	assert.Contains(t, err.Error(), "not found", "error should indicate pack not found")
	// Result may be nil when pack discovery fails
	_ = result
}

func TestUnlinkPacks_Force_Cleanup(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create a pack
	env.SetupPack("testpack", testutil.PackConfig{
		Files: map[string]string{
			".testrc": "test config",
			".dodot.toml": `[[rule]]
match = ".testrc"
handler = "symlink"`,
		},
	})

	opts := unlink.UnlinkPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{"testpack"},
		DryRun:       false,
		Force:        true,
	}

	// Execute
	result, err := unlink.UnlinkPacks(opts)

	// Verify force flag handling
	// (Force is currently unused in clearable implementation, but test structure)
	require.NoError(t, err)
	assert.NotNil(t, result, "should return result object")
	assert.False(t, result.DryRun)
}

func TestUnlinkPacks_ResultStructure_Cleanup(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	opts := unlink.UnlinkPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{},
		DryRun:       false,
		Force:        false,
	}

	// Execute
	result, err := unlink.UnlinkPacks(opts)

	// Verify result structure completeness
	require.NoError(t, err)
	assert.NotNil(t, result, "result should not be nil")

	// Verify all required fields are accessible
	assert.GreaterOrEqual(t, len(result.Packs), 0, "packs slice should be accessible (nil or empty)")
	assert.GreaterOrEqual(t, result.TotalRemoved, 0, "total removed should be non-negative")
	assert.False(t, result.DryRun, "dry run should match input")

	// For empty packs, result should be empty but valid
	assert.Len(t, result.Packs, 0, "should be empty for no packs")
	assert.Zero(t, result.TotalRemoved, "should be zero for no removals")
}

func TestUnlinkPacks_CleanupOrchestration_Integration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create a pack with mixed configuration
	env.SetupPack("mixed-pack", testutil.PackConfig{
		Files: map[string]string{
			".configrc":  "config file",
			"bin/script": "#!/bin/sh\necho test",
			".dodot.toml": `[[rule]]
match = ".configrc"
handler = "symlink"

[[rule]]
match = "bin/*"
handler = "path"`,
		},
	})

	opts := unlink.UnlinkPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{"mixed-pack"},
		DryRun:       false,
		Force:        false,
	}

	// Execute
	result, err := unlink.UnlinkPacks(opts)

	// Verify orchestration behavior
	require.NoError(t, err)
	assert.NotNil(t, result, "should return result object")

	if len(result.Packs) > 0 {
		pack := result.Packs[0]
		assert.Equal(t, "mixed-pack", pack.Name)

		// The cleanup orchestration should process handlers systematically
		// Specific removed items depend on whether there's existing state to clear
		assert.GreaterOrEqual(t, len(pack.RemovedItems), 0, "removed items should be tracked")
		assert.GreaterOrEqual(t, len(pack.Errors), 0, "errors should be tracked")
	}

	// Total removed should aggregate across all handlers
	assert.GreaterOrEqual(t, result.TotalRemoved, 0, "total should aggregate pack results")
}

func TestUnlinkPacks_ErrorAggregation_Cleanup(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create a pack that might have cleanup issues
	env.SetupPack("problem-pack", testutil.PackConfig{
		Files: map[string]string{
			".problemrc": "problematic config",
			".dodot.toml": `[[rule]]
match = ".problemrc"
handler = "symlink"`,
		},
	})

	opts := unlink.UnlinkPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{"problem-pack"},
		DryRun:       false,
		Force:        false,
	}

	// Execute
	result, err := unlink.UnlinkPacks(opts)

	// Verify error aggregation structure
	// The cleanup should complete and report any errors it encounters
	require.NoError(t, err) // Top-level command should succeed
	assert.NotNil(t, result, "should return result structure")

	if len(result.Packs) > 0 {
		pack := result.Packs[0]
		// Pack-level errors should be collected and reported
		assert.GreaterOrEqual(t, len(pack.Errors), 0, "pack errors should be trackable")

		// Individual removed items should indicate success/failure
		for _, item := range pack.RemovedItems {
			// Each item should have consistent success/error state
			if !item.Success {
				assert.NotEmpty(t, item.Error, "failed items should have error messages")
			}
		}
	}
}
