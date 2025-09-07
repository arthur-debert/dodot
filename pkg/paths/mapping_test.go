// Test Type: Business Logic Test
// Description: Tests for pack file mapping with force_home support

package paths_test

import (
	"fmt"
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestMapPackFileToSystem_PackLevelForceHome(t *testing.T) {
	// Save original config and restore after test
	originalConfig := config.Get()
	defer func() {
		config.Initialize(originalConfig)
		config.ClearPackConfigs()
	}()

	// Initialize config with some root-level force_home exceptions
	testConfig := &config.Config{
		LinkPaths: config.LinkPaths{
			CoreUnixExceptions: map[string]bool{
				"ssh":    true,
				"bashrc": true,
			},
		},
	}
	config.Initialize(testConfig)

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
		name          string
		packForceHome []string
		relPath       string
		expectedPath  string
		description   string
	}{
		// Pack-level force_home tests
		{
			name:          "pack_force_home_exact_match",
			packForceHome: []string{"myconfig", "special.conf"},
			relPath:       "myconfig",
			expectedPath:  filepath.Join(homeDir, ".myconfig"),
			description:   "Pack force_home should place file in $HOME with dot prefix",
		},
		{
			name:          "pack_force_home_glob_match",
			packForceHome: []string{"*.conf"},
			relPath:       "app.conf",
			expectedPath:  filepath.Join(homeDir, ".app.conf"),
			description:   "Pack force_home glob should match and place in $HOME",
		},
		{
			name:          "pack_force_home_subdirectory",
			packForceHome: []string{"configs/*"},
			relPath:       "configs/app.conf",
			expectedPath:  filepath.Join(homeDir, ".configs", "app.conf"),
			description:   "Pack force_home should work for subdirectory files",
		},
		{
			name:          "pack_force_home_overrides_smart_default",
			packForceHome: []string{"nvim/*"},
			relPath:       "nvim/init.lua",
			expectedPath:  filepath.Join(homeDir, ".nvim", "init.lua"),
			description:   "Pack force_home should override smart default (XDG)",
		},
		{
			name:          "no_pack_force_home_uses_smart_default",
			packForceHome: []string{},
			relPath:       "nvim/init.lua",
			expectedPath:  filepath.Join(xdgConfigHome, "nvim", "init.lua"),
			description:   "Without pack force_home, should use smart default",
		},

		// Interaction with root-level force_home
		{
			name:          "root_force_home_still_works",
			packForceHome: []string{"myconfig"},
			relPath:       "ssh/config",
			expectedPath:  filepath.Join(homeDir, ".ssh", "config"),
			description:   "Root-level force_home should still work",
		},
		{
			name:          "pack_overrides_for_same_file",
			packForceHome: []string{"bashrc"},
			relPath:       "bashrc",
			expectedPath:  filepath.Join(homeDir, ".bashrc"),
			description:   "Both pack and root have same file - should still work",
		},

		// Explicit overrides still take precedence
		{
			name:          "explicit_home_override_beats_pack_force",
			packForceHome: []string{"*"},
			relPath:       "_home/myconfig",
			expectedPath:  filepath.Join(homeDir, ".myconfig"),
			description:   "Explicit _home/ should take precedence over pack force_home",
		},
		{
			name:          "explicit_xdg_override_beats_pack_force",
			packForceHome: []string{"*"},
			relPath:       "_xdg/app/config",
			expectedPath:  filepath.Join(xdgConfigHome, "app", "config"),
			description:   "Explicit _xdg/ should take precedence over pack force_home",
		},

		// Edge cases
		{
			name:          "empty_force_home_patterns",
			packForceHome: []string{},
			relPath:       "topfile",
			expectedPath:  filepath.Join(homeDir, ".topfile"),
			description:   "Top-level files should still go to $HOME by default",
		},
		{
			name:          "multiple_patterns_match",
			packForceHome: []string{"*.conf", "configs/*", "special"},
			relPath:       "configs/app.conf",
			expectedPath:  filepath.Join(homeDir, ".configs", "app.conf"),
			description:   "Multiple patterns can match same file",
		},
	}

	for i, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Use unique pack name for each test
			packName := fmt.Sprintf("testpack-%d", i)

			// Register pack config if it has force_home
			if len(tt.packForceHome) > 0 {
				packConfig := config.PackConfig{
					Symlink: config.Symlink{
						ForceHome: tt.packForceHome,
					},
				}
				config.RegisterPackConfig(packName, packConfig)
			}

			// Create pack without config (config comes from registry)
			pack := &types.Pack{
				Name: packName,
				Path: "/test/pack",
			}

			result := p.MapPackFileToSystem(pack, tt.relPath)
			assert.Equal(t, tt.expectedPath, result, tt.description)
		})
	}
}

