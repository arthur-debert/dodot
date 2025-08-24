package symlink_test

import (
	"io/fs"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/arthur-debert/dodot/pkg/handlers/symlink"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestSymlinkHandler_Clear tests the Clear method with actual symlinks
func TestSymlinkHandler_Clear(t *testing.T) {
	handler := symlink.NewSymlinkHandler()

	// Create temporary directories
	tempDir := t.TempDir()
	dataDir := filepath.Join(tempDir, "data")
	homeDir := filepath.Join(tempDir, "home")
	packDir := filepath.Join(tempDir, "pack")

	// Create directories
	require.NoError(t, os.MkdirAll(dataDir, 0755))
	require.NoError(t, os.MkdirAll(homeDir, 0755))
	require.NoError(t, os.MkdirAll(packDir, 0755))

	// Create source file in pack
	sourceFile := filepath.Join(packDir, ".vimrc")
	require.NoError(t, os.WriteFile(sourceFile, []byte("vim config"), 0644))

	// Create intermediate symlink directory
	symlinksDir := filepath.Join(dataDir, "packs", "testpack", "symlinks")
	require.NoError(t, os.MkdirAll(symlinksDir, 0755))

	// Create intermediate symlink
	intermediatePath := filepath.Join(symlinksDir, ".vimrc")
	require.NoError(t, os.Symlink(sourceFile, intermediatePath))

	// Create user-facing symlink
	userSymlink := filepath.Join(homeDir, ".vimrc")
	require.NoError(t, os.Symlink(intermediatePath, userSymlink))

	// Create mock FS that uses real filesystem operations
	mockFS := &testFS{base: tempDir}

	// Create mock paths
	mockPaths := &testPaths{
		dataDir: dataDir,
		homeDir: homeDir,
	}

	// Create context
	ctx := types.ClearContext{
		Pack: types.Pack{
			Name: "testpack",
			Path: packDir,
		},
		FS:     mockFS,
		Paths:  mockPaths,
		DryRun: false,
	}

	// Execute Clear
	clearedItems, err := handler.Clear(ctx)
	require.NoError(t, err)

	// Verify results
	assert.Len(t, clearedItems, 1)
	assert.Equal(t, "symlink", clearedItems[0].Type)
	assert.Equal(t, userSymlink, clearedItems[0].Path)
	assert.Contains(t, clearedItems[0].Description, "Removed symlink to .vimrc")

	// Verify user symlink was removed
	_, err = os.Lstat(userSymlink)
	assert.True(t, os.IsNotExist(err), "User symlink should be removed")

	// Verify intermediate symlink still exists (will be removed by datastore)
	_, err = os.Lstat(intermediatePath)
	assert.NoError(t, err, "Intermediate symlink should still exist")
}

// TestSymlinkHandler_Clear_DryRun tests dry run mode
func TestSymlinkHandler_Clear_DryRun(t *testing.T) {
	handler := symlink.NewSymlinkHandler()

	// Create temporary directories
	tempDir := t.TempDir()
	dataDir := filepath.Join(tempDir, "data")
	homeDir := filepath.Join(tempDir, "home")
	packDir := filepath.Join(tempDir, "pack")

	// Create directories
	require.NoError(t, os.MkdirAll(dataDir, 0755))
	require.NoError(t, os.MkdirAll(homeDir, 0755))
	require.NoError(t, os.MkdirAll(packDir, 0755))

	// Create source file in pack
	sourceFile := filepath.Join(packDir, ".bashrc")
	require.NoError(t, os.WriteFile(sourceFile, []byte("bash config"), 0644))

	// Create intermediate symlink directory
	symlinksDir := filepath.Join(dataDir, "packs", "testpack", "symlinks")
	require.NoError(t, os.MkdirAll(symlinksDir, 0755))

	// Create intermediate symlink
	intermediatePath := filepath.Join(symlinksDir, ".bashrc")
	require.NoError(t, os.Symlink(sourceFile, intermediatePath))

	// Create user-facing symlink
	userSymlink := filepath.Join(homeDir, ".bashrc")
	require.NoError(t, os.Symlink(intermediatePath, userSymlink))

	// Create mock FS that uses real filesystem operations
	mockFS := &testFS{base: tempDir}

	// Create mock paths
	mockPaths := &testPaths{
		dataDir: dataDir,
		homeDir: homeDir,
	}

	// Create context with dry run
	ctx := types.ClearContext{
		Pack: types.Pack{
			Name: "testpack",
			Path: packDir,
		},
		FS:     mockFS,
		Paths:  mockPaths,
		DryRun: true,
	}

	// Execute Clear
	clearedItems, err := handler.Clear(ctx)
	require.NoError(t, err)

	// Verify results
	assert.Len(t, clearedItems, 1)
	assert.Equal(t, "symlink", clearedItems[0].Type)
	assert.Equal(t, userSymlink, clearedItems[0].Path)
	assert.Contains(t, clearedItems[0].Description, "Would remove symlink to .bashrc")

	// Verify user symlink was NOT removed (dry run)
	_, err = os.Lstat(userSymlink)
	assert.NoError(t, err, "User symlink should still exist in dry run")
}

// testFS is a filesystem implementation that operates on real files
// but within a test directory
type testFS struct {
	base string
}

func (f *testFS) normalizePath(path string) string {
	// If path is already within base, return as-is
	if strings.HasPrefix(path, f.base) {
		return path
	}
	// Otherwise, make it relative to base
	return filepath.Join(f.base, path)
}

func (f *testFS) Stat(name string) (fs.FileInfo, error) {
	return os.Stat(f.normalizePath(name))
}

func (f *testFS) Lstat(name string) (fs.FileInfo, error) {
	return os.Lstat(f.normalizePath(name))
}

func (f *testFS) ReadDir(name string) ([]fs.DirEntry, error) {
	return os.ReadDir(f.normalizePath(name))
}

func (f *testFS) ReadFile(name string) ([]byte, error) {
	return os.ReadFile(f.normalizePath(name))
}

func (f *testFS) WriteFile(name string, data []byte, perm fs.FileMode) error {
	return os.WriteFile(f.normalizePath(name), data, perm)
}

func (f *testFS) MkdirAll(path string, perm fs.FileMode) error {
	return os.MkdirAll(f.normalizePath(path), perm)
}

func (f *testFS) Remove(name string) error {
	return os.Remove(f.normalizePath(name))
}

func (f *testFS) RemoveAll(path string) error {
	return os.RemoveAll(f.normalizePath(path))
}

func (f *testFS) Symlink(oldname, newname string) error {
	// Symlinks might point outside base, so only normalize newname
	return os.Symlink(oldname, f.normalizePath(newname))
}

func (f *testFS) Readlink(name string) (string, error) {
	return os.Readlink(f.normalizePath(name))
}

// testPaths provides path resolution for tests
type testPaths struct {
	dataDir string
	homeDir string
}

func (p *testPaths) PackHandlerDir(packName, handlerName string) string {
	return filepath.Join(p.dataDir, "packs", packName, handlerName)
}

func (p *testPaths) MapPackFileToSystem(pack *types.Pack, relPath string) string {
	return filepath.Join(p.homeDir, relPath)
}
