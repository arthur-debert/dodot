//go:build integration
// +build integration

package install_test

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/commands"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/synthfs"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestInstallScriptActuallyExecutes(t *testing.T) {
	t.Run("install script creates side effect when executed", func(t *testing.T) {
		// Create temp directories
		tempDir := t.TempDir()
		homeDir := filepath.Join(tempDir, "home")
		dotfilesRoot := filepath.Join(tempDir, "dotfiles")
		packDir := filepath.Join(dotfilesRoot, "testpack")

		// Create directories
		require.NoError(t, os.MkdirAll(homeDir, 0755))
		require.NoError(t, os.MkdirAll(packDir, 0755))

		// Set paths via environment variables
		t.Setenv("HOME", homeDir)
		t.Setenv("DOTFILES_ROOT", dotfilesRoot)
		t.Setenv("DODOT_DATA_DIR", filepath.Join(homeDir, ".local", "share", "dodot"))

		// Create marker file path
		markerFile := filepath.Join(tempDir, "install-executed.marker")

		// Create install script that creates a marker file
		installScript := filepath.Join(packDir, "install.sh")
		scriptContent := `#!/bin/sh
echo "Running install script for $DODOT_PACK"
echo "Working directory: $(pwd)"
touch "` + markerFile + `"
echo "SUCCESS" > "` + markerFile + `"
exit 0
`
		require.NoError(t, os.WriteFile(installScript, []byte(scriptContent), 0755))

		// Create pack.toml
		packToml := filepath.Join(packDir, "pack.toml")
		packContent := `name = "testpack"
description = "Test pack"

[[install]]
trigger = { file_name = "install.sh" }
`
		require.NoError(t, os.WriteFile(packToml, []byte(packContent), 0644))

		// Run install command
		result, err := commands.InstallPacks(commands.InstallPacksOptions{
			DotfilesRoot: dotfilesRoot,
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

		// Verify the install script actually ran
		info, err := os.Stat(markerFile)
		require.NoError(t, err, "Marker file should exist after install script execution")
		assert.False(t, info.IsDir(), "Marker should be a file")

		// Check content
		content, err := os.ReadFile(markerFile)
		require.NoError(t, err)
		assert.Equal(t, "SUCCESS\n", string(content), "Marker file should contain SUCCESS")

		// Verify sentinel file was created
		pathsInstance, err := paths.New(dotfilesRoot)
		require.NoError(t, err)
		sentinelPath := filepath.Join(pathsInstance.InstallDir(), "testpack")
		_, err = os.Stat(sentinelPath)
		assert.NoError(t, err, "Sentinel file should exist after successful execution")
	})

	t.Run("install script failure prevents sentinel creation", func(t *testing.T) {
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

		// Create install script that fails
		installScript := filepath.Join(packDir, "install.sh")
		scriptContent := `#!/bin/sh
echo "This install script will fail"
exit 1
`
		require.NoError(t, os.WriteFile(installScript, []byte(scriptContent), 0755))

		// Create pack.toml
		packToml := filepath.Join(packDir, "pack.toml")
		packContent := `name = "failpack"
description = "Test pack that fails"

[[install]]
trigger = { file_name = "install.sh" }
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
		assert.Error(t, err, "Command execution should fail when script exits with non-zero")

		// Verify sentinel file was NOT created due to failure
		pathsInstance, err := paths.New(dotfilesRoot)
		require.NoError(t, err)
		sentinelPath := filepath.Join(pathsInstance.InstallDir(), "failpack")
		_, err = os.Stat(sentinelPath)
		assert.True(t, os.IsNotExist(err), "Sentinel file should not exist after failed execution")
	})
}
