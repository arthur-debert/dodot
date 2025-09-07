package symlink

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/operations"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestComputeTargetPath_ForceHome(t *testing.T) {
	// Set up HOME for consistent testing
	originalHome := os.Getenv("HOME")
	testHome := "/test/home"
	require.NoError(t, os.Setenv("HOME", testHome))
	defer func() {
		require.NoError(t, os.Setenv("HOME", originalHome))
	}()

	// Also set XDG_CONFIG_HOME for testing
	originalXDG := os.Getenv("XDG_CONFIG_HOME")
	require.NoError(t, os.Setenv("XDG_CONFIG_HOME", filepath.Join(testHome, ".config")))
	defer func() {
		require.NoError(t, os.Setenv("XDG_CONFIG_HOME", originalXDG))
	}()

	handler := &Handler{}

	// Config with force_home patterns
	cfg := &config.Config{
		LinkPaths: config.LinkPaths{
			CoreUnixExceptions: map[string]bool{
				"ssh":       true,
				"gitconfig": true,
				"bashrc":    true,
				"profile":   true,
				"aws":       true,
			},
		},
	}

	tests := []struct {
		name     string
		file     operations.FileInput
		expected string
		reason   string
	}{
		// Layer 3: Explicit overrides (highest priority)
		{
			name:     "explicit_home_override",
			file:     operations.FileInput{RelativePath: "_home/myconfig"},
			expected: filepath.Join(testHome, ".myconfig"),
			reason:   "_home/ prefix should force to $HOME with dot prefix",
		},
		{
			name:     "explicit_xdg_override",
			file:     operations.FileInput{RelativePath: "_xdg/myapp/config"},
			expected: filepath.Join(testHome, ".config/myapp/config"),
			reason:   "_xdg/ prefix should force to XDG_CONFIG_HOME",
		},

		// Layer 2: Force home configuration
		{
			name:     "force_home_ssh_config",
			file:     operations.FileInput{RelativePath: "ssh/config"},
			expected: filepath.Join(testHome, ".ssh/config"),
			reason:   "ssh is in force_home, should go to ~/.ssh/config",
		},
		{
			name:     "force_home_gitconfig",
			file:     operations.FileInput{RelativePath: "gitconfig"},
			expected: filepath.Join(testHome, ".gitconfig"),
			reason:   "gitconfig is in force_home, should go to ~/.gitconfig",
		},
		{
			name:     "force_home_bashrc",
			file:     operations.FileInput{RelativePath: "bashrc"},
			expected: filepath.Join(testHome, ".bashrc"),
			reason:   "bashrc is in force_home, should go to ~/.bashrc",
		},
		{
			name:     "force_home_aws_credentials",
			file:     operations.FileInput{RelativePath: "aws/credentials"},
			expected: filepath.Join(testHome, ".aws/credentials"),
			reason:   "aws is in force_home, should go to ~/.aws/credentials",
		},

		// Layer 1: Smart defaults
		{
			name:     "default_top_level_file",
			file:     operations.FileInput{RelativePath: "vimrc"},
			expected: filepath.Join(testHome, ".vimrc"),
			reason:   "top-level files default to $HOME with dot prefix",
		},
		{
			name:     "default_subdirectory_file",
			file:     operations.FileInput{RelativePath: "nvim/init.vim"},
			expected: filepath.Join(testHome, ".config/nvim/init.vim"),
			reason:   "subdirectory files default to XDG_CONFIG_HOME",
		},
		{
			name:     "strip_config_prefix",
			file:     operations.FileInput{RelativePath: "config/myapp/settings"},
			expected: filepath.Join(testHome, ".config/myapp/settings"),
			reason:   "config/ prefix should be stripped to avoid duplication",
		},
		{
			name:     "strip_dotconfig_prefix",
			file:     operations.FileInput{RelativePath: ".config/myapp/settings"},
			expected: filepath.Join(testHome, ".config/myapp/settings"),
			reason:   ".config/ prefix should be stripped to avoid duplication",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := handler.computeTargetPath(testHome, tt.file, cfg)
			assert.Equal(t, tt.expected, result, tt.reason)
		})
	}
}

