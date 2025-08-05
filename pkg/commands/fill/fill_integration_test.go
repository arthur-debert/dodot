package fill

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/commands/initialize"
	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/synthfs"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestFillPack_Integration(t *testing.T) {
	// Setup test environment
	testEnv := testutil.NewTestEnvironment(t, "fill-integration")
	defer testEnv.Cleanup()

	// Create a pack directory
	packName := "testpack"
	packPath := filepath.Join(testEnv.DotfilesRoot(), packName)
	require.NoError(t, os.MkdirAll(packPath, 0755))

	// Create .dodot.toml
	packConfig := `# Test pack config`
	require.NoError(t, os.WriteFile(
		filepath.Join(packPath, ".dodot.toml"),
		[]byte(packConfig),
		0644,
	))

	// Execute FillPack
	result, err := FillPack(FillPackOptions{
		DotfilesRoot: testEnv.DotfilesRoot(),
		PackName:     packName,
	})
	require.NoError(t, err)
	require.NotNil(t, result)

	// Verify result
	assert.Equal(t, packName, result.PackName)
	assert.NotEmpty(t, result.FilesCreated)
	assert.NotEmpty(t, result.Operations)

	// Execute operations through synthfs
	testPaths, err := paths.New(testEnv.DotfilesRoot())
	require.NoError(t, err)
	executor := synthfs.NewSynthfsExecutorWithPaths(false, testPaths)
	_, err = executor.ExecuteOperations(result.Operations)
	require.NoError(t, err)

	// Verify files were created
	expectedFiles := []string{"Brewfile", "install.sh", "aliases.sh"}
	for _, filename := range expectedFiles {
		filePath := filepath.Join(packPath, filename)
		assert.FileExists(t, filePath, "File %s should have been created", filename)

		// Read and verify content
		content, err := os.ReadFile(filePath)
		require.NoError(t, err)
		contentStr := string(content)

		// Verify PACK_NAME was replaced
		assert.NotContains(t, contentStr, "PACK_NAME")
		assert.Contains(t, contentStr, packName)

		// Verify file has appropriate content
		switch filename {
		case "Brewfile":
			assert.Contains(t, contentStr, "Homebrew dependencies")
			assert.Contains(t, contentStr, "brew '")
		case "install.sh":
			assert.Contains(t, contentStr, "#!/usr/bin/env bash")
			assert.Contains(t, contentStr, "dodot install")
		case "aliases.sh":
			assert.Contains(t, contentStr, "#!/usr/bin/env sh")
			assert.Contains(t, contentStr, "Shell aliases")
		}
	}

	// Test that fill doesn't overwrite existing files
	existingContent := "# Custom Brewfile content"
	require.NoError(t, os.WriteFile(
		filepath.Join(packPath, "Brewfile"),
		[]byte(existingContent),
		0644,
	))

	// Run FillPack again
	result2, err := FillPack(FillPackOptions{
		DotfilesRoot: testEnv.DotfilesRoot(),
		PackName:     packName,
	})
	require.NoError(t, err)

	// Brewfile should not be in files created
	assert.NotContains(t, result2.FilesCreated, "Brewfile")

	// Verify Brewfile content wasn't changed
	content, err := os.ReadFile(filepath.Join(packPath, "Brewfile"))
	require.NoError(t, err)
	assert.Equal(t, existingContent, string(content))
}

func TestFillPack_NonExistentPack(t *testing.T) {
	// Setup test environment
	testEnv := testutil.NewTestEnvironment(t, "fill-nonexistent")
	defer testEnv.Cleanup()

	// Try to fill a non-existent pack
	result, err := FillPack(FillPackOptions{
		DotfilesRoot: testEnv.DotfilesRoot(),
		PackName:     "nonexistent",
	})

	// Should return error
	assert.Error(t, err)
	assert.Nil(t, result)
	assert.Contains(t, err.Error(), "pack \"nonexistent\" not found")
}

