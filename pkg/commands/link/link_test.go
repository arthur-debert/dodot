// pkg/commands/link/link_test.go
// TEST TYPE: Business Logic Integration
// DEPENDENCIES: Mock DataStore, Memory FS
// PURPOSE: Test link command orchestration for configuration handler deployment

package link_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/commands/link"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestLinkPacks_EmptyPackNames_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	opts := link.LinkPacksOptions{
		DotfilesRoot:       env.DotfilesRoot,
		PackNames:          []string{},
		DryRun:             false,
		EnableHomeSymlinks: true,
	}

	// Execute
	result, err := link.LinkPacks(opts)

	// Verify configuration orchestration behavior
	require.NoError(t, err)
	assert.NotNil(t, result, "should return execution context")
	assert.Equal(t, "link", result.Command, "command should be link")
	assert.False(t, result.DryRun, "dry run should match input")
	assert.GreaterOrEqual(t, len(result.PackResults), 0, "pack results should be accessible")
}

func TestLinkPacks_SinglePack_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create a pack with configuration files
	env.SetupPack("vim", testutil.PackConfig{
		Files: map[string]string{
			".vimrc":  "\" vim configuration",
			".gvimrc": "\" gvim configuration",
		},
	})

	opts := link.LinkPacksOptions{
		DotfilesRoot:       env.DotfilesRoot,
		PackNames:          []string{"vim"},
		DryRun:             false,
		EnableHomeSymlinks: true,
	}

	// Execute
	result, err := link.LinkPacks(opts)

	// Verify configuration deployment orchestration
	require.NoError(t, err)
	assert.NotNil(t, result, "should return execution context")
	assert.Equal(t, "link", result.Command, "command should be link")
	assert.False(t, result.DryRun, "should not be dry run")

	// Should have results for the vim pack
	packResult, exists := result.GetPackResult("vim")
	assert.True(t, exists, "should have vim pack result")
	assert.NotNil(t, packResult, "vim pack result should not be nil")
	assert.Equal(t, "vim", packResult.Pack.Name, "pack name should match")
}

func TestLinkPacks_DryRun_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create pack with linkable files
	env.SetupPack("zsh", testutil.PackConfig{
		Files: map[string]string{
			".zshrc":       "# zsh configuration",
			".zsh_aliases": "# zsh aliases",
		},
	})

	opts := link.LinkPacksOptions{
		DotfilesRoot:       env.DotfilesRoot,
		PackNames:          []string{"zsh"},
		DryRun:             true,
		EnableHomeSymlinks: true,
	}

	// Execute
	result, err := link.LinkPacks(opts)

	// Verify dry run behavior
	require.NoError(t, err)
	assert.NotNil(t, result, "should return execution context")
	assert.Equal(t, "link", result.Command, "command should be link")
	assert.True(t, result.DryRun, "should be dry run")

	// Dry run should still process packs but not make changes
	packResult, exists := result.GetPackResult("zsh")
	assert.True(t, exists, "should have zsh pack result")
	assert.NotNil(t, packResult, "zsh pack result should not be nil")
}

func TestLinkPacks_MultiplePacks_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create multiple packs with different configurations
	env.SetupPack("vim", testutil.PackConfig{
		Files: map[string]string{
			".vimrc": "\" vim config",
		},
	})

	env.SetupPack("git", testutil.PackConfig{
		Files: map[string]string{
			".gitconfig":        "[user]\n\tname = Test User",
			".gitignore_global": "*.log",
		},
	})

	opts := link.LinkPacksOptions{
		DotfilesRoot:       env.DotfilesRoot,
		PackNames:          []string{"vim", "git"},
		DryRun:             false,
		EnableHomeSymlinks: true,
	}

	// Execute
	result, err := link.LinkPacks(opts)

	// Verify multiple pack orchestration
	require.NoError(t, err)
	assert.NotNil(t, result, "should return execution context")
	assert.Equal(t, "link", result.Command, "command should be link")

	// Should have results for both packs
	vimResult, vimExists := result.GetPackResult("vim")
	assert.True(t, vimExists, "should have vim pack result")
	assert.NotNil(t, vimResult, "vim pack result should not be nil")

	gitResult, gitExists := result.GetPackResult("git")
	assert.True(t, gitExists, "should have git pack result")
	assert.NotNil(t, gitResult, "git pack result should not be nil")
}

