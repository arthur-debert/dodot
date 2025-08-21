package paths

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
)

// TestPathValidation tests path validation logic
// This is a unit test - it tests pure validation logic without filesystem operations
func TestPathValidation(t *testing.T) {
	p, err := New("/test/dotfiles")
	testutil.AssertNoError(t, err)

	tests := []struct {
		name        string
		path        string
		expectError bool
		description string
	}{
		{
			name:        "empty path",
			path:        "",
			expectError: true,
			description: "Empty paths should be rejected",
		},
		{
			name:        "valid absolute path",
			path:        "/home/user/file.txt",
			expectError: false,
			description: "Absolute paths should be valid",
		},
		{
			name:        "valid relative path",
			path:        "relative/path/file.txt",
			expectError: false,
			description: "Relative paths should be valid",
		},
		{
			name:        "path with tilde",
			path:        "~/dotfiles/file.txt",
			expectError: false,
			description: "Paths with tilde should be valid",
		},
		{
			name:        "path with double dots",
			path:        "/home/../usr/file.txt",
			expectError: false,
			description: "Paths with .. should be normalized",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, err := p.NormalizePath(tt.path)

			if tt.expectError {
				testutil.AssertError(t, err)
			} else {
				testutil.AssertNoError(t, err)
				if tt.path != "" {
					testutil.AssertTrue(t, filepath.IsAbs(result),
						"Normalized path should be absolute: %s", result)
				}
			}
		})
	}
}

// TestCrossPlatformPaths tests cross-platform path handling
// This is a unit test - it tests pure path manipulation logic
func TestCrossPlatformPaths(t *testing.T) {
	p, err := New("/test/dotfiles")
	testutil.AssertNoError(t, err)

	// Test path separator handling
	tests := []struct {
		name     string
		method   func(string) string
		input    string
		validate func(t *testing.T, result string)
	}{
		{
			name:   "pack path with forward slashes",
			method: p.PackPath,
			input:  "vim/config",
			validate: func(t *testing.T, result string) {
				expected := filepath.Join("/test/dotfiles", "vim", "config")
				testutil.AssertEqual(t, expected, result)
			},
		},
		{
			name: "state path with mixed separators",
			method: func(s string) string {
				parts := strings.Split(s, "/")
				if len(parts) >= 2 {
					return p.StatePath(parts[0], parts[1])
				}
				return ""
			},
			input: "mypack/handler",
			validate: func(t *testing.T, result string) {
				testutil.AssertTrue(t, strings.Contains(result, "mypack"),
					"Result should contain pack name")
				testutil.AssertTrue(t, strings.Contains(result, "handler.json"),
					"Result should contain handler name with .json")
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := tt.method(tt.input)
			tt.validate(t, result)
		})
	}
}

// TestPathExpansionEdgeCases tests tilde expansion edge cases
// This is a unit test with minimal OS interaction (just for home directory)
func TestPathExpansionEdgeCases(t *testing.T) {
	tests := []struct {
		name     string
		input    string
		expected string
		desc     string
	}{
		{
			name:     "empty string",
			input:    "",
			expected: "",
			desc:     "Empty string should remain empty",
		},
		{
			name:     "just tilde",
			input:    "~",
			expected: os.Getenv("HOME"),
			desc:     "Single tilde should expand to home",
		},
		{
			name:     "tilde with trailing slash",
			input:    "~/",
			expected: "", // Will be set dynamically
			desc:     "Tilde with slash expands to home",
		},
		{
			name:     "tilde in middle",
			input:    "/path/~/file",
			expected: "/path/~/file",
			desc:     "Tilde in middle should not expand",
		},
		{
			name:     "tilde other user",
			input:    "~otheruser/path",
			expected: "~otheruser/path",
			desc:     "Other user's home should not expand",
		},
		{
			name:     "multiple tildes",
			input:    "~/~/path",
			expected: os.Getenv("HOME") + "/~/path",
			desc:     "Only first tilde should expand",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := ExpandHome(tt.input)
			expected := tt.expected

			// Handle dynamic expectations
			if tt.input == "~/" {
				homeDir, _ := os.UserHomeDir()
				expected = homeDir
			} else if expected == os.Getenv("HOME") && expected != "" {
				homeDir, _ := os.UserHomeDir()
				expected = homeDir
			}

			testutil.AssertEqual(t, expected, result)
		})
	}
}

