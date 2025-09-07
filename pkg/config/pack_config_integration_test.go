package config_test

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestPackConfigIntegration verifies that pack-level config loading
// works correctly with all supported sections: [pack], [symlink], and [mappings]
func TestPackConfigIntegration(t *testing.T) {
	// Create test structure
	tmpDir := t.TempDir()
	pack1Dir := filepath.Join(tmpDir, "vim")
	pack2Dir := filepath.Join(tmpDir, "myapp")
	require.NoError(t, os.MkdirAll(pack1Dir, 0755))
	require.NoError(t, os.MkdirAll(pack2Dir, 0755))

	// Root config with baseline settings
	rootConfig := `
# Root dotfiles configuration
[pack]
ignore = ["*.bak", "temp/"]

[symlink]
force_home = ["ssh", "gitconfig"]
protected_paths = [".ssh/id_rsa"]

[mappings]
path = "bin"
install = "install.sh"
shell = ["profile.sh"]
`
	require.NoError(t, os.WriteFile(filepath.Join(tmpDir, "dodot.toml"), []byte(rootConfig), 0644))

	// Vim pack config - adds vim-specific settings
	vimConfig := `
[pack]
ignore = ["*.swp", "*.swo"]

[symlink]
force_home = ["vim", "vimrc"]
protected_paths = [".vim/secret"]

[mappings]
shell = ["vim-aliases.sh"]
`
	require.NoError(t, os.WriteFile(filepath.Join(pack1Dir, ".dodot.toml"), []byte(vimConfig), 0644))

	// MyApp pack config - adds app-specific settings
	myappConfig := `
[pack]
ignore = ["logs/", "*.log"]

[symlink]
force_home = ["myapp"]
protected_paths = [".myapp/credentials", ".myapp/keys/"]

[mappings]
install = "setup.sh"  # Override root install script
shell = ["myapp-env.sh"]
homebrew = "Brewfile.myapp"
`
	require.NoError(t, os.WriteFile(filepath.Join(pack2Dir, "dodot.toml"), []byte(myappConfig), 0644))

	// Load root config
	rootCfg, err := config.GetRootConfig(tmpDir)
	require.NoError(t, err)

	t.Run("vim_pack_config", func(t *testing.T) {
		cfg, err := config.GetPackConfig(rootCfg, pack1Dir)
		require.NoError(t, err)

		// Pack ignore should include root + vim patterns
		assert.Contains(t, cfg.Patterns.PackIgnore, "*.bak") // from root
		assert.Contains(t, cfg.Patterns.PackIgnore, "temp/") // from root
		assert.Contains(t, cfg.Patterns.PackIgnore, "*.swp") // from vim pack
		assert.Contains(t, cfg.Patterns.PackIgnore, "*.swo") // from vim pack
		assert.Contains(t, cfg.Patterns.PackIgnore, ".git")  // from defaults

		// Force home should include root + vim patterns
		assert.True(t, cfg.LinkPaths.CoreUnixExceptions["ssh"])       // from root
		assert.True(t, cfg.LinkPaths.CoreUnixExceptions["gitconfig"]) // from root
		assert.True(t, cfg.LinkPaths.CoreUnixExceptions["vim"])       // from vim pack
		assert.True(t, cfg.LinkPaths.CoreUnixExceptions["vimrc"])     // from vim pack

		// Protected paths should include root + vim paths
		assert.True(t, cfg.Security.ProtectedPaths[".ssh/id_rsa"])          // from root
		assert.True(t, cfg.Security.ProtectedPaths[".vim/secret"])          // from vim pack
		assert.True(t, cfg.Security.ProtectedPaths[".ssh/authorized_keys"]) // from defaults

		// Mappings
		assert.Equal(t, "bin", cfg.Mappings.Path)                // from root
		assert.Equal(t, "install.sh", cfg.Mappings.Install)      // from root
		assert.Contains(t, cfg.Mappings.Shell, "profile.sh")     // from root
		assert.Contains(t, cfg.Mappings.Shell, "vim-aliases.sh") // from vim pack
	})

	t.Run("myapp_pack_config", func(t *testing.T) {
		cfg, err := config.GetPackConfig(rootCfg, pack2Dir)
		require.NoError(t, err)

		// Pack ignore should include root + myapp patterns
		assert.Contains(t, cfg.Patterns.PackIgnore, "*.bak") // from root
		assert.Contains(t, cfg.Patterns.PackIgnore, "logs/") // from myapp pack
		assert.Contains(t, cfg.Patterns.PackIgnore, "*.log") // from myapp pack

		// Force home should include root + myapp patterns
		assert.True(t, cfg.LinkPaths.CoreUnixExceptions["ssh"])   // from root
		assert.True(t, cfg.LinkPaths.CoreUnixExceptions["myapp"]) // from myapp pack

		// Protected paths should include root + myapp paths
		assert.True(t, cfg.Security.ProtectedPaths[".ssh/id_rsa"])        // from root
		assert.True(t, cfg.Security.ProtectedPaths[".myapp/credentials"]) // from myapp pack
		assert.True(t, cfg.Security.ProtectedPaths[".myapp/keys/"])       // from myapp pack

		// Mappings - install is overridden by pack
		assert.Equal(t, "bin", cfg.Mappings.Path)                // from root
		assert.Equal(t, "setup.sh", cfg.Mappings.Install)        // OVERRIDDEN by myapp pack
		assert.Equal(t, "Brewfile.myapp", cfg.Mappings.Homebrew) // from myapp pack
		assert.Contains(t, cfg.Mappings.Shell, "profile.sh")     // from root
		assert.Contains(t, cfg.Mappings.Shell, "myapp-env.sh")   // from myapp pack
	})

	t.Run("pack_without_config", func(t *testing.T) {
		// Create a pack without config file
		noConfigPack := filepath.Join(tmpDir, "noconfig")
		require.NoError(t, os.MkdirAll(noConfigPack, 0755))

		cfg, err := config.GetPackConfig(rootCfg, noConfigPack)
		require.NoError(t, err)

		// Should have same values as root config
		assert.Equal(t, rootCfg.Patterns.PackIgnore, cfg.Patterns.PackIgnore)
		assert.Equal(t, rootCfg.LinkPaths.CoreUnixExceptions, cfg.LinkPaths.CoreUnixExceptions)
		assert.Equal(t, rootCfg.Security.ProtectedPaths, cfg.Security.ProtectedPaths)
		assert.Equal(t, rootCfg.Mappings, cfg.Mappings)
	})
}

