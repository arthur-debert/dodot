// Test Type: Unit Test
// Description: Tests for the config package - pack configuration handling

package config_test

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestPackConfig_IsIgnored(t *testing.T) {
	tests := []struct {
		name     string
		config   config.PackConfig
		filename string
		want     bool
	}{
		{
			name: "exact_match_ignored",
			config: config.PackConfig{
				Ignore: []config.IgnoreRule{
					{Path: "secret.txt"},
					{Path: "backup.bak"},
				},
			},
			filename: "secret.txt",
			want:     true,
		},
		{
			name: "pattern_match_ignored",
			config: config.PackConfig{
				Ignore: []config.IgnoreRule{
					{Path: "*.tmp"},
					{Path: "*.cache"},
				},
			},
			filename: "data.tmp",
			want:     true,
		},
		{
			name: "no_match_not_ignored",
			config: config.PackConfig{
				Ignore: []config.IgnoreRule{
					{Path: "*.tmp"},
					{Path: "secret.txt"},
				},
			},
			filename: "config.conf",
			want:     false,
		},
		{
			name:     "empty_ignore_rules",
			config:   config.PackConfig{},
			filename: "anything.txt",
			want:     false,
		},
		{
			name: "complex_pattern_match",
			config: config.PackConfig{
				Ignore: []config.IgnoreRule{
					{Path: "test_*_backup.txt"},
					{Path: "[!.]*.swp"},
				},
			},
			filename: "test_data_backup.txt",
			want:     true,
		},
		{
			name: "glob_star_pattern",
			config: config.PackConfig{
				Ignore: []config.IgnoreRule{
					{Path: "*.log"},
					{Path: "temp*"},
				},
			},
			filename: "application.log",
			want:     true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := tt.config.IsIgnored(tt.filename)
			assert.Equal(t, tt.want, got)
		})
	}
}

func TestPackConfig_FindOverride(t *testing.T) {
	tests := []struct {
		name          string
		config        config.PackConfig
		filename      string
		wantOverride  bool
		wantHandler   string
		wantWithValue interface{}
	}{
		{
			name: "exact_match_preferred",
			config: config.PackConfig{
				Override: []config.OverrideRule{
					{
						Path:    "*.sh",
						Handler: "shell",
						With:    map[string]interface{}{"placement": "append"},
					},
					{
						Path:    "init.sh",
						Handler: "special",
						With:    map[string]interface{}{"priority": "high"},
					},
				},
			},
			filename:      "init.sh",
			wantOverride:  true,
			wantHandler:   "special",
			wantWithValue: "high",
		},
		{
			name: "pattern_match",
			config: config.PackConfig{
				Override: []config.OverrideRule{
					{
						Path:    "*.conf",
						Handler: "symlink",
						With:    map[string]interface{}{"force": true},
					},
				},
			},
			filename:      "app.conf",
			wantOverride:  true,
			wantHandler:   "symlink",
			wantWithValue: true,
		},
		{
			name: "no_match",
			config: config.PackConfig{
				Override: []config.OverrideRule{
					{
						Path:    "*.sh",
						Handler: "shell",
					},
					{
						Path:    "*.conf",
						Handler: "symlink",
					},
				},
			},
			filename:     "data.txt",
			wantOverride: false,
		},
		{
			name:         "empty_overrides",
			config:       config.PackConfig{},
			filename:     "anything.txt",
			wantOverride: false,
		},
		{
			name: "longer_pattern_wins",
			config: config.PackConfig{
				Override: []config.OverrideRule{
					{
						Path:    "*.sh",
						Handler: "generic",
					},
					{
						Path:    "scripts/*.sh",
						Handler: "specific",
					},
				},
			},
			filename:     "scripts/deploy.sh",
			wantOverride: true,
			wantHandler:  "specific",
		},
		{
			name: "multiple_patterns_longest_wins",
			config: config.PackConfig{
				Override: []config.OverrideRule{
					{
						Path:    "*",
						Handler: "fallback",
					},
					{
						Path:    "*.config",
						Handler: "config",
					},
					{
						Path:    "app.*.config",
						Handler: "app-config",
					},
				},
			},
			filename:     "app.prod.config",
			wantOverride: true,
			wantHandler:  "app-config",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			override := tt.config.FindOverride(tt.filename)

			if tt.wantOverride {
				require.NotNil(t, override)
				assert.Equal(t, tt.wantHandler, override.Handler)

				// Check specific With values if expected
				if tt.wantWithValue != nil && override.With != nil {
					// Find the first key in the With map for testing
					for key, value := range override.With {
						if key == "priority" || key == "force" || key == "placement" {
							assert.Equal(t, tt.wantWithValue, value)
							break
						}
					}
				}
			} else {
				assert.Nil(t, override)
			}
		})
	}
}

