package packs

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestShouldIgnore_UsesLoadedConfig(t *testing.T) {
	// Save original config and restore after test
	originalConfig := config.Get()
	defer config.Initialize(originalConfig)

	tests := []struct {
		name          string
		packIgnore    []string
		directoryName string
		shouldIgnore  bool
	}{
		{
			name:          "uses_custom_patterns",
			packIgnore:    []string{"temp*", "backup*", "*.old"},
			directoryName: "temp-files",
			shouldIgnore:  true,
		},
		{
			name:          "allows_non_matching_directory",
			packIgnore:    []string{"temp*", "backup*"},
			directoryName: "mypack",
			shouldIgnore:  false,
		},
		{
			name:          "handles_glob_patterns",
			packIgnore:    []string{"test-*", "*.tmp"},
			directoryName: "test-data",
			shouldIgnore:  true,
		},
		{
			name:          "exact_match",
			packIgnore:    []string{"node_modules", ".git"},
			directoryName: "node_modules",
			shouldIgnore:  true,
		},
		{
			name:          "empty_patterns_allows_all",
			packIgnore:    []string{},
			directoryName: "anything",
			shouldIgnore:  false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Create a test config with specified patterns
			testConfig := &config.Config{
				Patterns: config.Patterns{
					PackIgnore: tt.packIgnore,
				},
			}
			config.Initialize(testConfig)

			// Test the function
			result := shouldIgnore(tt.directoryName)
			assert.Equal(t, tt.shouldIgnore, result)
		})
	}
}

func TestGetPackCandidatesFS_RespectsConfiguredIgnorePatterns(t *testing.T) {
	// This test verifies that pack discovery respects the configured ignore patterns
	// rather than just the hardcoded defaults

	// Save original config and restore after test
	originalConfig := config.Get()
	defer config.Initialize(originalConfig)

	// Create test filesystem
	tempDir := t.TempDir()

	// Create various directories
	dirs := []string{
		"vim",
		"git",
		"test-pack",
		"backup-old",
		"temp123",
		".hidden",
		".config",
		"my-data",
		"node_modules",
	}

	for _, dir := range dirs {
		err := os.MkdirAll(filepath.Join(tempDir, dir), 0755)
		require.NoError(t, err)
	}

	tests := []struct {
		name               string
		packIgnore         []string
		expectedCandidates []string
	}{
		{
			name:       "custom_patterns_override_defaults",
			packIgnore: []string{"test-*", "backup-*", "temp*"},
			expectedCandidates: []string{
				filepath.Join(tempDir, ".config"),
				filepath.Join(tempDir, "git"),
				filepath.Join(tempDir, "my-data"),
				filepath.Join(tempDir, "node_modules"), // Not ignored with custom patterns
				filepath.Join(tempDir, "vim"),
			},
		},
		{
			name:       "allows_normally_ignored_with_empty_patterns",
			packIgnore: []string{},
			expectedCandidates: []string{
				filepath.Join(tempDir, ".config"),
				filepath.Join(tempDir, "backup-old"),
				filepath.Join(tempDir, "git"),
				filepath.Join(tempDir, "my-data"),
				filepath.Join(tempDir, "node_modules"),
				filepath.Join(tempDir, "temp123"),
				filepath.Join(tempDir, "test-pack"),
				filepath.Join(tempDir, "vim"),
			},
		},
		{
			name:       "specific_ignore_list",
			packIgnore: []string{"vim", "git", "my-data"},
			expectedCandidates: []string{
				filepath.Join(tempDir, ".config"),
				filepath.Join(tempDir, "backup-old"),
				filepath.Join(tempDir, "node_modules"),
				filepath.Join(tempDir, "temp123"),
				filepath.Join(tempDir, "test-pack"),
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Set up config with custom patterns
			testConfig := &config.Config{
				Patterns: config.Patterns{
					PackIgnore: tt.packIgnore,
				},
			}
			config.Initialize(testConfig)

			// Get pack candidates
			candidates, err := GetPackCandidates(tempDir)
			require.NoError(t, err)

			// Verify the expected candidates
			assert.ElementsMatch(t, tt.expectedCandidates, candidates)
		})
	}
}

func TestShouldIgnoreWithPatterns(t *testing.T) {
	tests := []struct {
		name         string
		patterns     []string
		dirName      string
		shouldIgnore bool
	}{
		{
			name:         "exact_match",
			patterns:     []string{".git", "node_modules", ".DS_Store"},
			dirName:      "node_modules",
			shouldIgnore: true,
		},
		{
			name:         "glob_pattern_star",
			patterns:     []string{"*.tmp", "test-*"},
			dirName:      "test-data",
			shouldIgnore: true,
		},
		{
			name:         "glob_pattern_question",
			patterns:     []string{"temp?", "backup??"},
			dirName:      "temp1",
			shouldIgnore: true,
		},
		{
			name:         "no_match",
			patterns:     []string{"*.tmp", "backup*"},
			dirName:      "mypack",
			shouldIgnore: false,
		},
		{
			name:         "empty_patterns",
			patterns:     []string{},
			dirName:      "anything",
			shouldIgnore: false,
		},
		{
			name:         "complex_pattern",
			patterns:     []string{"[._]*", "backup-[0-9]*"},
			dirName:      "backup-123",
			shouldIgnore: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := shouldIgnoreWithPatterns(tt.dirName, tt.patterns)
			assert.Equal(t, tt.shouldIgnore, result)
		})
	}
}
