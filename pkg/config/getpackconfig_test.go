package config

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestGetPackConfig_LoadsAndMergesAllSections(t *testing.T) {
	tmpDir := t.TempDir()
	packDir := filepath.Join(tmpDir, "mypack")
	require.NoError(t, os.MkdirAll(packDir, 0755))

	// Create root config with some defaults
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

	// Create pack config that adds to the root config
	packConfig := `
[pack]
ignore = ["pack-specific", "*.tmp"]

[symlink]
force_home = ["myapp", "custom"]
protected_paths = [".myapp/secret", ".custom/credentials"]

[mappings]
shell = ["aliases.sh", "functions.sh"]
homebrew = "Brewfile.local"
`
	require.NoError(t, os.WriteFile(filepath.Join(packDir, "dodot.toml"), []byte(packConfig), 0644))

	// Load root config
	rootCfg, err := GetRootConfig(tmpDir)
	require.NoError(t, err)

	// Load pack config
	packCfg, err := GetPackConfig(rootCfg, packDir)
	require.NoError(t, err)

	// Test [pack] section - ignore patterns should be merged
	t.Run("pack_ignore_patterns_merged", func(t *testing.T) {
		// Should have all patterns from defaults, root config, and pack config
		expectedIgnore := []string{
			// Defaults from embedded config
			".git", ".svn", ".hg", "node_modules", ".DS_Store",
			"*.swp", "*~", "#*#", ".env*", ".terraform/",
			// From root config
			"root-ignore", "*.log",
			// From pack config
			"pack-specific", "*.tmp",
		}

		for _, pattern := range expectedIgnore {
			assert.Contains(t, packCfg.Patterns.PackIgnore, pattern,
				"Pack ignore should contain pattern: %s", pattern)
		}
	})

	// Test [symlink] section - force_home should be merged
	t.Run("force_home_merged", func(t *testing.T) {
		// Should have all force_home from defaults, root, and pack
		expectedForceHome := map[string]bool{
			// From root config (and defaults)
			"ssh":       true,
			"gitconfig": true,
			"aws":       true, // from defaults
			"bashrc":    true, // from defaults
			// From pack config
			"myapp":  true,
			"custom": true,
		}

		for key, expected := range expectedForceHome {
			assert.Equal(t, expected, packCfg.LinkPaths.CoreUnixExceptions[key],
				"Force home should have %s=%v", key, expected)
		}
	})

	// Test [symlink] section - protected_paths should be merged
	t.Run("protected_paths_merged", func(t *testing.T) {
		// Should have all protected paths from defaults, root, and pack
		expectedProtected := map[string]bool{
			// From root config
			".ssh/id_rsa": true,
			".gnupg":      true,
			// From defaults
			".ssh/authorized_keys": true,
			".password-store":      true,
			// From pack config
			".myapp/secret":       true,
			".custom/credentials": true,
		}

		for path, expected := range expectedProtected {
			assert.Equal(t, expected, packCfg.Security.ProtectedPaths[path],
				"Protected paths should have %s=%v", path, expected)
		}
	})

	// Test [mappings] section
	t.Run("mappings_merged", func(t *testing.T) {
		// Debug output
		t.Logf("Mappings.Path: %s", packCfg.Mappings.Path)
		t.Logf("Mappings.Install: %s", packCfg.Mappings.Install)
		t.Logf("Mappings.Shell: %v", packCfg.Mappings.Shell)
		t.Logf("Mappings.Homebrew: %s", packCfg.Mappings.Homebrew)

		// Path and install should come from root (if not overridden)
		assert.Equal(t, "bin", packCfg.Mappings.Path)
		assert.Equal(t, "install.sh", packCfg.Mappings.Install)

		// Homebrew should come from pack
		assert.Equal(t, "Brewfile.local", packCfg.Mappings.Homebrew)

		// Shell behavior: Check what actually happens
		// It appears shell arrays are being merged/appended
		assert.Contains(t, packCfg.Mappings.Shell, "aliases.sh")
		assert.Contains(t, packCfg.Mappings.Shell, "functions.sh")
	})
}

func TestGetPackConfig_PrefersDotPrefixedFile(t *testing.T) {
	tmpDir := t.TempDir()
	packDir := filepath.Join(tmpDir, "mypack")
	require.NoError(t, os.MkdirAll(packDir, 0755))

	// Create root config
	rootCfg := &Config{
		Patterns: Patterns{
			PackIgnore: []string{".git"},
		},
	}

	// Create both dodot.toml and .dodot.toml in pack
	config1 := `
[pack]
ignore = ["from-dodot-toml"]
`
	config2 := `
[pack]
ignore = ["from-dot-dodot-toml"]
`

	require.NoError(t, os.WriteFile(filepath.Join(packDir, "dodot.toml"), []byte(config1), 0644))
	require.NoError(t, os.WriteFile(filepath.Join(packDir, ".dodot.toml"), []byte(config2), 0644))

	// Load pack config
	packCfg, err := GetPackConfig(rootCfg, packDir)
	require.NoError(t, err)

	// Should use .dodot.toml (checked first)
	assert.Contains(t, packCfg.Patterns.PackIgnore, "from-dot-dodot-toml")
	assert.NotContains(t, packCfg.Patterns.PackIgnore, "from-dodot-toml")
}

func TestGetPackConfig_NoPackConfig(t *testing.T) {
	tmpDir := t.TempDir()
	packDir := filepath.Join(tmpDir, "mypack")
	require.NoError(t, os.MkdirAll(packDir, 0755))

	// Create root config
	rootCfg := &Config{
		Patterns: Patterns{
			PackIgnore: []string{".git", "node_modules"},
		},
		LinkPaths: LinkPaths{
			CoreUnixExceptions: map[string]bool{
				"ssh": true,
			},
		},
	}

	// Load pack config (no pack config file exists)
	packCfg, err := GetPackConfig(rootCfg, packDir)
	require.NoError(t, err)

	// Should return root config unchanged
	assert.Equal(t, rootCfg.Patterns.PackIgnore, packCfg.Patterns.PackIgnore)
	assert.Equal(t, rootCfg.LinkPaths.CoreUnixExceptions, packCfg.LinkPaths.CoreUnixExceptions)
}

func TestGetPackConfig_InvalidTOML(t *testing.T) {
	tmpDir := t.TempDir()
	packDir := filepath.Join(tmpDir, "mypack")
	require.NoError(t, os.MkdirAll(packDir, 0755))

	rootCfg := &Config{}

	// Create invalid TOML
	invalidConfig := `
[pack
ignore = ["missing closing bracket"
`
	require.NoError(t, os.WriteFile(filepath.Join(packDir, "dodot.toml"), []byte(invalidConfig), 0644))

	// Should return error
	_, err := GetPackConfig(rootCfg, packDir)
	require.Error(t, err)
	assert.Contains(t, err.Error(), "failed to load pack config")
}