func TestLoadPackConfig(t *testing.T) {
	tests := []struct {
		name        string
		tomlContent string
		wantError   bool
		validate    func(t *testing.T, cfg config.PackConfig)
	}{
		{
			name: "valid_config_all_sections",
			tomlContent: `
[[ignore]]
path = "*.tmp"

[[ignore]]
path = "secret.txt"

[[override]]
path = "*.sh"
handler = "shell"
[override.with]
placement = "prepend"

[[override]]
path = "config.toml"
handler = "symlink"
[override.with]
force = true

[mappings]
bin = "~/bin"
config = "~/.config/app"
`,
			wantError: false,
			validate: func(t *testing.T, cfg config.PackConfig) {
				assert.Len(t, cfg.Ignore, 2)
				assert.Equal(t, "*.tmp", cfg.Ignore[0].Path)
				assert.Equal(t, "secret.txt", cfg.Ignore[1].Path)

				assert.Len(t, cfg.Override, 2)
				assert.Equal(t, "*.sh", cfg.Override[0].Path)
				assert.Equal(t, "shell", cfg.Override[0].Handler)
				assert.Equal(t, "prepend", cfg.Override[0].With["placement"])

				assert.Equal(t, "config.toml", cfg.Override[1].Path)
				assert.Equal(t, "symlink", cfg.Override[1].Handler)
				assert.Equal(t, true, cfg.Override[1].With["force"])

				assert.Len(t, cfg.Mappings, 2)
				assert.Equal(t, "~/bin", cfg.Mappings["bin"])
				assert.Equal(t, "~/.config/app", cfg.Mappings["config"])
			},
		},
		{
			name:        "empty_config",
			tomlContent: ``,
			wantError:   false,
			validate: func(t *testing.T, cfg config.PackConfig) {
				assert.Empty(t, cfg.Ignore)
				assert.Empty(t, cfg.Override)
				assert.Empty(t, cfg.Mappings)
			},
		},
		{
			name:        "invalid_toml",
			tomlContent: `[invalid toml content`,
			wantError:   true,
		},
		{
			name: "only_ignore_section",
			tomlContent: `
[[ignore]]
path = "*.log"

[[ignore]]
path = "cache/"
`,
			wantError: false,
			validate: func(t *testing.T, cfg config.PackConfig) {
				assert.Len(t, cfg.Ignore, 2)
				assert.Empty(t, cfg.Override)
				assert.Empty(t, cfg.Mappings)
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Create temporary config file
			tempDir := t.TempDir()
			configPath := filepath.Join(tempDir, ".dodot.toml")

			err := os.WriteFile(configPath, []byte(tt.tomlContent), 0644)
			require.NoError(t, err)

			// Test loading
			cfg, err := config.LoadPackConfig(configPath)

			if tt.wantError {
				assert.Error(t, err)
			} else {
				require.NoError(t, err)
				if tt.validate != nil {
					tt.validate(t, cfg)
				}
			}
		})
	}
}

func TestLoadPackConfig_FileErrors(t *testing.T) {
	t.Run("non_existent_file", func(t *testing.T) {
		cfg, err := config.LoadPackConfig("/non/existent/path/.dodot.toml")
		assert.Error(t, err)
		assert.Contains(t, err.Error(), "failed to read config file")
		assert.Equal(t, config.PackConfig{}, cfg)
	})

	t.Run("directory_instead_of_file", func(t *testing.T) {
		tempDir := t.TempDir()
		_, err := config.LoadPackConfig(tempDir)
		assert.Error(t, err)
		assert.Contains(t, err.Error(), "failed to read config file")
	})
}

func TestFileExists(t *testing.T) {
	tempDir := t.TempDir()

	// Create a test file
	existingFile := filepath.Join(tempDir, "exists.txt")
	err := os.WriteFile(existingFile, []byte("test"), 0644)
	require.NoError(t, err)

	// Create a directory
	existingDir := filepath.Join(tempDir, "testdir")
	err = os.Mkdir(existingDir, 0755)
	require.NoError(t, err)

	tests := []struct {
		name string
		path string
		want bool
	}{
		{
			name: "existing_file",
			path: existingFile,
			want: true,
		},
		{
			name: "non_existent_file",
			path: filepath.Join(tempDir, "nonexistent.txt"),
			want: false,
		},
		{
			name: "directory_returns_false",
			path: existingDir,
			want: false,
		},
		{
			name: "empty_path",
			path: "",
			want: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := config.FileExists(tt.path)
			assert.Equal(t, tt.want, got)
		})
	}
}
