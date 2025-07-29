package config

import (
	"fmt"
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

// Helper function to create bool pointers
func boolPtr(b bool) *bool {
	return &b
}

func TestLoadPackConfig(t *testing.T) {
	tests := []struct {
		name        string
		content     string
		expected    types.PackConfig
		expectError bool
		errorMsg    string
	}{
		{
			name: "valid_full_config",
			content: `description = "Test pack with all fields"
priority = 10
disabled = false

[[matchers]]
trigger = "filename"
powerup = "symlink"
pattern = ".*\\.conf"
priority = 100
enabled = true
target = "~"

[[matchers]]
trigger = "directory"
powerup = "shell_profile"
pattern = "scripts"
priority = 50`,
			expected: types.PackConfig{
				Description: "Test pack with all fields",
				Priority:    10,
				Disabled:    false,
				Matchers: []types.MatcherConfig{
					{
						Trigger:        "filename",
						PowerUp:        "symlink",
						Pattern:        ".*\\.conf",
						Priority:       100,
						Enabled:        boolPtr(true),
						Target:         "~",
						PowerUpOptions: map[string]interface{}{},
					},
					{
						Trigger:        "directory",
						PowerUp:        "shell_profile",
						Pattern:        "scripts",
						Priority:       50,
						Enabled:        boolPtr(true),
						PowerUpOptions: map[string]interface{}{},
					},
				},
			},
		},
		{
			name: "minimal_valid_config",
			content: `description = "Minimal pack"`,
			expected: types.PackConfig{
				Description: "Minimal pack",
				Priority:    0,
				Disabled:    false,
				Matchers:    []types.MatcherConfig{},
			},
		},
		{
			name: "config_with_defaults",
			content: `[[matchers]]
trigger = "filename"
powerup = "symlink"`,
			expected: types.PackConfig{
				Description: "",
				Priority:    0,
				Disabled:    false,
				Matchers: []types.MatcherConfig{
					{
						Trigger:        "filename",
						PowerUp:        "symlink",
						Pattern:        "",
						Priority:       0,
						Enabled:        boolPtr(true),
						PowerUpOptions: map[string]interface{}{},
					},
				},
			},
		},
		{
			name: "invalid_toml_syntax",
			content: `description = "Invalid TOML
priority = 10`,
			expectError: true,
			errorMsg:    "failed to parse TOML",
		},
		{
			name: "invalid_field_type",
			content: `description = "Test"
priority = "not a number"`,
			expectError: true,
			errorMsg:    "failed to parse TOML",
		},
		{
			name: "empty_file",
			content: "",
			expected: types.PackConfig{
				Description: "",
				Priority:    0,
				Disabled:    false,
				Matchers:    []types.MatcherConfig{},
			},
		},
		{
			name: "only_comments",
			content: `# This is a comment
# Another comment`,
			expected: types.PackConfig{
				Description: "",
				Priority:    0,
				Disabled:    false,
				Matchers:    []types.MatcherConfig{},
			},
		},
		{
			name: "nested_powerup_options",
			content: `[[matchers]]
trigger = "filename"
powerup = "symlink"
[matchers.options]
target = "~/.config"
force = true
nested = { key = "value", num = 42 }`,
			expected: types.PackConfig{
				Matchers: []types.MatcherConfig{
					{
						Trigger: "filename",
						PowerUp: "symlink",
						Pattern: "",
						Priority: 0,
						Enabled: boolPtr(true),
						Options: map[string]interface{}{
							"target": "~/.config",
							"force":  true,
							"nested": map[string]interface{}{
								"key": "value",
								"num": int64(42),
							},
						},
						PowerUpOptions: map[string]interface{}{},
					},
				},
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Create temp file with content
			tmpDir := testutil.TempDir(t, "config-test")
			configPath := filepath.Join(tmpDir, ".dodot.toml")
			testutil.CreateFile(t, tmpDir, ".dodot.toml", tt.content)

			// Test LoadPackConfig
			result, err := LoadPackConfig(configPath)

			if tt.expectError {
				testutil.AssertError(t, err)
				testutil.AssertContains(t, err.Error(), tt.errorMsg)
				return
			}

			testutil.AssertNoError(t, err)
			testutil.AssertEqual(t, tt.expected.Description, result.Description)
			testutil.AssertEqual(t, tt.expected.Priority, result.Priority)
			testutil.AssertEqual(t, tt.expected.Disabled, result.Disabled)
			testutil.AssertEqual(t, len(tt.expected.Matchers), len(result.Matchers))

			// Compare matchers
			for i, expectedMatcher := range tt.expected.Matchers {
				if i >= len(result.Matchers) {
					break
				}
				actualMatcher := result.Matchers[i]
				testutil.AssertEqual(t, expectedMatcher.Trigger, actualMatcher.Trigger)
				testutil.AssertEqual(t, expectedMatcher.PowerUp, actualMatcher.PowerUp)
				testutil.AssertEqual(t, expectedMatcher.Pattern, actualMatcher.Pattern)
				testutil.AssertEqual(t, expectedMatcher.Priority, actualMatcher.Priority)
				testutil.AssertEqual(t, expectedMatcher.Enabled, actualMatcher.Enabled)
				testutil.AssertEqual(t, expectedMatcher.Target, actualMatcher.Target)
				
				// Compare Options field
				if expectedMatcher.Options != nil {
					testutil.AssertNotNil(t, actualMatcher.Options, "Options should not be nil")
					testutil.AssertEqual(t, len(expectedMatcher.Options), len(actualMatcher.Options))
					for k, v := range expectedMatcher.Options {
						testutil.AssertEqual(t, v, actualMatcher.Options[k])
					}
				}
			}
		})
	}
}

func TestLoadPackConfig_FileErrors(t *testing.T) {
	tests := []struct {
		name        string
		setupFunc   func(t *testing.T) string
		expectError bool
		errorMsg    string
	}{
		{
			name: "non_existent_file",
			setupFunc: func(t *testing.T) string {
				return "/non/existent/path/.dodot.toml"
			},
			expectError: true,
			errorMsg:    "failed to read config file",
		},
		{
			name: "directory_instead_of_file",
			setupFunc: func(t *testing.T) string {
				tmpDir := testutil.TempDir(t, "config-test")
				dirPath := filepath.Join(tmpDir, ".dodot.toml")
				err := os.Mkdir(dirPath, 0755)
				testutil.AssertNoError(t, err)
				return dirPath
			},
			expectError: true,
			errorMsg:    "failed to read config file",
		},
		{
			name: "no_read_permission",
			setupFunc: func(t *testing.T) string {
				if os.Getuid() == 0 {
					t.Skip("Cannot test permission errors as root")
				}
				tmpDir := testutil.TempDir(t, "config-test")
				configPath := filepath.Join(tmpDir, ".dodot.toml")
				testutil.CreateFile(t, tmpDir, ".dodot.toml", "description = \"test\"")
				err := os.Chmod(configPath, 0000)
				testutil.AssertNoError(t, err)
				return configPath
			},
			expectError: true,
			errorMsg:    "failed to read config file",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			configPath := tt.setupFunc(t)
			
			_, err := LoadPackConfig(configPath)
			
			if tt.expectError {
				testutil.AssertError(t, err)
				testutil.AssertContains(t, err.Error(), tt.errorMsg)
			} else {
				testutil.AssertNoError(t, err)
			}
		})
	}
}

