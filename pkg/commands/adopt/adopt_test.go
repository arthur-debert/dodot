// pkg/commands/adopt/adopt_test.go
// TEST TYPE: Business Logic Integration
// DEPENDENCIES: Mock DataStore, Memory FS
// PURPOSE: Test adopt command orchestration for file adoption and pack management

package adopt_test

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/commands/adopt"
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestAdoptFiles_EmptySourcePaths_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	opts := adopt.AdoptFilesOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "test-pack",
		SourcePaths:  []string{},
		Force:        false,
		FileSystem:   env.FS,
	}

	// Execute
	result, err := adopt.AdoptFiles(opts)

	// Verify empty sources handling
	require.NoError(t, err)
	assert.NotNil(t, result, "should return adoption result")
	assert.Equal(t, "test-pack", result.PackName, "pack name should match")
	assert.Len(t, result.AdoptedFiles, 0, "should adopt no files")
}

func TestAdoptFiles_SingleFile_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create a file to adopt in the mock home directory
	homeFile := filepath.Join(env.HomeDir, ".gitconfig")
	err := env.FS.WriteFile(homeFile, []byte("user.name = Test User"), 0644)
	require.NoError(t, err)

	opts := adopt.AdoptFilesOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "git",
		SourcePaths:  []string{homeFile},
		Force:        false,
		FileSystem:   env.FS,
	}

	// Execute
	result, err := adopt.AdoptFiles(opts)

	// Verify single file adoption orchestration
	require.NoError(t, err)
	assert.NotNil(t, result, "should return adoption result")
	assert.Equal(t, "git", result.PackName, "pack name should match")

	// Should have adopted one file
	assert.Len(t, result.AdoptedFiles, 1, "should adopt one file")
	adopted := result.AdoptedFiles[0]
	assert.Equal(t, homeFile, adopted.OriginalPath, "original path should match")
	assert.Equal(t, homeFile, adopted.SymlinkPath, "symlink path should match original")
	assert.Contains(t, adopted.NewPath, "git/gitconfig", "new path should be in git pack")

	// Verify file was moved and symlinked (orchestration behavior)
	// File should no longer exist at original location as regular file
	info, err := env.FS.Lstat(homeFile)
	require.NoError(t, err)
	assert.True(t, info.Mode()&os.ModeSymlink != 0, "original location should be symlink")

	// File should exist at new location
	_, err = env.FS.Stat(adopted.NewPath)
	assert.NoError(t, err, "file should exist at new path")
}

func TestAdoptFiles_MultipleFiles_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

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

	opts := adopt.AdoptFilesOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "shell",
		SourcePaths:  sourcePaths,
		Force:        false,
		FileSystem:   env.FS,
	}

	// Execute
	result, err := adopt.AdoptFiles(opts)

	// Verify multiple file adoption orchestration
	require.NoError(t, err)
	assert.NotNil(t, result, "should return adoption result")
	assert.Equal(t, "shell", result.PackName, "pack name should match")
	assert.Len(t, result.AdoptedFiles, 3, "should adopt all three files")

	// Verify all files were processed
	adoptedPaths := make(map[string]bool)
	for _, adopted := range result.AdoptedFiles {
		adoptedPaths[filepath.Base(adopted.OriginalPath)] = true
		assert.Contains(t, adopted.NewPath, "shell/", "new path should be in shell pack")
	}

	for filename := range files {
		assert.True(t, adoptedPaths[filename], "should have adopted %s", filename)
	}
}

func TestAdoptFiles_NonExistentFile_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	nonExistentFile := filepath.Join(env.HomeDir, ".nonexistent")

	opts := adopt.AdoptFilesOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "test",
		SourcePaths:  []string{nonExistentFile},
		Force:        false,
		FileSystem:   env.FS,
	}

	// Execute
	result, err := adopt.AdoptFiles(opts)

	// Verify error handling for non-existent files
	assert.Error(t, err, "should return error for non-existent file")
	assert.Contains(t, err.Error(), "source file does not exist", "error should mention non-existent file")
	// Result should be nil on error
	assert.Nil(t, result, "result should be nil on error")
}

