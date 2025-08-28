package paths

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestMapPackFileToSystem(t *testing.T) {
	// Save original environment
	originalHome := os.Getenv("HOME")
	originalXDG := os.Getenv("XDG_CONFIG_HOME")
	defer func() {
		_ = os.Setenv("HOME", originalHome)
		if originalXDG != "" {
			_ = os.Setenv("XDG_CONFIG_HOME", originalXDG)
		} else {
			_ = os.Unsetenv("XDG_CONFIG_HOME")
		}
	}()

	testHome := "/home/testuser"
	require.NoError(t, os.Setenv("HOME", testHome))
	// Explicitly unset XDG_CONFIG_HOME to ensure it's calculated from HOME
	require.NoError(t, os.Unsetenv("XDG_CONFIG_HOME"))

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
			expected: filepath.Join(testHome, ".config/app/settings.json"),
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
		if originalXDG != "" {
			_ = os.Setenv("XDG_CONFIG_HOME", originalXDG)
		} else {
			_ = os.Unsetenv("XDG_CONFIG_HOME")
		}
	}()

	testHome := "/home/testuser"
	require.NoError(t, os.Setenv("HOME", testHome))
	// Let the code calculate XDG_CONFIG_HOME from HOME
	require.NoError(t, os.Unsetenv("XDG_CONFIG_HOME"))

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
		if originalXDG != "" {
			_ = os.Setenv("XDG_CONFIG_HOME", originalXDG)
		} else {
			_ = os.Unsetenv("XDG_CONFIG_HOME")
		}
	}()

	testHome := "/home/testuser"
	require.NoError(t, os.Setenv("HOME", testHome))
	// Let the code calculate XDG_CONFIG_HOME from HOME
	require.NoError(t, os.Unsetenv("XDG_CONFIG_HOME"))

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

// TestLayer2ExceptionList tests the exception list behavior
func TestLayer2ExceptionList(t *testing.T) {
	// Save original environment
	originalHome := os.Getenv("HOME")
	originalXDG := os.Getenv("XDG_CONFIG_HOME")
	defer func() {
		_ = os.Setenv("HOME", originalHome)
		if originalXDG != "" {
			_ = os.Setenv("XDG_CONFIG_HOME", originalXDG)
		} else {
			_ = os.Unsetenv("XDG_CONFIG_HOME")
		}
	}()

	testHome := "/home/testuser"
	require.NoError(t, os.Setenv("HOME", testHome))
	require.NoError(t, os.Unsetenv("XDG_CONFIG_HOME"))

	// Initialize config to ensure we have the proper force_home exceptions
	config.Initialize(nil)

	p, err := New("")
	require.NoError(t, err)

	pack := &types.Pack{
		Name: "test",
		Path: "/dotfiles/test",
	}

	tests := []struct {
		name     string
		relPath  string
		expected string
	}{
		{
			name:     "ssh directory goes to HOME",
			relPath:  "ssh/config",
			expected: filepath.Join(testHome, ".ssh/config"),
		},
		{
			name:     "gitconfig goes to HOME",
			relPath:  "gitconfig",
			expected: filepath.Join(testHome, ".gitconfig"),
		},
		{
			name:     "aws directory goes to HOME",
			relPath:  "aws/credentials",
			expected: filepath.Join(testHome, ".aws/credentials"),
		},
		{
			name:     "bashrc goes to HOME",
			relPath:  "bashrc",
			expected: filepath.Join(testHome, ".bashrc"),
		},
		{
			name:     "zshrc goes to HOME",
			relPath:  "zshrc",
			expected: filepath.Join(testHome, ".zshrc"),
		},
		{
			name:     "profile goes to HOME",
			relPath:  "profile",
			expected: filepath.Join(testHome, ".profile"),
		},
		{
			name:     "docker directory goes to XDG",
			relPath:  "docker/config.json",
			expected: filepath.Join(testHome, ".config/docker/config.json"),
		},
		{
			name:     "kube directory goes to HOME",
			relPath:  "kube/config",
			expected: filepath.Join(testHome, ".kube/config"),
		},
		{
			name:     "gnupg directory goes to XDG",
			relPath:  "gnupg/pubring.kbx",
			expected: filepath.Join(testHome, ".config/gnupg/pubring.kbx"),
		},
		{
			name:     "non-exception still goes to XDG",
			relPath:  "nvim/init.lua",
			expected: filepath.Join(testHome, ".config/nvim/init.lua"),
		},
		{
			name:     "exception with dot prefix already",
			relPath:  ".ssh/known_hosts",
			expected: filepath.Join(testHome, ".ssh/known_hosts"),
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := p.MapPackFileToSystem(pack, tt.relPath)
			assert.Equal(t, tt.expected, result)
		})
	}
}