func TestLoadPackConfig_LargeFile(t *testing.T) {
	// Test with a large config file
	tmpDir := testutil.TempDir(t, "config-test")
	
	// Generate large content
	content := `description = "Large config test"
priority = 5
`
	
	// Add many matchers
	for i := 0; i < 100; i++ {
		content += fmt.Sprintf(`
[[matchers]]
trigger = "filename"
powerup = "symlink"
pattern = "file%d"
priority = %d
enabled = true
`, i, i)
	}
	
	configPath := filepath.Join(tmpDir, ".dodot.toml")
	testutil.CreateFile(t, tmpDir, ".dodot.toml", content)
	
	result, err := LoadPackConfig(configPath)
	testutil.AssertNoError(t, err)
	testutil.AssertEqual(t, "Large config test", result.Description)
	testutil.AssertEqual(t, 100, len(result.Matchers))
}

func TestFileExists(t *testing.T) {
	tests := []struct {
		name     string
		setup    func(t *testing.T) string
		expected bool
	}{
		{
			name: "existing_file",
			setup: func(t *testing.T) string {
				tmpDir := testutil.TempDir(t, "fileexists-test")
				path := filepath.Join(tmpDir, "test.txt")
				testutil.CreateFile(t, tmpDir, "test.txt", "content")
				return path
			},
			expected: true,
		},
		{
			name: "non_existent_file",
			setup: func(t *testing.T) string {
				return "/non/existent/file.txt"
			},
			expected: false,
		},
		{
			name: "directory_not_file",
			setup: func(t *testing.T) string {
				tmpDir := testutil.TempDir(t, "fileexists-test")
				return tmpDir
			},
			expected: false,
		},
		{
			name: "symlink_to_file",
			setup: func(t *testing.T) string {
				tmpDir := testutil.TempDir(t, "fileexists-test")
				filePath := filepath.Join(tmpDir, "target.txt")
				linkPath := filepath.Join(tmpDir, "link.txt")
				testutil.CreateFile(t, tmpDir, "target.txt", "content")
				err := os.Symlink(filePath, linkPath)
				testutil.AssertNoError(t, err)
				return linkPath
			},
			expected: true,
		},
		{
			name: "broken_symlink",
			setup: func(t *testing.T) string {
				tmpDir := testutil.TempDir(t, "fileexists-test")
				linkPath := filepath.Join(tmpDir, "broken.txt")
				err := os.Symlink("/non/existent/target", linkPath)
				testutil.AssertNoError(t, err)
				return linkPath
			},
			expected: false,
		},
		{
			name: "empty_path",
			setup: func(t *testing.T) string {
				return ""
			},
			expected: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			path := tt.setup(t)
			result := FileExists(path)
			testutil.AssertEqual(t, tt.expected, result)
		})
	}
}

// Benchmark tests
func BenchmarkLoadPackConfig(b *testing.B) {
	// Create a test config
	tmpDir := b.TempDir()
	content := `description = "Benchmark test"
priority = 10

[[matchers]]
trigger = "filename"
powerup = "symlink"
pattern = ".*\\.conf"
[matchers.options]
target = "~"

[[matchers]]
trigger = "directory"
powerup = "shell_profile"
pattern = "scripts"`

	configPath := filepath.Join(tmpDir, ".dodot.toml")
	err := os.WriteFile(configPath, []byte(content), 0644)
	if err != nil {
		b.Fatal(err)
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_, err := LoadPackConfig(configPath)
		if err != nil {
			b.Fatal(err)
		}
	}
}

func BenchmarkFileExists(b *testing.B) {
	// Create a test file
	tmpDir := b.TempDir()
	filePath := filepath.Join(tmpDir, "test.txt")
	err := os.WriteFile(filePath, []byte("test"), 0644)
	if err != nil {
		b.Fatal(err)
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_ = FileExists(filePath)
	}
}