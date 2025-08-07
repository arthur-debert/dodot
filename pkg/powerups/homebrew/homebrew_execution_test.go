//go:build integration
// +build integration

package homebrew_test

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/commands"
	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/synthfs"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestBrewfileActuallyExecutes(t *testing.T) {
	t.Run("brewfile executes brew bundle command", func(t *testing.T) {
		// Create test pack with home directory
		pack, homeDir := testutil.SetupTestPackWithHome(t, "testpack")

		// Set paths via environment variables
		t.Setenv("DOTFILES_ROOT", pack.Root)
		t.Setenv("DODOT_DATA_DIR", filepath.Join(homeDir, ".local", "share", "dodot"))

		tempDir := filepath.Dir(pack.Root)

		// Create a mock brew script that logs calls
		mockBrewPath := filepath.Join(tempDir, "bin")
		require.NoError(t, os.MkdirAll(mockBrewPath, 0755))

		mockBrewScript := filepath.Join(mockBrewPath, "brew")
		brewLogFile := filepath.Join(tempDir, "brew-calls.log")

		mockBrewContent := `#!/bin/sh
echo "$@" >> "` + brewLogFile + `"
echo "Mock brew executed with args: $@"
# Simulate success
exit 0
`
		require.NoError(t, os.WriteFile(mockBrewScript, []byte(mockBrewContent), 0755))

		// Add mock brew to PATH
		t.Setenv("PATH", mockBrewPath+":"+os.Getenv("PATH"))

		// Create Brewfile
		brewfileContent := `# Test Brewfile
brew "git"
brew "vim"
cask "visual-studio-code"
`
		pack.AddFile(t, "Brewfile", brewfileContent)

		// Create pack.toml
		packContent := `name = "testpack"
description = "Test pack"

[[brewfile]]
trigger = { file_name = "Brewfile" }
`
		pack.AddFile(t, "pack.toml", packContent)

		// Run install command
		result, err := commands.InstallPacks(commands.InstallPacksOptions{
			DotfilesRoot: pack.Root,
			PackNames:    []string{"testpack"},
			DryRun:       false,
			Force:        true, // Force to run even if previously run
		})
		require.NoError(t, err)
		require.NotNil(t, result)

		// Execute the operations
		require.NotEmpty(t, result.Operations)

		// Execute operations using combined executor
		executor := synthfs.NewSynthfsExecutor(false)
		_, err = executor.ExecuteOperations(result.Operations)
		require.NoError(t, err)

		// Verify brew was actually called
		brewLog, err := os.ReadFile(brewLogFile)
		require.NoError(t, err, "Brew log file should exist after execution")

		logContent := string(brewLog)
		assert.Contains(t, logContent, "bundle --file", "Brew should be called with 'bundle --file'")
		assert.Contains(t, logContent, brewfile, "Brew should be called with the Brewfile path")

		// Verify sentinel file was created
		pathsInstance, err := paths.New(dotfilesRoot)
		require.NoError(t, err)
		sentinelPath := filepath.Join(pathsInstance.HomebrewDir(), "testpack")
		_, err = os.Stat(sentinelPath)
		assert.NoError(t, err, "Sentinel file should exist after successful execution")
	})

	t.Run("brewfile failure prevents sentinel creation", func(t *testing.T) {
		// Create temp directories
		tempDir := t.TempDir()
		homeDir := filepath.Join(tempDir, "home")
		dotfilesRoot := filepath.Join(tempDir, "dotfiles")
		packDir := filepath.Join(dotfilesRoot, "failpack")

		// Create directories
		require.NoError(t, os.MkdirAll(homeDir, 0755))
		require.NoError(t, os.MkdirAll(packDir, 0755))

		// Set paths via environment variables
		t.Setenv("HOME", homeDir)
		t.Setenv("DOTFILES_ROOT", dotfilesRoot)
		t.Setenv("DODOT_DATA_DIR", filepath.Join(homeDir, ".local", "share", "dodot"))

		// Create a mock brew script that fails
		mockBrewPath := filepath.Join(tempDir, "bin")
		require.NoError(t, os.MkdirAll(mockBrewPath, 0755))

		mockBrewScript := filepath.Join(mockBrewPath, "brew")
		mockBrewContent := `#!/bin/sh
echo "Mock brew failing"
exit 1
`
		require.NoError(t, os.WriteFile(mockBrewScript, []byte(mockBrewContent), 0755))

		// Add mock brew to PATH
		t.Setenv("PATH", mockBrewPath+":"+os.Getenv("PATH"))

		// Create Brewfile
		brewfile := filepath.Join(packDir, "Brewfile")
		brewfileContent := `brew "nonexistent-package"`
		require.NoError(t, os.WriteFile(brewfile, []byte(brewfileContent), 0644))

		// Create pack.toml
		packToml := filepath.Join(packDir, "pack.toml")
		packContent := `name = "failpack"
description = "Test pack that fails"

[[brewfile]]
trigger = { file_name = "Brewfile" }
`
		require.NoError(t, os.WriteFile(packToml, []byte(packContent), 0644))

		// Run install command
		result, err := commands.InstallPacks(commands.InstallPacksOptions{
			DotfilesRoot: dotfilesRoot,
			PackNames:    []string{"failpack"},
			DryRun:       false,
			Force:        true,
		})
		require.NoError(t, err)
		require.NotNil(t, result)

		// Execute the operations
		require.NotEmpty(t, result.Operations)

		// Execute operations using combined executor - this should fail
		executor := synthfs.NewSynthfsExecutor(false)
		_, err = executor.ExecuteOperations(result.Operations)
		assert.Error(t, err, "Command execution should fail when brew exits with non-zero")

		// Verify sentinel file was NOT created due to failure
		pathsInstance, err := paths.New(dotfilesRoot)
		require.NoError(t, err)
		sentinelPath := filepath.Join(pathsInstance.HomebrewDir(), "failpack")
		_, err = os.Stat(sentinelPath)
		assert.True(t, os.IsNotExist(err), "Sentinel file should not exist after failed execution")
	})
}

