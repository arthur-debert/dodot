// pkg/pack/adopt_test.go
// TEST TYPE: Business Logic Integration
// DEPENDENCIES: Mock DataStore, Memory FS
// PURPOSE: Test adopt command orchestration for file adoption and pack management

package pack_test

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/arthur-debert/dodot/pkg/pack"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestAdopt_EmptySourcePaths_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create the pack first
	env.SetupPack("test-pack", testutil.PackConfig{})

	opts := pack.AdoptOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "test-pack",
		SourcePaths:  []string{},
		Force:        false,
		FileSystem:   env.FS,
	}

	// Execute
	result, err := pack.Adopt(opts)

	// Verify empty sources handling - adopt succeeds but adopts nothing
	require.NoError(t, err)
	require.NotNil(t, result)
	assert.Equal(t, 0, result.Metadata.FilesAdopted, "should adopt zero files")
	assert.Empty(t, result.Metadata.AdoptedPaths, "should have no adopted paths")
}

func TestAdopt_SingleFile_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create the pack first
	env.SetupPack("git", testutil.PackConfig{})

	// Create a file to adopt in the mock home directory
	homeFile := filepath.Join(env.HomeDir, ".gitconfig")
	err := env.FS.WriteFile(homeFile, []byte("user.name = Test User"), 0644)
	require.NoError(t, err)

	opts := pack.AdoptOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "git",
		SourcePaths:  []string{homeFile},
		Force:        false,
		FileSystem:   env.FS,
	}

	// Execute
	result, err := pack.Adopt(opts)

	// Verify single file adoption orchestration
	require.NoError(t, err)
	assert.NotNil(t, result, "should return adoption result")
	assert.Equal(t, "adopt", result.Command, "command should be adopt")
	assert.Equal(t, 1, result.Metadata.FilesAdopted, "should adopt one file")
	assert.Len(t, result.Metadata.AdoptedPaths, 1, "should have one adopted path")
	assert.Equal(t, homeFile, result.Metadata.AdoptedPaths[0], "adopted path should match")
	// Note: Pack status is only available if GetPackStatus function is provided

	// Verify file was moved and symlinked (orchestration behavior)
	// File should no longer exist at original location as regular file
	info, err := env.FS.Lstat(homeFile)
	require.NoError(t, err)
	assert.True(t, info.Mode()&os.ModeSymlink != 0, "original location should be symlink")

	// File should exist at new location in the pack
	newPath := filepath.Join(env.DotfilesRoot, "git", "gitconfig")
	_, err = env.FS.Stat(newPath)
	assert.NoError(t, err, "file should exist at new path")
}

func TestAdopt_MultipleFiles_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create the pack first
	env.SetupPack("shell", testutil.PackConfig{})

	// Create multiple files to adopt
	files := map[string]string{
		".bashrc":       "# bashrc content",
		".bash_profile": "# profile content",
		".bash_aliases": "# aliases content",
	}

	var sourcePaths []string
	for filename, content := range files {
		filePath := filepath.Join(env.HomeDir, filename)
		err := env.FS.WriteFile(filePath, []byte(content), 0644)
		require.NoError(t, err)
		sourcePaths = append(sourcePaths, filePath)
	}

	opts := pack.AdoptOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "shell",
		SourcePaths:  sourcePaths,
		Force:        false,
		FileSystem:   env.FS,
	}

	// Execute
	result, err := pack.Adopt(opts)

	// Verify multiple file adoption orchestration
	require.NoError(t, err)
	assert.NotNil(t, result, "should return adoption result")
	assert.Equal(t, "adopt", result.Command, "command should be adopt")
	assert.Equal(t, 3, result.Metadata.FilesAdopted, "should adopt all three files")
	assert.Len(t, result.Metadata.AdoptedPaths, 3, "should have three adopted paths")
	// Note: Pack status is only available if GetPackStatus function is provided

	// Verify all files were processed
	adoptedPaths := make(map[string]bool)
	for _, path := range result.Metadata.AdoptedPaths {
		adoptedPaths[filepath.Base(path)] = true
	}

	for filename := range files {
		assert.True(t, adoptedPaths[filename], "should have adopted %s", filename)
	}
}

