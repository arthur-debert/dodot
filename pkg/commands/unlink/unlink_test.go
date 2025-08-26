package unlink

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestUnlinkPacks_Integration(t *testing.T) {
	// This test verifies unlinking after actual deployment
	tmpDir := t.TempDir()

	// Set up a mock home directory for testing
	mockHome := filepath.Join(tmpDir, "home")
	require.NoError(t, os.MkdirAll(mockHome, 0755))
	origHome := os.Getenv("HOME")
	require.NoError(t, os.Setenv("HOME", mockHome))
	defer func() {
		_ = os.Setenv("HOME", origHome)
	}()

	// Create a pack with various files
	packDir := filepath.Join(tmpDir, "test")
	require.NoError(t, os.MkdirAll(packDir, 0755))
	require.NoError(t, os.WriteFile(filepath.Join(packDir, ".vimrc"), []byte("vim config"), 0644))
	require.NoError(t, os.WriteFile(filepath.Join(packDir, "script.sh"), []byte("#!/bin/sh\necho test"), 0755))
	require.NoError(t, os.MkdirAll(filepath.Join(packDir, "bin"), 0755))
	require.NoError(t, os.WriteFile(filepath.Join(packDir, "bin/tool"), []byte("#!/bin/sh\ntool"), 0755))

	// Create paths instance
	testPaths, err := paths.New(tmpDir)
	require.NoError(t, err)

	// Create filesystem and datastore
	fs := filesystem.NewOS()
	ds := datastore.New(fs, testPaths)

	// Deploy some items using the datastore
	// 1. Create a symlink
	intermediatePath, err := ds.Link("test", filepath.Join(packDir, ".vimrc"))
	require.NoError(t, err)

	// Create the user-facing symlink (normally done by executor)
	userSymlink := filepath.Join(mockHome, ".vimrc")
	require.NoError(t, os.Symlink(intermediatePath, userSymlink))

	// 2. Add a directory to PATH
	err = ds.AddToPath("test", filepath.Join(packDir, "bin"))
	require.NoError(t, err)

	// 3. Add to shell profile
	err = ds.AddToShellProfile("test", filepath.Join(packDir, "script.sh"))
	require.NoError(t, err)

	// 4. Record a provisioning (this should NOT be removed)
	err = ds.RecordProvisioning("test", "install.sh.sentinel", "sha256:abc123")
	require.NoError(t, err)

	// Debug paths
	t.Logf("Mock home: %s", mockHome)
	t.Logf("User symlink: %s", userSymlink)
	t.Logf("Intermediate path: %s", intermediatePath)
	t.Logf("Pack handler dir: %s", testPaths.PackHandlerDir("test", "symlinks"))

	// Verify deployment exists
	assert.FileExists(t, userSymlink)
	assert.DirExists(t, testPaths.PackHandlerDir("test", "symlinks"))
	assert.DirExists(t, testPaths.PackHandlerDir("test", "path"))
	assert.DirExists(t, testPaths.PackHandlerDir("test", "shell"))
	assert.DirExists(t, testPaths.PackHandlerDir("test", "sentinels"))

	// Now unlink the pack
	result, err := UnlinkPacks(UnlinkPacksOptions{
		DotfilesRoot: tmpDir,
		PackNames:    []string{"test"},
		DryRun:       false,
	})

	require.NoError(t, err)
	require.NotNil(t, result)

	// Verify results
	assert.False(t, result.DryRun)
	require.Len(t, result.Packs, 1)
	pack := result.Packs[0]
	assert.Equal(t, "test", pack.Name)
	assert.Empty(t, pack.Errors)

	// Check removed items
	var symlinkRemoved, pathDirRemoved, shellDirRemoved bool
	for _, item := range pack.RemovedItems {
		if item.Type == "symlink" && item.Path == userSymlink {
			symlinkRemoved = true
			assert.True(t, item.Success)
		}
		if item.Type == "path_directory" {
			pathDirRemoved = true
			assert.True(t, item.Success)
		}
		if item.Type == "shell_directory" {
			shellDirRemoved = true
			assert.True(t, item.Success)
		}
	}

	assert.True(t, symlinkRemoved, "User-facing symlink should be removed")
	assert.True(t, pathDirRemoved, "Path directory should be removed")
	assert.True(t, shellDirRemoved, "Shell profile directory should be removed")

	// Verify actual removal
	assert.NoFileExists(t, userSymlink, "User symlink should be gone")
	assert.NoDirExists(t, testPaths.PackHandlerDir("test", "symlinks"))
	assert.NoDirExists(t, testPaths.PackHandlerDir("test", "path"))
	assert.NoDirExists(t, testPaths.PackHandlerDir("test", "shell"))

	// Verify sentinels were NOT removed
	assert.DirExists(t, testPaths.PackHandlerDir("test", "sentinels"), "Sentinels should remain")
}

