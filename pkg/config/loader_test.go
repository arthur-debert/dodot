package config

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestLoadConfiguration(t *testing.T) {
	t.Run("loads_defaults_and_app_config", func(t *testing.T) {
		// Set a temp DOTFILES_ROOT
		tmpDir := t.TempDir()
		oldRoot := os.Getenv("DOTFILES_ROOT")
		require.NoError(t, os.Setenv("DOTFILES_ROOT", tmpDir))
		defer func() {
			require.NoError(t, os.Setenv("DOTFILES_ROOT", oldRoot))
		}()

		cfg, err := LoadConfiguration()
		require.NoError(t, err)

		// Check values are loaded
		assert.NotEmpty(t, cfg.ShellIntegration.BashZshSnippet)
		assert.Equal(t, os.FileMode(0755), cfg.FilePermissions.Directory)
		assert.Contains(t, cfg.Patterns.PackIgnore, ".git")
		assert.True(t, cfg.LinkPaths.CoreUnixExceptions["ssh"])
		assert.True(t, cfg.Security.ProtectedPaths[".ssh/id_rsa"])
	})

	t.Run("loads_root_config", func(t *testing.T) {
		tmpDir := t.TempDir()

		// Create root config
		rootConfig := filepath.Join(tmpDir, "dodot.toml")
		err := os.WriteFile(rootConfig, []byte(`
[pack]
ignore = ["custom-ignore"]

[symlink]
force_home = ["myapp"]
protected_paths = ["my-secret"]

[mappings]
path = "scripts"
install = "setup.sh"
`), 0644)
		require.NoError(t, err)

		cfg, err := GetRootConfig(tmpDir)
		require.NoError(t, err)

		// Check that pack.ignore merged with defaults
		assert.Contains(t, cfg.Patterns.PackIgnore, "custom-ignore")
		assert.Contains(t, cfg.Patterns.PackIgnore, ".git") // from defaults
		assert.True(t, cfg.LinkPaths.CoreUnixExceptions["myapp"])
		assert.True(t, cfg.Security.ProtectedPaths["my-secret"])

		// Check mappings
		assert.Equal(t, "scripts", cfg.Mappings.Path)
		assert.Equal(t, "setup.sh", cfg.Mappings.Install)
	})

	t.Run("pack_config_overrides", func(t *testing.T) {
		tmpDir := t.TempDir()
		packDir := filepath.Join(tmpDir, "mypack")
		require.NoError(t, os.MkdirAll(packDir, 0755))

		// Create root config
		rootConfig := filepath.Join(tmpDir, "dodot.toml")
		err := os.WriteFile(rootConfig, []byte(`
[pack]
ignore = ["root-ignore"]

[mappings]
path = "bin"
install = "install.sh"
`), 0644)
		require.NoError(t, err)

		// Create pack config
		packConfig := filepath.Join(packDir, ".dodot.toml")
		err = os.WriteFile(packConfig, []byte(`
[pack]
ignore = ["pack-ignore"]

[mappings]
install = "setup.sh"
shell = ["init.sh"]
`), 0644)
		require.NoError(t, err)

		// Set DOTFILES_ROOT
		oldRoot := os.Getenv("DOTFILES_ROOT")
		require.NoError(t, os.Setenv("DOTFILES_ROOT", tmpDir))
		defer func() {
			require.NoError(t, os.Setenv("DOTFILES_ROOT", oldRoot))
		}()

		// Load root config first
		rootCfg, err := GetRootConfig(tmpDir)
		require.NoError(t, err)

		// Then load pack config
		packCfg, err := GetPackConfig(rootCfg, packDir)
		require.NoError(t, err)

		// Check pack.ignore merged with defaults and root
		assert.Contains(t, packCfg.Patterns.PackIgnore, "pack-ignore")
		assert.Contains(t, packCfg.Patterns.PackIgnore, "root-ignore")
		assert.Contains(t, packCfg.Patterns.PackIgnore, ".git")      // from defaults
		assert.Equal(t, "bin", packCfg.Mappings.Path)                // unchanged
		assert.Equal(t, "setup.sh", packCfg.Mappings.Install)        // overridden
		assert.Equal(t, []string{"init.sh"}, packCfg.Mappings.Shell) // new
	})
}
