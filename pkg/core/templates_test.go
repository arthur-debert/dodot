package core

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	// Import to register powerups and triggers
	_ "github.com/arthur-debert/dodot/pkg/powerups"
	_ "github.com/arthur-debert/dodot/pkg/triggers"
)

func TestGetCompletePackTemplate(t *testing.T) {
	// Test getting all template files
	templates, err := GetCompletePackTemplate("testpack")
	require.NoError(t, err)

	// We should have templates for: Brewfile, install.sh, and aliases.sh files
	// Note: path.sh isn't included because shell_add_path uses a directory trigger, not filename
	assert.GreaterOrEqual(t, len(templates), 3)

	// Check that we have expected templates
	templateMap := make(map[string]PackTemplateFile)
	for _, tmpl := range templates {
		templateMap[tmpl.Filename] = tmpl
	}

	// Check Brewfile template
	brewfile, exists := templateMap["Brewfile"]
	assert.True(t, exists, "Should have Brewfile template")
	assert.Equal(t, "homebrew", brewfile.PowerUpName)
	assert.Contains(t, brewfile.Content, "Homebrew dependencies for testpack pack")
	assert.Equal(t, uint32(0644), brewfile.Mode)

	// Check install.sh template
	install, exists := templateMap["install.sh"]
	assert.True(t, exists, "Should have install.sh template")
	assert.Equal(t, "install_script", install.PowerUpName)
	assert.Contains(t, install.Content, "dodot install script for testpack pack")
	assert.Equal(t, uint32(0755), install.Mode)

	// Check that PACK_NAME was replaced
	for _, tmpl := range templates {
		assert.NotContains(t, tmpl.Content, "PACK_NAME", "PACK_NAME should be replaced with actual pack name")
		assert.Contains(t, tmpl.Content, "testpack", "Content should contain actual pack name")
	}
}

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
