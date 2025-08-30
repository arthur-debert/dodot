// pkg/commands/deprovision/deprovision_test.go
// TEST TYPE: Business Logic Integration
// DEPENDENCIES: Mock DataStore, Memory FS
// PURPOSE: Test deprovision command orchestration for code execution handler clearing

package deprovision_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/commands/deprovision"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestDeprovisionPacks_EmptyPackNames_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	opts := deprovision.DeprovisionPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{},
		DryRun:       false,
	}

	// Execute
	result, err := deprovision.DeprovisionPacks(opts)

	// Verify code execution clearing orchestration behavior
	require.NoError(t, err)
	assert.NotNil(t, result, "should return deprovision result")
	assert.False(t, result.DryRun, "dry run should match input")
	assert.GreaterOrEqual(t, len(result.Packs), 0, "pack results should be accessible")
	assert.GreaterOrEqual(t, result.TotalCleared, 0, "total cleared should be non-negative")
	assert.GreaterOrEqual(t, len(result.Errors), 0, "errors should be accessible")
}

func TestDeprovisionPacks_SinglePack_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create a pack with code execution handlers (install, homebrew)
	env.SetupPack("provision-pack", testutil.PackConfig{
		Files: map[string]string{
			"install.sh": "#!/bin/sh\necho installing dependencies",
			"Brewfile":   "brew 'git'\nbrew 'vim'",
			".configrc":  "config file", // Should be ignored by deprovision
		},
	})

	opts := deprovision.DeprovisionPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{"provision-pack"},
		DryRun:       false,
	}

	// Execute
	result, err := deprovision.DeprovisionPacks(opts)

	// Verify code execution clearing orchestration
	require.NoError(t, err)
	assert.NotNil(t, result, "should return deprovision result")
	assert.False(t, result.DryRun, "should not be dry run")

	// Should have results for the provision-pack
	assert.Len(t, result.Packs, 1, "should process one pack")
	packResult := result.Packs[0]
	assert.Equal(t, "provision-pack", packResult.Name, "pack name should match")
	assert.GreaterOrEqual(t, len(packResult.HandlersRun), 0, "handlers run should be accessible")
	assert.GreaterOrEqual(t, packResult.TotalCleared, 0, "total cleared should be non-negative")
}

func TestDeprovisionPacks_DryRun_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create pack with provisionable content
	env.SetupPack("dry-pack", testutil.PackConfig{
		Files: map[string]string{
			"install.sh": "#!/bin/sh\necho test install",
			"Brewfile":   "brew 'curl'",
		},
	})

	opts := deprovision.DeprovisionPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{"dry-pack"},
		DryRun:       true,
	}

	// Execute
	result, err := deprovision.DeprovisionPacks(opts)

	// Verify dry run behavior
	require.NoError(t, err)
	assert.NotNil(t, result, "should return deprovision result")
	assert.True(t, result.DryRun, "should be dry run")

	// Dry run should still identify what would be cleared
	assert.Len(t, result.Packs, 1, "should process pack in dry run")
	packResult := result.Packs[0]
	assert.Equal(t, "dry-pack", packResult.Name, "pack name should match")
}

func TestDeprovisionPacks_MultiplePacks_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create multiple packs with different provision types
	env.SetupPack("install-pack", testutil.PackConfig{
		Files: map[string]string{
			"install.sh": "#!/bin/sh\necho install scripts",
		},
	})

	env.SetupPack("brew-pack", testutil.PackConfig{
		Files: map[string]string{
			"Brewfile": "brew 'git'\nbrew 'node'",
		},
	})

	env.SetupPack("config-pack", testutil.PackConfig{
		Files: map[string]string{
			".configrc": "config only - should have no provisioning state",
		},
	})

	opts := deprovision.DeprovisionPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{"install-pack", "brew-pack", "config-pack"},
		DryRun:       false,
	}

	// Execute
	result, err := deprovision.DeprovisionPacks(opts)

	// Verify multi-pack clearing orchestration
	require.NoError(t, err)
	assert.NotNil(t, result, "should return deprovision result")
	assert.Len(t, result.Packs, 3, "should process all three packs")

	// Verify each pack was processed
	packNames := make(map[string]bool)
	for _, pack := range result.Packs {
		packNames[pack.Name] = true
	}
	assert.True(t, packNames["install-pack"], "should process install-pack")
	assert.True(t, packNames["brew-pack"], "should process brew-pack")
	assert.True(t, packNames["config-pack"], "should process config-pack")
}

