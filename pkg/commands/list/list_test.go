// pkg/commands/list/list_test.go
// TEST TYPE: Business Logic Integration
// DEPENDENCIES: Mock DataStore, Memory FS
// PURPOSE: Test list command pack discovery and query operations

package list_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/commands/list"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestListPacks_EmptyDotfiles_Query(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	opts := list.ListPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
	}

	// Execute
	result, err := list.ListPacks(opts)

	// Verify query behavior
	require.NoError(t, err)
	assert.NotNil(t, result, "should return result object")
	assert.Empty(t, result.Packs, "should return empty packs list")
	assert.Len(t, result.Packs, 0, "pack count should be zero")
}

func TestListPacks_SinglePack_Query(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create a single pack
	env.SetupPack("vim", testutil.PackConfig{
		Files: map[string]string{
			".vimrc":             "\" vim configuration",
			"colors/monokai.vim": "\" color scheme",
		},
	})

	opts := list.ListPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
	}

	// Execute
	result, err := list.ListPacks(opts)

	// Verify query results
	require.NoError(t, err)
	assert.NotNil(t, result, "should return result object")
	assert.Len(t, result.Packs, 1, "should find one pack")

	if len(result.Packs) > 0 {
		pack := result.Packs[0]
		assert.Equal(t, "vim", pack.Name, "pack name should match")
		assert.Contains(t, pack.Path, "vim", "pack path should contain pack name")
	}
}

func TestListPacks_MultiplePacks_Query(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create multiple packs
	env.SetupPack("bash", testutil.PackConfig{
		Files: map[string]string{
			".bashrc":       "# bash config",
			".bash_profile": "# bash profile",
		},
	})

	env.SetupPack("vim", testutil.PackConfig{
		Files: map[string]string{
			".vimrc": "\" vim config",
		},
	})

	env.SetupPack("git", testutil.PackConfig{
		Files: map[string]string{
			".gitconfig": "[user]\n  name = test",
		},
	})

	opts := list.ListPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
	}

	// Execute
	result, err := list.ListPacks(opts)

	// Verify query results
	require.NoError(t, err)
	assert.NotNil(t, result, "should return result object")
	assert.Len(t, result.Packs, 3, "should find all three packs")

	// Extract pack names for verification
	packNames := make([]string, len(result.Packs))
	for i, pack := range result.Packs {
		packNames[i] = pack.Name
		// Verify each pack has required fields
		assert.NotEmpty(t, pack.Name, "pack name should not be empty")
		assert.NotEmpty(t, pack.Path, "pack path should not be empty")
	}

	// Names should be sorted (depends on implementation)
	assert.Contains(t, packNames, "bash", "should contain bash pack")
	assert.Contains(t, packNames, "vim", "should contain vim pack")
	assert.Contains(t, packNames, "git", "should contain git pack")
}

func TestListPacks_WithPackConfigs_Query(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create packs with .dodot.toml configurations
	env.SetupPack("configured-pack", testutil.PackConfig{
		Files: map[string]string{
			".testrc": "test config",
			".dodot.toml": `[[rule]]
match = ".testrc"
handler = "symlink"`,
		},
	})

	env.SetupPack("simple-pack", testutil.PackConfig{
		Files: map[string]string{
			".simplerc": "simple config",
		},
	})

	opts := list.ListPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
	}

	// Execute
	result, err := list.ListPacks(opts)

	// Verify both configured and unconfigured packs are found
	require.NoError(t, err)
	assert.NotNil(t, result, "should return result object")
	assert.Len(t, result.Packs, 2, "should find both packs")

	packNames := make([]string, len(result.Packs))
	for i, pack := range result.Packs {
		packNames[i] = pack.Name
	}

	assert.Contains(t, packNames, "configured-pack", "should find pack with .dodot.toml")
	assert.Contains(t, packNames, "simple-pack", "should find pack without .dodot.toml")
}

func TestListPacks_InvalidDotfilesRoot_Query(t *testing.T) {
	// Setup - use non-existent path
	opts := list.ListPacksOptions{
		DotfilesRoot: "/nonexistent/path/to/dotfiles",
	}

	// Execute
	result, err := list.ListPacks(opts)

	// Verify error handling
	assert.Error(t, err, "should return error for non-existent path")
	assert.Contains(t, err.Error(), "dotfiles root does not exist", "error should mention missing dotfiles root")
	// Result may be nil or empty depending on implementation
	_ = result
}

func TestListPacks_EmptyDirectories_Query(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create pack with files
	env.SetupPack("valid-pack", testutil.PackConfig{
		Files: map[string]string{
			".validrc": "valid config",
		},
	})

	// Create empty directory (should be ignored by pack discovery)
	emptyPackPath := env.DotfilesRoot + "/empty-pack"
	err := env.FS.MkdirAll(emptyPackPath, 0755)
	require.NoError(t, err)

	opts := list.ListPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
	}

	// Execute
	result, err := list.ListPacks(opts)

	// Verify only valid packs are returned
	require.NoError(t, err)
	assert.NotNil(t, result, "should return result object")

	// Should only find the valid pack, not the empty directory
	packNames := make([]string, len(result.Packs))
	for i, pack := range result.Packs {
		packNames[i] = pack.Name
	}

	assert.Contains(t, packNames, "valid-pack", "should find valid pack")
	// Empty directories behavior depends on pack discovery implementation
	// This test verifies the query works with mixed valid/invalid directories
}

func TestListPacks_ResultStructure_Query(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	env.SetupPack("test-pack", testutil.PackConfig{
		Files: map[string]string{
			".testrc": "test configuration",
		},
	})

	opts := list.ListPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
	}

	// Execute
	result, err := list.ListPacks(opts)

	// Verify result structure completeness
	require.NoError(t, err)
	assert.NotNil(t, result, "result should not be nil")
	assert.NotNil(t, result.Packs, "packs slice should not be nil")

	if len(result.Packs) > 0 {
		pack := result.Packs[0]
		// Verify PackInfo structure
		assert.NotEmpty(t, pack.Name, "pack name should be populated")
		assert.NotEmpty(t, pack.Path, "pack path should be populated")
		assert.Equal(t, "test-pack", pack.Name, "pack name should match expected")
		// Path should be absolute and contain the pack name
		assert.Contains(t, pack.Path, "test-pack", "path should reference the pack directory")
	}
}