func TestMapPackFileToSystem_LayerPriority(t *testing.T) {
	// This test verifies the layer priority:
	// Layer 3 (explicit) > Layer 2 (force_home - pack overrides root) > Layer 1 (smart default)

	homeDir, err := paths.GetHomeDirectory()
	require.NoError(t, err)

	// Set XDG_CONFIG_HOME
	xdgConfigHome := filepath.Join(homeDir, ".config")
	err = os.Setenv("XDG_CONFIG_HOME", xdgConfigHome)
	require.NoError(t, err)

	// Initialize with root-level force_home
	testConfig := &config.Config{
		LinkPaths: config.LinkPaths{
			CoreUnixExceptions: map[string]bool{
				"vim": true,
			},
		},
	}
	config.Initialize(testConfig)
	defer func() {
		config.Initialize(config.Default())
		config.ClearPackConfigs()
	}()

	p, err := paths.New("")
	require.NoError(t, err)

	tests := []struct {
		name          string
		packName      string
		packForceHome []string
		relPath       string
		expectedPath  string
		activeLayer   string
	}{
		{
			name:          "layer_2_pack_force_home",
			packName:      "testpack1",
			packForceHome: []string{"nvim/*"},
			relPath:       "nvim/init.lua",
			expectedPath:  filepath.Join(homeDir, ".nvim", "init.lua"),
			activeLayer:   "Layer 2 - Pack force_home",
		},
		{
			name:          "layer_3_explicit_home",
			packName:      "testpack2",
			packForceHome: []string{},
			relPath:       "_home/myconfig",
			expectedPath:  filepath.Join(homeDir, ".myconfig"),
			activeLayer:   "Layer 3 - Explicit _home/",
		},
		{
			name:          "layer_3_explicit_xdg",
			packName:      "testpack3",
			packForceHome: []string{},
			relPath:       "_xdg/app/config",
			expectedPath:  filepath.Join(xdgConfigHome, "app", "config"),
			activeLayer:   "Layer 3 - Explicit _xdg/",
		},
		{
			name:          "layer_2_root_force_home",
			packName:      "testpack4",
			packForceHome: []string{},
			relPath:       "vim/vimrc",
			expectedPath:  filepath.Join(homeDir, ".vim", "vimrc"),
			activeLayer:   "Layer 2 - Root force_home (no pack override)",
		},
		{
			name:          "layer_1_smart_default_toplevel",
			packName:      "testpack5",
			packForceHome: []string{},
			relPath:       "tmux.conf",
			expectedPath:  filepath.Join(homeDir, ".tmux.conf"),
			activeLayer:   "Layer 1 - Smart default (top-level)",
		},
		{
			name:          "layer_1_smart_default_subdir",
			packName:      "testpack6",
			packForceHome: []string{},
			relPath:       "app/config.toml",
			expectedPath:  filepath.Join(xdgConfigHome, "app", "config.toml"),
			activeLayer:   "Layer 1 - Smart default (subdirectory)",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Register pack config if it has force_home
			if len(tt.packForceHome) > 0 {
				packConfig := config.PackConfig{
					Symlink: config.Symlink{
						ForceHome: tt.packForceHome,
					},
				}
				config.RegisterPackConfig(tt.packName, packConfig)
			}

			pack := &types.Pack{
				Name: tt.packName,
				Path: "/test/pack",
			}

			result := p.MapPackFileToSystem(pack, tt.relPath)
			assert.Equal(t, tt.expectedPath, result, "Active layer: "+tt.activeLayer)
		})
	}
}

