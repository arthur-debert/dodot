package config

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestGetPackConfig_SimpleReplacement(t *testing.T) {
	tmpDir := t.TempDir()
	packDir := filepath.Join(tmpDir, "mypack")
	require.NoError(t, os.MkdirAll(packDir, 0755))

	// Create root config
	rootConfig := `
[pack]
ignore = ["root-ignore", "*.log"]

[symlink]
force_home = ["ssh", "gitconfig"]
protected_paths = [".ssh/id_rsa", ".gnupg"]

[mappings]
path = "bin"
install = "install.sh"
shell = ["profile.sh"]
`
	require.NoError(t, os.WriteFile(filepath.Join(tmpDir, "dodot.toml"), []byte(rootConfig), 0644))

	// Create pack config that replaces arrays
	packConfig := `
[pack]
ignore = ["pack-specific", "*.tmp"]

[symlink]
force_home = ["myapp"]
protected_paths = [".myapp/secret"]

[mappings]
shell = ["aliases.sh"]
homebrew = "Brewfile.local"
`
	require.NoError(t, os.WriteFile(filepath.Join(packDir, "dodot.toml"), []byte(packConfig), 0644))

	// Load root config
	rootCfg, err := GetRootConfig(tmpDir)
	require.NoError(t, err)

	// Load pack config
	packCfg, err := GetPackConfig(rootCfg, packDir)
	require.NoError(t, err)

	t.Run("arrays_are_replaced", func(t *testing.T) {
		// Pack ignore replaces root
		assert.Equal(t, []string{"pack-specific", "*.tmp"}, packCfg.Patterns.PackIgnore)

		// Force home replaces root
		assert.Equal(t, map[string]bool{"myapp": true}, packCfg.LinkPaths.CoreUnixExceptions)

		// Protected paths replaces root
		assert.Equal(t, map[string]bool{".myapp/secret": true}, packCfg.Security.ProtectedPaths)

		// Shell scripts replaced
		assert.Equal(t, []string{"aliases.sh"}, packCfg.Mappings.Shell)
	})

	t.Run("scalars_are_overridden", func(t *testing.T) {
		// Path unchanged (not in pack config)
		assert.Equal(t, "bin", packCfg.Mappings.Path)

		// Install unchanged (not in pack config)
		assert.Equal(t, "install.sh", packCfg.Mappings.Install)

		// Homebrew added (new in pack config)
		assert.Equal(t, "Brewfile.local", packCfg.Mappings.Homebrew)
	})
}

func TestGetPackConfig_NoPackConfig(t *testing.T) {
	tmpDir := t.TempDir()
	packDir := filepath.Join(tmpDir, "pack-without-config")
	require.NoError(t, os.MkdirAll(packDir, 0755))

	// Create only root config
	rootConfig := `
[pack]
ignore = ["*.log"]

[symlink]
force_home = ["ssh"]
`
	require.NoError(t, os.WriteFile(filepath.Join(tmpDir, "dodot.toml"), []byte(rootConfig), 0644))

	// Set DOTFILES_ROOT so pack config loading finds the root config
	oldRoot := os.Getenv("DOTFILES_ROOT")
	require.NoError(t, os.Setenv("DOTFILES_ROOT", tmpDir))
	defer func() {
		require.NoError(t, os.Setenv("DOTFILES_ROOT", oldRoot))
	}()

	rootCfg, err := GetRootConfig(tmpDir)
	require.NoError(t, err)

	// Load pack config (no pack config file exists)
	packCfg, err := GetPackConfig(rootCfg, packDir)
	require.NoError(t, err)

	// Should have root config values (which replaced defaults)
	assert.Equal(t, []string{"*.log"}, packCfg.Patterns.PackIgnore)
	// Force home includes both from root and defaults that aren't arrays
	assert.True(t, packCfg.LinkPaths.CoreUnixExceptions["ssh"])
}