func TestUnlinkPacks_DryRun(t *testing.T) {
	// Test that dry run doesn't actually remove anything
	tmpDir := t.TempDir()

	// Set up a mock home directory for testing
	mockHome := filepath.Join(tmpDir, "home")
	require.NoError(t, os.MkdirAll(mockHome, 0755))
	origHome := os.Getenv("HOME")
	require.NoError(t, os.Setenv("HOME", mockHome))
	defer func() {
		_ = os.Setenv("HOME", origHome)
	}()

	// Create a pack
	packDir := filepath.Join(tmpDir, "test")
	require.NoError(t, os.MkdirAll(packDir, 0755))
	require.NoError(t, os.WriteFile(filepath.Join(packDir, ".vimrc"), []byte("vim config"), 0644))

	// Create paths and datastore
	testPaths, err := paths.New(tmpDir)
	require.NoError(t, err)
	fs := filesystem.NewOS()
	ds := datastore.New(fs, testPaths)

	// Deploy a symlink
	intermediatePath, err := ds.Link("test", filepath.Join(packDir, ".vimrc"))
	require.NoError(t, err)
	userSymlink := filepath.Join(mockHome, ".vimrc")
	require.NoError(t, os.Symlink(intermediatePath, userSymlink))

	// Run unlink with dry-run
	result, err := UnlinkPacks(UnlinkPacksOptions{
		DotfilesRoot: tmpDir,
		PackNames:    []string{"test"},
		DryRun:       true,
	})

	require.NoError(t, err)
	require.NotNil(t, result)
	assert.True(t, result.DryRun)

	// Verify nothing was actually removed
	assert.FileExists(t, userSymlink)
	assert.DirExists(t, testPaths.PackHandlerDir("test", "symlinks"))

	// But result should show what would be removed
	require.Len(t, result.Packs, 1)
	assert.Greater(t, len(result.Packs[0].RemovedItems), 0)
}

func TestUnlinkPacks_NonExistentPack(t *testing.T) {
	tmpDir := t.TempDir()

	result, err := UnlinkPacks(UnlinkPacksOptions{
		DotfilesRoot: tmpDir,
		PackNames:    []string{"nonexistent"},
	})

	// Should get an error about pack not found
	assert.Error(t, err)
	assert.Nil(t, result)
}

func TestUnlinkPacks_SymlinkPointsElsewhere(t *testing.T) {
	// Test that we don't remove symlinks that don't point to our intermediate
	tmpDir := t.TempDir()

	// Set up a mock home directory for testing
	mockHome := filepath.Join(tmpDir, "home")
	require.NoError(t, os.MkdirAll(mockHome, 0755))
	origHome := os.Getenv("HOME")
	require.NoError(t, os.Setenv("HOME", mockHome))
	defer func() {
		_ = os.Setenv("HOME", origHome)
	}()

	// Create a pack
	packDir := filepath.Join(tmpDir, "test")
	require.NoError(t, os.MkdirAll(packDir, 0755))
	require.NoError(t, os.WriteFile(filepath.Join(packDir, ".vimrc"), []byte("vim config"), 0644))

	// Create paths and datastore
	testPaths, err := paths.New(tmpDir)
	require.NoError(t, err)
	fs := filesystem.NewOS()
	ds := datastore.New(fs, testPaths)

	// Deploy a symlink
	_, err = ds.Link("test", filepath.Join(packDir, ".vimrc"))
	require.NoError(t, err)

	// Create a user symlink that points somewhere else
	userSymlink := filepath.Join(mockHome, ".vimrc")
	otherTarget := filepath.Join(tmpDir, "other", ".vimrc")
	require.NoError(t, os.MkdirAll(filepath.Dir(otherTarget), 0755))
	require.NoError(t, os.WriteFile(otherTarget, []byte("other vim"), 0644))
	require.NoError(t, os.Symlink(otherTarget, userSymlink))

	// Run unlink
	result, err := UnlinkPacks(UnlinkPacksOptions{
		DotfilesRoot: tmpDir,
		PackNames:    []string{"test"},
		DryRun:       false,
	})

	require.NoError(t, err)
	require.NotNil(t, result)

	// The user symlink should NOT be removed since it points elsewhere
	assert.FileExists(t, userSymlink)

	// Verify it still points to the other target
	target, err := os.Readlink(userSymlink)
	require.NoError(t, err)
	assert.Equal(t, otherTarget, target)
}
