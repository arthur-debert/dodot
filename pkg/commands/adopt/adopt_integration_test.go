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

// TestAdoptExceptionListBehavior tests that exception list files are adopted correctly
func TestAdoptExceptionListBehavior(t *testing.T) {
	// Setup test filesystem
	root := testutil.TempDir(t, "adopt-exception-test")
	dotfilesPath := filepath.Join(root, "dotfiles")
	homePath := filepath.Join(root, "home")

	// Create dotfiles directory
	require.NoError(t, os.MkdirAll(dotfilesPath, 0755))

	// Set HOME to test directory
	oldHome := os.Getenv("HOME")
	require.NoError(t, os.Setenv("HOME", homePath))
	defer func() { _ = os.Setenv("HOME", oldHome) }()

	// Unset XDG_CONFIG_HOME to ensure it's calculated from HOME
	oldXDG := os.Getenv("XDG_CONFIG_HOME")
	require.NoError(t, os.Unsetenv("XDG_CONFIG_HOME"))
	defer func() {
		if oldXDG != "" {
			_ = os.Setenv("XDG_CONFIG_HOME", oldXDG)
		}
	}()

	// Create exception list files (should go to HOME)
	testutil.CreateFile(t, homePath, ".ssh/config", "Host github.com\n  User git")
	testutil.CreateFile(t, homePath, ".gitconfig", "[user]\n  name = Test")
	testutil.CreateFile(t, homePath, ".aws/credentials", "[default]\nregion = us-east-1")

	// Create non-exception files (would normally go to XDG)
	testutil.CreateFile(t, filepath.Join(homePath, ".config"), "nvim/init.lua", "vim.opt.number = true")

	// Test adopting ssh config (exception list)
	result1, err := AdoptFiles(AdoptFilesOptions{
		DotfilesRoot: dotfilesPath,
		PackName:     "ssh",
		SourcePaths:  []string{filepath.Join(homePath, ".ssh/config")},
		Force:        false,
	})
	require.NoError(t, err)
	assert.Equal(t, 1, len(result1.AdoptedFiles))

	// Verify ssh config was placed correctly in pack (without dot)
	expectedPath := filepath.Join(dotfilesPath, "ssh", "ssh", "config")
	assert.True(t, testutil.FileExists(t, expectedPath), "ssh/config should exist in pack")

	// Test adopting gitconfig (exception list)
	result2, err := AdoptFiles(AdoptFilesOptions{
		DotfilesRoot: dotfilesPath,
		PackName:     "git",
		SourcePaths:  []string{filepath.Join(homePath, ".gitconfig")},
		Force:        false,
	})
	require.NoError(t, err)
	assert.Equal(t, 1, len(result2.AdoptedFiles))

	// Verify gitconfig was placed correctly in pack (without dot)
	expectedPath2 := filepath.Join(dotfilesPath, "git", "gitconfig")
	assert.True(t, testutil.FileExists(t, expectedPath2), "gitconfig should exist in pack")

	// Test adopting aws credentials (exception list)
	result3, err := AdoptFiles(AdoptFilesOptions{
		DotfilesRoot: dotfilesPath,
		PackName:     "aws",
		SourcePaths:  []string{filepath.Join(homePath, ".aws/credentials")},
		Force:        false,
	})
	require.NoError(t, err)
	assert.Equal(t, 1, len(result3.AdoptedFiles))

	// Verify aws credentials was placed correctly in pack (without dot)
	expectedPath3 := filepath.Join(dotfilesPath, "aws", "aws", "credentials")
	assert.True(t, testutil.FileExists(t, expectedPath3), "aws/credentials should exist in pack")

	// Deploy them back and verify they go to the right place
	testutil.CreateFile(t, dotfilesPath, "ssh/.dodot.toml", `[matchers]
[[matchers.items]]
triggers = [{ type = "always" }]
handler = { type = "symlink" }`)

	testutil.CreateFile(t, dotfilesPath, "git/.dodot.toml", `[matchers]
[[matchers.items]]
triggers = [{ type = "always" }]
handler = { type = "symlink" }`)

	testutil.CreateFile(t, dotfilesPath, "aws/.dodot.toml", `[matchers]
[[matchers.items]]
triggers = [{ type = "always" }]
handler = { type = "symlink" }`)

	// TODO: Add deploy test once we have the deploy command integrated
}

