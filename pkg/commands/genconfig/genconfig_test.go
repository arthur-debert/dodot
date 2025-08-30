package genconfig

import (
	"path/filepath"
	"strings"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestGenConfig(t *testing.T) {
	t.Run("output to stdout", func(t *testing.T) {
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
		defer env.Cleanup()

		result, err := GenConfig(GenConfigOptions{
			DotfilesRoot: env.DotfilesRoot,
			Write:        false,
			FileSystem:   env.FS,
		})

		require.NoError(t, err)
		assert.NotEmpty(t, result.ConfigContent)
		assert.Contains(t, result.ConfigContent, "[pack]")
		assert.Contains(t, result.ConfigContent, "[symlink]")
		assert.Contains(t, result.ConfigContent, "[mappings]")
		assert.Empty(t, result.FilesWritten)

		// Verify that configuration values are commented out
		lines := strings.Split(result.ConfigContent, "\n")
		for _, line := range lines {
			trimmed := strings.TrimSpace(line)
			if trimmed == "" || strings.HasPrefix(trimmed, "#") ||
				(strings.HasPrefix(trimmed, "[") && strings.HasSuffix(trimmed, "]")) {
				continue
			}
			assert.Fail(t, "Found uncommented configuration line", "Line: %s", line)
		}

		// Verify specific commented values
		assert.Contains(t, result.ConfigContent, "# ignore = [")
		assert.Contains(t, result.ConfigContent, "# force_home = [")
		assert.Contains(t, result.ConfigContent, "# protected_paths = [")
		assert.Contains(t, result.ConfigContent, "# path = \"bin\"")
		assert.Contains(t, result.ConfigContent, "# install = \"install.sh\"")

		// Verify comments are preserved
		assert.Contains(t, result.ConfigContent, "# This is the config file for dodot")
		assert.Contains(t, result.ConfigContent, "# .ssh/ - security critical")
	})

	t.Run("write to single pack", func(t *testing.T) {
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
		defer env.Cleanup()

		// Create a pack
		env.SetupPack("vim", testutil.PackConfig{})

		result, err := GenConfig(GenConfigOptions{
			DotfilesRoot: env.DotfilesRoot,
			PackNames:    []string{"vim"},
			Write:        true,
			FileSystem:   env.FS,
		})

		require.NoError(t, err)
		assert.Len(t, result.FilesWritten, 1)
		assert.Contains(t, result.FilesWritten[0], filepath.Join("vim", ".dodot.toml"))

		// Check file exists and has correct content
		configPath := filepath.Join(env.DotfilesRoot, "vim", ".dodot.toml")
		content, err := env.FS.ReadFile(configPath)
		require.NoError(t, err)
		assert.Contains(t, string(content), "[pack]")
		assert.Contains(t, string(content), "# ignore = [")
	})

	t.Run("write to multiple packs", func(t *testing.T) {
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
		defer env.Cleanup()

		// Create multiple packs
		env.SetupPack("vim", testutil.PackConfig{})
		env.SetupPack("git", testutil.PackConfig{})

		result, err := GenConfig(GenConfigOptions{
			DotfilesRoot: env.DotfilesRoot,
			PackNames:    []string{"vim", "git"},
			Write:        true,
			FileSystem:   env.FS,
		})

		require.NoError(t, err)
		assert.Len(t, result.FilesWritten, 2)

		// Check both files exist
		for _, pack := range []string{"vim", "git"} {
			configPath := filepath.Join(env.DotfilesRoot, pack, ".dodot.toml")
			_, err := env.FS.Stat(configPath)
			assert.NoError(t, err)
		}
	})

	t.Run("skip existing config files", func(t *testing.T) {
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
		defer env.Cleanup()

		// Create a pack with existing config
		env.SetupPack("vim", testutil.PackConfig{
			Files: map[string]string{
				".dodot.toml": "# existing config",
			},
		})

		result, err := GenConfig(GenConfigOptions{
			DotfilesRoot: env.DotfilesRoot,
			PackNames:    []string{"vim"},
			Write:        true,
			FileSystem:   env.FS,
		})

		require.NoError(t, err)
		assert.Empty(t, result.FilesWritten) // No files written because it already exists

		// Check file wasn't overwritten
		configPath := filepath.Join(env.DotfilesRoot, "vim", ".dodot.toml")
		content, err := env.FS.ReadFile(configPath)
		require.NoError(t, err)
		assert.Equal(t, "# existing config", string(content))
	})

	t.Run("create missing pack directories", func(t *testing.T) {
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
		defer env.Cleanup()

		// Don't create pack directory beforehand
		result, err := GenConfig(GenConfigOptions{
			DotfilesRoot: env.DotfilesRoot,
			PackNames:    []string{"newpack"},
			Write:        true,
			FileSystem:   env.FS,
		})

		require.NoError(t, err)
		assert.Len(t, result.FilesWritten, 1)

		// Check directory and file were created
		configPath := filepath.Join(env.DotfilesRoot, "newpack", ".dodot.toml")
		_, err = env.FS.Stat(configPath)
		assert.NoError(t, err)
	})

	t.Run("write to current directory when no packs specified", func(t *testing.T) {
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
		defer env.Cleanup()

		result, err := GenConfig(GenConfigOptions{
			DotfilesRoot: env.DotfilesRoot,
			Write:        true,
			FileSystem:   env.FS,
		})

		require.NoError(t, err)
		assert.Len(t, result.FilesWritten, 1)
		assert.Equal(t, ".dodot.toml", result.FilesWritten[0])

		// Check file exists in current directory
		_, err = env.FS.Stat(".dodot.toml")
		assert.NoError(t, err)
	})
}

func TestGenConfigIntegration(t *testing.T) {
	t.Run("generated config works with init command", func(t *testing.T) {
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
		defer env.Cleanup()

		// Generate config content
		result, err := GenConfig(GenConfigOptions{
			DotfilesRoot: env.DotfilesRoot,
			Write:        false,
			FileSystem:   env.FS,
		})
		require.NoError(t, err)

		// The content should be the same as what init command uses
		// This ensures consistency across commands
		assert.Contains(t, result.ConfigContent, "[pack]")
		assert.Contains(t, result.ConfigContent, "[symlink]")
		assert.Contains(t, result.ConfigContent, "[mappings]")

		// All values should be commented out
		lines := strings.Split(result.ConfigContent, "\n")
		for _, line := range lines {
			trimmed := strings.TrimSpace(line)
			if trimmed != "" && !strings.HasPrefix(trimmed, "#") &&
				(!strings.HasPrefix(trimmed, "[") || !strings.HasSuffix(trimmed, "]")) {
				assert.Fail(t, "Found uncommented value line", "Line: %s", line)
			}
		}
	})
}

func TestCommentOutConfigValues(t *testing.T) {
	tests := []struct {
		name     string
		input    string
		expected string
	}{
		{
			name: "simple config",
			input: `[section]
key = "value"`,
			expected: `[section]
# key = "value"`,
		},
		{
			name: "preserves comments",
			input: `# This is a comment
[section]
key = "value"
# Another comment
key2 = "value2"`,
			expected: `# This is a comment
[section]
# key = "value"
# Another comment
# key2 = "value2"`,
		},
		{
			name: "preserves blank lines",
			input: `[section]

key = "value"

[section2]`,
			expected: `[section]

# key = "value"

[section2]`,
		},
		{
			name: "handles arrays",
			input: `[section]
array = [
    "item1",
    "item2"
]`,
			expected: `[section]
# array = [
#     "item1",
#     "item2"
# ]`,
		},
		{
			name: "handles already commented lines",
			input: `[section]
# already commented
key = "value"`,
			expected: `[section]
# already commented
# key = "value"`,
		},
		{
			name: "preserves section headers",
			input: `[pack]
ignore = []
[symlink]
force_home = []`,
			expected: `[pack]
# ignore = []
[symlink]
# force_home = []`,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := commentOutConfigValues(tt.input)
			assert.Equal(t, tt.expected, result)
		})
	}
}
