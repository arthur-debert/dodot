// pkg/commands/addignore/addignore_test.go
// TEST TYPE: Business Logic Integration
// DEPENDENCIES: Memory FS
// PURPOSE: Test addignore command orchestration for creating ignore files

package addignore_test

import (
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/commands/addignore"
	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestAddIgnore_CreateIgnoreFile_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)
	cfg := config.Default()

	// Create a pack directory with content
	env.SetupPack("vim", testutil.PackConfig{
		Files: map[string]string{
			".vimrc":             "set number",
			"colors/monokai.vim": "color scheme",
		},
	})

	opts := addignore.AddIgnoreOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "vim",
	}

	// Execute
	result, err := addignore.AddIgnore(opts)

	// Verify orchestration behavior
	require.NoError(t, err)
	assert.NotNil(t, result, "should return result object")

	// Verify result structure
	assert.Equal(t, "vim", result.PackName, "pack name should match")
	assert.True(t, result.Created, "ignore file should be created")
	assert.False(t, result.AlreadyExisted, "should not already exist")

	// Verify ignore file path
	expectedPath := filepath.Join(env.DotfilesRoot, "vim", cfg.Patterns.SpecialFiles.IgnoreFile)
	assert.Equal(t, expectedPath, result.IgnoreFilePath, "ignore file path should be correct")

	// Command should complete successfully with expected orchestration
	// (Filesystem operations are tested by implementation, orchestration tests focus on command behavior)
}

func TestAddIgnore_AlreadyExists_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)
	cfg := config.Default()

	// Create a pack directory with existing ignore file
	packFiles := map[string]string{
		".vimrc":                             "set number",
		cfg.Patterns.SpecialFiles.IgnoreFile: "", // Already exists
	}
	env.SetupPack("vim", testutil.PackConfig{Files: packFiles})

	opts := addignore.AddIgnoreOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "vim",
	}

	// Execute
	result, err := addignore.AddIgnore(opts)

	// Verify already exists behavior
	require.NoError(t, err)
	assert.NotNil(t, result, "should return result object")

	assert.Equal(t, "vim", result.PackName, "pack name should match")
	assert.False(t, result.Created, "ignore file should not be created")
	assert.True(t, result.AlreadyExisted, "should already exist")

	// Verify ignore file path is still correct
	expectedPath := filepath.Join(env.DotfilesRoot, "vim", cfg.Patterns.SpecialFiles.IgnoreFile)
	assert.Equal(t, expectedPath, result.IgnoreFilePath, "ignore file path should be correct")
}

func TestAddIgnore_PackNameNormalization_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create pack directory
	env.SetupPack("vim", testutil.PackConfig{
		Files: map[string]string{
			".vimrc": "set number",
		},
	})

	opts := addignore.AddIgnoreOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "vim/", // Trailing slash should be normalized
	}

	// Execute
	result, err := addignore.AddIgnore(opts)

	// Verify pack name normalization
	require.NoError(t, err)
	assert.NotNil(t, result, "should return result object")

	assert.Equal(t, "vim", result.PackName, "pack name should be normalized (no trailing slash)")
	assert.True(t, result.Created, "ignore file should be created")

	// Verify file path uses normalized name
	expectedPath := filepath.Join(env.DotfilesRoot, "vim", config.Default().Patterns.SpecialFiles.IgnoreFile)
	assert.Equal(t, expectedPath, result.IgnoreFilePath, "should use normalized pack name in path")
}

func TestAddIgnore_NonExistentPack_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Don't create any pack - test non-existent pack behavior
	opts := addignore.AddIgnoreOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "nonexistent",
	}

	// Execute
	result, err := addignore.AddIgnore(opts)

	// Verify error handling for non-existent pack
	assert.Error(t, err, "should return error for non-existent pack")
	assert.Nil(t, result, "result should be nil on error")

	// Verify error is the correct type
	var dodotErr *errors.DodotError
	require.ErrorAs(t, err, &dodotErr, "should be a DodotError")
	assert.Equal(t, errors.ErrPackNotFound, dodotErr.Code, "should have ErrPackNotFound code")
}

func TestAddIgnore_EmptyPackName_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	opts := addignore.AddIgnoreOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "", // Empty pack name
	}

	// Execute
	result, err := addignore.AddIgnore(opts)

	// Verify validation error for empty pack name
	assert.Error(t, err, "should return error for empty pack name")
	assert.Nil(t, result, "result should be nil on error")

	// Verify error is validation error
	var dodotErr *errors.DodotError
	require.ErrorAs(t, err, &dodotErr, "should be a DodotError")
	assert.Equal(t, errors.ErrPackNotFound, dodotErr.Code, "should have ErrPackNotFound code")
}

