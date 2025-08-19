package adopt

import (
	"os"
	"path/filepath"
	"testing"

	_ "github.com/arthur-debert/dodot/pkg/matchers" // register default matchers
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestAdoptFullWorkflow tests the complete adopt workflow with real file system operations
func TestAdoptFullWorkflow(t *testing.T) {
	// Setup test filesystem
	root := testutil.TempDir(t, "adopt-integration-test")
	dotfilesPath := filepath.Join(root, "dotfiles")
	homePath := filepath.Join(root, "home")

	// Create dotfiles directory
	require.NoError(t, os.MkdirAll(dotfilesPath, 0755))

	// Set HOME to test directory
	oldHome := os.Getenv("HOME")
	require.NoError(t, os.Setenv("HOME", homePath))
	defer func() { _ = os.Setenv("HOME", oldHome) }()

	// Set XDG_CONFIG_HOME
	require.NoError(t, os.Setenv("XDG_CONFIG_HOME", filepath.Join(homePath, ".config")))
	defer func() { _ = os.Unsetenv("XDG_CONFIG_HOME") }()

	// Create initial files
	gitconfig := filepath.Join(homePath, ".gitconfig")
	testutil.CreateFile(t, homePath, ".gitconfig", "[user]\n\tname = Test User\n\temail = test@example.com")

	starshipConfig := filepath.Join(homePath, ".config", "starship", "starship.toml")
	testutil.CreateFile(t, filepath.Join(homePath, ".config", "starship"), "starship.toml", "format = \"$all$character\"")

	// Test 1: Adopt a single file
	result1, err := AdoptFiles(AdoptFilesOptions{
		DotfilesRoot: dotfilesPath,
		PackName:     "git",
		SourcePaths:  []string{gitconfig},
		Force:        false,
	})
	require.NoError(t, err)
	assert.Equal(t, 1, len(result1.AdoptedFiles))

	// Verify file was moved
	_, err = os.Stat(filepath.Join(dotfilesPath, "git", "gitconfig"))
	require.NoError(t, err)

	// Verify symlink exists and works
	info, err := os.Lstat(gitconfig)
	require.NoError(t, err)
	assert.True(t, info.Mode()&os.ModeSymlink != 0)

	// Verify content is accessible through symlink
	content, err := os.ReadFile(gitconfig)
	require.NoError(t, err)
	assert.Contains(t, string(content), "Test User")

	// Test 2: Adopt file with directory structure preservation
	result2, err := AdoptFiles(AdoptFilesOptions{
		DotfilesRoot: dotfilesPath,
		PackName:     "starship",
		SourcePaths:  []string{starshipConfig},
		Force:        false,
	})
	require.NoError(t, err)
	assert.Equal(t, 1, len(result2.AdoptedFiles))

	// Verify directory structure was preserved
	movedPath := filepath.Join(dotfilesPath, "starship", "starship", "starship.toml")
	_, err = os.Stat(movedPath)
	require.NoError(t, err)

	// Test 3: Try to adopt already managed file (idempotent)
	result3, err := AdoptFiles(AdoptFilesOptions{
		DotfilesRoot: dotfilesPath,
		PackName:     "git",
		SourcePaths:  []string{gitconfig},
		Force:        false,
	})
	require.NoError(t, err)
	assert.Equal(t, 0, len(result3.AdoptedFiles), "Should not re-adopt already managed file")
}

// TestAdoptWithExistingDestination tests the behavior when destination already exists
func TestAdoptWithExistingDestination(t *testing.T) {
	// Setup test filesystem
	root := testutil.TempDir(t, "adopt-integration-test")
	dotfilesPath := filepath.Join(root, "dotfiles")
	homePath := filepath.Join(root, "home")

	// Create dotfiles directory
	require.NoError(t, os.MkdirAll(dotfilesPath, 0755))

	// Set HOME to test directory
	oldHome := os.Getenv("HOME")
	require.NoError(t, os.Setenv("HOME", homePath))
	defer func() { _ = os.Setenv("HOME", oldHome) }()

	// Create source file
	gitconfig := filepath.Join(homePath, ".gitconfig")
	testutil.CreateFile(t, homePath, ".gitconfig", "[user]\n\tname = New User")

	// Create existing destination file
	existingPath := filepath.Join(dotfilesPath, "git", "gitconfig")
	testutil.CreateFile(t, filepath.Join(dotfilesPath, "git"), "gitconfig", "[user]\n\tname = Old User")

	// Test 1: Without force flag - should fail
	_, err := AdoptFiles(AdoptFilesOptions{
		DotfilesRoot: dotfilesPath,
		PackName:     "git",
		SourcePaths:  []string{gitconfig},
		Force:        false,
	})
	require.Error(t, err)
	assert.Contains(t, err.Error(), "destination already exists")

	// Verify source file was not moved
	_, err = os.Stat(gitconfig)
	require.NoError(t, err)

	// Test 2: With force flag - should succeed
	result, err := AdoptFiles(AdoptFilesOptions{
		DotfilesRoot: dotfilesPath,
		PackName:     "git",
		SourcePaths:  []string{gitconfig},
		Force:        true,
	})
	require.NoError(t, err)
	assert.Equal(t, 1, len(result.AdoptedFiles))

	// Verify new content replaced old
	content, err := os.ReadFile(existingPath)
	require.NoError(t, err)
	assert.Contains(t, string(content), "New User")
	assert.NotContains(t, string(content), "Old User")
}

// TestAdoptMultipleFiles tests adopting multiple files in one operation
func TestAdoptMultipleFiles(t *testing.T) {
	// Setup test filesystem
	root := testutil.TempDir(t, "adopt-integration-test")
	dotfilesPath := filepath.Join(root, "dotfiles")
	homePath := filepath.Join(root, "home")

	// Create dotfiles directory
	require.NoError(t, os.MkdirAll(dotfilesPath, 0755))

	// Set HOME to test directory
	oldHome := os.Getenv("HOME")
	require.NoError(t, os.Setenv("HOME", homePath))
	defer func() { _ = os.Setenv("HOME", oldHome) }()

	// Create multiple shell config files
	files := map[string]string{
		".bashrc":       "# bashrc content",
		".bash_profile": "# bash_profile content",
		".bash_aliases": "# aliases content",
	}

	sourcePaths := []string{}
	for name, content := range files {
		path := filepath.Join(homePath, name)
		testutil.CreateFile(t, homePath, name, content)
		sourcePaths = append(sourcePaths, path)
	}

	// Adopt all files at once
	result, err := AdoptFiles(AdoptFilesOptions{
		DotfilesRoot: dotfilesPath,
		PackName:     "shell",
		SourcePaths:  sourcePaths,
		Force:        false,
	})
	require.NoError(t, err)
	assert.Equal(t, len(files), len(result.AdoptedFiles))

	// Verify all files were moved and symlinked
	for name, expectedContent := range files {
		// Check symlink exists
		symlinkPath := filepath.Join(homePath, name)
		info, err := os.Lstat(symlinkPath)
		require.NoError(t, err)
		assert.True(t, info.Mode()&os.ModeSymlink != 0)

		// Check file was moved to pack
		movedName := name[1:] // Remove leading dot
		movedPath := filepath.Join(dotfilesPath, "shell", movedName)
		_, err = os.Stat(movedPath)
		require.NoError(t, err)

		// Verify content through symlink
		content, err := os.ReadFile(symlinkPath)
		require.NoError(t, err)
		assert.Equal(t, expectedContent, string(content))
	}
}
