package config

import (
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
)

func TestLoadPackConfig(t *testing.T) {
	tests := []struct {
		name        string
		content     string
		expected    types.PackConfig
		expectError bool
		errorMsg    string
	}{
		{
			name: "complete_config",
			content: `
[files]
"test.conf" = "symlink"
"*.bak" = "ignore"
"scripts/" = "shell_profile"`,
			expected: types.PackConfig{
				Files: map[string]string{
					"test.conf": "symlink",
					"*.bak":     "ignore",
					"scripts/":  "shell_profile",
				},
			},
		},
		{
			name:    "minimal_config",
			content: ``,
			expected: types.PackConfig{
				Files: map[string]string{},
			},
		},
		{
			name: "only_files_section",
			content: `[files]
"app.conf" = "test-powerup"
"*.log" = "ignore"
"install.sh" = "install"`,
			expected: types.PackConfig{
				Files: map[string]string{
					"app.conf":   "test-powerup",
					"*.log":      "ignore",
					"install.sh": "install",
				},
			},
		},
		{
			name:        "invalid_toml",
			content:     `invalid = [toml`,
			expectError: true,
			errorMsg:    "failed to parse TOML",
		},
		{
			name:    "empty_files_section",
			content: `[files]`,
			expected: types.PackConfig{
				Files: map[string]string{},
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Create a temporary file with the test content
			tmpDir := t.TempDir()
			configPath := filepath.Join(tmpDir, ".dodot.toml")

			err := os.WriteFile(configPath, []byte(tt.content), 0644)
			if err != nil {
				t.Fatalf("Failed to write test config: %v", err)
			}

			// Load the config
			got, err := LoadPackConfig(configPath)

			// Check error expectations
			if tt.expectError {
				if err == nil {
					t.Errorf("Expected error but got none")
				} else if tt.errorMsg != "" && !contains(err.Error(), tt.errorMsg) {
					t.Errorf("Expected error containing %q, got %q", tt.errorMsg, err.Error())
				}
				return
			}

			if err != nil {
				t.Errorf("Unexpected error: %v", err)
				return
			}

			// Compare the results
			if !reflect.DeepEqual(got, tt.expected) {
				t.Errorf("LoadPackConfig() = %+v, want %+v", got, tt.expected)
			}
		})
	}
}

func TestLoadPackConfig_FileNotFound(t *testing.T) {
	_, err := LoadPackConfig("/non/existent/file.toml")
	if err == nil {
		t.Error("Expected error for non-existent file")
	}
}

// Helper function
func contains(s, substr string) bool {
	return strings.Contains(s, substr)
}

func TestFileExists(t *testing.T) {
	// Create temp directory
	tmpDir := t.TempDir()

	// Create a test file
	testFile := filepath.Join(tmpDir, "test.txt")
	err := os.WriteFile(testFile, []byte("test"), 0644)
	if err != nil {
		t.Fatal(err)
	}

	// Create a test directory
	testDir := filepath.Join(tmpDir, "testdir")
	err = os.Mkdir(testDir, 0755)
	if err != nil {
		t.Fatal(err)
	}

	tests := []struct {
		name     string
		path     string
		expected bool
	}{
		{"existing file", testFile, true},
		{"existing directory", testDir, false},
		{"non-existent file", filepath.Join(tmpDir, "nonexistent.txt"), false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := FileExists(tt.path)
			if result != tt.expected {
				t.Errorf("FileExists(%q) = %v, want %v", tt.path, result, tt.expected)
			}
		})
	}
}
