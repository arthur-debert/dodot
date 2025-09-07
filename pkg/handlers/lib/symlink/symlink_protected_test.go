package symlink

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/operations"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestHandler_ToOperations_ProtectedPaths(t *testing.T) {
	// Config is now passed as parameter, not global

	// Clear pack configs
	// Pack configs no longer needed with dependency injection

	tests := []struct {
		name           string
		protectedPaths map[string]bool
		files          []operations.FileInput
		expectError    bool
		errorContains  string
	}{
		{
			name: "allows_non_protected_file",
			protectedPaths: map[string]bool{
				".ssh/id_rsa": true,
				".gnupg":      true,
			},
			files: []operations.FileInput{
				{
					PackName:     "vim",
					RelativePath: "vimrc",
					SourcePath:   "/pack/vim/vimrc",
				},
			},
			expectError: false,
		},
		{
			name: "blocks_exact_match_protected_file",
			protectedPaths: map[string]bool{
				".ssh/id_rsa": true,
			},
			files: []operations.FileInput{
				{
					PackName:     "ssh",
					RelativePath: ".ssh/id_rsa",
					SourcePath:   "/pack/ssh/.ssh/id_rsa",
				},
			},
			expectError:   true,
			errorContains: "cannot symlink protected file: .ssh/id_rsa",
		},
		{
			name: "blocks_file_without_dot_prefix",
			protectedPaths: map[string]bool{
				".ssh/authorized_keys": true,
			},
			files: []operations.FileInput{
				{
					PackName:     "ssh",
					RelativePath: "ssh/authorized_keys",
					SourcePath:   "/pack/ssh/ssh/authorized_keys",
				},
			},
			expectError:   true,
			errorContains: "cannot symlink protected file: ssh/authorized_keys",
		},
		{
			name: "blocks_file_in_protected_directory",
			protectedPaths: map[string]bool{
				".gnupg": true,
			},
			files: []operations.FileInput{
				{
					PackName:     "gpg",
					RelativePath: ".gnupg/private-keys-v1.d/key.key",
					SourcePath:   "/pack/gpg/.gnupg/private-keys-v1.d/key.key",
				},
			},
			expectError:   true,
			errorContains: "cannot symlink protected file: .gnupg/private-keys-v1.d/key.key",
		},
		{
			name: "blocks_first_protected_file_in_batch",
			protectedPaths: map[string]bool{
				".aws/credentials": true,
			},
			files: []operations.FileInput{
				{
					PackName:     "configs",
					RelativePath: "vimrc",
					SourcePath:   "/pack/configs/vimrc",
				},
				{
					PackName:     "configs",
					RelativePath: ".aws/credentials",
					SourcePath:   "/pack/configs/.aws/credentials",
				},
			},
			expectError:   true,
			errorContains: "cannot symlink protected file: .aws/credentials",
		},
		{
			name: "handles_nested_path_correctly",
			protectedPaths: map[string]bool{
				".config/gh/hosts.yml": true,
			},
			files: []operations.FileInput{
				{
					PackName:     "gh",
					RelativePath: ".config/gh/config.yml", // Not protected
					SourcePath:   "/pack/gh/.config/gh/config.yml",
				},
			},
			expectError: false,
		},
		{
			name: "blocks_nested_protected_path",
			protectedPaths: map[string]bool{
				".config/gh/hosts.yml": true,
			},
			files: []operations.FileInput{
				{
					PackName:     "gh",
					RelativePath: ".config/gh/hosts.yml",
					SourcePath:   "/pack/gh/.config/gh/hosts.yml",
				},
			},
			expectError:   true,
			errorContains: "cannot symlink protected file: .config/gh/hosts.yml",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Create test config with specified protected paths
			testConfig := &config.Config{
				Security: config.Security{
					ProtectedPaths: tt.protectedPaths,
				},
			}

			handler := NewHandler()
			ops, err := handler.ToOperations(tt.files, testConfig)

			if tt.expectError {
				require.Error(t, err)
				assert.Contains(t, err.Error(), tt.errorContains)
				assert.Nil(t, ops)
			} else {
				require.NoError(t, err)
				assert.NotNil(t, ops)
				// Should create 2 operations per file (data link + user link)
				assert.Len(t, ops, len(tt.files)*2)
			}
		})
	}
}

