package config

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestLoadConfiguration(t *testing.T) {
	t.Run("loads and merges defaults successfully", func(t *testing.T) {
		cfg, err := LoadConfiguration()
		require.NoError(t, err)
		assert.NotNil(t, cfg)

		// Check value from system defaults
		assert.Equal(t, "warn", cfg.Logging.DefaultLevel)
		// Check value from user defaults (transformed)
		assert.True(t, cfg.Security.ProtectedPaths[".ssh/id_rsa"])
		assert.Contains(t, cfg.Patterns.PackIgnore, ".env*") // from user defaults
		assert.NotEmpty(t, cfg.Matchers)
	})

	t.Run("user config file overrides and merges with defaults", func(t *testing.T) {
		tempDir := t.TempDir()
		configPath := filepath.Join(tempDir, "dodot", "config.yaml")
		require.NoError(t, os.MkdirAll(filepath.Dir(configPath), 0755))

		userConfig := `
logging:
  default_level: debug
pack:
  ignore:
    - ".cache"
symlink:
  protected_paths:
    - ".mysecret"
`
		err := os.WriteFile(configPath, []byte(userConfig), 0644)
		require.NoError(t, err)

		t.Setenv("XDG_CONFIG_HOME", tempDir)
		t.Setenv("DOTFILES_ROOT", "")

		cfg, err := LoadConfiguration()
		require.NoError(t, err)

		// Check that user value overrides default
		assert.Equal(t, "debug", cfg.Logging.DefaultLevel)
		// Check that user list is merged with default list
		assert.Contains(t, cfg.Patterns.PackIgnore, ".git")   // from default
		assert.Contains(t, cfg.Patterns.PackIgnore, ".cache") // from user config
		// Check that user map is merged with default map
		assert.True(t, cfg.Security.ProtectedPaths[".ssh/id_rsa"]) // from default
		assert.True(t, cfg.Security.ProtectedPaths[".mysecret"])   // from user
	})

	t.Run("environment variable overrides all other configs", func(t *testing.T) {
		tempDir := t.TempDir()
		configPath := filepath.Join(tempDir, "dodot", "config.yaml")
		require.NoError(t, os.MkdirAll(filepath.Dir(configPath), 0755))
		userConfig := `
logging:
  default_level: debug
pack:
  ignore: ".cache" # env var format is comma-separated
`
		err := os.WriteFile(configPath, []byte(userConfig), 0644)
		require.NoError(t, err)

		t.Setenv("XDG_CONFIG_HOME", tempDir)
		t.Setenv("DODOT_LOGGING_DEFAULT_LEVEL", "trace")
		t.Setenv("DODOT_PACK_IGNORE", ".git,.newignore")
		t.Setenv("DOTFILES_ROOT", "")

		cfg, err := LoadConfiguration()
		require.NoError(t, err)

		// Check that env var value overrides user config and default
		assert.Equal(t, "trace", cfg.Logging.DefaultLevel)
		// Check that env var for a slice replaces the default and user lists
		assert.Equal(t, []string{".git", ".newignore"}, cfg.Patterns.PackIgnore)
	})
}
