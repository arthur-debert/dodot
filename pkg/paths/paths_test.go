// Test Type: Unit Test
// Description: Tests for the paths package - main Paths struct and functions

package paths_test

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestNew(t *testing.T) {
	tests := []struct {
		name         string
		dotfilesRoot string
		setup        func()
		cleanup      func()
		expectError  bool
		errorCode    errors.ErrorCode
		verify       func(t *testing.T, p paths.Paths)
	}{
		{
			name:         "explicit_dotfiles_root",
			dotfilesRoot: "/tmp/test-dotfiles",
			setup: func() {
				_ = os.MkdirAll("/tmp/test-dotfiles", 0755)
			},
			cleanup: func() {
				_ = os.RemoveAll("/tmp/test-dotfiles")
			},
			expectError: false,
			verify: func(t *testing.T, p paths.Paths) {
				assert.Equal(t, "/tmp/test-dotfiles", p.DotfilesRoot())
				assert.False(t, p.UsedFallback())
			},
		},
		{
			name:         "empty_path_uses_auto_detection",
			dotfilesRoot: "",
			setup:        func() {},
			cleanup:      func() {},
			expectError:  false,
			verify: func(t *testing.T, p paths.Paths) {
				// Just verify it doesn't error and has some dotfiles root
				assert.NotEmpty(t, p.DotfilesRoot())
			},
		},
		{
			name:         "tilde_home_expansion",
			dotfilesRoot: "~/dotfiles",
			setup:        func() {},
			cleanup:      func() {},
			expectError:  false,
			verify: func(t *testing.T, p paths.Paths) {
				assert.NotContains(t, p.DotfilesRoot(), "~")
				assert.Contains(t, p.DotfilesRoot(), "dotfiles")
			},
		},
		{
			name:         "invalid_dotfiles_root",
			dotfilesRoot: "/nonexistent/path/that/should/not/exist/ever",
			setup:        func() {},
			cleanup:      func() {},
			expectError:  true,
			errorCode:    errors.ErrNotFound,
		},
		{
			name:         "file_as_dotfiles_root",
			dotfilesRoot: "/tmp/test-file",
			setup: func() {
				f, _ := os.Create("/tmp/test-file")
				_ = f.Close()
			},
			cleanup: func() {
				_ = os.Remove("/tmp/test-file")
			},
			expectError: true,
			errorCode:   errors.ErrInvalidInput,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if tt.setup != nil {
				tt.setup()
			}
			if tt.cleanup != nil {
				defer tt.cleanup()
			}

			p, err := paths.New(tt.dotfilesRoot)

			if tt.expectError {
				assert.Error(t, err)
				if terr, ok := err.(*errors.DodotError); ok {
					assert.Equal(t, tt.errorCode, terr.Code)
				}
			} else {
				assert.NoError(t, err)
				assert.NotNil(t, p)
				if tt.verify != nil {
					tt.verify(t, p)
				}
			}
		})
	}
}

func TestPathsGetters(t *testing.T) {
	// Create a test dotfiles directory
	tmpDir := t.TempDir()
	dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
	require.NoError(t, os.MkdirAll(dotfilesRoot, 0755))

	p, err := paths.New(dotfilesRoot)
	require.NoError(t, err)

	t.Run("DotfilesRoot", func(t *testing.T) {
		assert.Equal(t, dotfilesRoot, p.DotfilesRoot())
	})

	t.Run("PackPath", func(t *testing.T) {
		assert.Equal(t, filepath.Join(dotfilesRoot, "vim"), p.PackPath("vim"))
		assert.Equal(t, filepath.Join(dotfilesRoot, "bash"), p.PackPath("bash"))
	})

	t.Run("PackConfigPath", func(t *testing.T) {
		assert.Equal(t, filepath.Join(dotfilesRoot, "vim", ".dodot.toml"), p.PackConfigPath("vim"))
	})

	t.Run("DataDir", func(t *testing.T) {
		dataDir := p.DataDir()
		assert.NotEmpty(t, dataDir)
		assert.Contains(t, dataDir, "dodot")
	})

	t.Run("ConfigDir", func(t *testing.T) {
		configDir := p.ConfigDir()
		assert.NotEmpty(t, configDir)
		assert.Contains(t, configDir, "dodot")
	})

	t.Run("CacheDir", func(t *testing.T) {
		cacheDir := p.CacheDir()
		assert.NotEmpty(t, cacheDir)
		assert.Contains(t, cacheDir, "dodot")
	})

	t.Run("StateDir", func(t *testing.T) {
		stateDir := p.StateDir()
		assert.Equal(t, filepath.Join(p.DataDir(), "state"), stateDir)
	})

	t.Run("TemplatesDir", func(t *testing.T) {
		templatesDir := p.TemplatesDir()
		assert.Equal(t, filepath.Join(p.DataDir(), "templates"), templatesDir)
	})

	t.Run("StatePath", func(t *testing.T) {
		statePath := p.StatePath("vim", "install")
		assert.Contains(t, statePath, "vim")
		assert.Contains(t, statePath, "install.json")
		assert.Contains(t, statePath, "state")
	})

	t.Run("DeployedDir", func(t *testing.T) {
		deployedDir := p.DeployedDir()
		assert.Equal(t, filepath.Join(p.DataDir(), "deployed"), deployedDir)
	})

	t.Run("ShellProfileDir", func(t *testing.T) {
		profileDir := p.ShellProfileDir()
		assert.Equal(t, filepath.Join(p.DeployedDir(), "shell_profiles"), profileDir)
	})

	t.Run("PathDir", func(t *testing.T) {
		pathDir := p.PathDir()
		assert.Equal(t, filepath.Join(p.DeployedDir(), "path"), pathDir)
	})

	t.Run("ShellSourceDir", func(t *testing.T) {
		sourceDir := p.ShellSourceDir()
		assert.Equal(t, filepath.Join(p.DeployedDir(), "shell_source"), sourceDir)
	})

	t.Run("SymlinkDir", func(t *testing.T) {
		symlinkDir := p.SymlinkDir()
		assert.Equal(t, filepath.Join(p.DeployedDir(), "symlink"), symlinkDir)
	})

	t.Run("ShellDir", func(t *testing.T) {
		shellDir := p.ShellDir()
		assert.Equal(t, filepath.Join(p.DeployedDir(), "shell"), shellDir)
	})

	t.Run("InitScriptPath", func(t *testing.T) {
		initPath := p.InitScriptPath()
		assert.Equal(t, filepath.Join(p.DeployedDir(), "init.sh"), initPath)
	})

	t.Run("ProvisionDir", func(t *testing.T) {
		provisionDir := p.ProvisionDir()
		assert.Equal(t, filepath.Join(p.DeployedDir(), "provision"), provisionDir)
	})

	t.Run("HomebrewDir", func(t *testing.T) {
		homebrewDir := p.HomebrewDir()
		assert.Equal(t, filepath.Join(p.DeployedDir(), "homebrew"), homebrewDir)
	})

	t.Run("SentinelPath", func(t *testing.T) {
		sentinelPath := p.SentinelPath("install", "vim")
		assert.Contains(t, sentinelPath, "vim")
		assert.Contains(t, sentinelPath, "install")
	})

	t.Run("LogFilePath", func(t *testing.T) {
		logPath := p.LogFilePath()
		assert.Contains(t, logPath, "dodot.log")
	})

	t.Run("PackHandlerDir", func(t *testing.T) {
		handlerDir := p.PackHandlerDir("vim", "install")
		assert.Contains(t, handlerDir, "vim")
		assert.Contains(t, handlerDir, "install")
		assert.Contains(t, handlerDir, "deployed")
	})
}

