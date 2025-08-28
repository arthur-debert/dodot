package paths

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil_old"
)

// TestPathSecurityValidation tests path security with filesystem operations
// This is an integration test because it creates temp directories
func TestPathSecurityValidation(t *testing.T) {
	// Create a temporary directory structure for testing
	tmpRoot := testutil.TempDir(t, "path-security")
	dotfilesDir := filepath.Join(tmpRoot, "dotfiles")
	testutil.CreateDir(t, tmpRoot, "dotfiles")
	testutil.CreateDir(t, tmpRoot, "outside")

	p, err := New(dotfilesDir)
	testutil.AssertNoError(t, err)

	tests := []struct {
		name           string
		path           string
		shouldBeInside bool
		description    string
	}{
		{
			name:           "file inside dotfiles",
			path:           filepath.Join(dotfilesDir, "vim", "vimrc"),
			shouldBeInside: true,
			description:    "Files inside dotfiles should be detected",
		},
		{
			name:           "dotfiles root itself",
			path:           dotfilesDir,
			shouldBeInside: true,
			description:    "Dotfiles root should be considered inside",
		},
		{
			name:           "file outside dotfiles",
			path:           filepath.Join(tmpRoot, "outside", "file.txt"),
			shouldBeInside: false,
			description:    "Files outside dotfiles should be detected",
		},
		{
			name:           "path traversal attempt",
			path:           filepath.Join(dotfilesDir, "..", "outside", "file.txt"),
			shouldBeInside: false,
			description:    "Path traversal should be detected",
		},
		{
			name:           "symlink target outside (path only)",
			path:           "/etc/passwd",
			shouldBeInside: false,
			description:    "System files should be outside dotfiles",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			isInside, err := p.IsInDotfiles(tt.path)
			testutil.AssertNoError(t, err)
			testutil.AssertEqual(t, tt.shouldBeInside, isInside)
		})
	}
}

// TestEnvironmentVariableHandling tests environment variable handling
// This is an integration test because it manipulates OS environment
func TestEnvironmentVariableHandling(t *testing.T) {
	// Save original environment
	origDotfilesRoot := os.Getenv(EnvDotfilesRoot)
	origDodotDataDir := os.Getenv(EnvDodotDataDir)
	origDodotConfigDir := os.Getenv(EnvDodotConfigDir)
	origDodotCacheDir := os.Getenv(EnvDodotCacheDir)
	origXdgStateHome := os.Getenv("XDG_STATE_HOME")

	t.Cleanup(func() {
		_ = os.Setenv(EnvDotfilesRoot, origDotfilesRoot)
		_ = os.Setenv(EnvDodotDataDir, origDodotDataDir)
		_ = os.Setenv(EnvDodotConfigDir, origDodotConfigDir)
		_ = os.Setenv(EnvDodotCacheDir, origDodotCacheDir)
		_ = os.Setenv("XDG_STATE_HOME", origXdgStateHome)
	})

	tests := []struct {
		name     string
		envSetup map[string]string
		validate func(t *testing.T, p Paths)
	}{
		{
			name: "DOTFILES_ROOT with spaces",
			envSetup: map[string]string{
				EnvDotfilesRoot: "/path with spaces/dotfiles",
			},
			validate: func(t *testing.T, p Paths) {
				testutil.AssertEqual(t, "/path with spaces/dotfiles", p.DotfilesRoot())
			},
		},
		{
			name: "DOTFILES_ROOT with tilde",
			envSetup: map[string]string{
				EnvDotfilesRoot: "~/my-dotfiles",
			},
			validate: func(t *testing.T, p Paths) {
				homeDir, _ := os.UserHomeDir()
				expected := filepath.Join(homeDir, "my-dotfiles")
				testutil.AssertEqual(t, expected, p.DotfilesRoot())
			},
		},
		{
			name: "Custom XDG_STATE_HOME",
			envSetup: map[string]string{
				"XDG_STATE_HOME": "/custom/state",
			},
			validate: func(t *testing.T, p Paths) {
				expected := filepath.Join("/custom/state", "dodot", "dodot.log")
				testutil.AssertEqual(t, expected, p.LogFilePath())
			},
		},
		{
			name: "All custom directories",
			envSetup: map[string]string{
				EnvDodotDataDir:   "/custom/data/dodot",
				EnvDodotConfigDir: "/custom/config/dodot",
				EnvDodotCacheDir:  "/custom/cache/dodot",
			},
			validate: func(t *testing.T, p Paths) {
				testutil.AssertEqual(t, "/custom/data/dodot", p.DataDir())
				testutil.AssertEqual(t, "/custom/config/dodot", p.ConfigDir())
				testutil.AssertEqual(t, "/custom/cache/dodot", p.CacheDir())
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Clear all relevant env vars
			_ = os.Unsetenv(EnvDotfilesRoot)
			_ = os.Unsetenv(EnvDodotDataDir)
			_ = os.Unsetenv(EnvDodotConfigDir)
			_ = os.Unsetenv(EnvDodotCacheDir)
			_ = os.Unsetenv("XDG_STATE_HOME")

			// Set up test environment
			for k, v := range tt.envSetup {
				_ = os.Setenv(k, v)
			}

			p, err := New("")
			testutil.AssertNoError(t, err)

			if tt.validate != nil {
				tt.validate(t, p)
			}
		})
	}
}
