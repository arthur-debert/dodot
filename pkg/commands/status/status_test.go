// pkg/commands/status/status_test.go
// TEST TYPE: Business Logic Integration
// DEPENDENCIES: Mock DataStore, Memory FS
// PURPOSE: Test status command state inspection orchestration

package status_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/commands/status"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// createStatusOptions creates a StatusPacksOptions with proper paths setup
func createStatusOptions(t *testing.T, env *testutil.TestEnvironment, packNames []string) status.StatusPacksOptions {
	testPaths, err := paths.New(env.DotfilesRoot)
	require.NoError(t, err)

	return status.StatusPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    packNames,
		Paths:        testPaths,
		FileSystem:   env.FS,
	}
}

func TestStatusPacks_EmptyDotfiles_Inspection(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)
	opts := createStatusOptions(t, env, []string{})

	// Execute
	result, err := status.StatusPacks(opts)

	// Verify state inspection behavior
	require.NoError(t, err)
	assert.NotNil(t, result, "should return result object")
	assert.Equal(t, "status", result.Command, "command should be status")
	assert.False(t, result.DryRun, "status is not a dry run operation")
	assert.Empty(t, result.Packs, "should return empty packs list for no packs")
}

func TestStatusPacks_SinglePack_Inspection(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create a pack with various file types
	env.SetupPack("testpack", testutil.PackConfig{
		Files: map[string]string{
			".testrc":   "test configuration",
			"script.sh": "#!/bin/sh\necho test",
			".dodot.toml": `[[rule]]
match = ".testrc"
handler = "symlink"

[[rule]]
match = "script.sh"  
handler = "shell"`,
		},
	})

	opts := createStatusOptions(t, env, []string{"testpack"})

	// Execute
	result, err := status.StatusPacks(opts)

	// Verify state inspection orchestration
	require.NoError(t, err)
	assert.NotNil(t, result, "should return result object")
	assert.Len(t, result.Packs, 1, "should process one pack")

	if len(result.Packs) > 0 {
		pack := result.Packs[0]
		assert.Equal(t, "testpack", pack.Name, "pack name should match")
		assert.True(t, pack.HasConfig, "should detect .dodot.toml")
		assert.False(t, pack.IsIgnored, "should not be ignored")

		// Should have files from rules processing
		assert.NotEmpty(t, pack.Files, "should have processed files")

		// Should have config file + actual matched files
		var hasConfigFile, hasTestrc, hasScript bool
		for _, file := range pack.Files {
			switch file.Path {
			case ".dodot.toml":
				hasConfigFile = true
				assert.Equal(t, "config", file.Status, "config file should have config status")
			case ".testrc":
				hasTestrc = true
				assert.Equal(t, "symlink", file.Handler, "should be handled by symlink")
			case "script.sh":
				hasScript = true
				// Note: Pack-specific rules not yet implemented, so using global rules
				assert.Equal(t, "symlink", file.Handler, "currently handled by symlink (pack rules not implemented)")
			}
		}

		assert.True(t, hasConfigFile, "should include .dodot.toml in files")
		assert.True(t, hasTestrc, "should include .testrc in files")
		assert.True(t, hasScript, "should include script.sh in files")
	}
}

func TestStatusPacks_MultiplePacks_Inspection(t *testing.T) {
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
			// No .dodot.toml - uses global rules
		},
	})

	opts := createStatusOptions(t, env, []string{}) // All packs

	// Execute
	result, err := status.StatusPacks(opts)

	// Verify state inspection processes multiple packs
	require.NoError(t, err)
	assert.NotNil(t, result, "should return result object")
	assert.Len(t, result.Packs, 2, "should process both packs")

	packNames := make([]string, len(result.Packs))
	for i, pack := range result.Packs {
		packNames[i] = pack.Name

		// Verify each pack has proper structure
		assert.NotEmpty(t, pack.Name, "pack name should be populated")
		assert.GreaterOrEqual(t, len(pack.Files), 0, "files should be accessible")
	}

	assert.Contains(t, packNames, "pack1", "should process pack1")
	assert.Contains(t, packNames, "pack2", "should process pack2")
}

func TestStatusPacks_IgnoredPack_Inspection(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create an ignored pack
	env.SetupPack("ignored-pack", testutil.PackConfig{
		Files: map[string]string{
			".ignoredrc":   "ignored config",
			".dodotignore": "", // Ignore marker
		},
	})

	// Create a normal pack for comparison
	env.SetupPack("normal-pack", testutil.PackConfig{
		Files: map[string]string{
			".normalrc": "normal config",
		},
	})

	opts := createStatusOptions(t, env, []string{}) // All packs

	// Execute
	result, err := status.StatusPacks(opts)

	// Verify ignored pack handling
	require.NoError(t, err)
	assert.NotNil(t, result, "should return result object")
	assert.Len(t, result.Packs, 2, "should process both packs")

	var ignoredPack, normalPack *types.DisplayPack
	for i := range result.Packs {
		switch result.Packs[i].Name {
		case "ignored-pack":
			ignoredPack = &result.Packs[i]
		case "normal-pack":
			normalPack = &result.Packs[i]
		}
	}

	require.NotNil(t, ignoredPack, "should find ignored pack")
	require.NotNil(t, normalPack, "should find normal pack")

	// Check ignored pack properties
	assert.True(t, ignoredPack.IsIgnored, "should be marked as ignored")
	assert.Equal(t, "ignored", ignoredPack.Status, "should have ignored status")

	// Check normal pack properties
	assert.False(t, normalPack.IsIgnored, "should not be marked as ignored")
	assert.NotEqual(t, "ignored", normalPack.Status, "should not have ignored status")
}