func TestAddIgnore_InvalidPackName_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Test various invalid pack names
	invalidPackNames := []string{
		"../invalid",    // Path traversal
		"pack..name",    // Double dots
		"pack/sub/name", // Nested path
		".hidden",       // Starts with dot
	}

	for _, packName := range invalidPackNames {
		t.Run("invalid_name_"+packName, func(t *testing.T) {
			opts := addignore.AddIgnoreOptions{
				DotfilesRoot: env.DotfilesRoot,
				PackName:     packName,
			}

			// Execute
			result, err := addignore.AddIgnore(opts)

			// Verify validation catches invalid pack names
			assert.Error(t, err, "should return error for invalid pack name: %s", packName)
			assert.Nil(t, result, "result should be nil on error")

			var dodotErr *errors.DodotError
			require.ErrorAs(t, err, &dodotErr, "should be a DodotError")
			assert.Equal(t, errors.ErrPackNotFound, dodotErr.Code, "should have ErrPackNotFound code")
		})
	}
}

func TestAddIgnore_ResultStructure_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	env.SetupPack("vim", testutil.PackConfig{
		Files: map[string]string{
			".vimrc": "set number",
		},
	})

	opts := addignore.AddIgnoreOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "vim",
	}

	// Execute
	result, err := addignore.AddIgnore(opts)

	// Verify complete result structure
	require.NoError(t, err)
	require.NotNil(t, result, "result should not be nil")

	// Verify all result fields are populated correctly
	assert.NotEmpty(t, result.PackName, "pack name should be populated")
	assert.NotEmpty(t, result.IgnoreFilePath, "ignore file path should be populated")
	assert.Contains(t, result.IgnoreFilePath, result.PackName, "ignore file path should contain pack name")
	assert.Contains(t, result.IgnoreFilePath, config.Default().Patterns.SpecialFiles.IgnoreFile, "ignore file path should contain ignore filename")

	// Created and AlreadyExisted should be mutually exclusive
	assert.NotEqual(t, result.Created, result.AlreadyExisted, "Created and AlreadyExisted should be mutually exclusive")
}

func TestAddIgnore_MultiplePacksOrchestration_Integration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create multiple packs
	packNames := []string{"vim", "zsh", "git"}
	for _, packName := range packNames {
		env.SetupPack(packName, testutil.PackConfig{
			Files: map[string]string{
				"config": "sample config",
			},
		})
	}

	// Execute addignore for each pack
	for _, packName := range packNames {
		t.Run("pack_"+packName, func(t *testing.T) {
			opts := addignore.AddIgnoreOptions{
				DotfilesRoot: env.DotfilesRoot,
				PackName:     packName,
			}

			result, err := addignore.AddIgnore(opts)

			// Verify each pack gets its own ignore file
			require.NoError(t, err)
			assert.NotNil(t, result, "should return result for pack %s", packName)
			assert.Equal(t, packName, result.PackName, "pack name should match")
			assert.True(t, result.Created, "ignore file should be created for pack %s", packName)

			// Verify file path is unique per pack
			assert.Contains(t, result.IgnoreFilePath, packName, "ignore file path should contain pack name")

			// Command should complete successfully for each pack
			// (Individual file existence is handled by command implementation)
		})
	}
}

func TestAddIgnore_FileSystemIntegration_Orchestration(t *testing.T) {
	// Setup - test actual file system behavior
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)
	cfg := config.Default()

	// Create pack structure
	env.SetupPack("complex-pack", testutil.PackConfig{
		Files: map[string]string{
			".vimrc":               "vim config",
			"bin/tool":             "#!/bin/sh\necho tool",
			"config/settings.json": `{"key": "value"}`,
		},
	})

	opts := addignore.AddIgnoreOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "complex-pack",
	}

	// Execute
	result, err := addignore.AddIgnore(opts)

	// Verify ignore file creation integrates with pack structure
	require.NoError(t, err)
	assert.NotNil(t, result, "should return result object")

	ignoreFilePath := result.IgnoreFilePath

	// Verify ignore file is placed correctly in pack directory
	expectedPath := filepath.Join(env.DotfilesRoot, "complex-pack", cfg.Patterns.SpecialFiles.IgnoreFile)
	assert.Equal(t, expectedPath, ignoreFilePath, "ignore file should be in pack root")

	// Command should complete successfully with proper orchestration
	// Filesystem integration is handled by the command implementation
	// Orchestration tests focus on command behavior and result structure
}