func TestAdopt_NonExistentFile_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create the pack first
	env.SetupPack("test", testutil.PackConfig{})

	nonExistentFile := filepath.Join(env.HomeDir, ".nonexistent")

	opts := pack.AdoptOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "test",
		SourcePaths:  []string{nonExistentFile},
		Force:        false,
		FileSystem:   env.FS,
	}

	// Execute
	result, err := pack.Adopt(opts)

	// Verify error handling for non-existent files
	assert.Error(t, err, "should return error for non-existent file")
	assert.Contains(t, err.Error(), "source file does not exist", "error should mention non-existent file")
	// Result should be nil on error
	assert.Nil(t, result, "result should be nil on error")
}

func TestAdopt_NewPackCreation_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create file to adopt
	sourceFile := filepath.Join(env.HomeDir, ".vimrc")
	err := env.FS.WriteFile(sourceFile, []byte("set number"), 0644)
	require.NoError(t, err)

	opts := pack.AdoptOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "newpack", // Pack doesn't exist yet
		SourcePaths:  []string{sourceFile},
		Force:        false,
		FileSystem:   env.FS,
	}

	// Execute
	result, err := pack.Adopt(opts)

	// Verify error for non-existent pack
	require.Error(t, err)
	assert.Contains(t, err.Error(), "does not exist")
	assert.Contains(t, err.Error(), "dodot init newpack")
	assert.Nil(t, result)
}

func TestAdopt_ForceOverwrite_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create existing pack with conflicting file
	packDir := filepath.Join(env.DotfilesRoot, "git")
	err := env.FS.MkdirAll(packDir, 0755)
	require.NoError(t, err)

	existingFile := filepath.Join(packDir, "gitconfig")
	err = env.FS.WriteFile(existingFile, []byte("old content"), 0644)
	require.NoError(t, err)

	// Create source file with new content
	sourceFile := filepath.Join(env.HomeDir, ".gitconfig")
	err = env.FS.WriteFile(sourceFile, []byte("new content"), 0644)
	require.NoError(t, err)

	opts := pack.AdoptOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "git",
		SourcePaths:  []string{sourceFile},
		Force:        true, // Key: force overwrite
		FileSystem:   env.FS,
	}

	// Execute
	result, err := pack.Adopt(opts)

	// Verify force overwrite orchestration
	require.NoError(t, err)
	assert.NotNil(t, result, "should return adoption result")
	assert.Equal(t, "adopt", result.Command, "command should be adopt")
	assert.Equal(t, 1, result.Metadata.FilesAdopted, "should adopt file with force")

	// Verify new content was written
	content, err := env.FS.ReadFile(existingFile)
	require.NoError(t, err)
	assert.Equal(t, "new content", string(content), "file should have new content")
}

func TestAdopt_ConflictWithoutForce_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create existing pack with conflicting file
	packDir := filepath.Join(env.DotfilesRoot, "git")
	err := env.FS.MkdirAll(packDir, 0755)
	require.NoError(t, err)

	existingFile := filepath.Join(packDir, "gitconfig")
	err = env.FS.WriteFile(existingFile, []byte("existing content"), 0644)
	require.NoError(t, err)

	// Create source file
	sourceFile := filepath.Join(env.HomeDir, ".gitconfig")
	err = env.FS.WriteFile(sourceFile, []byte("new content"), 0644)
	require.NoError(t, err)

	opts := pack.AdoptOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "git",
		SourcePaths:  []string{sourceFile},
		Force:        false, // Key: no force
		FileSystem:   env.FS,
	}

	// Execute
	result, err := pack.Adopt(opts)

	// Verify conflict handling without force
	assert.Error(t, err, "should return error for existing destination")
	assert.Contains(t, err.Error(), "destination already exists", "error should mention conflict")
	assert.Contains(t, err.Error(), "use --force to overwrite", "error should suggest force flag")
	assert.Nil(t, result, "result should be nil on error")

	// Verify original file remains unchanged
	content, err := env.FS.ReadFile(existingFile)
	require.NoError(t, err)
	assert.Equal(t, "existing content", string(content), "existing file should be unchanged")
}

func TestAdopt_InvalidPackName_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create file to adopt
	sourceFile := filepath.Join(env.HomeDir, ".testrc")
	err := env.FS.WriteFile(sourceFile, []byte("test content"), 0644)
	require.NoError(t, err)

	opts := pack.AdoptOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "", // Invalid: empty pack name
		SourcePaths:  []string{sourceFile},
		Force:        false,
		FileSystem:   env.FS,
	}

	// Execute
	result, err := pack.Adopt(opts)

	// Verify pack name validation
	assert.Error(t, err, "should return error for empty pack name")
	assert.Contains(t, err.Error(), "pack name cannot be empty")
	assert.Nil(t, result, "result should be nil on error")
}

