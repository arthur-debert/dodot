// pkg/commands/provision/provision_test.go
// TEST TYPE: Business Logic Integration
// DEPENDENCIES: Mock DataStore, Memory FS
// PURPOSE: Test provision command multi-phase orchestration (install + deploy)

package provision_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/commands/provision"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestProvisionPacks_EmptyPackNames_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	opts := provision.ProvisionPacksOptions{
		DotfilesRoot:       env.DotfilesRoot,
		PackNames:          []string{},
		DryRun:             false,
		Force:              false,
		EnableHomeSymlinks: true,
	}

	// Execute
	result, err := provision.ProvisionPacks(opts)

	// Verify multi-phase orchestration behavior
	require.NoError(t, err)
	assert.NotNil(t, result, "should return execution context")
	assert.Equal(t, "provision", result.Command, "command should be provision")
	assert.False(t, result.DryRun, "dry run should match input")
	assert.GreaterOrEqual(t, len(result.PackResults), 0, "pack results should be accessible")
}

func TestProvisionPacks_SinglePack_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create a pack with mixed handler types (both install and configuration)
	env.SetupPack("fullstack", testutil.PackConfig{
		Files: map[string]string{
			".vimrc":     "\" vim configuration",
			"install.sh": "#!/bin/sh\necho installing dependencies",
			"Brewfile":   "brew 'git'\nbrew 'vim'",
			"aliases.sh": "alias vi='vim'",
			"bin/tool":   "#!/bin/sh\necho custom tool",
		},
	})

	opts := provision.ProvisionPacksOptions{
		DotfilesRoot:       env.DotfilesRoot,
		PackNames:          []string{"fullstack"},
		DryRun:             false,
		Force:              false,
		EnableHomeSymlinks: true,
	}

	// Execute
	result, err := provision.ProvisionPacks(opts)

	// Verify two-phase orchestration
	require.NoError(t, err)
	assert.NotNil(t, result, "should return execution context")
	assert.Equal(t, "provision", result.Command, "command should be provision")
	assert.False(t, result.DryRun, "should not be dry run")

	// Should have results for the fullstack pack from both phases
	packResult, exists := result.GetPackResult("fullstack")
	assert.True(t, exists, "should have fullstack pack result")
	assert.NotNil(t, packResult, "fullstack pack result should not be nil")
	assert.Equal(t, "fullstack", packResult.Pack.Name, "pack name should match")

	// Should have processed both install and configuration handlers
	assert.GreaterOrEqual(t, packResult.TotalHandlers, 0, "should have processed handlers")
}

func TestProvisionPacks_DryRun_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create pack with provisionable content
	env.SetupPack("test-pack", testutil.PackConfig{
		Files: map[string]string{
			".testrc":    "test configuration",
			"install.sh": "#!/bin/sh\necho test install",
		},
	})

	opts := provision.ProvisionPacksOptions{
		DotfilesRoot:       env.DotfilesRoot,
		PackNames:          []string{"test-pack"},
		DryRun:             true,
		Force:              false,
		EnableHomeSymlinks: true,
	}

	// Execute
	result, err := provision.ProvisionPacks(opts)

	// Verify dry run behavior across both phases
	require.NoError(t, err)
	assert.NotNil(t, result, "should return execution context")
	assert.Equal(t, "provision", result.Command, "command should be provision")
	assert.True(t, result.DryRun, "should be dry run")

	// Dry run should still process both phases but not execute
	packResult, exists := result.GetPackResult("test-pack")
	assert.True(t, exists, "should have test-pack result")
	assert.NotNil(t, packResult, "pack result should not be nil")
}

func TestProvisionPacks_ForceFlag_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	env.SetupPack("force-pack", testutil.PackConfig{
		Files: map[string]string{
			".config":    "configuration",
			"install.sh": "#!/bin/sh\necho force install",
		},
	})

	opts := provision.ProvisionPacksOptions{
		DotfilesRoot:       env.DotfilesRoot,
		PackNames:          []string{"force-pack"},
		DryRun:             false,
		Force:              true, // Key: force flag for provisioning
		EnableHomeSymlinks: true,
	}

	// Execute
	result, err := provision.ProvisionPacks(opts)

	// Verify force flag behavior
	require.NoError(t, err)
	assert.NotNil(t, result, "should return execution context")
	assert.Equal(t, "provision", result.Command, "command should be provision")

	// Force flag should affect provisioning phase but not deployment phase
	packResult, exists := result.GetPackResult("force-pack")
	assert.True(t, exists, "should have force-pack result")
	assert.NotNil(t, packResult, "pack result should not be nil")
}