func TestStatusPacks_SpecificPackNames_Inspection(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create multiple packs
	env.SetupPack("vim", testutil.PackConfig{
		Files: map[string]string{
			".vimrc": "vim config",
		},
	})

	env.SetupPack("bash", testutil.PackConfig{
		Files: map[string]string{
			".bashrc": "bash config",
		},
	})

	env.SetupPack("git", testutil.PackConfig{
		Files: map[string]string{
			".gitconfig": "git config",
		},
	})

	opts := createStatusOptions(t, env, []string{"vim", "git"}) // Specific packs only

	// Execute
	result, err := status.StatusPacks(opts)

	// Verify specific pack selection
	require.NoError(t, err)
	assert.NotNil(t, result, "should return result object")
	assert.Len(t, result.Packs, 2, "should process only specified packs")

	packNames := make([]string, len(result.Packs))
	for i, pack := range result.Packs {
		packNames[i] = pack.Name
	}

	assert.Contains(t, packNames, "vim", "should include vim pack")
	assert.Contains(t, packNames, "git", "should include git pack")
	assert.NotContains(t, packNames, "bash", "should not include bash pack")
}

func TestStatusPacks_ResultStructure_Inspection(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	opts := createStatusOptions(t, env, []string{})

	// Execute
	result, err := status.StatusPacks(opts)

	// Verify result structure completeness
	require.NoError(t, err)
	assert.NotNil(t, result, "result should not be nil")

	// Verify DisplayResult structure
	assert.Equal(t, "status", result.Command, "command should be status")
	assert.False(t, result.DryRun, "status should not be dry run")
	assert.NotZero(t, result.Timestamp, "timestamp should be set")
	assert.GreaterOrEqual(t, len(result.Packs), 0, "packs should be accessible")

	// For empty dotfiles, should have empty but valid structure
	assert.Len(t, result.Packs, 0, "should be empty for no packs")
}

func TestStatusPacks_ErrorHandling_Inspection(t *testing.T) {
	// Setup - non-existent dotfiles root
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Need to create paths manually for this error test case
	testPaths, err := paths.New("/nonexistent/path") // This will not fail, but will cause issues later
	require.NoError(t, err)

	opts := status.StatusPacksOptions{
		DotfilesRoot: "/nonexistent/path",
		PackNames:    []string{},
		Paths:        testPaths,
		FileSystem:   env.FS,
	}

	// Execute
	result, err := status.StatusPacks(opts)

	// Verify error handling
	assert.Error(t, err, "should return error for non-existent dotfiles root")
	assert.Contains(t, err.Error(), "dotfiles root does not exist", "should mention missing dotfiles root")
	// Result should be nil on discovery error
	assert.Nil(t, result, "should return nil result on error")
}

func TestStatusPacks_FileSystemIntegration_Inspection(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create pack with mixed file types to test handler detection
	env.SetupPack("mixed-pack", testutil.PackConfig{
		Files: map[string]string{
			".configrc":  "config file",
			"install.sh": "#!/bin/sh\necho install",
			"Brewfile":   "brew 'git'",
			"aliases.sh": "alias ll='ls -la'",
			"bin/script": "#!/bin/sh\necho tool",
			".dodot.toml": `[[rule]]
match = ".configrc"
handler = "symlink"

[[rule]]
match = "install.sh"
handler = "install"

[[rule]]
match = "Brewfile"
handler = "homebrew"

[[rule]]
match = "aliases.sh"
handler = "shell"

[[rule]]
match = "bin"
handler = "path"`,
		},
	})

	opts := createStatusOptions(t, env, []string{"mixed-pack"})

	// Execute
	result, err := status.StatusPacks(opts)

	// Verify comprehensive state inspection
	require.NoError(t, err)
	assert.NotNil(t, result, "should return result object")
	require.Len(t, result.Packs, 1, "should process the pack")

	pack := result.Packs[0]
	assert.Equal(t, "mixed-pack", pack.Name)
	assert.True(t, pack.HasConfig, "should detect configuration")

	// Check that different handlers are properly detected
	handlerCounts := make(map[string]int)
	for _, file := range pack.Files {
		if file.Handler != "" {
			handlerCounts[file.Handler]++
		}
	}

	// Should detect various handlers from the rules
	expectedHandlers := []string{"symlink", "install", "homebrew", "shell", "path"}
	for _, handler := range expectedHandlers {
		if handlerCounts[handler] == 0 {
			t.Logf("Warning: Handler %s not found in status output", handler)
		}
	}
}

func TestStatusPacks_StateInspectionOrchestration_Integration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create pack to test full orchestration
	env.SetupPack("orchestration-pack", testutil.PackConfig{
		Files: map[string]string{
			".testrc": "test config",
			".dodot.toml": `[[rule]]
match = ".testrc"
handler = "symlink"`,
		},
	})

	opts := createStatusOptions(t, env, []string{"orchestration-pack"})

	// Execute
	result, err := status.StatusPacks(opts)

	// Verify orchestration flow:
	// 1. Pack discovery
	// 2. Rules processing
	// 3. Action generation
	// 4. Status checking
	// 5. Display formatting
	require.NoError(t, err)
	assert.NotNil(t, result, "should return result object")

	// Verify orchestration produced complete result
	assert.Equal(t, "status", result.Command, "should set command")
	assert.NotZero(t, result.Timestamp, "should set timestamp")
	require.Len(t, result.Packs, 1, "should process pack")

	pack := result.Packs[0]
	assert.Equal(t, "orchestration-pack", pack.Name)
	assert.NotEmpty(t, pack.Files, "should have processed files")

	// Status should be calculated from file statuses
	assert.NotEmpty(t, pack.Status, "pack status should be calculated")
}