// TestLayer2ExceptionListReverse tests the reverse mapping for exception list
func TestLayer2ExceptionListReverse(t *testing.T) {
	// Save original environment
	originalHome := os.Getenv("HOME")
	originalXDG := os.Getenv("XDG_CONFIG_HOME")
	defer func() {
		_ = os.Setenv("HOME", originalHome)
		if originalXDG != "" {
			_ = os.Setenv("XDG_CONFIG_HOME", originalXDG)
		} else {
			_ = os.Unsetenv("XDG_CONFIG_HOME")
		}
	}()

	testHome := "/home/testuser"
	require.NoError(t, os.Setenv("HOME", testHome))
	require.NoError(t, os.Unsetenv("XDG_CONFIG_HOME"))

	// Initialize config to ensure we have the proper force_home exceptions
	config.Initialize(nil)

	p, err := New("")
	require.NoError(t, err)

	pack := &types.Pack{
		Name: "test",
		Path: "/dotfiles/test",
	}

	tests := []struct {
		name       string
		systemPath string
		expected   string
	}{
		{
			name:       "ssh config from HOME",
			systemPath: filepath.Join(testHome, ".ssh/config"),
			expected:   "/dotfiles/test/ssh/config",
		},
		{
			name:       "gitconfig from HOME",
			systemPath: filepath.Join(testHome, ".gitconfig"),
			expected:   "/dotfiles/test/gitconfig",
		},
		{
			name:       "aws credentials from HOME",
			systemPath: filepath.Join(testHome, ".aws/credentials"),
			expected:   "/dotfiles/test/aws/credentials",
		},
		{
			name:       "non-exception from XDG",
			systemPath: filepath.Join(testHome, ".config/nvim/init.lua"),
			expected:   "/dotfiles/test/nvim/init.lua",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := p.MapSystemFileToPack(pack, tt.systemPath)
			assert.Equal(t, tt.expected, result)
		})
	}
}

