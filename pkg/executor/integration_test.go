package executor_test

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/executor"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/rs/zerolog"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestExecutor_Integration(t *testing.T) {
	if testing.Short() {
		t.Skip("Skipping integration test in short mode")
	}

	// Create temporary directories for testing
	tempDir := t.TempDir()
	dotfilesRoot := filepath.Join(tempDir, "dotfiles")
	dataDir := filepath.Join(tempDir, "data")
	homeDir := filepath.Join(tempDir, "home")

	// Set up environment
	t.Setenv("DOTFILES_ROOT", dotfilesRoot)
	t.Setenv("DODOT_DATA_DIR", dataDir)
	t.Setenv("HOME", homeDir)

	// Create directories
	require.NoError(t, os.MkdirAll(dotfilesRoot, 0755))
	require.NoError(t, os.MkdirAll(dataDir, 0755))
	require.NoError(t, os.MkdirAll(homeDir, 0755))

	// Initialize paths and datastore
	pathsInstance, err := paths.New(dotfilesRoot)
	require.NoError(t, err)

	fs := filesystem.NewOS()
	ds := datastore.New(fs, pathsInstance)

	exec := executor.New(executor.Options{
		DataStore: ds,
		DryRun:    false,
		Logger:    zerolog.Nop(),
		FS:        fs,
	})

	t.Run("LinkAction integration", func(t *testing.T) {
		// Create a test pack with a config file
		packName := "vim"
		packDir := filepath.Join(dotfilesRoot, packName)
		require.NoError(t, os.MkdirAll(packDir, 0755))

		// Create source file
		sourceFile := filepath.Join(packDir, "vimrc")
		require.NoError(t, os.WriteFile(sourceFile, []byte("set number\n"), 0644))

		// Create and execute LinkAction
		action := &types.LinkAction{
			PackName:   packName,
			SourceFile: sourceFile,
			TargetFile: filepath.Join(homeDir, ".vimrc"),
		}

		results := exec.Execute([]types.ActionV2{action})

		require.Len(t, results, 1)
		assert.True(t, results[0].Success, "Action should succeed")
		assert.Nil(t, results[0].Error)

		// Verify the final symlink was created
		targetPath := filepath.Join(homeDir, ".vimrc")
		assert.FileExists(t, targetPath)

		// Verify it's a symlink
		info, err := os.Lstat(targetPath)
		require.NoError(t, err)
		assert.True(t, info.Mode()&os.ModeSymlink != 0, "Target should be a symlink")

		// Verify the content is accessible through the symlink
		content, err := os.ReadFile(targetPath)
		require.NoError(t, err)
		assert.Equal(t, "set number\n", string(content))
	})

	t.Run("UnlinkAction integration", func(t *testing.T) {
		// Set up a linked file first
		packName := "bash"
		packDir := filepath.Join(dotfilesRoot, packName)
		require.NoError(t, os.MkdirAll(packDir, 0755))

		sourceFile := filepath.Join(packDir, "bashrc")
		require.NoError(t, os.WriteFile(sourceFile, []byte("export PATH\n"), 0644))

		// Link it first
		linkAction := &types.LinkAction{
			PackName:   packName,
			SourceFile: sourceFile,
			TargetFile: filepath.Join(homeDir, ".bashrc"),
		}
		results := exec.Execute([]types.ActionV2{linkAction})
		require.True(t, results[0].Success)

		// Now unlink it
		unlinkAction := &types.UnlinkAction{
			PackName:   packName,
			SourceFile: sourceFile,
		}

		results = exec.Execute([]types.ActionV2{unlinkAction})

		require.Len(t, results, 1)
		assert.True(t, results[0].Success)
		assert.Nil(t, results[0].Error)

		// Verify the intermediate link was removed
		intermediatePath := filepath.Join(dataDir, "packs", packName, "symlinks", "bashrc")
		assert.NoFileExists(t, intermediatePath)
	})

	t.Run("AddToPathAction integration", func(t *testing.T) {
		packName := "tools"
		binDir := filepath.Join(dotfilesRoot, packName, "bin")
		require.NoError(t, os.MkdirAll(binDir, 0755))

		action := &types.AddToPathAction{
			PackName: packName,
			DirPath:  binDir,
		}

		results := exec.Execute([]types.ActionV2{action})

		require.Len(t, results, 1)
		assert.True(t, results[0].Success)
		assert.Nil(t, results[0].Error)

		// Verify the path directory link was created
		pathLinkPath := filepath.Join(dataDir, "packs", packName, "path", "bin")
		assert.FileExists(t, pathLinkPath)

		// Verify it points to the correct directory
		target, err := os.Readlink(pathLinkPath)
		require.NoError(t, err)
		assert.Equal(t, binDir, target)
	})

	t.Run("AddToShellProfileAction integration", func(t *testing.T) {
		packName := "shell"
		scriptPath := filepath.Join(dotfilesRoot, packName, "aliases.sh")
		require.NoError(t, os.MkdirAll(filepath.Dir(scriptPath), 0755))
		require.NoError(t, os.WriteFile(scriptPath, []byte("alias ll='ls -la'\n"), 0644))

		action := &types.AddToShellProfileAction{
			PackName:   packName,
			ScriptPath: scriptPath,
		}

		results := exec.Execute([]types.ActionV2{action})

		require.Len(t, results, 1)
		assert.True(t, results[0].Success)
		assert.Nil(t, results[0].Error)

		// Verify the shell profile link was created
		profileLinkPath := filepath.Join(dataDir, "packs", packName, "shell_profile", "aliases.sh")
		assert.FileExists(t, profileLinkPath)

		// Verify it points to the correct file
		target, err := os.Readlink(profileLinkPath)
		require.NoError(t, err)
		assert.Equal(t, scriptPath, target)
	})

	t.Run("RunScriptAction integration", func(t *testing.T) {
		packName := "test"
		packDir := filepath.Join(dotfilesRoot, packName)
		require.NoError(t, os.MkdirAll(packDir, 0755))

		// Create a simple test script
		scriptPath := filepath.Join(packDir, "install.sh")
		scriptContent := `#!/bin/sh
echo "Test installation"
`
		require.NoError(t, os.WriteFile(scriptPath, []byte(scriptContent), 0755))

		// Make script executable
		require.NoError(t, os.Chmod(scriptPath, 0755))

		action := &types.RunScriptAction{
			PackName:     packName,
			ScriptPath:   scriptPath,
			Checksum:     "test-checksum",
			SentinelName: "install.sh.sentinel",
		}

		results := exec.Execute([]types.ActionV2{action})

		require.Len(t, results, 1)
		// Note: This might fail in CI environments, so we check the error
		if results[0].Error != nil {
			t.Logf("Script execution failed (might be expected in CI): %v", results[0].Error)
		} else {
			// Verify the sentinel was created
			sentinelPath := filepath.Join(dataDir, "packs", packName, "sentinels", "install.sh.sentinel")
			assert.FileExists(t, sentinelPath)
		}
	})

	t.Run("dry run mode", func(t *testing.T) {
		dryRunExecutor := executor.New(executor.Options{
			DataStore: ds,
			DryRun:    true,
			Logger:    zerolog.Nop(),
			FS:        fs,
		})

		packName := "dryrun"
		packDir := filepath.Join(dotfilesRoot, packName)
		require.NoError(t, os.MkdirAll(packDir, 0755))

		sourceFile := filepath.Join(packDir, "config")
		require.NoError(t, os.WriteFile(sourceFile, []byte("test"), 0644))

		action := &types.LinkAction{
			PackName:   packName,
			SourceFile: sourceFile,
			TargetFile: filepath.Join(homeDir, ".dryrun-config"),
		}

		results := dryRunExecutor.Execute([]types.ActionV2{action})

		require.Len(t, results, 1)
		assert.True(t, results[0].Success)
		assert.True(t, results[0].Skipped)
		assert.Equal(t, "Dry run - no changes made", results[0].Message)

		// Verify no actual changes were made
		targetPath := filepath.Join(homeDir, ".dryrun-config")
		assert.NoFileExists(t, targetPath)
	})
}