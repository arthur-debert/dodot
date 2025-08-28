package rules

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestScanner_PatternMatching(t *testing.T) {
	tests := []struct {
		name     string
		files    []FileInfo
		rules    []config.Rule
		expected map[string]string // filename -> handler
	}{
		{
			name: "exact filename match",
			files: []FileInfo{
				{Path: "install.sh", Name: "install.sh", IsDirectory: false},
				{Path: "README.md", Name: "README.md", IsDirectory: false},
			},
			rules: []config.Rule{
				{Pattern: "install.sh", Handler: "install"},
				{Pattern: "*", Handler: "symlink"},
			},
			expected: map[string]string{
				"install.sh": "install",
				"README.md":  "symlink",
			},
		},
		{
			name: "glob pattern match",
			files: []FileInfo{
				{Path: "aliases.sh", Name: "aliases.sh", IsDirectory: false},
				{Path: "my-aliases.sh", Name: "my-aliases.sh", IsDirectory: false},
				{Path: "config", Name: "config", IsDirectory: false},
			},
			rules: []config.Rule{
				{Pattern: "*aliases.sh", Handler: "shell"},
				{Pattern: "*", Handler: "symlink"},
			},
			expected: map[string]string{
				"aliases.sh":    "shell",
				"my-aliases.sh": "shell",
				"config":        "symlink",
			},
		},
		{
			name: "directory pattern with trailing slash",
			files: []FileInfo{
				{Path: "bin", Name: "bin", IsDirectory: true},
				{Path: "lib", Name: "lib", IsDirectory: true},
				{Path: "bin.txt", Name: "bin.txt", IsDirectory: false},
			},
			rules: []config.Rule{
				{Pattern: "bin/", Handler: "path"},
				{Pattern: "*", Handler: "symlink"},
			},
			expected: map[string]string{
				"bin":     "path",
				"bin.txt": "symlink",
			},
		},
		{
			name: "exclusion patterns",
			files: []FileInfo{
				{Path: "config", Name: "config", IsDirectory: false},
				{Path: "config.bak", Name: "config.bak", IsDirectory: false},
				{Path: ".DS_Store", Name: ".DS_Store", IsDirectory: false},
			},
			rules: []config.Rule{
				{Pattern: "!*.bak"},
				{Pattern: "!.DS_Store"},
				{Pattern: "*", Handler: "symlink"},
			},
			expected: map[string]string{
				"config": "symlink",
				// config.bak and .DS_Store should be excluded
			},
		},
		{
			name: "matching order - exact before glob",
			files: []FileInfo{
				{Path: "test.sh", Name: "test.sh", IsDirectory: false},
			},
			rules: []config.Rule{
				{Pattern: "test.sh", Handler: "install"}, // Exact match
				{Pattern: "*.sh", Handler: "shell"},      // Glob pattern
				{Pattern: "*", Handler: "symlink"},       // Catchall
			},
			expected: map[string]string{
				"test.sh": "install", // Exact match wins over glob
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Create a temporary directory for testing
			tmpDir := t.TempDir()
			pack := types.Pack{
				Name: "test",
				Path: tmpDir,
			}

			// Create test files
			for _, f := range tt.files {
				path := filepath.Join(tmpDir, f.Path)
				if f.IsDirectory {
					require.NoError(t, os.Mkdir(path, 0755))
				} else {
					require.NoError(t, os.WriteFile(path, []byte("test"), 0644))
				}
			}

			// Run scanner
			scanner := NewScanner(tt.rules)
			matches, err := scanner.ScanPack(pack)
			require.NoError(t, err)

			// Check results
			matchMap := make(map[string]string)
			for _, m := range matches {
				matchMap[m.FileName] = m.Handler
			}

			assert.Equal(t, tt.expected, matchMap)
		})
	}
}

func TestScanner_HiddenFiles(t *testing.T) {
	tmpDir := t.TempDir()
	pack := types.Pack{
		Name: "test",
		Path: tmpDir,
	}

	// Create test files including hidden ones
	files := []string{
		"normal.txt",
		".hidden",
		".config", // Special case - should be included
		".gitignore",
	}

	for _, f := range files {
		require.NoError(t, os.WriteFile(filepath.Join(tmpDir, f), []byte("test"), 0644))
	}

	scanner := NewScanner([]config.Rule{
		{Pattern: "*", Handler: "symlink"},
	})

	matches, err := scanner.ScanPack(pack)
	require.NoError(t, err)

	// Should match normal.txt, .hidden, and .config but not .gitignore (which is skipped)
	assert.Len(t, matches, 3)

	matchedFiles := make(map[string]bool)
	for _, m := range matches {
		matchedFiles[m.FileName] = true
	}

	assert.True(t, matchedFiles["normal.txt"])
	assert.True(t, matchedFiles[".config"])
	assert.True(t, matchedFiles[".hidden"])     // Regular hidden files are included
	assert.False(t, matchedFiles[".gitignore"]) // Special files are excluded
}