// TestLayerPrecedence tests that Layer 2 (exception list) takes precedence over Layer 1
func TestLayerPrecedence(t *testing.T) {
	// Save original environment
	originalHome := os.Getenv("HOME")
	originalXDG := os.Getenv("XDG_CONFIG_HOME")
	defer func() {
		_ = os.Setenv("HOME", originalHome)
		if originalXDG != "" {
			_ = os.Setenv("XDG_CONFIG_HOME", originalXDG)
		} else {
			_ = os.Unsetenv("XDG_CONFIG_HOME")
		}
	}()

	testHome := "/home/testuser"
	require.NoError(t, os.Setenv("HOME", testHome))
	require.NoError(t, os.Unsetenv("XDG_CONFIG_HOME"))

	// Initialize config to ensure we have the proper force_home exceptions
	config.Initialize(nil)

	p, err := New("")
	require.NoError(t, err)

	pack := &types.Pack{
		Name: "test",
		Path: "/dotfiles/test",
	}

	tests := []struct {
		name     string
		relPath  string
		expected string
		reason   string
	}{
		{
			name:     "ssh/ is exception even though it's a directory",
			relPath:  "ssh/config",
			expected: filepath.Join(testHome, ".ssh/config"),
			reason:   "Layer 2 exception list should override Layer 1's subdirectory->XDG rule",
		},
		{
			name:     "docker/ is no longer exception, goes to XDG",
			relPath:  "docker/config.json",
			expected: filepath.Join(testHome, ".config/docker/config.json"),
			reason:   "Docker directory now follows standard XDG pattern",
		},
		{
			name:     "non-exception directory still goes to XDG",
			relPath:  "alacritty/alacritty.yml",
			expected: filepath.Join(testHome, ".config/alacritty/alacritty.yml"),
			reason:   "Non-exception subdirectories should follow Layer 1 rule",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := p.MapPackFileToSystem(pack, tt.relPath)
			assert.Equal(t, tt.expected, result, tt.reason)
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
		if originalXDG != "" {
			_ = os.Setenv("XDG_CONFIG_HOME", originalXDG)
		} else {
			_ = os.Unsetenv("XDG_CONFIG_HOME")
		}
	}()

	testHome := "/home/testuser"
	require.NoError(t, os.Setenv("HOME", testHome))
	// Let the code calculate XDG_CONFIG_HOME from HOME
	require.NoError(t, os.Unsetenv("XDG_CONFIG_HOME"))

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

	t.Run("config prefix stripping", func(t *testing.T) {
		// Ensure config/ prefix is stripped to avoid .config/config/...
		result := p.MapPackFileToSystem(pack, "config/app/settings.json")
		assert.Equal(t, filepath.Join(testHome, ".config/app/settings.json"), result)
		assert.NotContains(t, result, ".config/config/")
	})
}

// TestLayer3ExplicitOverrides tests the _home/ and _xdg/ override behavior
func TestLayer3ExplicitOverrides(t *testing.T) {
	// Save original environment
	originalHome := os.Getenv("HOME")
	originalXDG := os.Getenv("XDG_CONFIG_HOME")
	defer func() {
		_ = os.Setenv("HOME", originalHome)
		if originalXDG != "" {
			_ = os.Setenv("XDG_CONFIG_HOME", originalXDG)
		} else {
			_ = os.Unsetenv("XDG_CONFIG_HOME")
		}
	}()

	testHome := "/home/testuser"
	require.NoError(t, os.Setenv("HOME", testHome))
	require.NoError(t, os.Unsetenv("XDG_CONFIG_HOME"))

	// Initialize config to ensure we have the proper force_home exceptions
	config.Initialize(nil)

	p, err := New("")
	require.NoError(t, err)

	pack := &types.Pack{
		Name: "test",
		Path: "/dotfiles/test",
	}

	tests := []struct {
		name     string
		relPath  string
		expected string
	}{
		{
			name:     "_home/ file goes to HOME with dot",
			relPath:  "_home/myconfig",
			expected: filepath.Join(testHome, ".myconfig"),
		},
		{
			name:     "_home/ with subdirectory",
			relPath:  "_home/special/config",
			expected: filepath.Join(testHome, ".special/config"),
		},
		{
			name:     "_home/ file already with dot",
			relPath:  "_home/.hidden",
			expected: filepath.Join(testHome, ".hidden"),
		},
		{
			name:     "_xdg/ file goes to XDG_CONFIG_HOME",
			relPath:  "_xdg/myapp/config.toml",
			expected: filepath.Join(testHome, ".config/myapp/config.toml"),
		},
		{
			name:     "_xdg/ with deep nesting",
			relPath:  "_xdg/company/product/settings.json",
			expected: filepath.Join(testHome, ".config/company/product/settings.json"),
		},
		{
			name:     "_xdg/ single file",
			relPath:  "_xdg/app.conf",
			expected: filepath.Join(testHome, ".config/app.conf"),
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := p.MapPackFileToSystem(pack, tt.relPath)
			assert.Equal(t, tt.expected, result)
		})
	}
}

