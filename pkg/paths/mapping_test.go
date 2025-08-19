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
			name: "top-level file gets dot prefix",
			pack: &types.Pack{
				Name: "configs",
				Path: "/dotfiles/configs",
			},
			relPath:  "gitconfig",
			expected: filepath.Join(testHome, ".gitconfig"),
		},
		{
			name: "top-level file already with dot",
			pack: &types.Pack{
				Name: "configs",
				Path: "/dotfiles/configs",
			},
			relPath:  ".hidden",
			expected: filepath.Join(testHome, ".hidden"),
		},
		{
			name: "subdirectory file goes to XDG_CONFIG_HOME",
			pack: &types.Pack{
				Name: "nvim",
				Path: "/dotfiles/nvim",
			},
			relPath:  "nvim/init.lua",
			expected: filepath.Join(testHome, ".config/nvim/init.lua"),
		},
		{
			name: "deeply nested file goes to XDG_CONFIG_HOME",
			pack: &types.Pack{
				Name: "dev",
				Path: "/dotfiles/dev",
			},
			relPath:  "config/app/settings.json",
			expected: filepath.Join(testHome, ".config/config/app/settings.json"),
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
			expected:   "/dotfiles/configs/ssh/config",
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

// TestPathMappingSymmetry verifies that the Layer 1 mapping is properly symmetric
func TestPathMappingSymmetry(t *testing.T) {
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

	// Test Layer 1 symmetry
	tests := []struct {
		name           string
		packFile       string
		expectedSystem string
	}{
		{
			name:           "top-level config file",
			packFile:       "gitconfig",
			expectedSystem: filepath.Join(testHome, ".gitconfig"),
		},
		{
			name:           "subdirectory config",
			packFile:       "nvim/init.lua",
			expectedSystem: filepath.Join(testHome, ".config/nvim/init.lua"),
		},
		{
			name:           "deeply nested config",
			packFile:       "app/config/settings.toml",
			expectedSystem: filepath.Join(testHome, ".config/app/config/settings.toml"),
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			// Map from pack to system
			systemPath := p.MapPackFileToSystem(pack, tc.packFile)
			assert.Equal(t, tc.expectedSystem, systemPath)

			// Map back from system to pack
			packPath := p.MapSystemFileToPack(pack, systemPath)

			// With Layer 1, mapping should now be symmetric
			expected := filepath.Join(pack.Path, tc.packFile)
			assert.Equal(t, expected, packPath, "Mapping should be symmetric")
		})
	}
}

// TestLayer1EdgeCases tests specific edge cases for Layer 1 mapping
func TestLayer1EdgeCases(t *testing.T) {
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

	t.Run("file already with dot prefix", func(t *testing.T) {
		result := p.MapPackFileToSystem(pack, ".bashrc")
		assert.Equal(t, filepath.Join(testHome, ".bashrc"), result)
	})

	t.Run("hidden directory mapping", func(t *testing.T) {
		// Files already under .config should have prefix stripped
		result := p.MapPackFileToSystem(pack, ".config/app/config")
		assert.Equal(t, filepath.Join(testHome, ".config/app/config"), result)
	})

	t.Run("reverse mapping hidden directory", func(t *testing.T) {
		// ~/.ssh/config should map to ssh/config in pack
		systemPath := filepath.Join(testHome, ".ssh/config")
		result := p.MapSystemFileToPack(pack, systemPath)
		assert.Equal(t, filepath.Join(pack.Path, "ssh/config"), result)
	})

	t.Run("double dot prevention", func(t *testing.T) {
		// Ensure we never create double dots
		result := p.MapPackFileToSystem(pack, ".gitignore")
		assert.Equal(t, filepath.Join(testHome, ".gitignore"), result)
		assert.NotContains(t, result, "..")
	})
}
