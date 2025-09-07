package config

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestKoanfSimpleLoading(t *testing.T) {
	t.Run("loads_defaults_and_app_config", func(t *testing.T) {
		k, err := NewSimpleConfig()
		require.NoError(t, err)

		// Check some values from defaults
		assert.NotEmpty(t, k.String("shell_integration.bash_zsh_snippet"))
		assert.Equal(t, int64(493), k.Int64("file_permissions.directory"))

		// Check values from app config (dodot.toml)
		packIgnore := k.Strings("pack.ignore")
		assert.Contains(t, packIgnore, ".git")
		assert.Contains(t, packIgnore, ".terraform/")

		forceHome := k.Strings("symlink.force_home")
		assert.Contains(t, forceHome, "ssh")
		assert.Contains(t, forceHome, "aws")

		protected := k.Strings("symlink.protected_paths")
		assert.Contains(t, protected, ".ssh/id_rsa")
		assert.Contains(t, protected, ".gnupg")
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

		k, err := GetSimpleRootConfig(tmpDir)
		require.NoError(t, err)

		// Check that root config replaced arrays
		packIgnore := k.Strings("pack.ignore")
		assert.Equal(t, []string{"custom-ignore"}, packIgnore)

		forceHome := k.Strings("symlink.force_home")
		assert.Equal(t, []string{"myapp"}, forceHome)

		// Check mappings
		assert.Equal(t, "scripts", k.String("mappings.path"))
		assert.Equal(t, "setup.sh", k.String("mappings.install"))
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

		// Load pack config with proper layering
		packK, err := GetSimplePackConfig(tmpDir, packDir)
		require.NoError(t, err)

		// Check overrides
		assert.Equal(t, []string{"pack-ignore"}, packK.Strings("pack.ignore"))
		assert.Equal(t, "bin", packK.String("mappings.path"))                 // unchanged
		assert.Equal(t, "setup.sh", packK.String("mappings.install"))         // overridden
		assert.Equal(t, []string{"init.sh"}, packK.Strings("mappings.shell")) // new
	})
}

func TestCompatibilityLayer(t *testing.T) {
	t.Run("converts_user_format_to_internal", func(t *testing.T) {
		tmpDir := t.TempDir()

		// Create config with user format
		configFile := filepath.Join(tmpDir, "dodot.toml")
		err := os.WriteFile(configFile, []byte(`
[pack]
ignore = ["*.tmp", "cache/"]

[symlink]
force_home = ["myapp", "tool"]
protected_paths = [".myapp/secret", ".tool/key"]

[mappings]
path = "scripts"
install = "setup.sh"
shell = ["env.sh", "aliases.sh"]
`), 0644)
		require.NoError(t, err)

		cfg, err := GetRootConfigNew(tmpDir)
		require.NoError(t, err)

		// Check conversions
		assert.Equal(t, []string{"*.tmp", "cache/"}, cfg.Patterns.PackIgnore)

		assert.True(t, cfg.LinkPaths.CoreUnixExceptions["myapp"])
		assert.True(t, cfg.LinkPaths.CoreUnixExceptions["tool"])

		assert.True(t, cfg.Security.ProtectedPaths[".myapp/secret"])
		assert.True(t, cfg.Security.ProtectedPaths[".tool/key"])

		assert.Equal(t, "scripts", cfg.Mappings.Path)
		assert.Equal(t, "setup.sh", cfg.Mappings.Install)
		assert.ElementsMatch(t, []string{"env.sh", "aliases.sh"}, cfg.Mappings.Shell)
	})

	t.Run("maintains_backward_compatibility", func(t *testing.T) {
		tmpDir := t.TempDir()
		packDir := filepath.Join(tmpDir, "pack1")
		require.NoError(t, os.MkdirAll(packDir, 0755))

		// Create root config
		rootConfig := filepath.Join(tmpDir, "dodot.toml")
		err := os.WriteFile(rootConfig, []byte(`
[pack]
ignore = ["root-pattern"]

[symlink]
force_home = ["ssh"]
`), 0644)
		require.NoError(t, err)

		// Create pack config
		packConfig := filepath.Join(packDir, "dodot.toml")
		err = os.WriteFile(packConfig, []byte(`
[pack]
ignore = ["pack-pattern"]

[symlink]
force_home = ["myapp"]
`), 0644)
		require.NoError(t, err)

		// Load configs using new system
		rootCfg, err := GetRootConfigNew(tmpDir)
		require.NoError(t, err)
		packCfg, err := GetPackConfigNew(rootCfg, packDir)
		require.NoError(t, err)

		// Root config should have its values
		assert.Equal(t, []string{"root-pattern"}, rootCfg.Patterns.PackIgnore)
		assert.True(t, rootCfg.LinkPaths.CoreUnixExceptions["ssh"])

		// Pack config should override
		assert.Equal(t, []string{"pack-pattern"}, packCfg.Patterns.PackIgnore)
		assert.True(t, packCfg.LinkPaths.CoreUnixExceptions["myapp"])
		assert.False(t, packCfg.LinkPaths.CoreUnixExceptions["ssh"]) // replaced
	})
}
