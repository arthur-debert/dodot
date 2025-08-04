package paths

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
)

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
