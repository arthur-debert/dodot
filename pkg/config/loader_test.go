package config

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestTransformUserToInternal(t *testing.T) {
	tests := []struct {
		name     string
		input    map[string]interface{}
		expected map[string]interface{}
	}{
		{
			name: "pack ignore transformation",
			input: map[string]interface{}{
				"pack": map[string]interface{}{
					"ignore": []interface{}{".git", "node_modules"},
				},
			},
			expected: map[string]interface{}{
				"patterns": map[string]interface{}{
					"pack_ignore": []interface{}{".git", "node_modules"},
				},
			},
		},
		{
			name: "symlink protected paths transformation",
			input: map[string]interface{}{
				"symlink": map[string]interface{}{
					"protected_paths": []interface{}{".ssh/id_rsa", ".gnupg"},
				},
			},
			expected: map[string]interface{}{
				"security": map[string]interface{}{
					"protected_paths": map[string]bool{
						".ssh/id_rsa": true,
						".gnupg":      true,
					},
				},
			},
		},
		{
			name: "symlink force_home transformation",
			input: map[string]interface{}{
				"symlink": map[string]interface{}{
					"force_home": []interface{}{"ssh", "gitconfig"},
				},
			},
			expected: map[string]interface{}{
				"link_paths": map[string]interface{}{
					"force_home": map[string]bool{
						"ssh":       true,
						"gitconfig": true,
					},
				},
			},
		},
		{
			name: "file_mapping pass-through",
			input: map[string]interface{}{
				"file_mapping": map[string]interface{}{
					"path":     "bin",
					"install":  "install.sh",
					"shell":    []interface{}{"aliases.sh", "profile.sh"},
					"homebrew": "Brewfile",
				},
			},
			expected: map[string]interface{}{
				"file_mapping": map[string]interface{}{
					"path":     "bin",
					"install":  "install.sh",
					"shell":    []interface{}{"aliases.sh", "profile.sh"},
					"homebrew": "Brewfile",
				},
			},
		},
		{
			name: "combined transformations",
			input: map[string]interface{}{
				"pack": map[string]interface{}{
					"ignore": []interface{}{".git"},
				},
				"symlink": map[string]interface{}{
					"protected_paths": []interface{}{".ssh/id_rsa"},
					"force_home":      []interface{}{"ssh"},
				},
				"file_mapping": map[string]interface{}{
					"path": "bin",
				},
			},
			expected: map[string]interface{}{
				"patterns": map[string]interface{}{
					"pack_ignore": []interface{}{".git"},
				},
				"security": map[string]interface{}{
					"protected_paths": map[string]bool{
						".ssh/id_rsa": true,
					},
				},
				"link_paths": map[string]interface{}{
					"force_home": map[string]bool{
						"ssh": true,
					},
				},
				"file_mapping": map[string]interface{}{
					"path": "bin",
				},
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := transformUserToInternal(tt.input)
			assert.Equal(t, tt.expected, result)
		})
	}
}

func TestMergeMaps(t *testing.T) {
	tests := []struct {
		name     string
		dest     map[string]interface{}
		src      map[string]interface{}
		expected map[string]interface{}
	}{
		{
			name: "merge nested maps",
			dest: map[string]interface{}{
				"patterns": map[string]interface{}{
					"pack_ignore": []string{"node_modules"},
				},
			},
			src: map[string]interface{}{
				"patterns": map[string]interface{}{
					"pack_ignore": []string{".git"},
				},
			},
			expected: map[string]interface{}{
				"patterns": map[string]interface{}{
					"pack_ignore": []interface{}{"node_modules", ".git"},
				},
			},
		},
		{
			name: "override scalars",
			dest: map[string]interface{}{
				"file_mapping": map[string]interface{}{
					"path": "bin",
				},
			},
			src: map[string]interface{}{
				"file_mapping": map[string]interface{}{
					"path": "scripts",
				},
			},
			expected: map[string]interface{}{
				"file_mapping": map[string]interface{}{
					"path": "scripts",
				},
			},
		},
		{
			name: "append slices",
			dest: map[string]interface{}{
				"matchers": []interface{}{
					map[string]interface{}{"name": "matcher1"},
				},
			},
			src: map[string]interface{}{
				"matchers": []interface{}{
					map[string]interface{}{"name": "matcher2"},
				},
			},
			expected: map[string]interface{}{
				"matchers": []interface{}{
					map[string]interface{}{"name": "matcher1"},
					map[string]interface{}{"name": "matcher2"},
				},
			},
		},
		{
			name: "add new keys",
			dest: map[string]interface{}{
				"security": map[string]interface{}{
					"protected_paths": map[string]bool{".ssh/id_rsa": true},
				},
			},
			src: map[string]interface{}{
				"file_mapping": map[string]interface{}{
					"path": "bin",
				},
			},
			expected: map[string]interface{}{
				"security": map[string]interface{}{
					"protected_paths": map[string]bool{".ssh/id_rsa": true},
				},
				"file_mapping": map[string]interface{}{
					"path": "bin",
				},
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			mergeMaps(tt.dest, tt.src)
			assert.Equal(t, tt.expected, tt.dest)
		})
	}
}

