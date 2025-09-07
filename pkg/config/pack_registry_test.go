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
}
