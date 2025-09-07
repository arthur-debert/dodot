package config

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestGetRootConfig_LoadsAndMergesPackIgnore(t *testing.T) {
	tests := []struct {
		name           string
		rootConfig     string
		expectedIgnore []string
		description    string
	}{
		{
			name:       "no_root_config",
			rootConfig: "", // No file created
			expectedIgnore: []string{
				// Should have defaults from embedded dodot.toml
				".git", ".svn", ".hg", "node_modules", ".DS_Store",
				"*.swp", "*~", "#*#", ".env*", ".terraform/",
			},
			description: "Should use defaults when no root config exists",
		},
		{
			name: "root_config_with_pack_ignore",
			rootConfig: `
[pack]
ignore = ["custom-ignore", "temp-*", "backup"]
`,
			expectedIgnore: []string{
				// Defaults from embedded dodot.toml
				".git", ".svn", ".hg", "node_modules", ".DS_Store",
				"*.swp", "*~", "#*#", ".env*", ".terraform/",
				// Custom patterns appended
				"custom-ignore", "temp-*", "backup",
			},
			description: "Should append custom patterns to defaults",
		},
		{
			name: "root_config_empty_pack_ignore",
			rootConfig: `
[pack]
ignore = []
`,
			expectedIgnore: []string{
				// Should still have defaults
				".git", ".svn", ".hg", "node_modules", ".DS_Store",
				"*.swp", "*~", "#*#", ".env*", ".terraform/",
			},
			description: "Empty array should not override defaults",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			tmpDir := t.TempDir()

			// Create root config if specified
			if tt.rootConfig != "" {
				configPath := filepath.Join(tmpDir, "dodot.toml")
				err := os.WriteFile(configPath, []byte(tt.rootConfig), 0644)
				require.NoError(t, err)
			}

			// Load config
			cfg, err := GetRootConfig(tmpDir)
			require.NoError(t, err)
			require.NotNil(t, cfg)

			// Debug output
			t.Logf("Test case: %s", tt.name)
			t.Logf("Loaded PackIgnore patterns: %v", cfg.Patterns.PackIgnore)
			t.Logf("Number of patterns: %d", len(cfg.Patterns.PackIgnore))

			// Check pack ignore patterns
			for _, expected := range tt.expectedIgnore {
				assert.Contains(t, cfg.Patterns.PackIgnore, expected, tt.description+" - should contain: "+expected)
			}
		})
	}
}

func TestGetRootConfig_LoadsMultipleSections(t *testing.T) {
	tmpDir := t.TempDir()

	// Create a comprehensive root config
	rootConfig := `
[pack]
ignore = ["vendor", "*.bak"]

[symlink]
force_home = ["myapp", "custom"]
protected_paths = [".myapp/secret", ".custom/credentials"]

[mappings]
path = "scripts"
install = "setup.sh"
shell = ["env.sh", "aliases.sh"]
homebrew = "Brewfile.local"
`

	configPath := filepath.Join(tmpDir, ".dodot.toml") // Test with .dodot.toml
	err := os.WriteFile(configPath, []byte(rootConfig), 0644)
	require.NoError(t, err)

	// Load config
	cfg, err := GetRootConfig(tmpDir)
	require.NoError(t, err)

	// Check pack ignore (should be appended to defaults)
	assert.Contains(t, cfg.Patterns.PackIgnore, "vendor")
	assert.Contains(t, cfg.Patterns.PackIgnore, "*.bak")
	assert.Contains(t, cfg.Patterns.PackIgnore, ".git") // default should still be there

	// Debug output
	t.Logf("CoreUnixExceptions: %v", cfg.LinkPaths.CoreUnixExceptions)

	// Check force_home (should be merged)
	assert.True(t, cfg.LinkPaths.CoreUnixExceptions["myapp"])
	assert.True(t, cfg.LinkPaths.CoreUnixExceptions["custom"])
	assert.True(t, cfg.LinkPaths.CoreUnixExceptions["ssh"]) // default should still be there

	// Check protected_paths
	assert.True(t, cfg.Security.ProtectedPaths[".myapp/secret"])
	assert.True(t, cfg.Security.ProtectedPaths[".custom/credentials"])

	// Check mappings
	assert.Equal(t, "scripts", cfg.Mappings.Path)
	assert.Equal(t, "setup.sh", cfg.Mappings.Install)
	assert.Contains(t, cfg.Mappings.Shell, "env.sh")
	assert.Contains(t, cfg.Mappings.Shell, "aliases.sh")
	assert.Equal(t, "Brewfile.local", cfg.Mappings.Homebrew)
}

func TestGetRootConfig_PrefersDotPrefixedFile(t *testing.T) {
	tmpDir := t.TempDir()

	// Create both dodot.toml and .dodot.toml
	config1 := `
[pack]
ignore = ["from-dodot-toml"]
`
	config2 := `
[pack]
ignore = ["from-dot-dodot-toml"]
`

	err := os.WriteFile(filepath.Join(tmpDir, "dodot.toml"), []byte(config1), 0644)
	require.NoError(t, err)
	err = os.WriteFile(filepath.Join(tmpDir, ".dodot.toml"), []byte(config2), 0644)
	require.NoError(t, err)

	// Load config
	cfg, err := GetRootConfig(tmpDir)
	require.NoError(t, err)

	// Debug
	t.Logf("Loaded PackIgnore patterns: %v", cfg.Patterns.PackIgnore)

	// Should use .dodot.toml (it's checked first)
	assert.Contains(t, cfg.Patterns.PackIgnore, "from-dot-dodot-toml")
	assert.NotContains(t, cfg.Patterns.PackIgnore, "from-dodot-toml")
}
