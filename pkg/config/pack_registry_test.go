package config_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/stretchr/testify/assert"
)

func TestPackRegistry(t *testing.T) {
	// Clear any existing configs
	config.ClearPackConfigs()

	t.Run("register_and_get_pack_config", func(t *testing.T) {
		packConfig := config.PackConfig{
			Symlink: config.Symlink{
				ForceHome:      []string{"*.conf"},
				ProtectedPaths: []string{"secret.key"},
			},
		}

		// Register config
		config.RegisterPackConfig("mypack", packConfig)

		// Retrieve config
		retrieved, exists := config.GetPackConfig("mypack")
		assert.True(t, exists)
		assert.Equal(t, packConfig, retrieved)

		// Non-existent pack
		_, exists = config.GetPackConfig("nonexistent")
		assert.False(t, exists)
	})

	t.Run("clear_pack_configs", func(t *testing.T) {
		// Register some configs
		config.RegisterPackConfig("pack1", config.PackConfig{})
		config.RegisterPackConfig("pack2", config.PackConfig{})

		// Clear all
		config.ClearPackConfigs()

		// Verify cleared
		_, exists1 := config.GetPackConfig("pack1")
		_, exists2 := config.GetPackConfig("pack2")
		assert.False(t, exists1)
		assert.False(t, exists2)
	})

	t.Run("get_merged_protected_paths_with_registry", func(t *testing.T) {
		// Clear registry
		config.ClearPackConfigs()

		// Save original config
		originalConfig := config.Get()
		defer config.Initialize(originalConfig)

		// Set up root config with protected paths
		rootConfig := &config.Config{
			Security: config.Security{
				ProtectedPaths: map[string]bool{
					".ssh/id_rsa": true,
					".gnupg":      true,
				},
			},
		}
		config.Initialize(rootConfig)

		// Register pack config
		packConfig := config.PackConfig{
			Symlink: config.Symlink{
				ProtectedPaths: []string{".myapp/secret", "private/*"},
			},
		}
		config.RegisterPackConfig("mypack", packConfig)

		// Get merged paths
		merged := config.GetMergedProtectedPaths("mypack")

		// Verify merge includes both root and pack paths
		assert.True(t, merged[".ssh/id_rsa"])
		assert.True(t, merged[".gnupg"])
		assert.True(t, merged[".myapp/secret"])
		assert.True(t, merged["private/*"])
		assert.Equal(t, 4, len(merged))

		// Test pack without config
		mergedNoConfig := config.GetMergedProtectedPaths("noconfig")
		assert.Equal(t, rootConfig.Security.ProtectedPaths, mergedNoConfig)
	})

	t.Run("get_force_home_patterns", func(t *testing.T) {
		// Clear registry
		config.ClearPackConfigs()

		// Save original config
		originalConfig := config.Get()
		defer config.Initialize(originalConfig)

		// Set up root config with CoreUnixExceptions
		rootConfig := &config.Config{
			LinkPaths: config.LinkPaths{
				CoreUnixExceptions: map[string]bool{
					"vim":  true,
					"bash": true,
				},
			},
		}
		config.Initialize(rootConfig)

		t.Run("pack_without_force_home_uses_root", func(t *testing.T) {
			// Register pack without force_home
			packConfig := config.PackConfig{}
			config.RegisterPackConfig("mypack", packConfig)

			patterns := config.GetForceHomePatterns("mypack")
			// Should have patterns for vim and bash with wildcards
			assert.Contains(t, patterns, "vim")
			assert.Contains(t, patterns, "vim/*")
			assert.Contains(t, patterns, "bash")
			assert.Contains(t, patterns, "bash/*")
		})

		t.Run("pack_with_force_home_extends_root", func(t *testing.T) {
			// Register pack with its own force_home
			packConfig := config.PackConfig{
				Symlink: config.Symlink{
					ForceHome: []string{"myapp/*", "config.toml"},
				},
			}
			config.RegisterPackConfig("otherpack", packConfig)

			patterns := config.GetForceHomePatterns("otherpack")
			// Should have both root and pack patterns
			assert.Contains(t, patterns, "vim")
			assert.Contains(t, patterns, "vim/*")
			assert.Contains(t, patterns, "bash")
			assert.Contains(t, patterns, "bash/*")
			assert.Contains(t, patterns, "myapp/*")
			assert.Contains(t, patterns, "config.toml")
		})
	})

	t.Run("is_force_home", func(t *testing.T) {
		// Clear registry
		config.ClearPackConfigs()

		// Save original config
		originalConfig := config.Get()
		defer config.Initialize(originalConfig)

		// Set up root config
		rootConfig := &config.Config{
			LinkPaths: config.LinkPaths{
				CoreUnixExceptions: map[string]bool{
					"vim": true,
				},
			},
		}
		config.Initialize(rootConfig)

		// Register pack with force_home
		packConfig := config.PackConfig{
			Symlink: config.Symlink{
				ForceHome: []string{"*.conf", "myapp/*"},
			},
		}
		config.RegisterPackConfig("mypack", packConfig)

		tests := []struct {
			packName string
			relPath  string
			expected bool
		}{
			// Pack with its own force_home
			{"mypack", "app.conf", true},        // matches *.conf
			{"mypack", "myapp/config", true},    // matches myapp/*
			{"mypack", "other/file.txt", false}, // no match
			{"mypack", "vim/vimrc", true},       // root exceptions still apply even when pack has force_home

			// Pack without force_home falls back to root
			{"nopack", "vim/vimrc", true},        // matches root exception
			{"nopack", "vim/colors/theme", true}, // matches root exception with wildcard
			{"nopack", "other/file", false},      // no match
		}

		for _, tt := range tests {
			result := config.IsForceHome(tt.packName, tt.relPath)
			assert.Equal(t, tt.expected, result, "IsForceHome(%q, %q) should be %v", tt.packName, tt.relPath, tt.expected)
		}
	})
}
