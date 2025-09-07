package packs_test

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/packs"
	"github.com/arthur-debert/dodot/pkg/packs/discovery"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestPackDiscoveryWithConfigIgnore_EndToEnd(t *testing.T) {
	// Create a dotfiles structure
	tmpDir := t.TempDir()

	// Create various pack directories
	packDirs := []string{
		"vim",
		"bash",
		"test-pack",
		"backup",
		"node_modules",
		"vendor",
		".git",
		".hidden",
	}

	for _, dir := range packDirs {
		packPath := filepath.Join(tmpDir, dir)
		require.NoError(t, os.MkdirAll(packPath, 0755))
		// Create a file to make it a valid pack
		require.NoError(t, os.WriteFile(filepath.Join(packPath, "file.txt"), []byte("test"), 0644))
	}

	// Test 1: Without custom config - should use defaults
	t.Run("with_default_config", func(t *testing.T) {
		candidates, err := packs.GetPackCandidates(tmpDir)
		require.NoError(t, err)

		// Extract names for easier checking
		var names []string
		for _, candidate := range candidates {
			names = append(names, filepath.Base(candidate))
		}

		// Should include normal packs
		assert.Contains(t, names, "vim")
		assert.Contains(t, names, "bash")
		assert.Contains(t, names, "test-pack")
		assert.Contains(t, names, "backup")
		assert.Contains(t, names, "vendor")

		// Should exclude defaults
		assert.NotContains(t, names, "node_modules") // in default ignore list
		assert.NotContains(t, names, ".git")         // in default ignore list
		assert.NotContains(t, names, ".hidden")      // hidden directory
	})

	// Test 2: With custom root config
	t.Run("with_custom_root_config", func(t *testing.T) {
		// Create a root config that ignores test-* and backup
		configContent := `
[pack]
ignore = ["test-*", "backup", "vendor"]
`
		configPath := filepath.Join(tmpDir, "dodot.toml")
		require.NoError(t, os.WriteFile(configPath, []byte(configContent), 0644))

		// Load config
		cfg, err := config.GetRootConfig(tmpDir)
		require.NoError(t, err)

		// Discover packs with config
		candidates, err := packs.GetPackCandidatesWithConfig(tmpDir, cfg)
		require.NoError(t, err)

		// Extract names
		var names []string
		for _, candidate := range candidates {
			names = append(names, filepath.Base(candidate))
		}

		// Should include normal packs
		assert.Contains(t, names, "vim")
		assert.Contains(t, names, "bash")

		// Should exclude custom patterns (these are added to defaults)
		assert.NotContains(t, names, "test-pack")    // matches test-*
		assert.NotContains(t, names, "backup")       // in custom ignore
		assert.NotContains(t, names, "vendor")       // in custom ignore
		assert.NotContains(t, names, "node_modules") // still in defaults
		assert.NotContains(t, names, ".git")         // still in defaults
	})

	// Test 3: Full pipeline with orchestration
	t.Run("with_orchestration", func(t *testing.T) {
		// Use the discovery helper that orchestration uses
		cfg, err := config.GetRootConfig(tmpDir)
		require.NoError(t, err)

		discoveredPacks, err := discovery.DiscoverAndSelectPacksWithConfig(tmpDir, nil, cfg)
		require.NoError(t, err)

		var packNames []string
		for _, pack := range discoveredPacks {
			packNames = append(packNames, pack.Name)
		}

		// Should have vim and bash
		assert.Contains(t, packNames, "vim")
		assert.Contains(t, packNames, "bash")

		// Should not have ignored packs
		assert.NotContains(t, packNames, "test-pack")
		assert.NotContains(t, packNames, "backup")
		assert.NotContains(t, packNames, "vendor")
		assert.NotContains(t, packNames, "node_modules")
	})
}