func TestAdopt_PackNameTrailingSlash_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create pack first (without trailing slash)
	env.SetupPack("test", testutil.PackConfig{})

	// Create file to adopt
	sourceFile := filepath.Join(env.HomeDir, ".testrc")
	err := env.FS.WriteFile(sourceFile, []byte("test content"), 0644)
	require.NoError(t, err)

	opts := pack.AdoptOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "test/", // Trailing slash (from shell completion)
		SourcePaths:  []string{sourceFile},
		Force:        false,
		FileSystem:   env.FS,
	}

	// Execute
	result, err := pack.Adopt(opts)

	// Verify trailing slash handling
	require.NoError(t, err)
	assert.NotNil(t, result, "should return adoption result")
	assert.Equal(t, "adopt", result.Command, "command should be adopt")
	assert.Equal(t, 1, result.Metadata.FilesAdopted, "should adopt the file")
	// Note: Pack status is only available if GetPackStatus function is provided
}

func TestAdopt_IdempotentBehavior_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create pack and file structure
	packDir := filepath.Join(env.DotfilesRoot, "git")
	err := env.FS.MkdirAll(packDir, 0755)
	require.NoError(t, err)

	// Create managed file in pack
	managedFile := filepath.Join(packDir, "gitconfig")
	err = env.FS.WriteFile(managedFile, []byte("managed content"), 0644)
	require.NoError(t, err)

	// Create symlink at original location (simulating already adopted file)
	symlinkPath := filepath.Join(env.HomeDir, ".gitconfig")
	err = env.FS.MkdirAll(env.HomeDir, 0755)
	require.NoError(t, err)
	err = env.FS.Symlink(managedFile, symlinkPath)
	require.NoError(t, err)

	opts := pack.AdoptOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "git",
		SourcePaths:  []string{symlinkPath},
		Force:        false,
		FileSystem:   env.FS,
	}

	// Execute
	result, err := pack.Adopt(opts)

	// Verify idempotent behavior (already managed files are skipped)
	require.NoError(t, err)
	assert.NotNil(t, result, "should return adoption result")
	assert.Equal(t, "adopt", result.Command, "command should be adopt")
	assert.Equal(t, 0, result.Metadata.FilesAdopted, "should not adopt already managed file")
	// Note: Pack status is only available if GetPackStatus function is provided
}

func TestAdopt_XDGConfigFile_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create the pack first
	env.SetupPack("starship", testutil.PackConfig{})

	// Create XDG config file
	configDir := filepath.Join(env.HomeDir, ".config", "starship")
	configFile := filepath.Join(configDir, "starship.toml")
	err := env.FS.MkdirAll(configDir, 0755)
	require.NoError(t, err)
	err = env.FS.WriteFile(configFile, []byte("format = \"$all$character\""), 0644)
	require.NoError(t, err)

	// Set XDG_CONFIG_HOME for path mapping
	oldXDG := os.Getenv("XDG_CONFIG_HOME")
	require.NoError(t, os.Setenv("XDG_CONFIG_HOME", filepath.Join(env.HomeDir, ".config")))
	defer func() {
		if oldXDG == "" {
			_ = os.Unsetenv("XDG_CONFIG_HOME")
		} else {
			_ = os.Setenv("XDG_CONFIG_HOME", oldXDG)
		}
	}()

	opts := pack.AdoptOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "starship",
		SourcePaths:  []string{configFile},
		Force:        false,
		FileSystem:   env.FS,
	}

	// Execute
	result, err := pack.Adopt(opts)

	// Verify XDG config adoption orchestration
	require.NoError(t, err)
	assert.NotNil(t, result, "should return adoption result")
	assert.Equal(t, "adopt", result.Command, "command should be adopt")
	assert.Equal(t, 1, result.Metadata.FilesAdopted, "should adopt XDG config file")
	// Note: Pack status is only available if GetPackStatus function is provided

	// Verify XDG structure is preserved in pack
	newPath := filepath.Join(env.DotfilesRoot, "starship", "starship", "starship.toml")
	_, err = env.FS.Stat(newPath)
	assert.NoError(t, err, "XDG structure should be preserved in pack")
}