// TestAdoptExplicitOverrideBehavior tests that explicit override directories are handled correctly
func TestAdoptExplicitOverrideBehavior(t *testing.T) {
	// Setup test filesystem
	root := testutil.TempDir(t, "adopt-override-test")
	dotfilesPath := filepath.Join(root, "dotfiles")
	homePath := filepath.Join(root, "home")

	// Create dotfiles directory
	require.NoError(t, os.MkdirAll(dotfilesPath, 0755))

	// Set HOME to test directory
	oldHome := os.Getenv("HOME")
	require.NoError(t, os.Setenv("HOME", homePath))
	defer func() { _ = os.Setenv("HOME", oldHome) }()

	// Set XDG_CONFIG_HOME explicitly
	xdgConfigPath := filepath.Join(homePath, ".config")
	oldXDG := os.Getenv("XDG_CONFIG_HOME")
	require.NoError(t, os.Setenv("XDG_CONFIG_HOME", xdgConfigPath))
	defer func() {
		if oldXDG != "" {
			_ = os.Setenv("XDG_CONFIG_HOME", oldXDG)
		} else {
			_ = os.Unsetenv("XDG_CONFIG_HOME")
		}
	}()

	// Create test files in HOME and XDG directories
	testutil.CreateFile(t, homePath, ".special-tool", "tool config")
	testutil.CreateFile(t, xdgConfigPath, "app/config.toml", "app config")

	// Test 1: Adopt a file that will go into _home/ directory
	result1, err := AdoptFiles(AdoptFilesOptions{
		DotfilesRoot: dotfilesPath,
		PackName:     "overrides",
		SourcePaths:  []string{filepath.Join(homePath, ".special-tool")},
		Force:        false,
	})
	require.NoError(t, err)
	assert.Equal(t, 1, len(result1.AdoptedFiles))

	// Verify file was placed in pack root (not _home/)
	// The adopt command doesn't automatically create _home/ directories
	expectedPath1 := filepath.Join(dotfilesPath, "overrides", "special-tool")
	assert.True(t, testutil.FileExists(t, expectedPath1), "File should be adopted without override prefix")

	// Test 2: Adopt a file from XDG_CONFIG_HOME
	result2, err := AdoptFiles(AdoptFilesOptions{
		DotfilesRoot: dotfilesPath,
		PackName:     "overrides",
		SourcePaths:  []string{filepath.Join(xdgConfigPath, "app/config.toml")},
		Force:        false,
	})
	require.NoError(t, err)
	assert.Equal(t, 1, len(result2.AdoptedFiles))

	// Verify file was placed preserving directory structure (not _xdg/)
	expectedPath2 := filepath.Join(dotfilesPath, "overrides", "app", "config.toml")
	assert.True(t, testutil.FileExists(t, expectedPath2), "File should be adopted preserving structure")

	// Test 3: Manually create _home/ and _xdg/ directories and verify deployment would work
	// This demonstrates how users would use explicit overrides
	testutil.CreateFile(t, filepath.Join(dotfilesPath, "overrides"), "_home/forced-home-file", "forced to home")
	testutil.CreateFile(t, filepath.Join(dotfilesPath, "overrides"), "_xdg/forced-xdg-file", "forced to xdg")

	// Verify the pack structure
	assert.True(t, testutil.FileExists(t, filepath.Join(dotfilesPath, "overrides", "_home", "forced-home-file")))
	assert.True(t, testutil.FileExists(t, filepath.Join(dotfilesPath, "overrides", "_xdg", "forced-xdg-file")))
}

// TestAdoptWithCustomMappings tests that custom mappings in .dodot.toml work correctly
func TestAdoptWithCustomMappings(t *testing.T) {
	// Setup test filesystem
	root := testutil.TempDir(t, "adopt-mappings-test")
	dotfilesPath := filepath.Join(root, "dotfiles")
	homePath := filepath.Join(root, "home")

	// Create dotfiles directory
	require.NoError(t, os.MkdirAll(dotfilesPath, 0755))

	// Set HOME to test directory
	oldHome := os.Getenv("HOME")
	require.NoError(t, os.Setenv("HOME", homePath))
	defer func() { _ = os.Setenv("HOME", oldHome) }()

	// Create a pack with custom mappings config
	packPath := filepath.Join(dotfilesPath, "custom")
	require.NoError(t, os.MkdirAll(packPath, 0755))

	// Create .dodot.toml with mappings
	dodotConfig := `[mappings]
"special/config.toml" = "$HOME/.special/location/config.toml"
"*.secret" = "$HOME/.secrets/"
"data/*.json" = "$XDG_CONFIG_HOME/myapp/data/"
`
	testutil.CreateFile(t, packPath, ".dodot.toml", dodotConfig)

	// Create test files to adopt
	testutil.CreateFile(t, homePath, ".myconfig", "config content")

	// Adopt a file
	result, err := AdoptFiles(AdoptFilesOptions{
		DotfilesRoot: dotfilesPath,
		PackName:     "custom",
		SourcePaths:  []string{filepath.Join(homePath, ".myconfig")},
		Force:        false,
	})
	require.NoError(t, err)
	assert.Equal(t, 1, len(result.AdoptedFiles))

	// Verify file was adopted normally (mappings don't affect adopt)
	expectedPath := filepath.Join(packPath, "myconfig")
	assert.True(t, testutil.FileExists(t, expectedPath), "File should be adopted without mapping")

	// Create files that match the mappings to show how they would deploy
	testutil.CreateFile(t, packPath, "special/config.toml", "special config")
	testutil.CreateFile(t, packPath, "api.secret", "secret data")
	testutil.CreateFile(t, packPath, "data/settings.json", "json data")

	// Verify the pack structure with mapped files
	assert.True(t, testutil.FileExists(t, filepath.Join(packPath, "special", "config.toml")))
	assert.True(t, testutil.FileExists(t, filepath.Join(packPath, "api.secret")))
	assert.True(t, testutil.FileExists(t, filepath.Join(packPath, "data", "settings.json")))
}

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
