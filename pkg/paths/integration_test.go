package paths

import (
	"os"
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
		testutil.AssertEqual(t, p1.ProvisionDir(), p2.ProvisionDir())
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

// Git discovery tests moved from git_discovery_test.go

func TestFindDotfilesRoot(t *testing.T) {
	// Save current directory to restore later
	originalCwd, err := os.Getwd()
	testutil.AssertNoError(t, err)
	defer func() {
		_ = os.Chdir(originalCwd)
	}()

	tests := []struct {
		name           string
		setupEnv       map[string]string
		setupFunc      func(t *testing.T) string
		expectedPath   string
		expectFallback bool
		skipIfNoGit    bool
	}{
		{
			name: "DOTFILES_ROOT env var takes precedence",
			setupEnv: map[string]string{
				EnvDotfilesRoot: "/env/dotfiles",
			},
			expectedPath:   "/env/dotfiles",
			expectFallback: false,
		},
		{
			name: "Git repository root discovery",
			setupFunc: func(t *testing.T) string {
				// Create a temporary git repo
				tmpDir := testutil.TempDir(t, "git-test")

				// Change to the temp directory
				err := os.Chdir(tmpDir)
				testutil.AssertNoError(t, err)

				// Initialize git repo
				testutil.RunCommand(t, "git", "init")

				// Create a subdirectory and change into it
				subDir := filepath.Join(tmpDir, "sub", "dir")
				err = os.MkdirAll(subDir, 0755)
				testutil.AssertNoError(t, err)

				err = os.Chdir(subDir)
				testutil.AssertNoError(t, err)

				return tmpDir
			},
			expectedPath:   "", // Will be set by setupFunc
			expectFallback: false,
			skipIfNoGit:    true,
		},
		{
			name: "Fallback to current directory when not in git repo",
			setupFunc: func(t *testing.T) string {
				// Create a temporary directory that's not a git repo
				tmpDir := testutil.TempDir(t, "no-git-test")

				err := os.Chdir(tmpDir)
				testutil.AssertNoError(t, err)

				return tmpDir
			},
			expectedPath:   "", // Will be set to cwd
			expectFallback: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if tt.skipIfNoGit && !isGitAvailable() {
				t.Skip("Git not available")
			}

			// Clear environment
			t.Setenv(EnvDotfilesRoot, "")

			// Set up environment
			for k, v := range tt.setupEnv {
				t.Setenv(k, v)
			}

			// Run setup function if provided
			expectedPath := tt.expectedPath
			if tt.setupFunc != nil {
				result := tt.setupFunc(t)
				if expectedPath == "" {
					expectedPath = result
				}
			}

			// Test findDotfilesRoot
			path, usedFallback, err := findDotfilesRoot()
			testutil.AssertNoError(t, err)

			if expectedPath != "" {
				// Resolve symlinks for comparison
				expected, _ := filepath.EvalSymlinks(expectedPath)
				actual, _ := filepath.EvalSymlinks(path)
				testutil.AssertEqual(t, expected, actual)
			}

			testutil.AssertEqual(t, tt.expectFallback, usedFallback)
		})
	}
}

func TestGitRootFromSubdirectory(t *testing.T) {
	if !isGitAvailable() {
		t.Skip("Git not available")
	}

	// Save current directory
	originalCwd, err := os.Getwd()
	testutil.AssertNoError(t, err)
	defer func() {
		_ = os.Chdir(originalCwd)
	}()

	// Create temporary git repo
	tmpDir := testutil.TempDir(t, "git-subdir-test")

	// Initialize git repo
	err = os.Chdir(tmpDir)
	testutil.AssertNoError(t, err)
	testutil.RunCommand(t, "git", "init")

	// Create nested subdirectories
	deepPath := filepath.Join(tmpDir, "a", "b", "c", "d")
	err = os.MkdirAll(deepPath, 0755)
	testutil.AssertNoError(t, err)

	// Change to deep subdirectory
	err = os.Chdir(deepPath)
	testutil.AssertNoError(t, err)

	// Test that git root is found correctly
	gitRoot, err := findGitRoot()
	testutil.AssertNoError(t, err)

	// Resolve symlinks for comparison (macOS /var -> /private/var)
	expectedPath, _ := filepath.EvalSymlinks(tmpDir)
	actualPath, _ := filepath.EvalSymlinks(gitRoot)
	testutil.AssertEqual(t, expectedPath, actualPath)
}