func TestFillPack_AllFilesExist(t *testing.T) {
	// Setup test environment
	testEnv := testutil.NewTestEnvironment(t, "fill-all-exist")
	defer testEnv.Cleanup()

	// Create a pack with all template files
	packName := "complete"
	packPath := filepath.Join(testEnv.DotfilesRoot(), packName)
	require.NoError(t, os.MkdirAll(packPath, 0755))

	// Create .dodot.toml
	packConfig := `# Test pack config`
	require.NoError(t, os.WriteFile(
		filepath.Join(packPath, ".dodot.toml"),
		[]byte(packConfig),
		0644,
	))

	// Get all templates and create files
	templates, err := core.GetCompletePackTemplate(packName)
	require.NoError(t, err)

	for _, tmpl := range templates {
		filePath := filepath.Join(packPath, tmpl.Filename)
		require.NoError(t, os.WriteFile(filePath, []byte("existing content"), os.FileMode(tmpl.Mode)))
	}

	// Execute FillPack
	result, err := FillPack(FillPackOptions{
		DotfilesRoot: testEnv.DotfilesRoot(),
		PackName:     packName,
	})
	require.NoError(t, err)

	// Should have no files to create
	assert.Empty(t, result.FilesCreated)
	assert.Empty(t, result.Operations)
}

func TestInitPack_Integration(t *testing.T) {
	// Setup test environment
	testEnv := testutil.NewTestEnvironment(t, "init-integration")
	defer testEnv.Cleanup()

	packName := "newpack"

	// Execute InitPack
	result, err := initialize.InitPack(initialize.InitPackOptions{
		DotfilesRoot: testEnv.DotfilesRoot(),
		PackName:     packName,
	})
	require.NoError(t, err)
	require.NotNil(t, result)

	// Verify result
	assert.Equal(t, packName, result.PackName)
	assert.NotEmpty(t, result.FilesCreated)
	assert.NotEmpty(t, result.Operations)

	// Execute operations through synthfs
	testPaths, err := paths.New(testEnv.DotfilesRoot())
	require.NoError(t, err)
	executor := synthfs.NewSynthfsExecutorWithPaths(false, testPaths)
	_, err = executor.ExecuteOperations(result.Operations)
	require.NoError(t, err)

	// Verify pack directory was created
	packPath := filepath.Join(testEnv.DotfilesRoot(), packName)
	assert.DirExists(t, packPath)

	// Verify all files were created
	assert.FileExists(t, filepath.Join(packPath, ".dodot.toml"))
	assert.FileExists(t, filepath.Join(packPath, "README.txt"))
	assert.FileExists(t, filepath.Join(packPath, "Brewfile"))
	assert.FileExists(t, filepath.Join(packPath, "install.sh"))
	assert.FileExists(t, filepath.Join(packPath, "aliases.sh"))

	// Verify .dodot.toml content
	packConfigContent, err := os.ReadFile(filepath.Join(packPath, ".dodot.toml"))
	require.NoError(t, err)
	assert.Contains(t, string(packConfigContent), "dodot configuration for "+packName+" pack")

	// Verify file permissions
	info, err := os.Stat(filepath.Join(packPath, "install.sh"))
	require.NoError(t, err)
	assert.True(t, info.Mode()&0100 != 0, "install.sh should be executable")
}

func TestInitPack_ExistingPack(t *testing.T) {
	// Setup test environment
	testEnv := testutil.NewTestEnvironment(t, "init-existing")
	defer testEnv.Cleanup()

	// Create existing pack
	packName := "existing"
	packPath := filepath.Join(testEnv.DotfilesRoot(), packName)
	require.NoError(t, os.MkdirAll(packPath, 0755))

	// Try to init existing pack
	result, err := initialize.InitPack(initialize.InitPackOptions{
		DotfilesRoot: testEnv.DotfilesRoot(),
		PackName:     packName,
	})

	// Should return error
	assert.Error(t, err)
	assert.Nil(t, result)
	assert.Contains(t, err.Error(), "pack \"existing\" already exists")
}

func TestInitPack_InvalidName(t *testing.T) {
	// Setup test environment
	testEnv := testutil.NewTestEnvironment(t, "init-invalid")
	defer testEnv.Cleanup()

	// Try invalid pack names
	invalidNames := []string{
		"",           // empty
		"pack/name",  // contains slash
		"pack\\name", // contains backslash
		"pack:name",  // contains colon
		"pack*name",  // contains asterisk
		"pack?name",  // contains question mark
		"pack<name>", // contains angle brackets
		"pack|name",  // contains pipe
	}

	for _, name := range invalidNames {
		result, err := initialize.InitPack(initialize.InitPackOptions{
			DotfilesRoot: testEnv.DotfilesRoot(),
			PackName:     name,
		})

		assert.Error(t, err, "Pack name %q should be invalid", name)
		assert.Nil(t, result)
		if name == "" {
			assert.Contains(t, err.Error(), "pack name cannot be empty")
		} else {
			assert.Contains(t, err.Error(), "pack name contains invalid characters")
		}
	}
}