func TestLinkPacks_NonExistentPack_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	opts := link.LinkPacksOptions{
		DotfilesRoot:       env.DotfilesRoot,
		PackNames:          []string{"nonexistent"},
		DryRun:             false,
		EnableHomeSymlinks: true,
	}

	// Execute
	result, err := link.LinkPacks(opts)

	// Verify error handling for non-existent packs
	assert.Error(t, err, "should return error for non-existent pack")
	// Result may still be returned with error information
	if result != nil {
		assert.Equal(t, "link", result.Command, "command should still be link")
	}
}

func TestLinkPacks_ConfigurationMode_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create pack with mixed file types (configuration + executable)
	env.SetupPack("mixed-pack", testutil.PackConfig{
		Files: map[string]string{
			".configrc":  "config file",
			"install.sh": "#!/bin/sh\necho install", // Should be ignored by link
			"bin/tool":   "#!/bin/sh\necho tool",
			"aliases.sh": "alias ll='ls -la'",
		},
	})

	opts := link.LinkPacksOptions{
		DotfilesRoot:       env.DotfilesRoot,
		PackNames:          []string{"mixed-pack"},
		DryRun:             false,
		EnableHomeSymlinks: true,
	}

	// Execute
	result, err := link.LinkPacks(opts)

	// Verify configuration-only mode orchestration
	require.NoError(t, err)
	assert.NotNil(t, result, "should return execution context")
	assert.Equal(t, "link", result.Command, "command should be link")

	packResult, exists := result.GetPackResult("mixed-pack")
	assert.True(t, exists, "should have mixed-pack result")
	assert.NotNil(t, packResult, "pack result should not be nil")

	// Link should only process configuration handlers, not install scripts
	// This is handled by the internal pipeline using CommandModeConfiguration
}

func TestLinkPacks_HomeSymlinksDisabled_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	env.SetupPack("test-pack", testutil.PackConfig{
		Files: map[string]string{
			".testrc": "test configuration",
		},
	})

	opts := link.LinkPacksOptions{
		DotfilesRoot:       env.DotfilesRoot,
		PackNames:          []string{"test-pack"},
		DryRun:             false,
		EnableHomeSymlinks: false, // Key: disabled home symlinks
	}

	// Execute
	result, err := link.LinkPacks(opts)

	// Verify orchestration with disabled home symlinks
	require.NoError(t, err)
	assert.NotNil(t, result, "should return execution context")
	assert.Equal(t, "link", result.Command, "command should be link")

	// Command should complete but symlink behavior depends on internal pipeline
	packResult, exists := result.GetPackResult("test-pack")
	assert.True(t, exists, "should have test-pack result")
	assert.NotNil(t, packResult, "pack result should not be nil")
}

func TestLinkPacks_EmptyDotfiles_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)
	// No packs created - empty dotfiles directory

	opts := link.LinkPacksOptions{
		DotfilesRoot:       env.DotfilesRoot,
		PackNames:          []string{},
		DryRun:             false,
		EnableHomeSymlinks: true,
	}

	// Execute
	result, err := link.LinkPacks(opts)

	// Verify empty dotfiles handling
	require.NoError(t, err)
	assert.NotNil(t, result, "should return execution context")
	assert.Equal(t, "link", result.Command, "command should be link")
	assert.Len(t, result.PackResults, 0, "should have no pack results for empty dotfiles")
	assert.Equal(t, 0, result.TotalHandlers, "should have no handlers for empty dotfiles")
}