// TestLayer3Precedence tests that Layer 3 overrides take precedence over Layer 2 and Layer 1
func TestLayer3Precedence(t *testing.T) {
	// Save original environment
	originalHome := os.Getenv("HOME")
	originalXDG := os.Getenv("XDG_CONFIG_HOME")
	defer func() {
		_ = os.Setenv("HOME", originalHome)
		if originalXDG != "" {
			_ = os.Setenv("XDG_CONFIG_HOME", originalXDG)
		} else {
			_ = os.Unsetenv("XDG_CONFIG_HOME")
		}
	}()

	testHome := "/home/testuser"
	require.NoError(t, os.Setenv("HOME", testHome))
	require.NoError(t, os.Unsetenv("XDG_CONFIG_HOME"))

	// Initialize config to ensure we have the proper force_home exceptions
	config.Initialize(nil)

	p, err := New("")
	require.NoError(t, err)

	pack := &types.Pack{
		Name: "test",
		Path: "/dotfiles/test",
	}

	tests := []struct {
		name     string
		relPath  string
		expected string
		reason   string
	}{
		{
			name:     "_home/ overrides exception list",
			relPath:  "_home/ssh/config",
			expected: filepath.Join(testHome, ".ssh/config"),
			reason:   "Layer 3 _home/ should override Layer 2 exception list",
		},
		{
			name:     "_xdg/ overrides exception list",
			relPath:  "_xdg/ssh/config",
			expected: filepath.Join(testHome, ".config/ssh/config"),
			reason:   "Layer 3 _xdg/ should force ssh to XDG despite exception",
		},
		{
			name:     "_home/ overrides smart defaults for subdirs",
			relPath:  "_home/nvim/init.lua",
			expected: filepath.Join(testHome, ".nvim/init.lua"),
			reason:   "Layer 3 _home/ should override Layer 1 subdir->XDG rule",
		},
		{
			name:     "_xdg/ overrides smart defaults for top-level",
			relPath:  "_xdg/gitconfig",
			expected: filepath.Join(testHome, ".config/gitconfig"),
			reason:   "Layer 3 _xdg/ should override Layer 1 top-level->HOME rule",
		},
		{
			name:     "normal ssh still follows Layer 2",
			relPath:  "ssh/config",
			expected: filepath.Join(testHome, ".ssh/config"),
			reason:   "Without _home/ or _xdg/, Layer 2 exception should apply",
		},
		{
			name:     "normal nvim still follows Layer 1",
			relPath:  "nvim/init.lua",
			expected: filepath.Join(testHome, ".config/nvim/init.lua"),
			reason:   "Without _home/ or _xdg/, Layer 1 subdir rule should apply",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := p.MapPackFileToSystem(pack, tt.relPath)
			assert.Equal(t, tt.expected, result, tt.reason)
		})
	}
}

// TestExplicitOverrideHelpers tests the helper functions for Layer 3
func TestExplicitOverrideHelpers(t *testing.T) {
	tests := []struct {
		name         string
		path         string
		hasOverride  bool
		overrideType string
		stripped     string
	}{
		{
			name:         "home override",
			path:         "_home/config",
			hasOverride:  true,
			overrideType: "home",
			stripped:     "config",
		},
		{
			name:         "xdg override",
			path:         "_xdg/app/settings",
			hasOverride:  true,
			overrideType: "xdg",
			stripped:     "app/settings",
		},
		{
			name:         "no override",
			path:         "regular/path",
			hasOverride:  false,
			overrideType: "",
			stripped:     "regular/path",
		},
		{
			name:         "underscore but not override",
			path:         "_other/path",
			hasOverride:  false,
			overrideType: "",
			stripped:     "_other/path",
		},
		{
			name:         "home in middle of path",
			path:         "some/_home/path",
			hasOverride:  false,
			overrideType: "",
			stripped:     "some/_home/path",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			hasOverride, overrideType := hasExplicitOverride(tt.path)
			assert.Equal(t, tt.hasOverride, hasOverride)
			assert.Equal(t, tt.overrideType, overrideType)

			stripped := stripOverridePrefix(tt.path)
			assert.Equal(t, tt.stripped, stripped)
		})
	}
}