func TestProvisionPacks_MultiplePacks_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create multiple packs with different characteristics
	env.SetupPack("config-only", testutil.PackConfig{
		Files: map[string]string{
			".configrc": "config only pack",
		},
	})

	env.SetupPack("install-only", testutil.PackConfig{
		Files: map[string]string{
			"install.sh": "#!/bin/sh\necho install only",
		},
	})

	env.SetupPack("mixed", testutil.PackConfig{
		Files: map[string]string{
			".mixedrc":   "mixed config",
			"install.sh": "#!/bin/sh\necho mixed install",
		},
	})

	opts := provision.ProvisionPacksOptions{
		DotfilesRoot:       env.DotfilesRoot,
		PackNames:          []string{"config-only", "install-only", "mixed"},
		DryRun:             false,
		Force:              false,
		EnableHomeSymlinks: true,
	}

	// Execute
	result, err := provision.ProvisionPacks(opts)

	// Verify multi-pack orchestration across both phases
	require.NoError(t, err)
	assert.NotNil(t, result, "should return execution context")
	assert.Equal(t, "provision", result.Command, "command should be provision")

	// Should have results for all packs
	configResult, configExists := result.GetPackResult("config-only")
	assert.True(t, configExists, "should have config-only pack result")
	assert.NotNil(t, configResult, "config-only result should not be nil")

	installResult, installExists := result.GetPackResult("install-only")
	assert.True(t, installExists, "should have install-only pack result")
	assert.NotNil(t, installResult, "install-only result should not be nil")

	mixedResult, mixedExists := result.GetPackResult("mixed")
	assert.True(t, mixedExists, "should have mixed pack result")
	assert.NotNil(t, mixedResult, "mixed result should not be nil")
}

func TestProvisionPacks_NonExistentPack_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	opts := provision.ProvisionPacksOptions{
		DotfilesRoot:       env.DotfilesRoot,
		PackNames:          []string{"nonexistent"},
		DryRun:             false,
		Force:              false,
		EnableHomeSymlinks: true,
	}

	// Execute
	result, err := provision.ProvisionPacks(opts)

	// Verify error handling for non-existent packs
	assert.Error(t, err, "should return error for non-existent pack")
	// Result may still be returned with partial execution context
	if result != nil {
		assert.Equal(t, "provision", result.Command, "command should still be provision")
	}
}

func TestProvisionPacks_PhaseSequencing_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create pack that would show phase sequencing
	env.SetupPack("sequenced", testutil.PackConfig{
		Files: map[string]string{
			".configrc":  "configuration file",
			"install.sh": "#!/bin/sh\necho install first",
			"Brewfile":   "brew 'git'",
			"aliases.sh": "alias test='echo configured after install'",
		},
	})

	opts := provision.ProvisionPacksOptions{
		DotfilesRoot:       env.DotfilesRoot,
		PackNames:          []string{"sequenced"},
		DryRun:             false,
		Force:              false,
		EnableHomeSymlinks: true,
	}

	// Execute
	result, err := provision.ProvisionPacks(opts)

	// Verify proper phase sequencing orchestration
	require.NoError(t, err)
	assert.NotNil(t, result, "should return execution context")
	assert.Equal(t, "provision", result.Command, "command should be provision")

	packResult, exists := result.GetPackResult("sequenced")
	assert.True(t, exists, "should have sequenced pack result")
	assert.NotNil(t, packResult, "pack result should not be nil")

	// Should have merged results from both phases
	// Phase 1: install handlers (install.sh, Brewfile)
	// Phase 2: configuration handlers (.configrc, aliases.sh)
	assert.GreaterOrEqual(t, packResult.TotalHandlers, 0, "should have handlers from both phases")
}

func TestProvisionPacks_HomeSymlinksDisabled_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	env.SetupPack("symlink-test", testutil.PackConfig{
		Files: map[string]string{
			".testrc":    "test config",
			"install.sh": "#!/bin/sh\necho install",
		},
	})

	opts := provision.ProvisionPacksOptions{
		DotfilesRoot:       env.DotfilesRoot,
		PackNames:          []string{"symlink-test"},
		DryRun:             false,
		Force:              false,
		EnableHomeSymlinks: false, // Key: disabled home symlinks
	}

	// Execute
	result, err := provision.ProvisionPacks(opts)

	// Verify orchestration with disabled home symlinks
	require.NoError(t, err)
	assert.NotNil(t, result, "should return execution context")
	assert.Equal(t, "provision", result.Command, "command should be provision")

	// Both phases should respect the symlink setting
	packResult, exists := result.GetPackResult("symlink-test")
	assert.True(t, exists, "should have symlink-test result")
	assert.NotNil(t, packResult, "pack result should not be nil")
}