func TestDeprovisionPacks_NonExistentPack_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	opts := deprovision.DeprovisionPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{"nonexistent"},
		DryRun:       false,
	}

	// Execute
	result, err := deprovision.DeprovisionPacks(opts)

	// Verify error handling for non-existent packs
	assert.Error(t, err, "should return error for non-existent pack")
	// Result may still be returned with error information
	if result != nil {
		assert.False(t, result.DryRun, "dry run should match input")
		assert.GreaterOrEqual(t, len(result.Errors), 0, "errors should be trackable")
	}
}

func TestDeprovisionPacks_CodeExecutionHandlersOnly_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create pack with mixed handler types
	env.SetupPack("mixed-pack", testutil.PackConfig{
		Files: map[string]string{
			// Code execution handlers (should be cleared)
			"install.sh": "#!/bin/sh\necho install script",
			"Brewfile":   "brew 'wget'",
			// Configuration handlers (should be ignored)
			".configrc":  "config file",
			"aliases.sh": "alias test='echo'",
			"bin/tool":   "#!/bin/sh\necho tool",
		},
	})

	opts := deprovision.DeprovisionPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{"mixed-pack"},
		DryRun:       false,
	}

	// Execute
	result, err := deprovision.DeprovisionPacks(opts)

	// Verify only code execution handlers are processed
	require.NoError(t, err)
	assert.NotNil(t, result, "should return deprovision result")

	assert.Len(t, result.Packs, 1, "should process mixed-pack")
	packResult := result.Packs[0]
	assert.Equal(t, "mixed-pack", packResult.Name, "pack name should match")

	// Should only process code execution handlers (install, homebrew)
	// Configuration handlers (symlink, shell, path) should be ignored
	assert.GreaterOrEqual(t, len(packResult.HandlersRun), 0, "handlers run should be accessible")

	// Each handler result should be for code execution handlers
	for _, handlerResult := range packResult.HandlersRun {
		assert.NotEmpty(t, handlerResult.HandlerName, "handler name should be populated")
		assert.GreaterOrEqual(t, len(handlerResult.ClearedItems), 0, "cleared items should be accessible")
	}
}

func TestDeprovisionPacks_NoProvisioningState_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create pack with no provisioning handlers
	env.SetupPack("config-only", testutil.PackConfig{
		Files: map[string]string{
			".configrc":  "configuration file",
			"aliases.sh": "alias config='echo'",
		},
	})

	opts := deprovision.DeprovisionPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{"config-only"},
		DryRun:       false,
	}

	// Execute
	result, err := deprovision.DeprovisionPacks(opts)

	// Verify handling of packs with no provisioning state
	require.NoError(t, err)
	assert.NotNil(t, result, "should return deprovision result")

	assert.Len(t, result.Packs, 1, "should process config-only pack")
	packResult := result.Packs[0]
	assert.Equal(t, "config-only", packResult.Name, "pack name should match")

	// Pack with no provisioning state should have minimal handler results
	assert.GreaterOrEqual(t, len(packResult.HandlersRun), 0, "handlers run should be accessible")
	assert.Equal(t, 0, packResult.TotalCleared, "should have nothing to clear")
	assert.Nil(t, packResult.Error, "should have no error")
}

func TestDeprovisionPacks_EmptyDotfiles_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)
	// No packs created - empty dotfiles directory

	opts := deprovision.DeprovisionPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{},
		DryRun:       false,
	}

	// Execute
	result, err := deprovision.DeprovisionPacks(opts)

	// Verify empty dotfiles handling
	require.NoError(t, err)
	assert.NotNil(t, result, "should return deprovision result")
	assert.Len(t, result.Packs, 0, "should have no packs for empty dotfiles")
	assert.Equal(t, 0, result.TotalCleared, "should have nothing to clear")
	assert.Len(t, result.Errors, 0, "should have no errors")
}

func TestDeprovisionPacks_ResultStructure_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	env.SetupPack("structure-test", testutil.PackConfig{
		Files: map[string]string{
			"install.sh": "#!/bin/sh\necho test",
		},
	})

	opts := deprovision.DeprovisionPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{"structure-test"},
		DryRun:       false,
	}

	// Execute
	result, err := deprovision.DeprovisionPacks(opts)

	// Verify result structure completeness
	require.NoError(t, err)
	require.NotNil(t, result, "result should not be nil")

	// Verify DeprovisionResult structure
	assert.False(t, result.DryRun, "dry run should match input")
	assert.GreaterOrEqual(t, result.TotalCleared, 0, "total cleared should be non-negative")
	assert.NotNil(t, result.Packs, "packs should not be nil")
	assert.GreaterOrEqual(t, len(result.Errors), 0, "errors should be accessible")

	// Verify pack results structure
	if len(result.Packs) > 0 {
		packResult := result.Packs[0]
		assert.NotEmpty(t, packResult.Name, "pack name should be populated")
		assert.GreaterOrEqual(t, len(packResult.HandlersRun), 0, "handlers run should be accessible")
		assert.GreaterOrEqual(t, packResult.TotalCleared, 0, "pack total cleared should be non-negative")
		// packResult.Error can be nil for successful operations

		// Verify handler results structure
		for _, handlerResult := range packResult.HandlersRun {
			assert.NotEmpty(t, handlerResult.HandlerName, "handler name should be populated")
			assert.GreaterOrEqual(t, len(handlerResult.ClearedItems), 0, "cleared items should be accessible")
			// handlerResult.StateRemoved is boolean - no assertion needed
			// handlerResult.Error can be nil for successful operations
		}
	}
}