func TestPathsWithGitDiscovery(t *testing.T) {
	if !isGitAvailable() {
		t.Skip("Git not available")
	}

	// Save current directory
	originalCwd, err := os.Getwd()
	testutil.AssertNoError(t, err)
	defer func() {
		_ = os.Chdir(originalCwd)
	}()

	// Test 1: Git repo as dotfiles root
	t.Run("git repo discovery", func(t *testing.T) {
		// Clear environment to avoid interference
		t.Setenv(EnvDotfilesRoot, "")

		tmpDir := testutil.TempDir(t, "paths-git-test")
		err := os.Chdir(tmpDir)
		testutil.AssertNoError(t, err)
		testutil.RunCommand(t, "git", "init")

		p, err := New("")
		testutil.AssertNoError(t, err)

		// Resolve symlinks for comparison
		expectedPath, _ := filepath.EvalSymlinks(tmpDir)
		actualPath, _ := filepath.EvalSymlinks(p.DotfilesRoot())
		testutil.AssertEqual(t, expectedPath, actualPath)
		testutil.AssertFalse(t, p.UsedFallback())
	})

	// Test 2: Fallback with warning
	t.Run("fallback to cwd", func(t *testing.T) {
		// Clear environment to avoid interference
		t.Setenv(EnvDotfilesRoot, "")

		tmpDir := testutil.TempDir(t, "paths-no-git-test")
		err := os.Chdir(tmpDir)
		testutil.AssertNoError(t, err)

		p, err := New("")
		testutil.AssertNoError(t, err)

		// Resolve symlinks for comparison
		expectedPath, _ := filepath.EvalSymlinks(tmpDir)
		actualPath, _ := filepath.EvalSymlinks(p.DotfilesRoot())
		testutil.AssertEqual(t, expectedPath, actualPath)
		testutil.AssertTrue(t, p.UsedFallback())
	})
}

// Helper function to check if git is available
func isGitAvailable() bool {
	_, err := findGitRoot()
	// If we're in a git repo, git is available
	if err == nil {
		return true
	}

	// Try to run git --version
	cmd := testutil.CommandAvailable("git")
	return cmd
}

// TestCLIIntegration tests the CLI integration with paths
func TestCLIIntegration(t *testing.T) {
	// This test verifies that warnings are shown correctly
	tests := []struct {
		name           string
		envSetup       map[string]string
		setupFunc      func(t *testing.T) (cleanup func())
		expectFallback bool
	}{
		{
			name: "DOTFILES_ROOT set - no warning",
			envSetup: map[string]string{
				EnvDotfilesRoot: "/custom/dotfiles",
			},
			expectFallback: false,
		},
		{
			name: "Git repo - no warning",
			setupFunc: func(t *testing.T) (cleanup func()) {
				// We're already in a git repo (dodot), so no setup needed
				return func() {}
			},
			expectFallback: false,
		},
		{
			name: "No git, no env - shows warning",
			setupFunc: func(t *testing.T) (cleanup func()) {
				if !isGitAvailable() {
					t.Skip("Git not available")
				}

				// Save current directory
				originalCwd, err := os.Getwd()
				testutil.AssertNoError(t, err)

				// Create a temporary directory
				tmpDir := testutil.TempDir(t, "cli-test")
				err = os.Chdir(tmpDir)
				testutil.AssertNoError(t, err)

				return func() {
					_ = os.Chdir(originalCwd)
				}
			},
			expectFallback: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Clear environment
			t.Setenv(EnvDotfilesRoot, "")

			// Set up environment
			for k, v := range tt.envSetup {
				t.Setenv(k, v)
			}

			// Run setup if provided
			var cleanup func()
			if tt.setupFunc != nil {
				cleanup = tt.setupFunc(t)
				defer cleanup()
			}

			// Test paths initialization
			p, err := New("")
			testutil.AssertNoError(t, err)
			testutil.AssertEqual(t, tt.expectFallback, p.UsedFallback())
		})
	}
}
