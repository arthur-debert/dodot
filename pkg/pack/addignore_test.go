// pkg/pack/addignore_test.go
// TEST TYPE: Business Logic Integration
// DEPENDENCIES: Memory FS
// PURPOSE: Test addignore command orchestration for creating ignore files

package pack_test

import (
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/pack"
	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestAddIgnore_CreateIgnoreFile_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create a pack directory with content
	env.SetupPack("vim", testutil.PackConfig{
		Files: map[string]string{
			".vimrc":             "set number",
			"colors/monokai.vim": "color scheme",
		},
	})

	opts := pack.AddIgnoreOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "vim",
		FileSystem:   env.FS,
	}

	// Execute
	result, err := pack.AddIgnore(opts)

	// Verify orchestration behavior
	require.NoError(t, err)
	assert.NotNil(t, result, "should return result object")

	// Verify result structure
	assert.Equal(t, "add-ignore", result.Command, "command should be add-ignore")
	assert.True(t, result.Metadata.IgnoreCreated, "ignore file should be created")
	assert.False(t, result.Metadata.AlreadyExisted, "should not already exist")
	// Note: Pack status is only available if GetPackStatus function is provided

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

	opts := pack.AddIgnoreOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "vim",
		FileSystem:   env.FS,
	}

	// Execute
	result, err := pack.AddIgnore(opts)

	// Verify already exists behavior
	require.NoError(t, err)
	assert.NotNil(t, result, "should return result object")

	assert.Equal(t, "add-ignore", result.Command, "command should be add-ignore")
	assert.False(t, result.Metadata.IgnoreCreated, "ignore file should not be created")
	assert.True(t, result.Metadata.AlreadyExisted, "should already exist")
	// Note: Pack status is only available if GetPackStatus function is provided
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

	opts := pack.AddIgnoreOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "vim/", // Trailing slash should be normalized
		FileSystem:   env.FS,
	}

	// Execute
	result, err := pack.AddIgnore(opts)

	// Verify pack name normalization
	require.NoError(t, err)
	assert.NotNil(t, result, "should return result object")

	assert.Equal(t, "add-ignore", result.Command, "command should be add-ignore")
	assert.True(t, result.Metadata.IgnoreCreated, "ignore file should be created")
	// Note: Pack status is only available if GetPackStatus function is provided
}

func TestAddIgnore_NonExistentPack_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Don't create any pack - test non-existent pack behavior
	opts := pack.AddIgnoreOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "nonexistent",
		FileSystem:   env.FS,
	}

	// Execute
	result, err := pack.AddIgnore(opts)

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

	opts := pack.AddIgnoreOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "", // Empty pack name
		FileSystem:   env.FS,
	}

	// Execute
	result, err := pack.AddIgnore(opts)

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
			opts := pack.AddIgnoreOptions{
				DotfilesRoot: env.DotfilesRoot,
				PackName:     packName,
				FileSystem:   env.FS,
			}

			// Execute
			result, err := pack.AddIgnore(opts)

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

	opts := pack.AddIgnoreOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "vim",
		FileSystem:   env.FS,
	}

	// Execute
	result, err := pack.AddIgnore(opts)

	// Verify complete result structure
	require.NoError(t, err)
	require.NotNil(t, result, "result should not be nil")

	// Verify all result fields are populated correctly
	assert.Equal(t, "add-ignore", result.Command, "command should be add-ignore")
	// Note: Pack status is only available if GetPackStatus function is provided

	// Created and AlreadyExisted should be mutually exclusive
	assert.NotEqual(t, result.Metadata.IgnoreCreated, result.Metadata.AlreadyExisted, "Created and AlreadyExisted should be mutually exclusive")
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
			opts := pack.AddIgnoreOptions{
				DotfilesRoot: env.DotfilesRoot,
				PackName:     packName,
				FileSystem:   env.FS,
			}

			result, err := pack.AddIgnore(opts)

			// Verify each pack gets its own ignore file
			require.NoError(t, err)
			assert.NotNil(t, result, "should return result for pack %s", packName)
			assert.Equal(t, "add-ignore", result.Command, "command should be add-ignore")
			assert.True(t, result.Metadata.IgnoreCreated, "ignore file should be created for pack %s", packName)
			// Note: Pack status is only available if GetPackStatus function is provided

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

	opts := pack.AddIgnoreOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "complex-pack",
		FileSystem:   env.FS,
	}

	// Execute
	result, err := pack.AddIgnore(opts)

	// Verify ignore file creation integrates with pack structure
	require.NoError(t, err)
	assert.NotNil(t, result, "should return result object")
	assert.Equal(t, "add-ignore", result.Command, "command should be add-ignore")
	assert.True(t, result.Metadata.IgnoreCreated, "ignore file should be created")

	// Verify ignore file is placed correctly in pack directory
	expectedPath := filepath.Join(env.DotfilesRoot, "complex-pack", cfg.Patterns.SpecialFiles.IgnoreFile)
	_, err = env.FS.Stat(expectedPath)
	assert.NoError(t, err, "ignore file should exist in pack root")

	// Command should complete successfully with proper orchestration
	// Filesystem integration is handled by the command implementation
	// Orchestration tests focus on command behavior and result structure
}