func TestGetRootConfigPath(t *testing.T) {
	t.Run("with DOTFILES_ROOT", func(t *testing.T) {
		// Save and restore env var
		oldVal := os.Getenv("DOTFILES_ROOT")
		defer func() { _ = os.Setenv("DOTFILES_ROOT", oldVal) }()

		_ = os.Setenv("DOTFILES_ROOT", "/tmp/dotfiles")
		path := getRootConfigPath()
		assert.Equal(t, "/tmp/dotfiles/.dodot.toml", path)
	})

	t.Run("without DOTFILES_ROOT", func(t *testing.T) {
		// Save and restore env var
		oldVal := os.Getenv("DOTFILES_ROOT")
		defer func() { _ = os.Setenv("DOTFILES_ROOT", oldVal) }()

		_ = os.Unsetenv("DOTFILES_ROOT")
		path := getRootConfigPath()
		assert.Equal(t, ".dodot.toml", path)
	})
}

func TestLoadPackConfiguration(t *testing.T) {
	// Create a temporary directory for testing
	tmpDir := t.TempDir()

	t.Run("no pack config exists", func(t *testing.T) {
		// Use a base config from LoadConfiguration to get proper defaults
		baseConfig, err := LoadConfiguration()
		require.NoError(t, err)

		packPath := filepath.Join(tmpDir, "pack1")
		require.NoError(t, os.MkdirAll(packPath, 0755))

		result, err := LoadPackConfiguration(baseConfig, packPath)
		assert.NoError(t, err)
		assert.NotNil(t, result)
		// Should have matchers from base config
		assert.NotEmpty(t, result.Matchers)
	})

	t.Run("pack config merges correctly", func(t *testing.T) {
		// Create a simple base config with known values
		baseConfig := &Config{
			Patterns: Patterns{
				PackIgnore: []string{"base1", "base2"},
				SpecialFiles: SpecialFiles{
					PackConfig: ".dodot.toml",
					IgnoreFile: ".dodotignore",
				},
			},
			FileMapping: FileMapping{
				Path:    "bin",
				Install: "install.sh",
				Shell:   []string{"base.sh"},
			},
		}

		packPath := filepath.Join(tmpDir, "pack2")
		require.NoError(t, os.MkdirAll(packPath, 0755))

		// Create pack config file
		packConfigContent := `
[pack]
ignore = ["pack1", "pack2"]

[file_mapping]
path = "scripts"
install = "setup.sh"
`
		packConfigPath := filepath.Join(packPath, ".dodot.toml")
		require.NoError(t, os.WriteFile(packConfigPath, []byte(packConfigContent), 0644))

		result, err := LoadPackConfiguration(baseConfig, packPath)
		assert.NoError(t, err)

		// Debug
		t.Logf("Result patterns: %+v", result.Patterns)
		t.Logf("Result file mapping: %+v", result.FileMapping)

		// Check that pack ignore was appended
		assert.ElementsMatch(t, result.Patterns.PackIgnore, []string{"base1", "base2", "pack1", "pack2"})

		// Check that file_mapping scalars were overridden
		assert.Equal(t, "scripts", result.FileMapping.Path)
		assert.Equal(t, "setup.sh", result.FileMapping.Install)
		// Shell wasn't specified in pack config, so should keep base value
		assert.Equal(t, []string{"base.sh"}, result.FileMapping.Shell)
	})

	t.Run("invalid toml in pack config", func(t *testing.T) {
		baseConfig := &Config{}
		packPath := filepath.Join(tmpDir, "pack3")
		require.NoError(t, os.MkdirAll(packPath, 0755))

		// Create pack config with invalid TOML
		packConfigPath := filepath.Join(packPath, ".dodot.toml")
		require.NoError(t, os.WriteFile(packConfigPath, []byte("[invalid toml"), 0644))

		result, err := LoadPackConfiguration(baseConfig, packPath)
		assert.Error(t, err)
		assert.Nil(t, result)
		assert.Contains(t, err.Error(), "failed to load pack config")
	})

	t.Run("stat error on pack config", func(t *testing.T) {
		baseConfig := &Config{}
		// Use a path that will cause stat to fail (not just file not found)
		packPath := filepath.Join(tmpDir, "nonexistent", "nested", "path")

		result, err := LoadPackConfiguration(baseConfig, packPath)
		// This should succeed because the parent directory doesn't exist,
		// which is treated as "no config file"
		assert.NoError(t, err)
		assert.NotNil(t, result)
		assert.Equal(t, baseConfig, result)
	})
}