// TestDeploymentPathStructure tests deployment path structure
// This is a unit test - it tests pure path computation logic
func TestDeploymentPathStructure(t *testing.T) {
	p, err := New("/test/dotfiles")
	testutil.AssertNoError(t, err)

	// Verify deployment directory structure
	deploymentPaths := map[string]string{
		"deployed root": p.DeployedDir(),
		"shell profile": p.ShellProfileDir(),
		"path":          p.PathDir(),
		"shell source":  p.ShellSourceDir(),
		"symlink":       p.SymlinkDir(),
		"shell":         p.ShellDir(),
		"install":       p.InstallDir(),
		"brewfile":      p.HomebrewDir(),
	}

	// All deployment paths should be under data directory
	dataDir := p.DataDir()
	for name, path := range deploymentPaths {
		t.Run(name, func(t *testing.T) {
			testutil.AssertTrue(t, strings.HasPrefix(path, dataDir),
				"%s path (%s) should be under data directory (%s)", name, path, dataDir)
		})
	}

	// Verify specific relationships
	t.Run("deployment subdirectories", func(t *testing.T) {
		deployedDir := p.DeployedDir()
		testutil.AssertTrue(t, strings.HasPrefix(p.ShellProfileDir(), deployedDir),
			"Shell profile dir should be under deployed dir")
		testutil.AssertTrue(t, strings.HasPrefix(p.PathDir(), deployedDir),
			"Path dir should be under deployed dir")
		testutil.AssertTrue(t, strings.HasPrefix(p.ShellSourceDir(), deployedDir),
			"Shell source dir should be under deployed dir")
		testutil.AssertTrue(t, strings.HasPrefix(p.SymlinkDir(), deployedDir),
			"Symlink dir should be under deployed dir")
	})
}

// TestPathsConcurrentAccess tests concurrent access to paths
// This is a unit test - it tests thread safety without filesystem operations
func TestPathsConcurrentAccess(t *testing.T) {
	// Test that a single Paths instance can be safely accessed concurrently
	// This is what actually matters for our use case
	p, err := New("/test/dotfiles")
	testutil.AssertNoError(t, err)

	const numGoroutines = 20
	const numIterations = 100

	done := make(chan bool, numGoroutines)
	errors := make(chan error, numGoroutines*numIterations)

	// Start multiple goroutines that call various path methods concurrently
	for i := 0; i < numGoroutines; i++ {
		go func(id int) {
			defer func() { done <- true }()

			for j := 0; j < numIterations; j++ {
				// Call various path methods concurrently
				packName := fmt.Sprintf("pack%d", id%5)

				_ = p.PackPath(packName)
				_ = p.DataDir()
				_ = p.DeployedDir()
				_ = p.StatePath(packName, "handler")
				_ = p.ConfigDir()
				_ = p.CacheDir()

				// Test path normalization
				testPath := fmt.Sprintf("/test/path/%d", j)
				if normalized, err := p.NormalizePath(testPath); err != nil {
					errors <- fmt.Errorf("normalization error: %v", err)
				} else if !filepath.IsAbs(normalized) {
					errors <- fmt.Errorf("normalized path should be absolute: %s", normalized)
				}
			}
		}(i)
	}

	// Wait for all goroutines to complete
	for i := 0; i < numGoroutines; i++ {
		<-done
	}
	close(errors)

	// Check for any errors
	for err := range errors {
		t.Error(err)
	}
}

// TestValidatePackName tests pack name validation
// This is a pure unit test - no filesystem operations
func TestValidatePackName(t *testing.T) {
	tests := []struct {
		name    string
		input   string
		wantErr bool
	}{
		// Valid cases
		{
			name:    "simple name",
			input:   "vim",
			wantErr: false,
		},
		{
			name:    "name with dash",
			input:   "vim-config",
			wantErr: false,
		},
		{
			name:    "name with underscore",
			input:   "vim_config",
			wantErr: false,
		},
		{
			name:    "name with numbers",
			input:   "vim2",
			wantErr: false,
		},
		{
			name:    "name with mixed case",
			input:   "VimConfig",
			wantErr: false,
		},
		// Invalid cases
		{
			name:    "empty name",
			input:   "",
			wantErr: true,
		},
		{
			name:    "name with forward slash",
			input:   "vim/config",
			wantErr: true,
		},
		{
			name:    "name with backslash",
			input:   "vim\\config",
			wantErr: true,
		},
		{
			name:    "name with colon",
			input:   "vim:config",
			wantErr: true,
		},
		{
			name:    "name with asterisk",
			input:   "vim*",
			wantErr: true,
		},
		{
			name:    "name with question mark",
			input:   "vim?",
			wantErr: true,
		},
		{
			name:    "name with double quote",
			input:   "vim\"config",
			wantErr: true,
		},
		{
			name:    "name with less than",
			input:   "vim<config",
			wantErr: true,
		},
		{
			name:    "name with greater than",
			input:   "vim>config",
			wantErr: true,
		},
		{
			name:    "name with pipe",
			input:   "vim|config",
			wantErr: true,
		},
		{
			name:    "dot only",
			input:   ".",
			wantErr: true,
		},
		{
			name:    "double dot",
			input:   "..",
			wantErr: true,
		},
		{
			name:    "name with null byte",
			input:   "vim\x00config",
			wantErr: true,
		},
		{
			name:    "name with control character",
			input:   "vim\x01config",
			wantErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := ValidatePackName(tt.input)
			if tt.wantErr {
				assert.Error(t, err, "ValidatePackName(%q) should return error", tt.input)
			} else {
				assert.NoError(t, err, "ValidatePackName(%q) should not return error", tt.input)
			}
		})
	}
}