// TestLayer4CustomMappings tests the configuration file mapping behavior
func TestLayer4CustomMappings(t *testing.T) {
	// Save original environment
	originalHome := os.Getenv("HOME")
	originalXDG := os.Getenv("XDG_CONFIG_HOME")
	defer func() {
		_ = os.Setenv("HOME", originalHome)
		if originalXDG != "" {
			_ = os.Setenv("XDG_CONFIG_HOME", originalXDG)
		} else {
			_ = os.Unsetenv("XDG_CONFIG_HOME")
		}
	}()

	testHome := "/home/testuser"
	require.NoError(t, os.Setenv("HOME", testHome))
	require.NoError(t, os.Unsetenv("XDG_CONFIG_HOME"))

	// Initialize config to ensure we have the proper force_home exceptions
	config.Initialize(nil)

	p, err := New("")
	require.NoError(t, err)

	pack := &types.Pack{
		Name: "test",
		Path: "/dotfiles/test",
		Config: config.PackConfig{
			Mappings: map[string]string{
				"special/config": "$HOME/.special_location",
				"app.conf":       "$XDG_CONFIG_HOME/myapp/app.conf",
				"*.secret":       "$HOME/.secrets/",
				"data/*.json":    "$HOME/.local/share/myapp/",
			},
		},
	}

	tests := []struct {
		name     string
		relPath  string
		expected string
	}{
		{
			name:     "exact mapping with $HOME",
			relPath:  "special/config",
			expected: filepath.Join(testHome, ".special_location"),
		},
		{
			name:     "exact mapping with $XDG_CONFIG_HOME",
			relPath:  "app.conf",
			expected: filepath.Join(testHome, ".config/myapp/app.conf"),
		},
		{
			name:     "glob pattern *.secret",
			relPath:  "api.secret",
			expected: testHome + "/.secrets/",
		},
		{
			name:     "glob pattern with directory",
			relPath:  "data/settings.json",
			expected: testHome + "/.local/share/myapp/",
		},
		{
			name:     "no match falls through to lower layers",
			relPath:  "unmapped/file",
			expected: filepath.Join(testHome, ".config/unmapped/file"),
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := p.MapPackFileToSystem(pack, tt.relPath)
			assert.Equal(t, tt.expected, result)
		})
	}
}

// TestLayer4Precedence tests that Layer 4 takes precedence over all other layers
func TestLayer4Precedence(t *testing.T) {
	// Save original environment
	originalHome := os.Getenv("HOME")
	originalXDG := os.Getenv("XDG_CONFIG_HOME")
	defer func() {
		_ = os.Setenv("HOME", originalHome)
		if originalXDG != "" {
			_ = os.Setenv("XDG_CONFIG_HOME", originalXDG)
		} else {
			_ = os.Unsetenv("XDG_CONFIG_HOME")
		}
	}()

	testHome := "/home/testuser"
	require.NoError(t, os.Setenv("HOME", testHome))
	require.NoError(t, os.Unsetenv("XDG_CONFIG_HOME"))

	// Initialize config to ensure we have the proper force_home exceptions
	config.Initialize(nil)

	p, err := New("")
	require.NoError(t, err)

	pack := &types.Pack{
		Name: "test",
		Path: "/dotfiles/test",
		Config: config.PackConfig{
			Mappings: map[string]string{
				"ssh/config":   "$HOME/.ssh_custom/config",
				"_home/forced": "$XDG_CONFIG_HOME/not-home",
				"_xdg/forced":  "$HOME/.not-xdg",
				"gitconfig":    "$HOME/.config/git/config",
			},
		},
	}

	tests := []struct {
		name     string
		relPath  string
		expected string
		reason   string
	}{
		{
			name:     "Layer 4 overrides Layer 2 exception",
			relPath:  "ssh/config",
			expected: filepath.Join(testHome, ".ssh_custom/config"),
			reason:   "Custom mapping should override exception list",
		},
		{
			name:     "Layer 4 overrides Layer 3 _home",
			relPath:  "_home/forced",
			expected: filepath.Join(testHome, ".config/not-home"),
			reason:   "Custom mapping should override explicit _home/ directory",
		},
		{
			name:     "Layer 4 overrides Layer 3 _xdg",
			relPath:  "_xdg/forced",
			expected: filepath.Join(testHome, ".not-xdg"),
			reason:   "Custom mapping should override explicit _xdg/ directory",
		},
		{
			name:     "Layer 4 overrides Layer 1 top-level",
			relPath:  "gitconfig",
			expected: filepath.Join(testHome, ".config/git/config"),
			reason:   "Custom mapping should override smart default for top-level files",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := p.MapPackFileToSystem(pack, tt.relPath)
			assert.Equal(t, tt.expected, result, tt.reason)
		})
	}
}

