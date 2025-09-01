package core_test

import (
	"path/filepath"
	"strings"
	"testing"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestGenerateOutput_Config(t *testing.T) {
	t.Run("output to stdout", func(t *testing.T) {
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
		defer env.Cleanup()

		result, err := core.GenerateOutput(core.OutputOptions{
			Type:  core.OutputTypeConfig,
			Write: false,
			Config: &core.ConfigOutputOptions{
				DotfilesRoot: env.DotfilesRoot,
			},
			FileSystem: env.FS,
		})

		require.NoError(t, err)
		assert.NotEmpty(t, result.Content)
		assert.Contains(t, result.Content, "[pack]")
		assert.Contains(t, result.Content, "[symlink]")
		assert.Contains(t, result.Content, "[mappings]")
		assert.Empty(t, result.FilesWritten)

		// Verify that configuration values are commented out
		lines := strings.Split(result.Content, "\n")
		for _, line := range lines {
			trimmed := strings.TrimSpace(line)
			if trimmed == "" || strings.HasPrefix(trimmed, "#") ||
				(strings.HasPrefix(trimmed, "[") && strings.HasSuffix(trimmed, "]")) {
				continue
			}
			assert.Fail(t, "Found uncommented configuration line", "Line: %s", line)
		}

		// Verify specific commented values
		assert.Contains(t, result.Content, "# ignore = [")
		assert.Contains(t, result.Content, "# force_home = [")
		assert.Contains(t, result.Content, "# protected_paths = [")
		assert.Contains(t, result.Content, "# path = \"bin\"")
		assert.Contains(t, result.Content, "# install = \"install.sh\"")

		// Verify comments are preserved
		assert.Contains(t, result.Content, "# This is the config file for dodot")
	})

	t.Run("write to current directory", func(t *testing.T) {
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
		defer env.Cleanup()

		result, err := core.GenerateOutput(core.OutputOptions{
			Type:  core.OutputTypeConfig,
			Write: true,
			Config: &core.ConfigOutputOptions{
				DotfilesRoot: env.DotfilesRoot,
				PackNames:    []string{}, // Empty means current directory
			},
			FileSystem: env.FS,
		})

		require.NoError(t, err)
		assert.NotEmpty(t, result.Content)
		assert.Len(t, result.FilesWritten, 1)
		assert.Equal(t, ".dodot.toml", result.FilesWritten[0])

		// Verify file was written
		content, err := env.FS.ReadFile(".dodot.toml")
		require.NoError(t, err)
		assert.Equal(t, result.Content, string(content))
	})

	t.Run("write to pack directories", func(t *testing.T) {
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
		defer env.Cleanup()

		// Create pack directories
		pack1 := filepath.Join(env.DotfilesRoot, "vim")
		pack2 := filepath.Join(env.DotfilesRoot, "zsh")
		require.NoError(t, env.FS.MkdirAll(pack1, 0755))
		require.NoError(t, env.FS.MkdirAll(pack2, 0755))

		result, err := core.GenerateOutput(core.OutputOptions{
			Type:  core.OutputTypeConfig,
			Write: true,
			Config: &core.ConfigOutputOptions{
				DotfilesRoot: env.DotfilesRoot,
				PackNames:    []string{"vim", "zsh"},
			},
			FileSystem: env.FS,
		})

		require.NoError(t, err)
		assert.Len(t, result.FilesWritten, 2)

		// Verify files were written
		for _, pack := range []string{"vim", "zsh"} {
			configPath := filepath.Join(env.DotfilesRoot, pack, ".dodot.toml")
			content, err := env.FS.ReadFile(configPath)
			require.NoError(t, err)
			assert.Equal(t, result.Content, string(content))
		}
	})

	t.Run("skip existing files", func(t *testing.T) {
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
		defer env.Cleanup()

		// Create existing config file
		existingContent := "# Existing config\n[pack]\n"
		require.NoError(t, env.FS.WriteFile(".dodot.toml", []byte(existingContent), 0644))

		result, err := core.GenerateOutput(core.OutputOptions{
			Type:  core.OutputTypeConfig,
			Write: true,
			Config: &core.ConfigOutputOptions{
				DotfilesRoot: env.DotfilesRoot,
				PackNames:    []string{},
			},
			FileSystem: env.FS,
		})

		require.NoError(t, err)
		assert.Empty(t, result.FilesWritten)

		// Verify existing file was not overwritten
		content, err := env.FS.ReadFile(".dodot.toml")
		require.NoError(t, err)
		assert.Equal(t, existingContent, string(content))
	})
}

func TestGenerateOutput_Snippet(t *testing.T) {
	t.Run("bash snippet", func(t *testing.T) {
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
		defer env.Cleanup()

		result, err := core.GenerateOutput(core.OutputOptions{
			Type:  core.OutputTypeSnippet,
			Write: false,
			Snippet: &core.SnippetOutputOptions{
				Shell:   "bash",
				DataDir: "/home/user/.local/share/dodot",
			},
			FileSystem: env.FS,
		})

		require.NoError(t, err)
		assert.NotEmpty(t, result.Content)
		assert.Contains(t, result.Content, "dodot-init.sh")
		assert.Empty(t, result.FilesWritten) // Snippets are never written

		// Check metadata
		assert.Equal(t, "bash", result.Metadata["shell"])
		assert.Equal(t, "/home/user/.local/share/dodot", result.Metadata["dataDir"])
		assert.Equal(t, false, result.Metadata["installed"])
	})

	t.Run("fish snippet", func(t *testing.T) {
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
		defer env.Cleanup()

		result, err := core.GenerateOutput(core.OutputOptions{
			Type:  core.OutputTypeSnippet,
			Write: false,
			Snippet: &core.SnippetOutputOptions{
				Shell:   "fish",
				DataDir: "/home/user/.local/share/dodot",
			},
			FileSystem: env.FS,
		})

		require.NoError(t, err)
		assert.NotEmpty(t, result.Content)
		assert.Contains(t, result.Content, "dodot-init.fish")
		assert.Equal(t, "fish", result.Metadata["shell"])
	})

	t.Run("snippet with provision", func(t *testing.T) {
		// Skip this test for now - InstallShellIntegration needs to be refactored
		// to accept a filesystem parameter before this can work with memory FS
		t.Skip("InstallShellIntegration uses OS filesystem directly")
	})
}

func TestGenerateOutput_Errors(t *testing.T) {
	t.Run("missing config options", func(t *testing.T) {
		_, err := core.GenerateOutput(core.OutputOptions{
			Type:  core.OutputTypeConfig,
			Write: false,
			// Config is nil
		})
		assert.Error(t, err)
		assert.Contains(t, err.Error(), "config options required")
	})

	t.Run("missing snippet options", func(t *testing.T) {
		_, err := core.GenerateOutput(core.OutputOptions{
			Type:  core.OutputTypeSnippet,
			Write: false,
			// Snippet is nil
		})
		assert.Error(t, err)
		assert.Contains(t, err.Error(), "snippet options required")
	})

	t.Run("unknown output type", func(t *testing.T) {
		_, err := core.GenerateOutput(core.OutputOptions{
			Type: "unknown",
		})
		assert.Error(t, err)
		assert.Contains(t, err.Error(), "unknown output type")
	})
}