func TestConfigToMap(t *testing.T) {
	t.Run("basic config conversion", func(t *testing.T) {
		cfg := &Config{
			Security: Security{
				ProtectedPaths: map[string]bool{
					".ssh/id_rsa": true,
					".gnupg":      true,
				},
			},
			Patterns: Patterns{
				PackIgnore:      []string{".git", "node_modules"},
				CatchallExclude: []string{".dodot.toml", ".dodotignore"},
			},
			FileMapping: FileMapping{
				Path:     "bin",
				Install:  "install.sh",
				Shell:    []string{"aliases.sh"},
				Homebrew: "Brewfile",
			},
		}

		result := configToMap(cfg)

		// Check that the conversion worked
		security, ok := result["security"].(map[string]interface{})
		require.True(t, ok)
		protectedPaths, ok := security["protected_paths"].(map[string]bool)
		require.True(t, ok)
		assert.True(t, protectedPaths[".ssh/id_rsa"])
		assert.True(t, protectedPaths[".gnupg"])

		patterns, ok := result["patterns"].(map[string]interface{})
		require.True(t, ok)
		packIgnore, ok := patterns["pack_ignore"].([]string)
		require.True(t, ok)
		assert.Equal(t, []string{".git", "node_modules"}, packIgnore)

		catchallExclude, ok := patterns["catchall_exclude"].([]string)
		require.True(t, ok)
		assert.Equal(t, []string{".dodot.toml", ".dodotignore"}, catchallExclude)

		fileMapping, ok := result["file_mapping"].(map[string]interface{})
		require.True(t, ok)
		assert.Equal(t, "bin", fileMapping["path"])
		assert.Equal(t, "install.sh", fileMapping["install"])
	})

	t.Run("config with matchers", func(t *testing.T) {
		cfg := &Config{
			Security: Security{
				ProtectedPaths: map[string]bool{},
			},
			Matchers: []MatcherConfig{
				{
					Name:     "test-matcher",
					Priority: 50,
					Trigger: TriggerConfig{
						Type: "file",
						Data: map[string]interface{}{
							"pattern": "*.sh",
						},
					},
					Handler: HandlerConfig{
						Type: "symlink",
						Data: map[string]interface{}{
							"destination": "$HOME/bin",
						},
					},
				},
			},
		}

		result := configToMap(cfg)

		// Check matchers conversion
		matchers, ok := result["matchers"].([]interface{})
		require.True(t, ok)
		require.Len(t, matchers, 1)

		matcher, ok := matchers[0].(map[string]interface{})
		require.True(t, ok)
		assert.Equal(t, "test-matcher", matcher["name"])
		assert.Equal(t, 50, matcher["priority"])

		trigger, ok := matcher["trigger"].(map[string]interface{})
		require.True(t, ok)
		assert.Equal(t, "file", trigger["type"])

		handler, ok := matcher["handler"].(map[string]interface{})
		require.True(t, ok)
		assert.Equal(t, "symlink", handler["type"])
	})
}

func TestSliceHelpers(t *testing.T) {
	t.Run("isSlice", func(t *testing.T) {
		assert.True(t, isSlice([]interface{}{"a", "b"}))
		assert.True(t, isSlice([]string{"a", "b"}))
		assert.False(t, isSlice("not a slice"))
		assert.False(t, isSlice(123))
		assert.False(t, isSlice(map[string]interface{}{}))
	})

	t.Run("toInterfaceSlice", func(t *testing.T) {
		// Test with []interface{}
		interfaceSlice := []interface{}{"a", "b", "c"}
		result := toInterfaceSlice(interfaceSlice)
		assert.Equal(t, interfaceSlice, result)

		// Test with []string
		stringSlice := []string{"x", "y", "z"}
		result = toInterfaceSlice(stringSlice)
		assert.Equal(t, []interface{}{"x", "y", "z"}, result)

		// Test with non-slice
		result = toInterfaceSlice("not a slice")
		assert.Equal(t, []interface{}{}, result)
	})

	t.Run("appendSlices", func(t *testing.T) {
		// Test []interface{} + []interface{}
		dest := []interface{}{"a", "b"}
		src := []interface{}{"c", "d"}
		result := appendSlices(dest, src)
		assert.Equal(t, []interface{}{"a", "b", "c", "d"}, result)

		// Test []string + []string
		destStr := []string{"1", "2"}
		srcStr := []string{"3", "4"}
		result = appendSlices(destStr, srcStr)
		assert.Equal(t, []interface{}{"1", "2", "3", "4"}, result)

		// Test mixed types
		result = appendSlices(destStr, src)
		assert.Equal(t, []interface{}{"1", "2", "c", "d"}, result)
	})
}