func TestIsForceHome(t *testing.T) {
	cfg := &config.Config{
		LinkPaths: config.LinkPaths{
			CoreUnixExceptions: map[string]bool{
				"ssh":       true,
				"gitconfig": true,
				"bashrc":    true,
				"aws":       true,
			},
		},
	}

	tests := []struct {
		path     string
		expected bool
		reason   string
	}{
		// Exact matches
		{"ssh", true, "exact match"},
		{"gitconfig", true, "exact match"},
		{"bashrc", true, "exact match"},

		// With dot prefix
		{".ssh", true, "match with dot prefix"},
		{".gitconfig", true, "match with dot prefix"},
		{".bashrc", true, "match with dot prefix"},

		// Subdirectory matches
		{"ssh/config", true, "subdirectory of force_home pattern"},
		{"ssh/known_hosts", true, "subdirectory of force_home pattern"},
		{".ssh/config", true, "subdirectory with dot prefix"},
		{"aws/credentials", true, "aws subdirectory"},
		{"aws/config", true, "aws subdirectory"},

		// Non-matches
		{"vimrc", false, "not in force_home list"},
		{"nvim/init.vim", false, "not in force_home list"},
		{"sshconfig", false, "similar name but not exact match"},
		{"myssh/config", false, "different directory name"},
	}

	for _, tt := range tests {
		t.Run(tt.path, func(t *testing.T) {
			result := isForceHome(tt.path, cfg)
			assert.Equal(t, tt.expected, result, tt.reason)
		})
	}
}

func TestHandler_ForceHome_Integration(t *testing.T) {
	// Set up HOME
	originalHome := os.Getenv("HOME")
	testHome := "/test/home"
	require.NoError(t, os.Setenv("HOME", testHome))
	defer func() {
		require.NoError(t, os.Setenv("HOME", originalHome))
	}()

	// Also set XDG_CONFIG_HOME for consistent testing
	originalXDG := os.Getenv("XDG_CONFIG_HOME")
	require.NoError(t, os.Setenv("XDG_CONFIG_HOME", filepath.Join(testHome, ".config")))
	defer func() {
		require.NoError(t, os.Setenv("XDG_CONFIG_HOME", originalXDG))
	}()

	handler := NewHandler()

	cfg := &config.Config{
		LinkPaths: config.LinkPaths{
			CoreUnixExceptions: map[string]bool{
				"ssh": true,
			},
		},
	}

	files := []operations.FileInput{
		{
			RelativePath: "ssh/config",
			PackName:     "test-pack",
			SourcePath:   "/source/ssh/config",
		},
		{
			RelativePath: "nvim/init.vim",
			PackName:     "test-pack",
			SourcePath:   "/source/nvim/init.vim",
		},
	}

	ops, err := handler.ToOperations(files, cfg)
	require.NoError(t, err)
	require.Len(t, ops, 4) // 2 operations per file

	// Check the target paths
	// ssh/config should go to ~/.ssh/config due to force_home
	assert.Equal(t, filepath.Join(testHome, ".ssh/config"), ops[1].Target)

	// nvim/init.vim should go to XDG_CONFIG_HOME (default behavior)
	assert.Equal(t, filepath.Join(testHome, ".config/nvim/init.vim"), ops[3].Target)
}

func TestHandler_ForceHome_PackLevel(t *testing.T) {
	// Test that pack-level force_home would work if we had pack config
	// This is a placeholder test documenting expected behavior

	originalHome := os.Getenv("HOME")
	testHome := "/test/home"
	require.NoError(t, os.Setenv("HOME", testHome))
	defer func() {
		require.NoError(t, os.Setenv("HOME", originalHome))
	}()

	handler := NewHandler()

	// Root config with ssh as force_home
	// rootCfg := &config.Config{
	// 	LinkPaths: config.LinkPaths{
	// 		CoreUnixExceptions: map[string]bool{
	// 			"ssh": true,
	// 		},
	// 	},
	// }

	// Simulate pack config that would add "myapp" to force_home
	// Note: This would require passing pack config separately or merging it
	packCfg := &config.Config{
		LinkPaths: config.LinkPaths{
			CoreUnixExceptions: map[string]bool{
				"ssh":   true, // inherited from root
				"myapp": true, // pack-specific addition
			},
		},
	}

	files := []operations.FileInput{
		{
			RelativePath: "myapp/config.json",
			PackName:     "test-pack",
			SourcePath:   "/source/myapp/config.json",
		},
	}

	// With pack config, myapp should be forced to home
	ops, err := handler.ToOperations(files, packCfg)
	require.NoError(t, err)

	// myapp/config.json should go to ~/.myapp/config.json due to pack-level force_home
	assert.Equal(t, filepath.Join(testHome, ".myapp/config.json"), ops[1].Target)
}
