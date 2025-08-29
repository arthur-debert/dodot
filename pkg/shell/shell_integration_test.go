// pkg/shell/shell_integration_test.go
// TEST TYPE: Unit
// DEPENDENCIES: Real filesystem, environment setup
// PURPOSE: Test shell integration installation orchestration

package shell

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestInstallShellIntegration_Success(t *testing.T) {
	t.Run("installs all scripts successfully", func(t *testing.T) {
		// Arrange
		env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)
		dataDir := filepath.Join(env.XDGData, "test-data")

		// Create source scripts in development layout (pkg/shell/)
		bashScript := "#!/bin/bash\necho 'bash init script'"
		fishScript := "#!/usr/bin/fish\necho 'fish init script'"

		// Create scripts in the expected location for development setup
		shellDir := filepath.Join(env.DotfilesRoot, "..", "pkg", "shell")
		err := os.MkdirAll(shellDir, 0755)
		require.NoError(t, err)

		bashPath := filepath.Join(shellDir, "dodot-init.sh")
		fishPath := filepath.Join(shellDir, "dodot-init.fish")

		err = os.WriteFile(bashPath, []byte(bashScript), 0644)
		require.NoError(t, err)
		err = os.WriteFile(fishPath, []byte(fishScript), 0644)
		require.NoError(t, err)

		// Set PROJECT_ROOT environment to enable development path resolution
		t.Setenv("PROJECT_ROOT", filepath.Dir(env.DotfilesRoot))

		// Act
		err = InstallShellIntegration(dataDir)

		// Assert
		require.NoError(t, err)

		// Verify directory creation
		destShellDir := filepath.Join(dataDir, "shell")
		info, err := os.Stat(destShellDir)
		require.NoError(t, err)
		assert.True(t, info.IsDir())

		// Verify script installation
		bashDest := filepath.Join(destShellDir, "dodot-init.sh")
		fishDest := filepath.Join(destShellDir, "dodot-init.fish")

		bashContent, err := os.ReadFile(bashDest)
		require.NoError(t, err)
		assert.Equal(t, bashScript, string(bashContent))

		fishContent, err := os.ReadFile(fishDest)
		require.NoError(t, err)
		assert.Equal(t, fishScript, string(fishContent))

		// Verify permissions
		bashInfo, err := os.Stat(bashDest)
		require.NoError(t, err)
		assert.Equal(t, os.FileMode(0755), bashInfo.Mode().Perm())

		fishInfo, err := os.Stat(fishDest)
		require.NoError(t, err)
		assert.Equal(t, os.FileMode(0755), fishInfo.Mode().Perm())
	})
}

func TestInstallShellIntegration_SourceFileNotFound(t *testing.T) {
	t.Run("skips missing scripts with warning", func(t *testing.T) {
		// Arrange
		env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)
		dataDir := filepath.Join(env.XDGData, "test-data")

		// Create only fish script (bash will be missing)
		fishScript := "#!/usr/bin/fish\necho 'fish init script'"

		shellDir := filepath.Join(env.DotfilesRoot, "..", "pkg", "shell")
		err := os.MkdirAll(shellDir, 0755)
		require.NoError(t, err)

		fishPath := filepath.Join(shellDir, "dodot-init.fish")
		err = os.WriteFile(fishPath, []byte(fishScript), 0644)
		require.NoError(t, err)

		// Note: dodot-init.sh is intentionally missing

		// Set PROJECT_ROOT environment to enable development path resolution
		t.Setenv("PROJECT_ROOT", filepath.Dir(env.DotfilesRoot))

		// Act
		err = InstallShellIntegration(dataDir)

		// Assert
		require.NoError(t, err)

		// Verify only fish script was installed
		destShellDir := filepath.Join(dataDir, "shell")
		bashDest := filepath.Join(destShellDir, "dodot-init.sh")
		fishDest := filepath.Join(destShellDir, "dodot-init.fish")

		_, err = os.Stat(bashDest)
		assert.True(t, os.IsNotExist(err), "bash script should not be installed")

		fishContent, err := os.ReadFile(fishDest)
		require.NoError(t, err)
		assert.Equal(t, fishScript, string(fishContent))
	})
}

