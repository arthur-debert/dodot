// Test Type: Business Logic Test
// Description: Basic tests for pack file mapping with nil config

package paths_test

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestMapPackFileToSystem_BasicBehavior(t *testing.T) {
	// Get home directory for tests
	homeDir, err := paths.GetHomeDirectory()
	require.NoError(t, err)

	// Set XDG_CONFIG_HOME for predictable tests
	oldXDG := os.Getenv("XDG_CONFIG_HOME")
	xdgConfigHome := filepath.Join(homeDir, ".config")
	err = os.Setenv("XDG_CONFIG_HOME", xdgConfigHome)
	require.NoError(t, err)
	defer func() {
		_ = os.Setenv("XDG_CONFIG_HOME", oldXDG)
	}()

	// Create paths instance
	p, err := paths.New("")
	require.NoError(t, err)

	tests := []struct {
		name         string
		relPath      string
		expectedPath string
		description  string
	}{
		// Layer 1: Smart default mapping
		{
			name:         "top_level_file_goes_to_home",
			relPath:      "tmux.conf",
			expectedPath: filepath.Join(homeDir, ".tmux.conf"),
			description:  "Top-level files should go to $HOME with dot prefix",
		},
		{
			name:         "subdirectory_goes_to_xdg",
			relPath:      "nvim/init.lua",
			expectedPath: filepath.Join(xdgConfigHome, "nvim", "init.lua"),
			description:  "Subdirectory files should go to XDG_CONFIG_HOME",
		},
		{
			name:         "config_prefix_stripped",
			relPath:      "config/app/settings.toml",
			expectedPath: filepath.Join(xdgConfigHome, "app", "settings.toml"),
			description:  "config/ prefix should be stripped",
		},
		{
			name:         "dot_config_prefix_stripped",
			relPath:      ".config/app/settings.toml",
			expectedPath: filepath.Join(xdgConfigHome, "app", "settings.toml"),
			description:  ".config/ prefix should be stripped",
		},

		// Layer 2: Hardcoded force_home (with nil config)
		{
			name:         "ssh_forced_to_home",
			relPath:      "ssh/config",
			expectedPath: filepath.Join(homeDir, ".ssh", "config"),
			description:  "ssh files should be forced to $HOME",
		},
		{
			name:         "vim_forced_to_home",
			relPath:      "vim/vimrc",
			expectedPath: filepath.Join(homeDir, ".vim", "vimrc"),
			description:  "vim files should be forced to $HOME",
		},
		{
			name:         "bashrc_forced_to_home",
			relPath:      "bashrc",
			expectedPath: filepath.Join(homeDir, ".bashrc"),
			description:  "bashrc should be forced to $HOME",
		},

		// Layer 3: Explicit overrides (highest priority)
		{
			name:         "explicit_home_override",
			relPath:      "_home/myconfig",
			expectedPath: filepath.Join(homeDir, ".myconfig"),
			description:  "Explicit _home/ should place in $HOME",
		},
		{
			name:         "explicit_xdg_override",
			relPath:      "_xdg/app/config",
			expectedPath: filepath.Join(xdgConfigHome, "app", "config"),
			description:  "Explicit _xdg/ should place in XDG_CONFIG_HOME",
		},
		{
			name:         "explicit_home_with_subdir",
			relPath:      "_home/myapp/config.toml",
			expectedPath: filepath.Join(homeDir, ".myapp", "config.toml"),
			description:  "Explicit _home/ with subdirs should work",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			pack := &types.Pack{
				Name: "testpack",
				Path: "/test/pack",
			}

			result := p.MapPackFileToSystem(pack, tt.relPath, nil)
			assert.Equal(t, tt.expectedPath, result, tt.description)
		})
	}
}

func TestMapSystemFileToPack_BasicBehavior(t *testing.T) {
	// Get home directory
	homeDir, err := paths.GetHomeDirectory()
	require.NoError(t, err)

	// Set XDG_CONFIG_HOME
	xdgConfigHome := filepath.Join(homeDir, ".config")
	err = os.Setenv("XDG_CONFIG_HOME", xdgConfigHome)
	require.NoError(t, err)

	p, err := paths.New("")
	require.NoError(t, err)

	tests := []struct {
		name         string
		systemPath   string
		expectedPath string
		description  string
	}{
		// Dotfiles in $HOME
		{
			name:         "dotfile_in_home",
			systemPath:   filepath.Join(homeDir, ".tmux.conf"),
			expectedPath: "/test/pack/tmux.conf",
			description:  "Dotfiles should have dot stripped",
		},
		{
			name:         "hidden_dir_in_home",
			systemPath:   filepath.Join(homeDir, ".vim", "vimrc"),
			expectedPath: "/test/pack/vim/vimrc",
			description:  "Hidden dirs should have dot stripped",
		},

		// XDG config files
		{
			name:         "xdg_config_file",
			systemPath:   filepath.Join(xdgConfigHome, "nvim", "init.lua"),
			expectedPath: "/test/pack/nvim/init.lua",
			description:  "XDG config files should preserve structure",
		},

		// Hardcoded exceptions (still stored without dot)
		{
			name:         "ssh_exception",
			systemPath:   filepath.Join(homeDir, ".ssh", "config"),
			expectedPath: "/test/pack/ssh/config",
			description:  "SSH files stored without dot",
		},
		{
			name:         "vim_exception",
			systemPath:   filepath.Join(homeDir, ".vim", "vimrc"),
			expectedPath: "/test/pack/vim/vimrc",
			description:  "Vim files stored without dot",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			pack := &types.Pack{
				Name: "testpack",
				Path: "/test/pack",
			}

			result := p.MapSystemFileToPack(pack, tt.systemPath, nil)
			assert.Equal(t, tt.expectedPath, result, tt.description)
		})
	}
}