func TestNormalizePath(t *testing.T) {
	tmpDir := t.TempDir()
	dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
	require.NoError(t, os.MkdirAll(dotfilesRoot, 0755))

	p, err := paths.New(dotfilesRoot)
	require.NoError(t, err)

	tests := []struct {
		name        string
		path        string
		expected    string
		expectError bool
	}{
		{
			name:     "absolute_path",
			path:     "/home/user/file.txt",
			expected: "/home/user/file.txt",
		},
		{
			name:     "tilde_expansion",
			path:     "~/dotfiles/vim",
			expected: filepath.Join(os.Getenv("HOME"), "dotfiles/vim"),
		},
		{
			name:     "clean_path",
			path:     "/home/user/../user/./dotfiles",
			expected: "/home/user/dotfiles",
		},
		{
			name:        "invalid_path",
			path:        "",
			expectError: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, err := p.NormalizePath(tt.path)

			if tt.expectError {
				assert.Error(t, err)
			} else {
				assert.NoError(t, err)
				assert.Equal(t, tt.expected, result)
			}
		})
	}
}

func TestIsInDotfiles(t *testing.T) {
	tmpDir := t.TempDir()
	dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
	require.NoError(t, os.MkdirAll(dotfilesRoot, 0755))

	p, err := paths.New(dotfilesRoot)
	require.NoError(t, err)

	tests := []struct {
		name        string
		path        string
		expected    bool
		expectError bool
	}{
		{
			name:     "file_in_dotfiles",
			path:     filepath.Join(dotfilesRoot, "vim", "vimrc"),
			expected: true,
		},
		{
			name:     "dotfiles_root_itself",
			path:     dotfilesRoot,
			expected: true,
		},
		{
			name:     "file_outside_dotfiles",
			path:     "/etc/passwd",
			expected: false,
		},
		{
			name:     "relative_path_in_dotfiles",
			path:     "vim/vimrc",
			expected: false, // Relative paths are not resolved
		},
		{
			name:        "invalid_path",
			path:        "",
			expectError: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, err := p.IsInDotfiles(tt.path)

			if tt.expectError {
				assert.Error(t, err)
			} else {
				assert.NoError(t, err)
				assert.Equal(t, tt.expected, result)
			}
		})
	}
}

func TestExpandHome(t *testing.T) {
	homeDir := os.Getenv("HOME")
	if homeDir == "" {
		homeDir = os.Getenv("USERPROFILE") // Windows
	}

	tests := []struct {
		name     string
		path     string
		expected string
	}{
		{
			name:     "tilde_at_start",
			path:     "~/dotfiles",
			expected: filepath.Join(homeDir, "dotfiles"),
		},
		{
			name:     "no_tilde",
			path:     "/home/user/dotfiles",
			expected: "/home/user/dotfiles",
		},
		{
			name:     "tilde_not_at_start",
			path:     "/home/~user/dotfiles",
			expected: "/home/~user/dotfiles",
		},
		{
			name:     "just_tilde",
			path:     "~",
			expected: homeDir,
		},
		{
			name:     "empty_path",
			path:     "",
			expected: "",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := paths.ExpandHome(tt.path)
			assert.Equal(t, tt.expected, result)
		})
	}
}
