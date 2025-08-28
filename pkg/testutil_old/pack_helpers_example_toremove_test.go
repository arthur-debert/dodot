package testutil_test

import (
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil_old"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// Example of using pack helpers in a test
func ExampleTestPack() {
	// This would normally be in a test function
	t := &testing.T{}

	// Old way:
	// tmpDir := t.TempDir()
	// dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
	// vimDir := filepath.Join(dotfilesRoot, "vim")
	// require.NoError(t, os.MkdirAll(vimDir, 0755))
	// require.NoError(t, os.WriteFile(filepath.Join(vimDir, ".vimrc"), []byte("set number"), 0644))

	// New way:
	pack := testutil.SetupTestPack(t, "vim")
	pack.AddFile(t, ".vimrc", "set number")
}

// Example test showing the before and after
func TestExample_BeforeAndAfter(t *testing.T) {
	t.Run("using_helpers", func(t *testing.T) {
		// Setup pack with home directory
		pack, homeDir := testutil.SetupTestPackWithHome(t, "vim")

		// Add standard vim configuration
		pack.AddCommonDotfile(t, ".vimrc")
		pack.AddSymlinkRule(t, ".vimrc")

		// Test assertions
		assert.DirExists(t, pack.Dir)
		assert.FileExists(t, filepath.Join(pack.Dir, ".vimrc"))
		assert.FileExists(t, filepath.Join(pack.Dir, ".dodot.toml"))
		assert.DirExists(t, homeDir)
	})

	t.Run("multiple_packs", func(t *testing.T) {
		// Create multiple packs easily
		packs := testutil.SetupMultiplePacks(t, "vim", "bash", "git")

		// Add files to each pack
		packs["vim"].AddCommonDotfile(t, ".vimrc")
		packs["bash"].AddCommonDotfile(t, ".bashrc")
		packs["git"].AddCommonDotfile(t, ".gitconfig")

		// Add standard symlink config to all
		for _, pack := range packs {
			pack.AddStandardConfig(t, "symlink")
		}

		// Test assertions
		assert.Len(t, packs, 3)
		for name, pack := range packs {
			assert.Equal(t, name, pack.Name)
			assert.DirExists(t, pack.Dir)
		}
	})

	t.Run("install_pack", func(t *testing.T) {
		// Setup pack with install script
		pack := testutil.SetupTestPack(t, "tools")

		// Add install script
		pack.AddExecutable(t, "install.sh", `#!/bin/bash
echo "Installing tools..."
`)

		// Add install script config
		pack.AddStandardConfig(t, "install")

		// Test that install.sh is executable
		assert.FileExists(t, filepath.Join(pack.Dir, "install.sh"))
		// Note: File permissions test would go here
	})

	t.Run("homebrew_pack", func(t *testing.T) {
		// Setup pack with Brewfile
		pack := testutil.SetupTestPack(t, "homebrew")

		// Add Brewfile
		pack.AddFile(t, "Brewfile", `brew "git"
brew "vim"
cask "visual-studio-code"
`)

		// Add homebrew config
		pack.AddStandardConfig(t, "homebrew")

		// Test assertions
		assert.FileExists(t, filepath.Join(pack.Dir, "Brewfile"))
		assert.FileExists(t, filepath.Join(pack.Dir, ".dodot.toml"))
	})
}

// Example of how to use in integration tests
func TestIntegration_Example(t *testing.T) {
	// Import triggers and handlers
	// _ "github.com/arthur-debert/dodot/pkg/handlers"
	// _ "github.com/arthur-debert/dodot/pkg/triggers"

	tests := []struct {
		name  string
		setup func(t *testing.T) string // returns dotfilesRoot
	}{
		{
			name: "simple_vim_pack",
			setup: func(t *testing.T) string {
				pack := testutil.SetupTestPack(t, "vim")
				pack.AddCommonDotfile(t, ".vimrc")
				pack.AddSymlinkRule(t, ".vimrc")
				return pack.Root
			},
		},
		{
			name: "multiple_packs",
			setup: func(t *testing.T) string {
				packs := testutil.SetupMultiplePacks(t, "vim", "bash")

				packs["vim"].AddCommonDotfile(t, ".vimrc")
				packs["vim"].AddStandardConfig(t, "symlink")

				packs["bash"].AddCommonDotfile(t, ".bashrc")
				packs["bash"].AddStandardConfig(t, "symlink")

				// All packs share the same root
				return packs["vim"].Root
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			dotfilesRoot := tt.setup(t)
			require.NotEmpty(t, dotfilesRoot)
			assert.DirExists(t, dotfilesRoot)
		})
	}
}