func TestAdopt_PartialFailure_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create the pack first
	env.SetupPack("mixed", testutil.PackConfig{})

	// Create one valid file and one invalid path
	validFile := filepath.Join(env.HomeDir, ".gitconfig")
	err := env.FS.WriteFile(validFile, []byte("valid content"), 0644)
	require.NoError(t, err)

	invalidFile := filepath.Join(env.HomeDir, ".nonexistent")

	opts := pack.AdoptOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "mixed",
		SourcePaths:  []string{validFile, invalidFile}, // Mix of valid and invalid
		Force:        false,
		FileSystem:   env.FS,
	}

	// Execute
	result, err := pack.Adopt(opts)

	// Verify partial failure handling
	assert.Error(t, err, "should return error for invalid file")
	assert.Contains(t, err.Error(), "source file does not exist", "error should mention non-existent file")
	// On error, entire operation fails (no partial success)
	assert.Nil(t, result, "result should be nil on any file failure")

	// Verify valid file was not processed (atomic behavior)
	_, err = env.FS.Stat(validFile)
	assert.NoError(t, err, "valid file should remain at original location")
}

func TestAdopt_ResultStructure_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create the pack first
	env.SetupPack("structure-test", testutil.PackConfig{})

	// Create test file
	sourceFile := filepath.Join(env.HomeDir, ".testrc")
	err := env.FS.WriteFile(sourceFile, []byte("test content"), 0644)
	require.NoError(t, err)

	opts := pack.AdoptOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "structure-test",
		SourcePaths:  []string{sourceFile},
		Force:        false,
		FileSystem:   env.FS,
	}

	// Execute
	result, err := pack.Adopt(opts)

	// Verify result structure completeness
	require.NoError(t, err)
	require.NotNil(t, result, "result should not be nil")

	// Verify PackCommandResult structure
	assert.Equal(t, "adopt", result.Command, "command should be adopt")
	assert.Equal(t, 1, result.Metadata.FilesAdopted, "should have one adopted file")
	assert.Len(t, result.Metadata.AdoptedPaths, 1, "should have one adopted path")
	// Note: Pack status is only available if GetPackStatus function is provided

	// Verify adopted path structure
	assert.Len(t, result.Metadata.AdoptedPaths, 1, "should have adopted path")
	if len(result.Metadata.AdoptedPaths) > 0 {
		assert.Equal(t, sourceFile, result.Metadata.AdoptedPaths[0], "adopted path should match source")
	}
}

func TestAdopt_FileSystemIntegration_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create the pack first
	env.SetupPack("app", testutil.PackConfig{})

	// Create nested directory structure
	nestedDir := filepath.Join(env.HomeDir, ".config", "app", "deep")
	nestedFile := filepath.Join(nestedDir, "config.json")
	err := env.FS.MkdirAll(nestedDir, 0755)
	require.NoError(t, err)
	err = env.FS.WriteFile(nestedFile, []byte("{\"setting\": \"value\"}"), 0644)
	require.NoError(t, err)

	opts := pack.AdoptOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "app",
		SourcePaths:  []string{nestedFile},
		Force:        false,
		FileSystem:   env.FS,
	}

	// Execute
	result, err := pack.Adopt(opts)

	// Verify complex filesystem operation orchestration
	require.NoError(t, err)
	assert.NotNil(t, result, "should return adoption result")
	assert.Equal(t, "adopt", result.Command, "command should be adopt")
	assert.Equal(t, 1, result.Metadata.FilesAdopted, "should adopt nested file")

	// Verify symlink was created and get the actual destination path
	target, err := env.FS.Readlink(nestedFile)
	require.NoError(t, err)
	assert.True(t, strings.HasPrefix(target, env.DotfilesRoot), "symlink should point to dotfiles directory")
	assert.True(t, strings.Contains(target, "/app/"), "symlink should point to app pack")
	assert.True(t, strings.HasSuffix(target, "/config.json"), "symlink should point to config.json file")

	// Verify directory structure was created in pack (use actual target path)
	packConfigDir := filepath.Dir(target)
	info, err := env.FS.Stat(packConfigDir)
	require.NoError(t, err)
	assert.True(t, info.IsDir(), "nested directory should be created in pack")

	// Verify file content is preserved at the actual target location
	content, err := env.FS.ReadFile(target)
	require.NoError(t, err)
	assert.Equal(t, "{\"setting\": \"value\"}", string(content), "file content should be preserved")
}