func TestBrewPowerUpGeneratesExecuteOperation(t *testing.T) {
	t.Run("brew operations include execute command", func(t *testing.T) {
		// Create test directory
		tempDir := t.TempDir()
		brewfilePath := filepath.Join(tempDir, "Brewfile")

		// Create a Brewfile
		brewfileContent := `brew "git"`
		require.NoError(t, os.WriteFile(brewfilePath, []byte(brewfileContent), 0644))

		// Create test action
		action := types.Action{
			Type:   types.ActionTypeBrew,
			Source: brewfilePath,
			Target: "",
			Metadata: map[string]interface{}{
				"pack":     "testpack",
				"checksum": "abc123",
			},
		}

		// Create execution context
		ctx := core.NewExecutionContext(false)

		// Convert action to operations
		operations, err := core.ConvertActionsToOperationsWithContext([]types.Action{action}, ctx)
		require.NoError(t, err)

		// Verify we have all necessary operations
		require.Len(t, operations, 3, "Should have 3 operations: create dir, execute, write sentinel")

		// Check operation order and types
		assert.Equal(t, types.OperationCreateDir, operations[0].Type, "First operation should create directory")
		assert.Equal(t, types.OperationExecute, operations[1].Type, "Second operation should execute brew")
		assert.Equal(t, types.OperationWriteFile, operations[2].Type, "Third operation should write sentinel")

		// Verify execute operation details
		executeOp := operations[1]
		assert.Equal(t, "brew", executeOp.Command, "Should use brew command")
		assert.Equal(t, []string{"bundle", "--file", brewfilePath}, executeOp.Args, "Should pass correct arguments")
		assert.Equal(t, filepath.Dir(brewfilePath), executeOp.WorkingDir, "Should set working directory")

		// Log operations for debugging
		for i, op := range operations {
			t.Logf("Operation %d: Type=%s, Description=%s", i, op.Type, op.Description)
		}
	})
}
