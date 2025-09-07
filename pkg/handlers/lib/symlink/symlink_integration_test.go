package symlink_test

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/handlers/lib/symlink"
	"github.com/arthur-debert/dodot/pkg/operations"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestSymlinkHandler_WithRealConfig(t *testing.T) {
	// Create a temporary directory structure
	tmpDir := t.TempDir()

	// Create a root config with custom protected paths
	rootConfig := `
[symlink]
protected_paths = [".myapp/secret", ".custom/password.txt"]
`
	configPath := filepath.Join(tmpDir, "dodot.toml")
	require.NoError(t, os.WriteFile(configPath, []byte(rootConfig), 0644))

	// Load the config
	cfg, err := config.GetRootConfig(tmpDir)
	require.NoError(t, err)

	// Verify config loaded correctly
	assert.NotNil(t, cfg.Security.ProtectedPaths)
	assert.True(t, cfg.Security.ProtectedPaths[".myapp/secret"])
	assert.True(t, cfg.Security.ProtectedPaths[".custom/password.txt"])
	// Should also have defaults
	assert.True(t, cfg.Security.ProtectedPaths[".ssh/id_rsa"])
	assert.True(t, cfg.Security.ProtectedPaths[".gnupg"])

	// Test symlink handler with this config
	handler := symlink.NewHandler()

	tests := []struct {
		name          string
		file          string
		shouldError   bool
		errorContains string
	}{
		{
			name:        "allows_normal_file",
			file:        ".vimrc",
			shouldError: false,
		},
		{
			name:          "blocks_default_protected",
			file:          ".ssh/id_rsa",
			shouldError:   true,
			errorContains: "cannot symlink protected file",
		},
		{
			name:          "blocks_custom_protected",
			file:          ".myapp/secret",
			shouldError:   true,
			errorContains: "cannot symlink protected file",
		},
		{
			name:          "blocks_custom_protected_2",
			file:          ".custom/password.txt",
			shouldError:   true,
			errorContains: "cannot symlink protected file",
		},
		{
			name:          "blocks_subdirectory_of_protected",
			file:          ".gnupg/private-keys/key.asc",
			shouldError:   true,
			errorContains: "cannot symlink protected file",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			files := []operations.FileInput{
				{
					RelativePath: tt.file,
					PackName:     "test-pack",
				},
			}

			ops, err := handler.ToOperations(files, cfg)

			if tt.shouldError {
				require.Error(t, err)
				assert.Contains(t, err.Error(), tt.errorContains)
			} else {
				require.NoError(t, err)
				assert.NotNil(t, ops)
			}
		})
	}
}

func TestSymlinkHandler_PackLevelProtectedPaths(t *testing.T) {
	// This test documents the expected behavior for pack-level protected paths
	// Currently, pack-level protected paths are not implemented

	tmpDir := t.TempDir()
	packDir := filepath.Join(tmpDir, "mypack")
	require.NoError(t, os.MkdirAll(packDir, 0755))

	// Create root config
	rootConfig := `
[symlink]
protected_paths = [".root/secret"]
`
	require.NoError(t, os.WriteFile(filepath.Join(tmpDir, "dodot.toml"), []byte(rootConfig), 0644))

	// Create pack config
	packConfig := `
[symlink]
protected_paths = [".pack/secret"]
`
	require.NoError(t, os.WriteFile(filepath.Join(packDir, "dodot.toml"), []byte(packConfig), 0644))

	// Load root config
	rootCfg, err := config.GetRootConfig(tmpDir)
	require.NoError(t, err)

	// Load pack config
	packCfg, err := config.GetPackConfig(rootCfg, packDir)
	require.NoError(t, err)

	// Verify both configs have their protected paths
	assert.True(t, rootCfg.Security.ProtectedPaths[".root/secret"])
	assert.True(t, packCfg.Security.ProtectedPaths[".root/secret"]) // inherited
	assert.True(t, packCfg.Security.ProtectedPaths[".pack/secret"]) // pack-specific

	// Test with pack config
	handler := symlink.NewHandler()

	// Should block pack-specific protected path
	files := []operations.FileInput{
		{RelativePath: ".pack/secret", PackName: "mypack"},
	}
	_, err = handler.ToOperations(files, packCfg)
	require.Error(t, err)
	assert.Contains(t, err.Error(), "cannot symlink protected file")

	// Should also block root-level protected path
	files = []operations.FileInput{
		{RelativePath: ".root/secret", PackName: "mypack"},
	}
	_, err = handler.ToOperations(files, packCfg)
	require.Error(t, err)
	assert.Contains(t, err.Error(), "cannot symlink protected file")
}