// TestMappingHelpers tests the helper functions for Layer 4
func TestMappingHelpers(t *testing.T) {
	// Save original environment
	originalHome := os.Getenv("HOME")
	originalXDG := os.Getenv("XDG_CONFIG_HOME")
	defer func() {
		_ = os.Setenv("HOME", originalHome)
		if originalXDG != "" {
			_ = os.Setenv("XDG_CONFIG_HOME", originalXDG)
		} else {
			_ = os.Unsetenv("XDG_CONFIG_HOME")
		}
	}()

	testHome := "/home/testuser"
	require.NoError(t, os.Setenv("HOME", testHome))
	require.NoError(t, os.Setenv("XDG_CONFIG_HOME", "/custom/xdg"))

	t.Run("expandMapping", func(t *testing.T) {
		tests := []struct {
			name     string
			mapping  string
			expected string
		}{
			{
				name:     "expand $HOME",
				mapping:  "$HOME/.config/app",
				expected: "/home/testuser/.config/app",
			},
			{
				name:     "expand $XDG_CONFIG_HOME",
				mapping:  "$XDG_CONFIG_HOME/app",
				expected: "/custom/xdg/app",
			},
			{
				name:     "expand both variables",
				mapping:  "$HOME/data/$XDG_CONFIG_HOME/config",
				expected: "/home/testuser/data//custom/xdg/config",
			},
			{
				name:     "no variables",
				mapping:  "/absolute/path",
				expected: "/absolute/path",
			},
		}

		for _, tt := range tests {
			t.Run(tt.name, func(t *testing.T) {
				result := expandMapping(tt.mapping, testHome)
				assert.Equal(t, tt.expected, result)
			})
		}
	})

	t.Run("findMapping", func(t *testing.T) {
		mappings := map[string]string{
			"exact/match": "$HOME/.exact",
			"*.conf":      "$HOME/.config/",
			"src/*.go":    "$HOME/go/src/",
		}

		tests := []struct {
			name     string
			relPath  string
			expected string
		}{
			{
				name:     "exact match",
				relPath:  "exact/match",
				expected: "/home/testuser/.exact",
			},
			{
				name:     "glob match",
				relPath:  "app.conf",
				expected: "/home/testuser/.config/",
			},
			{
				name:     "glob with directory",
				relPath:  "src/main.go",
				expected: "/home/testuser/go/src/",
			},
			{
				name:     "no match",
				relPath:  "no/match/here",
				expected: "",
			},
		}

		for _, tt := range tests {
			t.Run(tt.name, func(t *testing.T) {
				result := findMapping(tt.relPath, mappings, testHome)
				assert.Equal(t, tt.expected, result)
			})
		}
	})
}
