package core

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestGetMissingTemplateFiles tests with filesystem operations
// This is an integration test because it creates temp directories and files
func TestGetMissingTemplateFiles(t *testing.T) {
	// Create a temporary directory for testing
	tempDir := t.TempDir()
	packPath := filepath.Join(tempDir, "mypack")
	require.NoError(t, os.MkdirAll(packPath, 0755))

	// Create Brewfile to test that it's not included in missing
	brewfilePath := filepath.Join(packPath, "Brewfile")
	require.NoError(t, os.WriteFile(brewfilePath, []byte("# existing brewfile"), 0644))

	// Get missing templates
	missing, err := GetMissingTemplateFiles(packPath, "mypack")
	require.NoError(t, err)

	// We should be missing install.sh and aliases.sh at minimum
	assert.GreaterOrEqual(t, len(missing), 2)

	// Brewfile should NOT be in missing list
	for _, tmpl := range missing {
		assert.NotEqual(t, "Brewfile", tmpl.Filename, "Brewfile exists so should not be missing")
	}

	// Check that install.sh is in missing list
	hasInstall := false
	for _, tmpl := range missing {
		if tmpl.Filename == "install.sh" {
			hasInstall = true
			assert.Equal(t, "install_script", tmpl.PowerUpName)
			break
		}
	}
	assert.True(t, hasInstall, "install.sh should be in missing templates")
}

// TestGetMissingTemplateFiles_AllExist tests when all files exist
// This is an integration test because it creates temp directories and files
func TestGetMissingTemplateFiles_AllExist(t *testing.T) {
	// Create a temporary directory with all template files
	tempDir := t.TempDir()
	packPath := filepath.Join(tempDir, "complete")
	require.NoError(t, os.MkdirAll(packPath, 0755))

	// Get all templates first
	allTemplates, err := GetCompletePackTemplate("complete")
	require.NoError(t, err)

	// Create all template files
	for _, tmpl := range allTemplates {
		filePath := filepath.Join(packPath, tmpl.Filename)
		require.NoError(t, os.WriteFile(filePath, []byte("existing content"), os.FileMode(tmpl.Mode)))
	}

	// Get missing templates
	missing, err := GetMissingTemplateFiles(packPath, "complete")
	require.NoError(t, err)

	// Should have no missing templates
	assert.Empty(t, missing, "Should have no missing templates when all exist")
}

// TestTemplateFileExists tests file existence checking
// This is an integration test because it creates temp files
func TestTemplateFileExists(t *testing.T) {
	tempDir := t.TempDir()

	// Test existing file
	existingFile := filepath.Join(tempDir, "exists.txt")
	require.NoError(t, os.WriteFile(existingFile, []byte("content"), 0644))

	exists, err := fileExists(existingFile)
	assert.NoError(t, err)
	assert.True(t, exists)

	// Test non-existing file
	nonExisting := filepath.Join(tempDir, "nothere.txt")
	exists, err = fileExists(nonExisting)
	assert.NoError(t, err)
	assert.False(t, exists)
}
