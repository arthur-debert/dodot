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

func TestLoadPackConfig(t *testing.T) {
	tests := []struct {
		name        string
		tomlContent string
		wantError   bool
		validate    func(t *testing.T, cfg config.PackConfig)
	}{
		{
			name: "valid_config_with_mappings",
			tomlContent: `
[mappings]
path = "bin"
install = "install.sh"
shell = ["aliases.sh", "profile.sh"]
homebrew = "Brewfile"
ignore = [".env.local", "*.secret"]
`,
			wantError: false,
			validate: func(t *testing.T, cfg config.PackConfig) {
				assert.Equal(t, "bin", cfg.Mappings.Path)
				assert.Equal(t, "install.sh", cfg.Mappings.Install)
				assert.Equal(t, []string{"aliases.sh", "profile.sh"}, cfg.Mappings.Shell)
				assert.Equal(t, "Brewfile", cfg.Mappings.Homebrew)
				assert.Equal(t, []string{".env.local", "*.secret"}, cfg.Mappings.Ignore)
			},
		},
		{
			name:        "empty_config",
			tomlContent: ``,
			wantError:   false,
			validate: func(t *testing.T, cfg config.PackConfig) {
				assert.Empty(t, cfg.Mappings.Path)
				assert.Empty(t, cfg.Mappings.Install)
				assert.Empty(t, cfg.Mappings.Shell)
				assert.Empty(t, cfg.Mappings.Homebrew)
				assert.Empty(t, cfg.Mappings.Ignore)
			},
		},
		{
			name:        "invalid_toml",
			tomlContent: `[invalid toml content`,
			wantError:   true,
		},
		{
			name: "only_ignore_in_mappings",
			tomlContent: `
[mappings]
ignore = ["*.log", "cache/"]
`,
			wantError: false,
			validate: func(t *testing.T, cfg config.PackConfig) {
				assert.Equal(t, []string{"*.log", "cache/"}, cfg.Mappings.Ignore)
				assert.Empty(t, cfg.Mappings.Path)
				assert.Empty(t, cfg.Mappings.Install)
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