func TestMapPackFileToSystem_PackOverridesRoot(t *testing.T) {
	// This test specifically verifies that pack-level force_home
	// takes precedence over root-level force_home exceptions

	homeDir, err := paths.GetHomeDirectory()
	require.NoError(t, err)

	// Set XDG_CONFIG_HOME
	xdgConfigHome := filepath.Join(homeDir, ".config")
	err = os.Setenv("XDG_CONFIG_HOME", xdgConfigHome)
	require.NoError(t, err)

	// Initialize with root-level force_home that does NOT include myapp
	// This simulates the case where root config doesn't force myapp to home
	testConfig := &config.Config{
		LinkPaths: config.LinkPaths{
			CoreUnixExceptions: map[string]bool{
				"vim":  true, // vim is in root exceptions
				"bash": true, // bash is in root exceptions
				// myapp is NOT in root exceptions
			},
		},
	}
	config.Initialize(testConfig)
	defer func() {
		config.Initialize(config.Default())
		config.ClearPackConfigs()
	}()

	p, err := paths.New("")
	require.NoError(t, err)

	tests := []struct {
		name          string
		packName      string
		packForceHome []string
		relPath       string
		expectedPath  string
		description   string
	}{
		{
			name:          "myapp_without_pack_override_goes_to_xdg",
			packName:      "myapp-pack",
			packForceHome: []string{},
			relPath:       "myapp/config.toml",
			expectedPath:  filepath.Join(xdgConfigHome, "myapp", "config.toml"),
			description:   "Without pack override, myapp should go to XDG (not in root exceptions)",
		},
		{
			name:          "pack_forces_myapp_to_home",
			packName:      "myapp-pack-forced",
			packForceHome: []string{"myapp/*"},
			relPath:       "myapp/config.toml",
			expectedPath:  filepath.Join(homeDir, ".myapp", "config.toml"),
			description:   "Pack force_home should override default and force myapp to HOME",
		},
		{
			name:          "pack_can_force_specific_files",
			packName:      "myapp-pack-specific",
			packForceHome: []string{"myapp/special.conf"},
			relPath:       "myapp/special.conf",
			expectedPath:  filepath.Join(homeDir, ".myapp", "special.conf"),
			description:   "Pack can force specific files to HOME",
		},
		{
			name:          "other_myapp_files_still_go_to_xdg",
			packName:      "myapp-pack-partial",
			packForceHome: []string{"myapp/special.conf"},
			relPath:       "myapp/regular.conf",
			expectedPath:  filepath.Join(xdgConfigHome, "myapp", "regular.conf"),
			description:   "Other files not in pack force_home go to XDG",
		},
		{
			name:          "root_exceptions_still_work",
			packName:      "vim-pack",
			packForceHome: []string{},
			relPath:       "vim/vimrc",
			expectedPath:  filepath.Join(homeDir, ".vim", "vimrc"),
			description:   "Root exceptions (vim) still work when pack has no overrides",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Register pack config if it has force_home
			if len(tt.packForceHome) > 0 {
				packConfig := config.PackConfig{
					Symlink: config.Symlink{
						ForceHome: tt.packForceHome,
					},
				}
				config.RegisterPackConfig(tt.packName, packConfig)
			}

			pack := &types.Pack{
				Name: tt.packName,
				Path: "/test/pack",
			}

			result := p.MapPackFileToSystem(pack, tt.relPath)
			assert.Equal(t, tt.expectedPath, result, tt.description)
		})
	}
}
