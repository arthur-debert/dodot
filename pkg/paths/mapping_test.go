package paths

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestMapPackFileToSystem(t *testing.T) {
	// Save original HOME
	originalHome := os.Getenv("HOME")
	defer func() {
		_ = os.Setenv("HOME", originalHome)
	}()

	testHome := "/home/testuser"
	require.NoError(t, os.Setenv("HOME", testHome))

	p, err := New("")
	require.NoError(t, err)

	tests := []struct {
		name     string
		pack     *types.Pack
		relPath  string
		expected string
	}{
		{
			name: "top-level file",
			pack: &types.Pack{
				Name: "configs",
				Path: "/dotfiles/configs",
			},
			relPath:  "gitconfig",
			expected: filepath.Join(testHome, "gitconfig"),
		},
		{
			name: "file in subdirectory",
			pack: &types.Pack{
				Name: "nvim",
				Path: "/dotfiles/nvim",
			},
			relPath:  "nvim/init.lua",
			expected: filepath.Join(testHome, "nvim/init.lua"),
		},
		{
			name: "deeply nested file",
			pack: &types.Pack{
				Name: "dev",
				Path: "/dotfiles/dev",
			},
			relPath:  "config/app/settings.json",
			expected: filepath.Join(testHome, "config/app/settings.json"),
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := p.MapPackFileToSystem(tt.pack, tt.relPath)
			assert.Equal(t, tt.expected, result)
		})
	}
}

func TestMapSystemFileToPack(t *testing.T) {
	// Save original environment
	originalHome := os.Getenv("HOME")
	originalXDG := os.Getenv("XDG_CONFIG_HOME")
	defer func() {
		_ = os.Setenv("HOME", originalHome)
		_ = os.Setenv("XDG_CONFIG_HOME", originalXDG)
	}()

	testHome := "/home/testuser"
	require.NoError(t, os.Setenv("HOME", testHome))
	require.NoError(t, os.Setenv("XDG_CONFIG_HOME", filepath.Join(testHome, ".config")))

	p, err := New("")
	require.NoError(t, err)

	tests := []struct {
		name       string
		pack       *types.Pack
		systemPath string
		expected   string
	}{
		{
			name: "dotfile in HOME",
			pack: &types.Pack{
				Name: "configs",
				Path: "/dotfiles/configs",
			},
			systemPath: filepath.Join(testHome, ".gitconfig"),
			expected:   "/dotfiles/configs/gitconfig",
		},
		{
			name: "file in XDG_CONFIG_HOME",
			pack: &types.Pack{
				Name: "configs",
				Path: "/dotfiles/configs",
			},
			systemPath: filepath.Join(testHome, ".config/nvim/init.lua"),
			expected:   "/dotfiles/configs/nvim/init.lua",
		},
		{
			name: "non-dotfile in HOME",
			pack: &types.Pack{
				Name: "misc",
				Path: "/dotfiles/misc",
			},
			systemPath: filepath.Join(testHome, "myconfig"),
			expected:   "/dotfiles/misc/myconfig",
		},
		{
			name: "file in hidden directory",
			pack: &types.Pack{
				Name: "configs",
				Path: "/dotfiles/configs",
			},
			systemPath: filepath.Join(testHome, ".ssh/config"),
			expected:   "/dotfiles/configs/config",
		},
		{
			name: "deeply nested in XDG",
			pack: &types.Pack{
				Name: "apps",
				Path: "/dotfiles/apps",
			},
			systemPath: filepath.Join(testHome, ".config/app/subdir/config.toml"),
			expected:   "/dotfiles/apps/app/subdir/config.toml",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := p.MapSystemFileToPack(tt.pack, tt.systemPath)
			assert.Equal(t, tt.expected, result)
		})
	}
}

// TestPathMappingCurrentBehavior verifies that the current mapping behavior is preserved
// Note: In Release A, we maintain existing behavior which may not be perfectly symmetric
func TestPathMappingCurrentBehavior(t *testing.T) {
	// Save original environment
	originalHome := os.Getenv("HOME")
	originalXDG := os.Getenv("XDG_CONFIG_HOME")
	defer func() {
		_ = os.Setenv("HOME", originalHome)
		_ = os.Setenv("XDG_CONFIG_HOME", originalXDG)
	}()

	testHome := "/home/testuser"
	require.NoError(t, os.Setenv("HOME", testHome))
	require.NoError(t, os.Setenv("XDG_CONFIG_HOME", filepath.Join(testHome, ".config")))

	p, err := New("")
	require.NoError(t, err)

	pack := &types.Pack{
		Name: "test",
		Path: "/dotfiles/test",
	}

	// Test current behavior
	tests := []struct {
		name           string
		packFile       string
		expectedSystem string
		expectSymmetry bool
	}{
		{
			name:           "simple config file",
			packFile:       "gitconfig",
			expectedSystem: filepath.Join(testHome, "gitconfig"),
			expectSymmetry: true, // This one happens to be symmetric
		},
		{
			name:           "nested config",
			packFile:       "nvim/init.lua",
			expectedSystem: filepath.Join(testHome, "nvim/init.lua"),
			expectSymmetry: false, // Current behavior loses the nvim directory when mapping back
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			// Map from pack to system
			systemPath := p.MapPackFileToSystem(pack, tc.packFile)
			assert.Equal(t, tc.expectedSystem, systemPath)

			// Map back from system to pack
			packPath := p.MapSystemFileToPack(pack, systemPath)

			if tc.expectSymmetry {
				expected := filepath.Join(pack.Path, tc.packFile)
				assert.Equal(t, expected, packPath, "Mapping should be symmetric")
			}
			// For non-symmetric cases, we just verify it doesn't panic
			// The asymmetry will be fixed in future releases
		})
	}
}