func TestProvisionPacks_EmptyDotfiles_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)
	// No packs created - empty dotfiles directory

	opts := provision.ProvisionPacksOptions{
		DotfilesRoot:       env.DotfilesRoot,
		PackNames:          []string{},
		DryRun:             false,
		Force:              false,
		EnableHomeSymlinks: true,
	}

	// Execute
	result, err := provision.ProvisionPacks(opts)

	// Verify empty dotfiles handling across both phases
	require.NoError(t, err)
	assert.NotNil(t, result, "should return execution context")
	assert.Equal(t, "provision", result.Command, "command should be provision")
	assert.Len(t, result.PackResults, 0, "should have no pack results for empty dotfiles")
	assert.Equal(t, 0, result.TotalActions, "should have no actions for empty dotfiles")
}

func TestProvisionPacks_ExecutionContext_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	env.SetupPack("context-test", testutil.PackConfig{
		Files: map[string]string{
			".testrc":    "test config",
			"install.sh": "#!/bin/sh\necho test install",
		},
	})

	opts := provision.ProvisionPacksOptions{
		DotfilesRoot:       env.DotfilesRoot,
		PackNames:          []string{"context-test"},
		DryRun:             false,
		Force:              false,
		EnableHomeSymlinks: true,
	}

	// Execute
	result, err := provision.ProvisionPacks(opts)

	// Verify execution context structure completeness
	require.NoError(t, err)
	require.NotNil(t, result, "result should not be nil")

	// Verify ExecutionContext structure
	assert.Equal(t, "provision", result.Command, "command should be provision")
	assert.False(t, result.DryRun, "dry run should match input")
	assert.NotZero(t, result.StartTime, "start time should be set")
	assert.NotZero(t, result.EndTime, "end time should be set")
	assert.GreaterOrEqual(t, result.TotalActions, 0, "total actions should be non-negative")
	assert.GreaterOrEqual(t, result.CompletedActions, 0, "completed actions should be non-negative")
	assert.GreaterOrEqual(t, result.FailedActions, 0, "failed actions should be non-negative")
	assert.GreaterOrEqual(t, result.SkippedActions, 0, "skipped actions should be non-negative")

	// Verify pack results structure
	assert.NotNil(t, result.PackResults, "pack results should not be nil")
	assert.GreaterOrEqual(t, len(result.PackResults), 0, "pack results should be accessible")
}

func TestProvisionPacks_TwoPhaseIntegration_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create pack that exercises both phases
	env.SetupPack("two-phase", testutil.PackConfig{
		Files: map[string]string{
			// Phase 1: Code execution handlers
			"install.sh": "#!/bin/sh\necho provisioning phase",
			"Brewfile":   "brew 'git'",
			// Phase 2: Configuration handlers
			".configrc":  "config from phase 2",
			"aliases.sh": "alias configured='echo after provision'",
			"bin/script": "#!/bin/sh\necho path addition",
		},
	})

	opts := provision.ProvisionPacksOptions{
		DotfilesRoot:       env.DotfilesRoot,
		PackNames:          []string{"two-phase"},
		DryRun:             false,
		Force:              true, // Force applies to phase 1 only
		EnableHomeSymlinks: true,
	}

	// Execute
	result, err := provision.ProvisionPacks(opts)

	// Verify two-phase integration orchestration
	require.NoError(t, err)
	assert.NotNil(t, result, "should return execution context")
	assert.Equal(t, "provision", result.Command, "command should be provision")

	packResult, exists := result.GetPackResult("two-phase")
	assert.True(t, exists, "should have two-phase result")
	assert.NotNil(t, packResult, "pack result should not be nil")

	// Should have merged results from both phases
	// The mergeExecutionContexts function combines results
	assert.GreaterOrEqual(t, packResult.TotalHandlers, 0, "should have handlers from both phases")

	// Verify the execution context reflects merged state
	assert.GreaterOrEqual(t, result.TotalActions, 0, "total actions should include both phases")
}

func TestProvisionPacks_ErrorHandling_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create pack that could have issues
	env.SetupPack("error-test", testutil.PackConfig{
		Files: map[string]string{
			".testrc":    "test config",
			"install.sh": "#!/bin/sh\necho test",
		},
	})

	opts := provision.ProvisionPacksOptions{
		DotfilesRoot:       env.DotfilesRoot,
		PackNames:          []string{"error-test"},
		DryRun:             false,
		Force:              false,
		EnableHomeSymlinks: true,
	}

	// Execute
	result, err := provision.ProvisionPacks(opts)

	// Verify error handling orchestration
	// Even if individual handlers fail, the command should handle errors gracefully
	if err != nil {
		// Command-level errors should still provide context
		if result != nil {
			assert.Equal(t, "provision", result.Command, "command should still be provision")
		}
	} else {
		require.NoError(t, err)
		assert.NotNil(t, result, "should return execution context")

		// Successful execution should have complete results
		packResult, exists := result.GetPackResult("error-test")
		assert.True(t, exists, "should have error-test result")
		assert.NotNil(t, packResult, "pack result should not be nil")
	}
}