func TestAdoptFiles_NewPackCreation_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create file to adopt
	sourceFile := filepath.Join(env.HomeDir, ".vimrc")
	err := env.FS.WriteFile(sourceFile, []byte("set number"), 0644)
	require.NoError(t, err)

	opts := adopt.AdoptFilesOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "newpack", // Pack doesn't exist yet
		SourcePaths:  []string{sourceFile},
		Force:        false,
		FileSystem:   env.FS,
	}

	// Execute
	result, err := adopt.AdoptFiles(opts)

	// Verify new pack creation orchestration
	require.NoError(t, err)
	assert.NotNil(t, result, "should return adoption result")
	assert.Equal(t, "newpack", result.PackName, "pack name should match")
	assert.Len(t, result.AdoptedFiles, 1, "should adopt the file")

	// Verify pack directory was created
	packPath := filepath.Join(env.DotfilesRoot, "newpack")
	info, err := env.FS.Stat(packPath)
	require.NoError(t, err)
	assert.True(t, info.IsDir(), "pack directory should be created")

	// Verify file was adopted into new pack
	adopted := result.AdoptedFiles[0]
	assert.Contains(t, adopted.NewPath, "newpack/vimrc", "file should be in new pack")
}

func TestAdoptFiles_ForceOverwrite_Orchestration(t *testing.T) {
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

	opts := adopt.AdoptFilesOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "git",
		SourcePaths:  []string{sourceFile},
		Force:        true, // Key: force overwrite
		FileSystem:   env.FS,
	}

	// Execute
	result, err := adopt.AdoptFiles(opts)

	// Verify force overwrite orchestration
	require.NoError(t, err)
	assert.NotNil(t, result, "should return adoption result")
	assert.Len(t, result.AdoptedFiles, 1, "should adopt file with force")

	// Verify new content was written
	content, err := env.FS.ReadFile(existingFile)
	require.NoError(t, err)
	assert.Equal(t, "new content", string(content), "file should have new content")
}

func TestAdoptFiles_ConflictWithoutForce_Orchestration(t *testing.T) {
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

	opts := adopt.AdoptFilesOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "git",
		SourcePaths:  []string{sourceFile},
		Force:        false, // Key: no force
		FileSystem:   env.FS,
	}

	// Execute
	result, err := adopt.AdoptFiles(opts)

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

func TestAdoptFiles_InvalidPackName_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create file to adopt
	sourceFile := filepath.Join(env.HomeDir, ".testrc")
	err := env.FS.WriteFile(sourceFile, []byte("test content"), 0644)
	require.NoError(t, err)

	opts := adopt.AdoptFilesOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "", // Invalid: empty pack name
		SourcePaths:  []string{sourceFile},
		Force:        false,
		FileSystem:   env.FS,
	}

	// Execute
	result, err := adopt.AdoptFiles(opts)

	// Verify pack name validation
	assert.Error(t, err, "should return error for empty pack name")
	var dodotErr *errors.DodotError
	if assert.ErrorAs(t, err, &dodotErr) {
		assert.Equal(t, errors.ErrPackNotFound, dodotErr.Code, "should have pack not found error code")
	}
	assert.Nil(t, result, "result should be nil on error")
}

func TestAdoptFiles_PackNameTrailingSlash_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create file to adopt
	sourceFile := filepath.Join(env.HomeDir, ".testrc")
	err := env.FS.WriteFile(sourceFile, []byte("test content"), 0644)
	require.NoError(t, err)

	opts := adopt.AdoptFilesOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "test/", // Trailing slash (from shell completion)
		SourcePaths:  []string{sourceFile},
		Force:        false,
		FileSystem:   env.FS,
	}

	// Execute
	result, err := adopt.AdoptFiles(opts)

	// Verify trailing slash handling
	require.NoError(t, err)
	assert.NotNil(t, result, "should return adoption result")
	assert.Equal(t, "test", result.PackName, "pack name should be normalized (no trailing slash)")
	assert.Len(t, result.AdoptedFiles, 1, "should adopt the file")
}

func TestAdoptFiles_IdempotentBehavior_Orchestration(t *testing.T) {
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

	opts := adopt.AdoptFilesOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "git",
		SourcePaths:  []string{symlinkPath},
		Force:        false,
		FileSystem:   env.FS,
	}

	// Execute
	result, err := adopt.AdoptFiles(opts)

	// Verify idempotent behavior (already managed files are skipped)
	require.NoError(t, err)
	assert.NotNil(t, result, "should return adoption result")
	assert.Equal(t, "git", result.PackName, "pack name should match")
	assert.Len(t, result.AdoptedFiles, 0, "should not adopt already managed file")
}

