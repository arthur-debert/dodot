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
			name: "full_config",
			content: `
[[ignore]]
  path = "README.md"
[[ignore]]
  path = "*.bak"

[[override]]
  path = "htoprc"
  powerup = "symlink"
  with = { target_dir = "~/.config/htop" }

[[override]]
  path = "my-exports.sh"
  powerup = "shell_profile"
`,
			expected: types.PackConfig{
				Ignore: []types.IgnoreRule{
					{Path: "README.md"},
					{Path: "*.bak"},
				},
				Override: []types.OverrideRule{
					{
						Path:    "htoprc",
						Powerup: "symlink",
						With:    map[string]interface{}{"target_dir": "~/.config/htop"},
					},
					{
						Path:    "my-exports.sh",
						Powerup: "shell_profile",
					},
				},
			},
		},
		{
			name:    "empty_config",
			content: ``,
			expected: types.PackConfig{
				Ignore:   nil,
				Override: nil,
			},
		},
		{
			name: "only_ignore",
			content: `
[[ignore]]
  path = "file.txt"
`,
			expected: types.PackConfig{
				Ignore:   []types.IgnoreRule{{Path: "file.txt"}},
				Override: nil,
			},
		},
		{
			name: "only_override",
			content: `
[[override]]
  path = "bashrc"
  powerup = "symlink"
`,
			expected: types.PackConfig{
				Ignore:   nil,
				Override: []types.OverrideRule{{Path: "bashrc", Powerup: "symlink"}},
			},
		},
		{
			name:        "invalid_toml",
			content:     `[[ignore] path = "test"`,
			expectError: true,
			errorMsg:    "failed to parse TOML",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			tmpDir := t.TempDir()
			configPath := filepath.Join(tmpDir, ".dodot.toml")
			if err := os.WriteFile(configPath, []byte(tt.content), 0644); err != nil {
				t.Fatalf("Failed to write temp config file: %v", err)
			}

			config, err := LoadPackConfig(configPath)

			if tt.expectError {
				if err == nil {
					t.Errorf("Expected an error, but got nil")
				} else if !strings.Contains(err.Error(), tt.errorMsg) {
					t.Errorf("Expected error message to contain %q, but got %q", tt.errorMsg, err.Error())
				}
				return
			}
			if err != nil {
				t.Fatalf("Unexpected error: %v", err)
			}

			if !reflect.DeepEqual(config, tt.expected) {
				t.Errorf("Expected config %+v, but got %+v", tt.expected, config)
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