func TestInstallShellIntegration_DirectoryCreationFailure(t *testing.T) {
	t.Run("returns error when directory creation fails", func(t *testing.T) {
		// Arrange
		env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

		// Use a path that will fail directory creation (inside a file)
		testFile := filepath.Join(env.XDGData, "blocking-file")
		err := os.WriteFile(testFile, []byte("blocking content"), 0644)
		require.NoError(t, err)

		// Try to create directory inside the file (will fail)
		dataDir := filepath.Join(testFile, "impossible")

		// Act
		err = InstallShellIntegration(dataDir)

		// Assert
		require.Error(t, err)
		assert.Contains(t, err.Error(), "failed to create shell directory")
	})
}

func TestInstallShellIntegration_EmptyDataDir(t *testing.T) {
	t.Run("handles empty data directory", func(t *testing.T) {
		// Arrange
		_ = testutil.NewTestEnvironment(t, testutil.EnvIsolated)
		dataDir := ""

		// Don't set PROJECT_ROOT, so scripts won't be found

		// Act
		err := InstallShellIntegration(dataDir)

		// Assert
		require.NoError(t, err)

		// Verify shell directory was created in the root location
		shellDir := filepath.Join(dataDir, "shell")
		info, err := os.Stat(shellDir)
		require.NoError(t, err)
		assert.True(t, info.IsDir())
	})
}

func TestInstallShellIntegration_NoScriptsFound(t *testing.T) {
	t.Run("succeeds with warning when no scripts are found", func(t *testing.T) {
		// Arrange
		env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)
		dataDir := filepath.Join(env.XDGData, "test-data")

		// Use a completely isolated temp directory that doesn't have project structure
		// Move to a temporary directory without the project layout
		originalPwd, err := os.Getwd()
		require.NoError(t, err)

		tempDir := t.TempDir()
		err = os.Chdir(tempDir)
		require.NoError(t, err)

		t.Cleanup(func() {
			_ = os.Chdir(originalPwd)
		})

		// Clear PROJECT_ROOT to ensure scripts can't be found
		t.Setenv("PROJECT_ROOT", "")

		// Act
		err = InstallShellIntegration(dataDir)

		// Assert
		require.NoError(t, err)

		// Verify directory was created but no scripts installed
		destShellDir := filepath.Join(dataDir, "shell")
		info, err := os.Stat(destShellDir)
		require.NoError(t, err)
		assert.True(t, info.IsDir())

		// Verify no scripts were installed (since they couldn't be found)
		entries, err := os.ReadDir(destShellDir)
		require.NoError(t, err)
		assert.Len(t, entries, 0, "no scripts should be installed when source scripts can't be found")
	})
}

func TestInstallShellIntegration_PermissionError(t *testing.T) {
	t.Run("returns error when source file permissions fail", func(t *testing.T) {
		// Arrange
		env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)
		dataDir := filepath.Join(env.XDGData, "test-data")

		// Create source script with restricted permissions
		bashScript := "#!/bin/bash\necho 'bash init script'"

		shellDir := filepath.Join(env.DotfilesRoot, "..", "pkg", "shell")
		err := os.MkdirAll(shellDir, 0755)
		require.NoError(t, err)

		bashPath := filepath.Join(shellDir, "dodot-init.sh")
		err = os.WriteFile(bashPath, []byte(bashScript), 0644)
		require.NoError(t, err)

		// Remove read permissions from source file to trigger error
		err = os.Chmod(bashPath, 0000)
		require.NoError(t, err)

		// Restore permissions for cleanup
		t.Cleanup(func() {
			_ = os.Chmod(bashPath, 0644)
		})

		// Set PROJECT_ROOT environment to enable development path resolution
		t.Setenv("PROJECT_ROOT", filepath.Dir(env.DotfilesRoot))

		// Act
		err = InstallShellIntegration(dataDir)

		// Assert
		require.Error(t, err)
		assert.Contains(t, err.Error(), "failed to open source script")
	})
}
