package paths

import (
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
)

func TestPathsIntegrationWithRealFileSystem(t *testing.T) {
	// Create a temporary directory structure that mimics a real dodot setup
	tmpRoot := testutil.TempDir(t, "paths-integration")

	// Set up directory structure
	dotfilesDir := filepath.Join(tmpRoot, "dotfiles")
	testutil.CreateDir(t, tmpRoot, "dotfiles")
	testutil.CreateDir(t, dotfilesDir, "vim")
	testutil.CreateDir(t, dotfilesDir, "git")
	testutil.CreateDir(t, dotfilesDir, "shell")

	// Create some files
	testutil.CreateFile(t, filepath.Join(dotfilesDir, "vim"), ".vimrc", "\" vim config")
	testutil.CreateFile(t, filepath.Join(dotfilesDir, "git"), ".gitconfig", "[user]\n\tname = Test")
	testutil.CreateFile(t, filepath.Join(dotfilesDir, "shell"), "aliases.sh", "alias ll='ls -la'")

	// Create pack config files
	testutil.CreateFile(t, filepath.Join(dotfilesDir, "vim"), ".dodot.toml", "")
	testutil.CreateFile(t, filepath.Join(dotfilesDir, "git"), ".dodot.toml", "")

	// Initialize paths with our test directory
	p, err := New(dotfilesDir)
	testutil.AssertNoError(t, err)

	t.Run("verify dotfiles root", func(t *testing.T) {
		testutil.AssertEqual(t, dotfilesDir, p.DotfilesRoot())
	})

	t.Run("pack paths exist", func(t *testing.T) {
		vimPath := p.PackPath("vim")
		gitPath := p.PackPath("git")

		testutil.AssertTrue(t, testutil.DirExists(t, vimPath), "vim pack should exist")
		testutil.AssertTrue(t, testutil.DirExists(t, gitPath), "git pack should exist")
	})

	t.Run("pack config paths", func(t *testing.T) {
		vimConfig := p.PackConfigPath("vim")
		testutil.AssertTrue(t, testutil.FileExists(t, vimConfig), "vim pack config should exist")
		testutil.AssertEqual(t, filepath.Join(dotfilesDir, "vim", ".dodot.toml"), vimConfig)
	})

	t.Run("file inside dotfiles detection", func(t *testing.T) {
		testCases := []struct {
			path     string
			expected bool
		}{
			{filepath.Join(dotfilesDir, "vim", ".vimrc"), true},
			{filepath.Join(dotfilesDir, "git", ".gitconfig"), true},
			{"/etc/passwd", false},
			{filepath.Join(tmpRoot, "outside.txt"), false},
		}

		for _, tc := range testCases {
			isInside, err := p.IsInDotfiles(tc.path)
			testutil.AssertNoError(t, err)
			testutil.AssertEqual(t, tc.expected, isInside,
				"IsInDotfiles(%s) should be %v", tc.path, tc.expected)
		}
	})
}

func TestXDGDirectoryCreation(t *testing.T) {
	// This test verifies that XDG directories would be created properly
	tmpRoot := testutil.TempDir(t, "xdg-test")

	// Set custom XDG directories
	t.Setenv(EnvDodotDataDir, filepath.Join(tmpRoot, "data"))
	t.Setenv(EnvDodotConfigDir, filepath.Join(tmpRoot, "config"))
	t.Setenv(EnvDodotCacheDir, filepath.Join(tmpRoot, "cache"))
	t.Setenv("XDG_STATE_HOME", filepath.Join(tmpRoot, "state"))

	p, err := New("")
	testutil.AssertNoError(t, err)

	// Verify paths are set correctly
	testutil.AssertEqual(t, filepath.Join(tmpRoot, "data"), p.DataDir())
	testutil.AssertEqual(t, filepath.Join(tmpRoot, "config"), p.ConfigDir())
	testutil.AssertEqual(t, filepath.Join(tmpRoot, "cache"), p.CacheDir())
	testutil.AssertEqual(t, filepath.Join(tmpRoot, "state", "dodot", "dodot.log"), p.LogFilePath())

	// Verify subdirectories
	testutil.AssertEqual(t, filepath.Join(tmpRoot, "data", "state"), p.StateDir())
	testutil.AssertEqual(t, filepath.Join(tmpRoot, "data", "backups"), p.BackupsDir())
	testutil.AssertEqual(t, filepath.Join(tmpRoot, "data", "deployed"), p.DeployedDir())
}

