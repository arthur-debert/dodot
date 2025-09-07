package symlink

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/operations"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestHandler_ProtectedPaths(t *testing.T) {
	tests := []struct {
		name          string
		files         []operations.FileInput
		config        *config.Config
		shouldError   bool
		errorContains string
	}{
		{
			name: "blocks_exact_protected_path",
			files: []operations.FileInput{
				{
					RelativePath: ".ssh/id_rsa",
					PackName:     "test-pack",
				},
			},
			config: &config.Config{
				Security: config.Security{
					ProtectedPaths: map[string]bool{
						".ssh/id_rsa": true,
					},
				},
			},
			shouldError:   true,
			errorContains: "cannot symlink protected file: .ssh/id_rsa",
		},
		{
			name: "blocks_protected_subdirectory",
			files: []operations.FileInput{
				{
					RelativePath: ".gnupg/private-keys-v1.d/secret.key",
					PackName:     "test-pack",
				},
			},
			config: &config.Config{
				Security: config.Security{
					ProtectedPaths: map[string]bool{
						".gnupg": true,
					},
				},
			},
			shouldError:   true,
			errorContains: "cannot symlink protected file: .gnupg/private-keys-v1.d/secret.key",
		},
		{
			name: "allows_non_protected_path",
			files: []operations.FileInput{
				{
					RelativePath: ".vimrc",
					PackName:     "test-pack",
				},
			},
			config: &config.Config{
				Security: config.Security{
					ProtectedPaths: map[string]bool{
						".ssh/id_rsa": true,
					},
				},
			},
			shouldError: false,
		},
		{
			name: "blocks_custom_protected_path",
			files: []operations.FileInput{
				{
					RelativePath: ".myapp/secrets.json",
					PackName:     "test-pack",
				},
			},
			config: &config.Config{
				Security: config.Security{
					ProtectedPaths: map[string]bool{
						".ssh/id_rsa":         true,
						".myapp/secrets.json": true,
					},
				},
			},
			shouldError:   true,
			errorContains: "cannot symlink protected file: .myapp/secrets.json",
		},
		{
			name: "handles_path_without_leading_dot",
			files: []operations.FileInput{
				{
					RelativePath: "ssh/id_rsa",
					PackName:     "test-pack",
				},
			},
			config: &config.Config{
				Security: config.Security{
					ProtectedPaths: map[string]bool{
						".ssh/id_rsa": true,
					},
				},
			},
			shouldError:   true,
			errorContains: "cannot symlink protected file: ssh/id_rsa",
		},
		{
			name: "uses_defaults_when_config_nil",
			files: []operations.FileInput{
				{
					RelativePath: ".ssh/authorized_keys",
					PackName:     "test-pack",
				},
			},
			config:        nil,
			shouldError:   true,
			errorContains: "cannot symlink protected file: .ssh/authorized_keys",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			handler := NewHandler()
			ops, err := handler.ToOperations(tt.files, tt.config)

			if tt.shouldError {
				require.Error(t, err)
				assert.Contains(t, err.Error(), tt.errorContains)
				assert.Nil(t, ops)
			} else {
				require.NoError(t, err)
				assert.NotNil(t, ops)
				// Should create 2 operations per file (CreateDataLink and CreateUserLink)
				assert.Len(t, ops, len(tt.files)*2)
			}
		})
	}
}

func TestIsProtected(t *testing.T) {
	protectedPaths := map[string]bool{
		".ssh/id_rsa":          true,
		".ssh/authorized_keys": true,
		".gnupg":               true,
		".aws/credentials":     true,
		"myapp/secret":         true, // without leading dot
	}

	tests := []struct {
		path     string
		expected bool
		reason   string
	}{
		// Exact matches
		{".ssh/id_rsa", true, "exact match with dot"},
		{"ssh/id_rsa", true, "match without leading dot"},
		{"./.ssh/id_rsa", true, "match with ./ prefix"},

		// Subdirectory matches
		{".gnupg/private-keys-v1.d/secret.key", true, "subdirectory of protected path"},
		{"gnupg/something/else", true, "subdirectory without leading dot"},

		// Non-matches
		{".vimrc", false, "not in protected list"},
		{".ssh-backup/id_rsa", false, "similar but different path"},
		{"ssh-keys/id_rsa", false, "different directory name"},

		// Path without leading dot in config
		{"myapp/secret", true, "exact match without dot"},
		{".myapp/secret", true, "with dot added"},
	}

	for _, tt := range tests {
		t.Run(tt.path, func(t *testing.T) {
			result := isProtected(tt.path, protectedPaths)
			assert.Equal(t, tt.expected, result, tt.reason)
		})
	}
}

func TestGetProtectedPaths(t *testing.T) {
	t.Run("returns_config_paths_when_available", func(t *testing.T) {
		cfg := &config.Config{
			Security: config.Security{
				ProtectedPaths: map[string]bool{
					".ssh/id_rsa":     true,
					".custom/secret":  true,
					".myapp/password": true,
				},
			},
		}

		paths := getProtectedPaths(cfg)
		assert.Len(t, paths, 3)
		assert.True(t, paths[".ssh/id_rsa"])
		assert.True(t, paths[".custom/secret"])
		assert.True(t, paths[".myapp/password"])
	})

	t.Run("returns_defaults_when_config_nil", func(t *testing.T) {
		paths := getProtectedPaths(nil)

		// Check some default paths
		assert.True(t, paths[".ssh/id_rsa"])
		assert.True(t, paths[".ssh/id_ed25519"])
		assert.True(t, paths[".gnupg"])
		assert.True(t, paths[".aws/credentials"])
		assert.True(t, paths[".password-store"])
	})

	t.Run("returns_defaults_when_wrong_type", func(t *testing.T) {
		paths := getProtectedPaths("not a config")

		// Should fallback to defaults
		assert.True(t, paths[".ssh/id_rsa"])
		assert.True(t, paths[".gnupg"])
	})
}