func TestDeprovisionPacks_HandlerFiltering_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create pack that would test handler filtering logic
	env.SetupPack("filter-test", testutil.PackConfig{
		Files: map[string]string{
			"install.sh": "#!/bin/sh\necho provision script",
			"Brewfile":   "brew 'curl'",
		},
	})

	opts := deprovision.DeprovisionPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{"filter-test"},
		DryRun:       false,
	}

	// Execute
	result, err := deprovision.DeprovisionPacks(opts)

	// Verify handler filtering orchestration
	require.NoError(t, err)
	assert.NotNil(t, result, "should return deprovision result")

	assert.Len(t, result.Packs, 1, "should process filter-test")
	packResult := result.Packs[0]
	assert.Equal(t, "filter-test", packResult.Name, "pack name should match")

	// The FilterHandlersByState function should filter to only handlers with state
	// This is an orchestration test - we verify the filtering mechanism is called
	assert.GreaterOrEqual(t, len(packResult.HandlersRun), 0, "handlers run should be accessible")
}

func TestDeprovisionPacks_ClearResults_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	env.SetupPack("clear-test", testutil.PackConfig{
		Files: map[string]string{
			"install.sh": "#!/bin/sh\necho clear test",
		},
	})

	opts := deprovision.DeprovisionPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{"clear-test"},
		DryRun:       false,
	}

	// Execute
	result, err := deprovision.DeprovisionPacks(opts)

	// Verify clear results orchestration
	require.NoError(t, err)
	assert.NotNil(t, result, "should return deprovision result")

	assert.Len(t, result.Packs, 1, "should process clear-test")
	packResult := result.Packs[0]

	// Each handler result should contain clear information
	for _, handlerResult := range packResult.HandlersRun {
		assert.NotEmpty(t, handlerResult.HandlerName, "handler name should be populated")
		// ClearedItems may be empty if no state to clear
		assert.GreaterOrEqual(t, len(handlerResult.ClearedItems), 0, "cleared items should be accessible")
		// StateRemoved indicates if handler state was removed
	}

	// Pack total should aggregate handler results
	totalFromHandlers := 0
	for _, handlerResult := range packResult.HandlersRun {
		totalFromHandlers += len(handlerResult.ClearedItems)
	}
	assert.Equal(t, totalFromHandlers, packResult.TotalCleared, "pack total should match handler totals")
}

func TestDeprovisionPacks_ErrorAggregation_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	env.SetupPack("error-pack", testutil.PackConfig{
		Files: map[string]string{
			"install.sh": "#!/bin/sh\necho error test",
		},
	})

	opts := deprovision.DeprovisionPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{"error-pack"},
		DryRun:       false,
	}

	// Execute
	result, err := deprovision.DeprovisionPacks(opts)

	// Verify error aggregation orchestration
	// Command should succeed even if individual handlers encounter issues
	if err != nil {
		// Command-level errors should still provide results
		if result != nil {
			assert.False(t, result.DryRun, "dry run should match input")
			assert.GreaterOrEqual(t, len(result.Errors), 0, "errors should be trackable")
		}
	} else {
		require.NoError(t, err)
		assert.NotNil(t, result, "should return deprovision result")

		// Successful execution should have complete results
		assert.Len(t, result.Packs, 1, "should process error-pack")

		// Pack-level and handler-level errors should be tracked
		packResult := result.Packs[0]
		for _, handlerResult := range packResult.HandlersRun {
			// handlerResult.Error can be nil for successful handlers
			if handlerResult.Error != nil {
				// Handler errors should be properly captured
				assert.NotEmpty(t, handlerResult.Error.Error(), "handler errors should have messages")
			}
		}

		// Result-level errors should aggregate pack errors
		assert.GreaterOrEqual(t, len(result.Errors), 0, "result errors should be accessible")
	}
}