func TestHandler_ToOperations_PackLevelProtectedPaths(t *testing.T) {
	// Test pack-level protected paths by including them in the config

	tests := []struct {
		name          string
		rootProtected map[string]bool
		packProtected []string
		files         []operations.FileInput
		expectError   bool
		errorContains string
	}{
		{
			name: "pack_protected_path_blocked",
			rootProtected: map[string]bool{
				".ssh/id_rsa": true,
			},
			packProtected: []string{".myapp/secret.key", "private/*"},
			files: []operations.FileInput{
				{
					PackName:     "mypack",
					RelativePath: ".myapp/secret.key",
					SourcePath:   "/pack/myapp/.myapp/secret.key",
				},
			},
			expectError:   true,
			errorContains: "cannot symlink protected file: .myapp/secret.key",
		},
		{
			name: "root_and_pack_both_work",
			rootProtected: map[string]bool{
				".ssh/id_rsa": true,
			},
			packProtected: []string{".myapp/secret.key"},
			files: []operations.FileInput{
				{
					PackName:     "mypack",
					RelativePath: ".ssh/id_rsa", // Root protected
					SourcePath:   "/pack/mypack/.ssh/id_rsa",
				},
			},
			expectError:   true,
			errorContains: "cannot symlink protected file: .ssh/id_rsa",
		},
		{
			name: "non_protected_allowed_with_pack_config",
			rootProtected: map[string]bool{
				".ssh/id_rsa": true,
			},
			packProtected: []string{".myapp/secret.key"},
			files: []operations.FileInput{
				{
					PackName:     "mypack",
					RelativePath: "config.toml",
					SourcePath:   "/pack/mypack/config.toml",
				},
			},
			expectError: false,
		},
		{
			name: "conflict_between_packs",
			rootProtected: map[string]bool{
				".gnupg": true,
			},
			packProtected: []string{},
			files: []operations.FileInput{
				{
					PackName:     "pack1",
					RelativePath: ".myapp/secret.key",
					SourcePath:   "/pack/pack1/.myapp/secret.key",
				},
				{
					PackName:     "pack2",
					RelativePath: ".myapp/secret.key", // Same target path - conflict
					SourcePath:   "/pack/pack2/.myapp/secret.key",
				},
			},
			expectError:   true,
			errorContains: "symlink conflict", // The actual error we get
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Create test config with merged protected paths
			// For testing, we'll merge pack-level paths into the config
			mergedPaths := make(map[string]bool)

			// Add root-level protected paths
			for path := range tt.rootProtected {
				mergedPaths[path] = true
			}

			// Add pack-level protected paths
			for _, path := range tt.packProtected {
				mergedPaths[path] = true
			}

			testConfig := &config.Config{
				Security: config.Security{
					ProtectedPaths: mergedPaths,
				},
			}

			handler := NewHandler()
			ops, err := handler.ToOperations(tt.files, testConfig)

			if tt.expectError {
				require.Error(t, err)
				assert.Contains(t, err.Error(), tt.errorContains)
				assert.Nil(t, ops)
			} else {
				require.NoError(t, err)
				assert.NotNil(t, ops)
				// Should create 2 operations per file
				assert.Len(t, ops, len(tt.files)*2)
			}
		})
	}
}

func TestIsProtected(t *testing.T) {
	protectedPaths := map[string]bool{
		".ssh/id_rsa":          true,
		".ssh/id_ed25519":      true,
		".gnupg":               true,
		".aws/credentials":     true,
		".config/gh/hosts.yml": true,
		".docker/config.json":  true,
	}

	tests := []struct {
		name     string
		filePath string
		expected bool
	}{
		// Exact matches
		{"exact_match_with_dot", ".ssh/id_rsa", true},
		{"exact_match_without_dot", "ssh/id_rsa", true},

		// Directory protection
		{"file_in_protected_directory", ".gnupg/trustdb.gpg", true},
		{"deep_file_in_protected_directory", ".gnupg/private-keys-v1.d/key.key", true},
		{"file_in_protected_directory_no_dot", "gnupg/trustdb.gpg", true},

		// Non-protected files
		{"non_protected_file", ".vimrc", false},
		{"non_protected_in_ssh", ".ssh/config", false},
		{"non_protected_config", ".config/nvim/init.vim", false},
		{"similar_but_different", ".aws/config", false},

		// Edge cases
		{"empty_path", "", false},
		{"single_dot", ".", false},
		{"double_dot", "..", false},
		{"path_with_leading_slash", "./.ssh/id_rsa", true},

		// Partial matches that should not be protected
		{"partial_match", ".docker/compose.yml", false},
		{"parent_not_protected", ".config/gh/config.yml", false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := isProtected(tt.filePath, protectedPaths)
			assert.Equal(t, tt.expected, result, "isProtected(%q) = %v, want %v", tt.filePath, result, tt.expected)
		})
	}
}
