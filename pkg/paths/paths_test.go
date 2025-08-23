package paths

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
)

func TestNew(t *testing.T) {
	tests := []struct {
		name         string
		dotfilesRoot string
		envSetup     map[string]string
		validate     func(t *testing.T, p Paths)
		wantErr      bool
	}{
		{
			name:         "explicit dotfiles root",
			dotfilesRoot: "/tmp/dotfiles",
			validate: func(t *testing.T, p Paths) {
				testutil.AssertEqual(t, "/tmp/dotfiles", p.DotfilesRoot())
			},
		},
		{
			name: "from DOTFILES_ROOT env",
			envSetup: map[string]string{
				EnvDotfilesRoot: "/env/dotfiles",
			},
			validate: func(t *testing.T, p Paths) {
				testutil.AssertEqual(t, "/env/dotfiles", p.DotfilesRoot())
			},
		},
		{
			name: "git repository or fallback",
			validate: func(t *testing.T, p Paths) {
				// This test will either find the git root if we're in a git repo,
				// or fall back to the current directory
				testutil.AssertNotEmpty(t, p.DotfilesRoot())
				// The path should be absolute
				testutil.AssertTrue(t, filepath.IsAbs(p.DotfilesRoot()), "Path should be absolute")
			},
		},
		{
			name:         "expand tilde in explicit path",
			dotfilesRoot: "~/my-dotfiles",
			validate: func(t *testing.T, p Paths) {
				homeDir, _ := os.UserHomeDir()
				expected := filepath.Join(homeDir, "my-dotfiles")
				testutil.AssertEqual(t, expected, p.DotfilesRoot())
			},
		},
		{
			name: "custom XDG directories",
			envSetup: map[string]string{
				EnvDodotDataDir:   "/custom/data",
				EnvDodotConfigDir: "/custom/config",
				EnvDodotCacheDir:  "/custom/cache",
			},
			validate: func(t *testing.T, p Paths) {
				testutil.AssertEqual(t, "/custom/data", p.DataDir())
				testutil.AssertEqual(t, "/custom/config", p.ConfigDir())
				testutil.AssertEqual(t, "/custom/cache", p.CacheDir())
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Clear relevant environment variables first
			t.Setenv(EnvDotfilesRoot, "")
			t.Setenv(EnvDodotDataDir, "")
			t.Setenv(EnvDodotConfigDir, "")
			t.Setenv(EnvDodotCacheDir, "")

			// Set up environment
			for k, v := range tt.envSetup {
				t.Setenv(k, v)
			}

			p, err := New(tt.dotfilesRoot)

			if tt.wantErr {
				testutil.AssertError(t, err)
				return
			}

			testutil.AssertNoError(t, err)
			testutil.AssertNotNil(t, p)

			if tt.validate != nil {
				tt.validate(t, p)
			}
		})
	}
}

func TestPackPaths(t *testing.T) {
	p, err := New("/test/dotfiles")
	testutil.AssertNoError(t, err)

	tests := []struct {
		name     string
		packName string
		method   func(string) string
		expected string
	}{
		{
			name:     "pack path",
			packName: "mypack",
			method:   p.PackPath,
			expected: "/test/dotfiles/mypack",
		},
		{
			name:     "pack config path",
			packName: "mypack",
			method:   p.PackConfigPath,
			expected: "/test/dotfiles/mypack/.dodot.toml",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := tt.method(tt.packName)
			testutil.AssertEqual(t, tt.expected, result)
		})
	}
}

func TestStatePaths(t *testing.T) {
	p, err := New("/test/dotfiles")
	testutil.AssertNoError(t, err)

	// Test state directory structure
	stateDir := p.StateDir()
	testutil.AssertTrue(t, strings.HasPrefix(stateDir, p.DataDir()), "StateDir should be under DataDir")

	// Test state file path
	statePath := p.StatePath("mypack", "provision")
	expected := filepath.Join(p.StateDir(), "mypack", "provision.json")
	testutil.AssertEqual(t, expected, statePath)
}

func TestExpandHome(t *testing.T) {
	homeDir, err := os.UserHomeDir()
	testutil.AssertNoError(t, err)

	tests := []struct {
		name     string
		input    string
		expected string
	}{
		{
			name:     "empty string",
			input:    "",
			expected: "",
		},
		{
			name:     "just tilde",
			input:    "~",
			expected: homeDir,
		},
		{
			name:     "tilde with path",
			input:    "~/dotfiles",
			expected: filepath.Join(homeDir, "dotfiles"),
		},
		{
			name:     "tilde other user",
			input:    "~other/path",
			expected: "~other/path", // Not expanded
		},
		{
			name:     "no tilde",
			input:    "/absolute/path",
			expected: "/absolute/path",
		},
		{
			name:     "relative path",
			input:    "relative/path",
			expected: "relative/path",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := ExpandHome(tt.input)
			testutil.AssertEqual(t, tt.expected, result)
		})
	}
}

func TestNormalizePath(t *testing.T) {
	p, err := New("/test/dotfiles")
	testutil.AssertNoError(t, err)

	homeDir, _ := os.UserHomeDir()

	tests := []struct {
		name     string
		input    string
		wantErr  bool
		validate func(t *testing.T, result string)
	}{
		{
			name:    "empty path",
			input:   "",
			wantErr: true,
		},
		{
			name:  "absolute path",
			input: "/absolute/path",
			validate: func(t *testing.T, result string) {
				testutil.AssertEqual(t, "/absolute/path", result)
			},
		},
		{
			name:  "relative path",
			input: "relative/path",
			validate: func(t *testing.T, result string) {
				// Should be made absolute
				testutil.AssertTrue(t, filepath.IsAbs(result), "Path should be absolute")
				testutil.AssertTrue(t, strings.HasSuffix(result, filepath.Join("relative", "path")), "Should end with original path")
			},
		},
		{
			name:  "path with tilde",
			input: "~/my/path",
			validate: func(t *testing.T, result string) {
				expected := filepath.Join(homeDir, "my/path")
				testutil.AssertEqual(t, expected, result)
			},
		},
		{
			name:  "path with dots",
			input: "/path/../other",
			validate: func(t *testing.T, result string) {
				testutil.AssertEqual(t, "/other", result)
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, err := p.NormalizePath(tt.input)

			if tt.wantErr {
				testutil.AssertError(t, err)
				return
			}

			testutil.AssertNoError(t, err)
			if tt.validate != nil {
				tt.validate(t, result)
			}
		})
	}
}

func TestIsInDotfiles(t *testing.T) {
	p, err := New("/test/dotfiles")
	testutil.AssertNoError(t, err)

	tests := []struct {
		name     string
		path     string
		expected bool
		wantErr  bool
	}{
		{
			name:     "inside dotfiles",
			path:     "/test/dotfiles/pack/file",
			expected: true,
		},
		{
			name:     "dotfiles root itself",
			path:     "/test/dotfiles",
			expected: true,
		},
		{
			name:     "outside dotfiles",
			path:     "/other/path",
			expected: false,
		},
		{
			name:     "parent of dotfiles",
			path:     "/test",
			expected: false,
		},
		{
			name:     "relative path inside",
			path:     "/test/dotfiles/../dotfiles/pack",
			expected: true,
		},
		{
			name:    "empty path",
			path:    "",
			wantErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, err := p.IsInDotfiles(tt.path)

			if tt.wantErr {
				testutil.AssertError(t, err)
				return
			}

			testutil.AssertNoError(t, err)
			testutil.AssertEqual(t, tt.expected, result)
		})
	}
}
