package packs_test

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/packs"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestGetPackCandidatesWithConfig_UsesConfigIgnorePatterns(t *testing.T) {
	// Create a temporary directory structure
	tmpDir := t.TempDir()

	// Create some directories
	require.NoError(t, os.MkdirAll(filepath.Join(tmpDir, "vim"), 0755))
	require.NoError(t, os.MkdirAll(filepath.Join(tmpDir, "node_modules"), 0755))
	require.NoError(t, os.MkdirAll(filepath.Join(tmpDir, "test-pack"), 0755))
	require.NoError(t, os.MkdirAll(filepath.Join(tmpDir, "backup"), 0755))

	// Create some files to make directories non-empty
	require.NoError(t, os.WriteFile(filepath.Join(tmpDir, "vim", "vimrc"), []byte("test"), 0644))
	require.NoError(t, os.WriteFile(filepath.Join(tmpDir, "test-pack", "file.txt"), []byte("test"), 0644))
	require.NoError(t, os.WriteFile(filepath.Join(tmpDir, "backup", "data.bak"), []byte("test"), 0644))

	tests := []struct {
		name               string
		config             *config.Config
		expectedCandidates []string
		description        string
	}{
		{
			name: "uses_config_ignore_patterns",
			config: &config.Config{
				Patterns: config.Patterns{
					PackIgnore: []string{"test-*", "backup"},
				},
			},
			expectedCandidates: []string{
				filepath.Join(tmpDir, "vim"),
				// node_modules is NOT in our custom ignore list, so it should be included
				filepath.Join(tmpDir, "node_modules"),
			},
			description: "Should use config ignore patterns instead of defaults",
		},
		{
			name:   "falls_back_to_global_when_nil",
			config: nil,
			expectedCandidates: []string{
				filepath.Join(tmpDir, "backup"),
				filepath.Join(tmpDir, "test-pack"),
				filepath.Join(tmpDir, "vim"),
				// node_modules is in the default ignore list
			},
			description: "Should use global config patterns when config is nil",
		},
		{
			name: "empty_ignore_patterns",
			config: &config.Config{
				Patterns: config.Patterns{
					PackIgnore: []string{},
				},
			},
			expectedCandidates: []string{
				filepath.Join(tmpDir, "backup"),
				filepath.Join(tmpDir, "node_modules"),
				filepath.Join(tmpDir, "test-pack"),
				filepath.Join(tmpDir, "vim"),
			},
			description: "Should include all directories when ignore patterns are empty",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			candidates, err := packs.GetPackCandidatesWithConfig(tmpDir, tt.config)
			require.NoError(t, err)

			assert.ElementsMatch(t, tt.expectedCandidates, candidates, tt.description)
		})
	}
}

func TestGetPackCandidatesWithConfig_PatternMatching(t *testing.T) {
	// Test specific pattern matching behavior
	tmpDir := t.TempDir()

	// Create directories with various names
	dirs := []string{
		"normal-pack",
		"test-pack",
		"test-another",
		"production",
		"temp",
		"temporary-files",
		".git",
		".hidden",
		".config", // This should NOT be ignored despite starting with dot
	}

	for _, dir := range dirs {
		require.NoError(t, os.MkdirAll(filepath.Join(tmpDir, dir), 0755))
		// Create a file to make it non-empty
		require.NoError(t, os.WriteFile(filepath.Join(tmpDir, dir, "file"), []byte("test"), 0644))
	}

	tests := []struct {
		name             string
		ignorePatterns   []string
		expectedIncluded []string
		expectedExcluded []string
	}{
		{
			name:             "glob_patterns",
			ignorePatterns:   []string{"test-*", "temp*"},
			expectedIncluded: []string{"normal-pack", "production", ".config"},
			expectedExcluded: []string{"test-pack", "test-another", "temp", "temporary-files", ".git", ".hidden"},
		},
		{
			name:             "exact_match",
			ignorePatterns:   []string{"production", "normal-pack"},
			expectedIncluded: []string{"test-pack", "test-another", "temp", "temporary-files", ".config"},
			expectedExcluded: []string{"production", "normal-pack", ".git", ".hidden"},
		},
		{
			name:             "complex_patterns",
			ignorePatterns:   []string{"*-pack", "temp"},
			expectedIncluded: []string{"production", "temporary-files", "test-another", ".config"},
			expectedExcluded: []string{"normal-pack", "test-pack", "temp", ".git", ".hidden"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			cfg := &config.Config{
				Patterns: config.Patterns{
					PackIgnore: tt.ignorePatterns,
				},
			}

			candidates, err := packs.GetPackCandidatesWithConfig(tmpDir, cfg)
			require.NoError(t, err)

			// Extract just the directory names from the full paths
			var candidateNames []string
			for _, candidate := range candidates {
				candidateNames = append(candidateNames, filepath.Base(candidate))
			}

			// Check that expected directories are included
			for _, expected := range tt.expectedIncluded {
				assert.Contains(t, candidateNames, expected, "Expected %s to be included", expected)
			}

			// Check that excluded directories are not present
			for _, excluded := range tt.expectedExcluded {
				assert.NotContains(t, candidateNames, excluded, "Expected %s to be excluded", excluded)
			}
		})
	}
}
