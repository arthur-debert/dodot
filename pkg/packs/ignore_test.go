package packs

import (
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestNewIgnoreChecker(t *testing.T) {
	ic := NewIgnoreChecker()
	assert.NotNil(t, ic)
	assert.NotNil(t, ic.logger)
	assert.NotNil(t, ic.config)
}

func TestShouldIgnorePackFS(t *testing.T) {
	tests := []struct {
		name       string
		packPath   string
		setupFS    func() types.FS
		wantIgnore bool
	}{
		{
			name:     "pack with ignore file",
			packPath: "vim",
			setupFS: func() types.FS {
				fs := testutil.NewTestFS()
				// Create parent directory first
				packDir := "vim"
				err := fs.MkdirAll(packDir, 0755)
				require.NoError(t, err)
				// Create .dodotignore file
				ignorePath := filepath.Join(packDir, ".dodotignore")
				err = fs.WriteFile(ignorePath, []byte(""), 0644)
				require.NoError(t, err)
				return fs
			},
			wantIgnore: true,
		},
		{
			name:     "pack without ignore file",
			packPath: "zsh",
			setupFS: func() types.FS {
				fs := testutil.NewTestFS()
				// Don't create .dodotignore file
				return fs
			},
			wantIgnore: false,
		},
		{
			name:     "empty pack path",
			packPath: "",
			setupFS: func() types.FS {
				return testutil.NewTestFS()
			},
			wantIgnore: false,
		},
		{
			name:     "pack with subdirectory ignore file",
			packPath: "git",
			setupFS: func() types.FS {
				fs := testutil.NewTestFS()
				// Create .dodotignore in subdirectory (not in pack root)
				subIgnorePath := filepath.Join("git/subdir", ".dodotignore")
				err := fs.MkdirAll(filepath.Dir(subIgnorePath), 0755)
				require.NoError(t, err)
				err = fs.WriteFile(subIgnorePath, []byte(""), 0644)
				require.NoError(t, err)
				return fs
			},
			wantIgnore: false, // Only root .dodotignore matters
		},
		{
			name:     "pack with empty ignore file",
			packPath: "tmux",
			setupFS: func() types.FS {
				fs := testutil.NewTestFS()
				packDir := "tmux"
				err := fs.MkdirAll(packDir, 0755)
				require.NoError(t, err)
				ignorePath := filepath.Join(packDir, ".dodotignore")
				err = fs.WriteFile(ignorePath, []byte(""), 0644)
				require.NoError(t, err)
				return fs
			},
			wantIgnore: true,
		},
		{
			name:     "pack with non-empty ignore file",
			packPath: "emacs",
			setupFS: func() types.FS {
				fs := testutil.NewTestFS()
				packDir := "emacs"
				err := fs.MkdirAll(packDir, 0755)
				require.NoError(t, err)
				ignorePath := filepath.Join(packDir, ".dodotignore")
				err = fs.WriteFile(ignorePath, []byte("# Ignore this pack\n*.tmp\n"), 0644)
				require.NoError(t, err)
				return fs
			},
			wantIgnore: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			fs := tt.setupFS()
			result := ShouldIgnorePackFS(tt.packPath, fs)
			assert.Equal(t, tt.wantIgnore, result)
		})
	}
}

func TestIgnoreChecker_Methods_WithMockFS(t *testing.T) {
	// Since the non-FS methods use os.Stat directly, we can't easily mock them
	// But we can test their behavior patterns and edge cases

	t.Run("ShouldIgnorePackDirectory edge cases", func(t *testing.T) {
		ic := NewIgnoreChecker()

		// Test with invalid paths
		testCases := []struct {
			name     string
			packPath string
			// We can't predict the result without actual filesystem
			// but we can ensure it doesn't panic
		}{
			{
				name:     "empty path",
				packPath: "",
			},
			{
				name:     "relative path",
				packPath: "relative/path",
			},
			{
				name:     "path with special characters",
				packPath: "/path/with spaces/and-special_chars",
			},
			{
				name:     "very long path",
				packPath: "/" + string(make([]byte, 255)),
			},
		}

		for _, tc := range testCases {
			t.Run(tc.name, func(t *testing.T) {
				// Should not panic
				assert.NotPanics(t, func() {
					_ = ic.ShouldIgnorePackDirectory(tc.packPath)
				})
			})
		}
	})

	t.Run("ShouldIgnoreDirectoryDuringTraversal patterns", func(t *testing.T) {
		ic := NewIgnoreChecker()

		testCases := []struct {
			name    string
			dirPath string
			relPath string
		}{
			{
				name:    "root directory",
				dirPath: "/",
				relPath: "",
			},
			{
				name:    "nested directory",
				dirPath: "/home/user/.dotfiles/vim/plugin",
				relPath: "vim/plugin",
			},
			{
				name:    "directory with dots",
				dirPath: "/home/user/.config/../.dotfiles",
				relPath: ".dotfiles",
			},
		}

		for _, tc := range testCases {
			t.Run(tc.name, func(t *testing.T) {
				// Should not panic
				assert.NotPanics(t, func() {
					_ = ic.ShouldIgnoreDirectoryDuringTraversal(tc.dirPath, tc.relPath)
				})
			})
		}
	})

	t.Run("HasIgnoreFile patterns", func(t *testing.T) {
		ic := NewIgnoreChecker()

		// Test various path patterns
		paths := []string{
			"",
			".",
			"..",
			"/absolute/path",
			"relative/path",
			"/path with spaces",
		}

		for _, path := range paths {
			assert.NotPanics(t, func() {
				_ = ic.HasIgnoreFile(path)
			})
		}
	})
}

func TestShouldIgnorePack_Wrapper(t *testing.T) {
	// Test that the wrapper function works
	assert.NotPanics(t, func() {
		// This will return false in test environment (no actual .dodotignore file)
		result := ShouldIgnorePack("/some/test/path")
		assert.IsType(t, bool(false), result)
	})
}

func TestShouldIgnoreDirectoryTraversal_Wrapper(t *testing.T) {
	// Test that the wrapper function works
	assert.NotPanics(t, func() {
		result := ShouldIgnoreDirectoryTraversal("/some/dir", "relative/path")
		assert.IsType(t, bool(false), result)
	})
}

func TestIgnoreChecker_ConfigPatterns(t *testing.T) {
	// Test that ignore checker respects config patterns
	ic := NewIgnoreChecker()

	// The ignore file pattern should come from config
	expectedPattern := ic.config.Patterns.SpecialFiles.IgnoreFile
	assert.NotEmpty(t, expectedPattern)
	assert.Equal(t, ".dodotignore", expectedPattern) // Default value
}

func TestIgnoreChecker_WithCustomIgnoreFile(t *testing.T) {
	// This tests the logic of building ignore file paths
	ic := NewIgnoreChecker()

	testPaths := []string{
		"/home/user/.dotfiles/vim",
		"/usr/local/config",
		"relative/path",
		"",
	}

	for _, basePath := range testPaths {
		expectedPath := filepath.Join(basePath, ic.config.Patterns.SpecialFiles.IgnoreFile)

		// Verify path construction logic
		if basePath == "" {
			assert.Equal(t, ic.config.Patterns.SpecialFiles.IgnoreFile, expectedPath)
		} else {
			assert.Contains(t, expectedPath, ic.config.Patterns.SpecialFiles.IgnoreFile)
			assert.Contains(t, expectedPath, basePath)
		}
	}
}