// TestPackConfigSectionSupport verifies all supported config sections work
func TestPackConfigSectionSupport(t *testing.T) {
	tmpDir := t.TempDir()
	packDir := filepath.Join(tmpDir, "test-pack")
	require.NoError(t, os.MkdirAll(packDir, 0755))

	// Comprehensive pack config with all sections
	packConfig := `
# Pack-specific configuration

# Files/directories to ignore during pack discovery
[pack]
ignore = [
    "*.tmp",
    "cache/",
    "node_modules/",
    ".env.local"
]

# Symlink behavior configuration  
[symlink]
# Force these to $HOME instead of XDG_CONFIG_HOME
force_home = [
    "myapp",      # .myapp/ directory
    "appconfig"   # .appconfig file
]

# Additional protected paths (won't be symlinked)
protected_paths = [
    ".myapp/secrets/",
    ".myapp/auth.json",
    ".config/myapp/tokens.json"
]

# File pattern to handler mappings
[mappings]
path = "scripts"              # Add scripts/ to PATH
install = "setup.sh"          # Installation script
shell = ["env.sh", "aliases.sh"]  # Shell integration scripts
homebrew = "Brewfile.local"   # Homebrew dependencies
`
	require.NoError(t, os.WriteFile(filepath.Join(packDir, ".dodot.toml"), []byte(packConfig), 0644))

	// Minimal root config
	rootCfg := &config.Config{
		Patterns: config.Patterns{
			PackIgnore: []string{".git"},
		},
	}

	// Load pack config
	cfg, err := config.GetPackConfig(rootCfg, packDir)
	require.NoError(t, err)

	// Verify all sections were loaded and transformed correctly

	// [pack] section
	assert.Contains(t, cfg.Patterns.PackIgnore, "*.tmp")
	assert.Contains(t, cfg.Patterns.PackIgnore, "cache/")
	assert.Contains(t, cfg.Patterns.PackIgnore, "node_modules/")
	assert.Contains(t, cfg.Patterns.PackIgnore, ".env.local")

	// [symlink] force_home
	assert.True(t, cfg.LinkPaths.CoreUnixExceptions["myapp"])
	assert.True(t, cfg.LinkPaths.CoreUnixExceptions["appconfig"])

	// [symlink] protected_paths
	assert.True(t, cfg.Security.ProtectedPaths[".myapp/secrets/"])
	assert.True(t, cfg.Security.ProtectedPaths[".myapp/auth.json"])
	assert.True(t, cfg.Security.ProtectedPaths[".config/myapp/tokens.json"])

	// [mappings] section
	assert.Equal(t, "scripts", cfg.Mappings.Path)
	assert.Equal(t, "setup.sh", cfg.Mappings.Install)
	assert.ElementsMatch(t, []string{"env.sh", "aliases.sh"}, cfg.Mappings.Shell)
	assert.Equal(t, "Brewfile.local", cfg.Mappings.Homebrew)
}