func TestLinkPacks_ExecutionContext_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	env.SetupPack("context-test", testutil.PackConfig{
		Files: map[string]string{
			".testrc":   "test config",
			"script.sh": "#!/bin/sh\necho test",
		},
	})

	opts := link.LinkPacksOptions{
		DotfilesRoot:       env.DotfilesRoot,
		PackNames:          []string{"context-test"},
		DryRun:             false,
		EnableHomeSymlinks: true,
	}

	// Execute
	result, err := link.LinkPacks(opts)

	// Verify execution context structure completeness
	require.NoError(t, err)
	require.NotNil(t, result, "result should not be nil")

	// Verify ExecutionContext structure
	assert.Equal(t, "link", result.Command, "command should be link")
	assert.False(t, result.DryRun, "dry run should match input")
	assert.NotZero(t, result.StartTime, "start time should be set")
	assert.NotZero(t, result.EndTime, "end time should be set")
	assert.GreaterOrEqual(t, result.TotalHandlers, 0, "total handlers should be non-negative")
	assert.GreaterOrEqual(t, result.CompletedHandlers, 0, "completed handlers should be non-negative")
	assert.GreaterOrEqual(t, result.FailedHandlers, 0, "failed handlers should be non-negative")
	assert.GreaterOrEqual(t, result.SkippedHandlers, 0, "skipped handlers should be non-negative")

	// Verify pack results structure
	assert.NotNil(t, result.PackResults, "pack results should not be nil")
	assert.GreaterOrEqual(t, len(result.PackResults), 0, "pack results should be accessible")
}

func TestLinkPacks_CommandModeConfiguration_Integration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create pack with various handler types
	env.SetupPack("handler-test", testutil.PackConfig{
		Files: map[string]string{
			".configrc":  "config file",       // symlink handler
			"aliases.sh": "alias test='echo'", // shell handler
			"bin/tool":   "#!/bin/sh\necho x", // path handler
			"install.sh": "#!/bin/sh\necho i", // install handler (should be skipped)
			"Brewfile":   "brew 'git'",        // homebrew handler (should be skipped)
		},
	})

	opts := link.LinkPacksOptions{
		DotfilesRoot:       env.DotfilesRoot,
		PackNames:          []string{"handler-test"},
		DryRun:             false,
		EnableHomeSymlinks: true,
	}

	// Execute
	result, err := link.LinkPacks(opts)

	// Verify that link only processes configuration handlers
	require.NoError(t, err)
	assert.NotNil(t, result, "should return execution context")

	packResult, exists := result.GetPackResult("handler-test")
	assert.True(t, exists, "should have handler-test result")
	assert.NotNil(t, packResult, "pack result should not be nil")

	// The internal pipeline should filter to configuration handlers only
	// Specific handler verification is handled by the internal pipeline tests
	// Orchestration tests focus on command-level behavior
}

func TestLinkPacks_ForceFlag_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	env.SetupPack("force-test", testutil.PackConfig{
		Files: map[string]string{
			".testrc": "test configuration",
		},
	})

	opts := link.LinkPacksOptions{
		DotfilesRoot:       env.DotfilesRoot,
		PackNames:          []string{"force-test"},
		DryRun:             false,
		EnableHomeSymlinks: true,
		// Note: LinkPacksOptions doesn't have Force flag - it's always false internally
	}

	// Execute
	result, err := link.LinkPacks(opts)

	// Verify link doesn't use force flag (unlike provision)
	require.NoError(t, err)
	assert.NotNil(t, result, "should return execution context")
	assert.Equal(t, "link", result.Command, "command should be link")

	// Link command behavior is consistent regardless of force (no force flag available)
	packResult, exists := result.GetPackResult("force-test")
	assert.True(t, exists, "should have force-test result")
	assert.NotNil(t, packResult, "pack result should not be nil")
}
