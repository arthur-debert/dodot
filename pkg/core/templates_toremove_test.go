//go:build ignore
// +build ignore

package core

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	// Import handler packages to register their factories
	_ "github.com/arthur-debert/dodot/pkg/handlers/homebrew"
	_ "github.com/arthur-debert/dodot/pkg/handlers/install"
	_ "github.com/arthur-debert/dodot/pkg/handlers/path"
	_ "github.com/arthur-debert/dodot/pkg/handlers/shell"
	_ "github.com/arthur-debert/dodot/pkg/handlers/symlink"
)

// TestGetCompletePackTemplate tests template generation logic
// This is a unit test - it only tests the template generation without filesystem operations
// Note: It does use the registry, which makes it slightly integration-ish, but it doesn't touch filesystem
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
	assert.Equal(t, "homebrew", brewfile.HandlerName)
	assert.Contains(t, brewfile.Content, "Homebrew dependencies for testpack pack")
	assert.Equal(t, uint32(0644), brewfile.Mode)

	// Check install.sh template
	install, exists := templateMap["install.sh"]
	assert.True(t, exists, "Should have install.sh template")
	assert.Equal(t, "install", install.HandlerName)
	assert.Contains(t, install.Content, "dodot install script for testpack pack")
	assert.Equal(t, uint32(0755), install.Mode)

	// Check that PACK_NAME was replaced
	for _, tmpl := range templates {
		assert.NotContains(t, tmpl.Content, "PACK_NAME", "PACK_NAME should be replaced with actual pack name")
		assert.Contains(t, tmpl.Content, "testpack", "Content should contain actual pack name")
	}
}