func TestMigrationFromOldPaths(t *testing.T) {
	// This test ensures backward compatibility during migration
	tmpRoot := testutil.TempDir(t, "migration-test")

	// Simulate old environment setup
	oldDataDir := filepath.Join(tmpRoot, "old-data")
	t.Setenv("DODOT_DATA_DIR", oldDataDir)

	// Test that paths API works with environment variables
	t.Run("paths API respects environment", func(t *testing.T) {
		p, err := New("")
		testutil.AssertNoError(t, err)

		dataDir := p.DataDir()
		testutil.AssertEqual(t, oldDataDir, dataDir)

		deployedDir := p.DeployedDir()
		testutil.AssertEqual(t, filepath.Join(oldDataDir, "deployed"), deployedDir)

		homebrewDir := p.HomebrewDir()
		testutil.AssertEqual(t, filepath.Join(oldDataDir, "homebrew"), homebrewDir)
	})

	// Test that new API gives consistent results
	t.Run("new API consistency", func(t *testing.T) {
		p1, err := New("")
		testutil.AssertNoError(t, err)
		p2, err := New("")
		testutil.AssertNoError(t, err)

		testutil.AssertEqual(t, p1.DataDir(), p2.DataDir())
		testutil.AssertEqual(t, p1.DeployedDir(), p2.DeployedDir())
		testutil.AssertEqual(t, p1.HomebrewDir(), p2.HomebrewDir())
		testutil.AssertEqual(t, p1.InstallDir(), p2.InstallDir())
	})
}

func TestPathNormalizationConsistency(t *testing.T) {
	p, err := New("/test/dotfiles")
	testutil.AssertNoError(t, err)

	// Test that the same logical path normalizes to the same result
	tests := []struct {
		name  string
		paths []string
	}{
		{
			name: "different representations of same path",
			paths: []string{
				"/test/dotfiles/vim/vimrc",
				"/test/dotfiles/./vim/vimrc",
				"/test/dotfiles/vim/../vim/vimrc",
				"/test/./dotfiles/vim/vimrc",
			},
		},
		{
			name: "paths with trailing slashes",
			paths: []string{
				"/test/dotfiles",
				"/test/dotfiles/",
				"/test/dotfiles//",
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if len(tt.paths) < 2 {
				t.Skip("Need at least 2 paths to compare")
			}

			// Normalize first path
			normalized1, err := p.NormalizePath(tt.paths[0])
			testutil.AssertNoError(t, err)

			// All other paths should normalize to the same result
			for i := 1; i < len(tt.paths); i++ {
				normalized, err := p.NormalizePath(tt.paths[i])
				testutil.AssertNoError(t, err)
				testutil.AssertEqual(t, normalized1, normalized,
					"Path %s should normalize to same as %s", tt.paths[i], tt.paths[0])
			}
		})
	}
}

func TestErrorHandling(t *testing.T) {
	tests := []struct {
		name        string
		setup       func()
		operation   func() error
		expectError bool
	}{
		{
			name: "normalize empty path",
			operation: func() error {
				p, _ := New("")
				_, err := p.NormalizePath("")
				return err
			},
			expectError: true,
		},
		{
			name: "is in dotfiles with empty path",
			operation: func() error {
				p, _ := New("/test")
				_, err := p.IsInDotfiles("")
				return err
			},
			expectError: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if tt.setup != nil {
				tt.setup()
			}

			err := tt.operation()
			if tt.expectError {
				testutil.AssertError(t, err)
			} else {
				testutil.AssertNoError(t, err)
			}
		})
	}
}
