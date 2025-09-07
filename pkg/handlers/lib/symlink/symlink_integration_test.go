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

	// Set DOTFILES_ROOT for pack config loading
	oldRoot := os.Getenv("DOTFILES_ROOT")
	require.NoError(t, os.Setenv("DOTFILES_ROOT", tmpDir))
	defer func() {
		require.NoError(t, os.Setenv("DOTFILES_ROOT", oldRoot))
	}()

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

func TestSymlinkHandler_ForceHomeWithRealConfig(t *testing.T) {
	// Create a temporary directory structure
	tmpDir := t.TempDir()

	// Create a root config with custom force_home patterns
	rootConfig := `
[symlink]
force_home = ["myapp", "custom"]
`
	configPath := filepath.Join(tmpDir, "dodot.toml")
	require.NoError(t, os.WriteFile(configPath, []byte(rootConfig), 0644))

	// Load the config
	cfg, err := config.GetRootConfig(tmpDir)
	require.NoError(t, err)

	// Verify config loaded correctly
	assert.NotNil(t, cfg.LinkPaths.CoreUnixExceptions)
	t.Logf("CoreUnixExceptions: %v", cfg.LinkPaths.CoreUnixExceptions)
	assert.True(t, cfg.LinkPaths.CoreUnixExceptions["myapp"])
	assert.True(t, cfg.LinkPaths.CoreUnixExceptions["custom"])
	// Should also have defaults
	assert.True(t, cfg.LinkPaths.CoreUnixExceptions["ssh"])
	// Note: gitconfig is in defaults.toml, not embedded dodot.toml
	// assert.True(t, cfg.LinkPaths.CoreUnixExceptions["gitconfig"])

	// Set up HOME for consistent testing
	originalHome := os.Getenv("HOME")
	testHome := "/test/home"
	require.NoError(t, os.Setenv("HOME", testHome))
	defer func() {
		require.NoError(t, os.Setenv("HOME", originalHome))
	}()

	// Also set XDG_CONFIG_HOME for consistent testing
	originalXDG := os.Getenv("XDG_CONFIG_HOME")
	require.NoError(t, os.Setenv("XDG_CONFIG_HOME", filepath.Join(testHome, ".config")))
	defer func() {
		require.NoError(t, os.Setenv("XDG_CONFIG_HOME", originalXDG))
	}()

	// Test symlink handler with this config
	handler := symlink.NewHandler()

	tests := []struct {
		name           string
		file           string
		expectedTarget string
		reason         string
	}{
		{
			name:           "default_force_home_ssh",
			file:           "ssh/config",
			expectedTarget: filepath.Join(testHome, ".ssh/config"),
			reason:         "ssh is in default force_home",
		},
		{
			name:           "custom_force_home_myapp",
			file:           "myapp/settings.json",
			expectedTarget: filepath.Join(testHome, ".myapp/settings.json"),
			reason:         "myapp is in custom force_home",
		},
		{
			name:           "custom_force_home_custom",
			file:           "custom",
			expectedTarget: filepath.Join(testHome, ".custom"),
			reason:         "custom is in custom force_home",
		},
		{
			name:           "normal_xdg_placement",
			file:           "nvim/init.vim",
			expectedTarget: filepath.Join(testHome, ".config/nvim/init.vim"),
			reason:         "nvim is not in force_home, should use XDG",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			files := []operations.FileInput{
				{
					RelativePath: tt.file,
					PackName:     "test-pack",
					SourcePath:   "/source/" + tt.file,
				},
			}

			ops, err := handler.ToOperations(files, cfg)
			require.NoError(t, err)
			require.Len(t, ops, 2) // CreateDataLink and CreateUserLink

			// Check the target path in CreateUserLink operation
			assert.Equal(t, tt.expectedTarget, ops[1].Target, tt.reason)
		})
	}
}
