// Test Type: Unit Test
// Description: Tests for the config package - pack configuration handling

package config_test

// Note: Some tests in this file use package config directly for testing
// unexported functions

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

func TestPackConfig_IsForceHome(t *testing.T) {
	tests := []struct {
		name      string
		forceHome []string
		relPath   string
		expected  bool
	}{
		{
			name:      "exact_match",
			forceHome: []string{"myconfig", "otherconfig"},
			relPath:   "myconfig",
			expected:  true,
		},
		{
			name:      "no_match",
			forceHome: []string{"myconfig", "otherconfig"},
			relPath:   "someconfig",
			expected:  false,
		},
		{
			name:      "glob_match_star",
			forceHome: []string{"*.conf", "*.ini"},
			relPath:   "app.conf",
			expected:  true,
		},
		{
			name:      "glob_match_question",
			forceHome: []string{"config?", "test?"},
			relPath:   "config1",
			expected:  true,
		},
		{
			name:      "subdirectory_exact",
			forceHome: []string{"configs/app.conf"},
			relPath:   "configs/app.conf",
			expected:  true,
		},
		{
			name:      "subdirectory_glob",
			forceHome: []string{"configs/*.conf"},
			relPath:   "configs/app.conf",
			expected:  true,
		},
		{
			name:      "basename_match",
			forceHome: []string{"*.conf"},
			relPath:   "some/deep/path/app.conf",
			expected:  true,
		},
		{
			name:      "empty_force_home",
			forceHome: []string{},
			relPath:   "anything",
			expected:  false,
		},
		{
			name:      "multiple_patterns",
			forceHome: []string{"*.conf", "special", "configs/*"},
			relPath:   "configs/something",
			expected:  true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			pc := &config.PackConfig{
				Symlink: config.Symlink{
					ForceHome: tt.forceHome,
				},
			}
			result := pc.IsForceHome(tt.relPath)
			assert.Equal(t, tt.expected, result)
		})
	}
}

func TestLoadPackConfig_WithSymlink(t *testing.T) {
	// Create temp directory and config file
	tempDir := t.TempDir()
	configPath := filepath.Join(tempDir, ".dodot.toml")

	configContent := `[mappings]
ignore = ["test-file.txt"]

[symlink]
force_home = ["*.conf", "special-config", "configs/*"]
`
	err := os.WriteFile(configPath, []byte(configContent), 0644)
	require.NoError(t, err)

	// Load the config
	cfg, err := config.LoadPackConfig(configPath)
	require.NoError(t, err)

	// Verify mappings still work
	assert.Equal(t, []string{"test-file.txt"}, cfg.Mappings.Ignore)

	// Verify symlink configuration loaded
	assert.Equal(t, []string{"*.conf", "special-config", "configs/*"}, cfg.Symlink.ForceHome)
}

func TestLoadPackConfig_NoSymlink(t *testing.T) {
	// Create temp directory and config file without symlink section
	tempDir := t.TempDir()
	configPath := filepath.Join(tempDir, ".dodot.toml")

	configContent := `[mappings]
ignore = ["test-file.txt"]
`
	err := os.WriteFile(configPath, []byte(configContent), 0644)
	require.NoError(t, err)

	// Load the config
	cfg, err := config.LoadPackConfig(configPath)
	require.NoError(t, err)

	// Verify mappings work
	assert.Equal(t, []string{"test-file.txt"}, cfg.Mappings.Ignore)

	// Verify symlink.force_home is empty (default)
	assert.Empty(t, cfg.Symlink.ForceHome)
}

func TestGetMergedProtectedPaths(t *testing.T) {
	tests := []struct {
		name            string
		rootProtected   map[string]bool
		packProtected   []string
		expectedPaths   []string
		unexpectedPaths []string
	}{
		{
			name: "merge_root_and_pack_protected",
			rootProtected: map[string]bool{
				".ssh/id_rsa":      true,
				".aws/credentials": true,
			},
			packProtected: []string{
				".myapp/secret.key",
				".config/app/token",
			},
			expectedPaths: []string{
				".ssh/id_rsa",
				".aws/credentials",
				".myapp/secret.key",
				".config/app/token",
			},
		},
		{
			name:          "pack_only_protected",
			rootProtected: map[string]bool{},
			packProtected: []string{
				"private.key",
				"secrets/*",
			},
			expectedPaths: []string{
				"private.key",
				"secrets/*",
			},
		},
		{
			name: "root_only_protected",
			rootProtected: map[string]bool{
				".gnupg":               true,
				".ssh/authorized_keys": true,
			},
			packProtected: []string{},
			expectedPaths: []string{
				".gnupg",
				".ssh/authorized_keys",
			},
		},
		{
			name: "duplicate_entries",
			rootProtected: map[string]bool{
				".ssh/id_rsa": true,
			},
			packProtected: []string{
				".ssh/id_rsa", // Duplicate - should still only appear once
			},
			expectedPaths: []string{
				".ssh/id_rsa",
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			pc := &config.PackConfig{
				Symlink: config.Symlink{
					ProtectedPaths: tt.packProtected,
				},
			}

			merged := pc.GetMergedProtectedPaths(tt.rootProtected)

			// Check expected paths are in merged
			for _, path := range tt.expectedPaths {
				assert.True(t, merged[path], "Expected %s to be in merged protected paths", path)
			}

			// Check we have the right number of entries
			assert.Equal(t, len(tt.expectedPaths), len(merged))
		})
	}
}

func TestLoadPackConfig_WithProtectedPaths(t *testing.T) {
	tempDir := t.TempDir()
	configPath := filepath.Join(tempDir, ".dodot.toml")

	configContent := `[mappings]
ignore = ["test-file.txt"]

[symlink]
force_home = ["*.conf"]
protected_paths = [".myapp/secret.key", "private/*", "credentials.json"]
`
	err := os.WriteFile(configPath, []byte(configContent), 0644)
	require.NoError(t, err)

	// Load the config
	cfg, err := config.LoadPackConfig(configPath)
	require.NoError(t, err)

	// Verify all sections loaded correctly
	assert.Equal(t, []string{"test-file.txt"}, cfg.Mappings.Ignore)
	assert.Equal(t, []string{"*.conf"}, cfg.Symlink.ForceHome)
	assert.Equal(t, []string{".myapp/secret.key", "private/*", "credentials.json"}, cfg.Symlink.ProtectedPaths)
}