func TestAdoptFiles_XDGConfigFile_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

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

	opts := adopt.AdoptFilesOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "starship",
		SourcePaths:  []string{configFile},
		Force:        false,
		FileSystem:   env.FS,
	}

	// Execute
	result, err := adopt.AdoptFiles(opts)

	// Verify XDG config adoption orchestration
	require.NoError(t, err)
	assert.NotNil(t, result, "should return adoption result")
	assert.Equal(t, "starship", result.PackName, "pack name should match")
	assert.Len(t, result.AdoptedFiles, 1, "should adopt XDG config file")

	// Verify XDG structure is preserved in pack
	adopted := result.AdoptedFiles[0]
	assert.Contains(t, adopted.NewPath, "starship/starship/starship.toml", "XDG structure should be preserved")
}

func TestAdoptFiles_PartialFailure_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create one valid file and one invalid path
	validFile := filepath.Join(env.HomeDir, ".gitconfig")
	err := env.FS.WriteFile(validFile, []byte("valid content"), 0644)
	require.NoError(t, err)

	invalidFile := filepath.Join(env.HomeDir, ".nonexistent")

	opts := adopt.AdoptFilesOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "mixed",
		SourcePaths:  []string{validFile, invalidFile}, // Mix of valid and invalid
		Force:        false,
		FileSystem:   env.FS,
	}

	// Execute
	result, err := adopt.AdoptFiles(opts)

	// Verify partial failure handling
	assert.Error(t, err, "should return error for invalid file")
	assert.Contains(t, err.Error(), "source file does not exist", "error should mention non-existent file")
	// On error, entire operation fails (no partial success)
	assert.Nil(t, result, "result should be nil on any file failure")

	// Verify valid file was not processed (atomic behavior)
	_, err = env.FS.Stat(validFile)
	assert.NoError(t, err, "valid file should remain at original location")
}

func TestAdoptFiles_ResultStructure_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create test file
	sourceFile := filepath.Join(env.HomeDir, ".testrc")
	err := env.FS.WriteFile(sourceFile, []byte("test content"), 0644)
	require.NoError(t, err)

	opts := adopt.AdoptFilesOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "structure-test",
		SourcePaths:  []string{sourceFile},
		Force:        false,
		FileSystem:   env.FS,
	}

	// Execute
	result, err := adopt.AdoptFiles(opts)

	// Verify result structure completeness
	require.NoError(t, err)
	require.NotNil(t, result, "result should not be nil")

	// Verify AdoptResult structure
	assert.Equal(t, "structure-test", result.PackName, "pack name should match")
	assert.NotNil(t, result.AdoptedFiles, "adopted files should not be nil")
	assert.Len(t, result.AdoptedFiles, 1, "should have one adopted file")

	// Verify AdoptedFile structure
	adopted := result.AdoptedFiles[0]
	assert.NotEmpty(t, adopted.OriginalPath, "original path should be populated")
	assert.NotEmpty(t, adopted.NewPath, "new path should be populated")
	assert.NotEmpty(t, adopted.SymlinkPath, "symlink path should be populated")
	assert.Equal(t, sourceFile, adopted.OriginalPath, "original path should match source")
	assert.Equal(t, sourceFile, adopted.SymlinkPath, "symlink path should match original")
}

func TestAdoptFiles_FileSystemIntegration_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create nested directory structure
	nestedDir := filepath.Join(env.HomeDir, ".config", "app", "deep")
	nestedFile := filepath.Join(nestedDir, "config.json")
	err := env.FS.MkdirAll(nestedDir, 0755)
	require.NoError(t, err)
	err = env.FS.WriteFile(nestedFile, []byte("{\"setting\": \"value\"}"), 0644)
	require.NoError(t, err)

	opts := adopt.AdoptFilesOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     "app",
		SourcePaths:  []string{nestedFile},
		Force:        false,
		FileSystem:   env.FS,
	}

	// Execute
	result, err := adopt.AdoptFiles(opts)

	// Verify complex filesystem operation orchestration
	require.NoError(t, err)
	assert.NotNil(t, result, "should return adoption result")
	assert.Len(t, result.AdoptedFiles, 1, "should adopt nested file")

	adopted := result.AdoptedFiles[0]

	// Verify directory structure was created in pack
	packConfigDir := filepath.Dir(adopted.NewPath)
	info, err := env.FS.Stat(packConfigDir)
	require.NoError(t, err)
	assert.True(t, info.IsDir(), "nested directory should be created in pack")

	// Verify symlink chain works correctly
	target, err := env.FS.Readlink(adopted.SymlinkPath)
	require.NoError(t, err)
	assert.Equal(t, adopted.NewPath, target, "symlink should point to adopted file")

	// Verify file content is preserved
	content, err := env.FS.ReadFile(adopted.NewPath)
	require.NoError(t, err)
	assert.Equal(t, "{\"setting\": \"value\"}", string(content), "file content should be preserved")
}
