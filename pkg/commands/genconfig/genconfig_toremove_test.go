package genconfig

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestGenConfig(t *testing.T) {
	t.Run("output to stdout", func(t *testing.T) {
		result, err := GenConfig(GenConfigOptions{
			Write: false,
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

	t.Run("write to current directory", func(t *testing.T) {
		tmpDir := t.TempDir()
		oldWd, _ := os.Getwd()
		defer func() { _ = os.Chdir(oldWd) }()
		require.NoError(t, os.Chdir(tmpDir))

		result, err := GenConfig(GenConfigOptions{
			Write: true,
		})
		require.NoError(t, err)
		assert.Len(t, result.FilesWritten, 1)
		assert.Equal(t, ".dodot.toml", result.FilesWritten[0])

		// Check file exists
		_, err = os.Stat(".dodot.toml")
		assert.NoError(t, err)
	})

	t.Run("write to pack directories", func(t *testing.T) {
		tmpDir := t.TempDir()

		// Create pack directories
		vimDir := filepath.Join(tmpDir, "vim")
		gitDir := filepath.Join(tmpDir, "git")
		require.NoError(t, os.MkdirAll(vimDir, 0755))
		require.NoError(t, os.MkdirAll(gitDir, 0755))

		result, err := GenConfig(GenConfigOptions{
			DotfilesRoot: tmpDir,
			PackNames:    []string{"vim", "git"},
			Write:        true,
		})
		require.NoError(t, err)
		assert.Len(t, result.FilesWritten, 2)

		// Check files exist
		_, err = os.Stat(filepath.Join(vimDir, ".dodot.toml"))
		assert.NoError(t, err)
		_, err = os.Stat(filepath.Join(gitDir, ".dodot.toml"))
		assert.NoError(t, err)
	})

	t.Run("skip existing files", func(t *testing.T) {
		tmpDir := t.TempDir()

		// Create existing file
		existingFile := filepath.Join(tmpDir, ".dodot.toml")
		require.NoError(t, os.WriteFile(existingFile, []byte("existing"), 0644))

		oldWd, _ := os.Getwd()
		defer func() { _ = os.Chdir(oldWd) }()
		require.NoError(t, os.Chdir(tmpDir))

		result, err := GenConfig(GenConfigOptions{
			Write: true,
		})
		require.NoError(t, err)
		assert.Empty(t, result.FilesWritten) // No files written because it already exists

		// Check file wasn't overwritten
		content, _ := os.ReadFile(existingFile)
		assert.Equal(t, "existing", string(content))
	})

	t.Run("create missing directories", func(t *testing.T) {
		tmpDir := t.TempDir()

		// Don't create pack directory beforehand
		result, err := GenConfig(GenConfigOptions{
			DotfilesRoot: tmpDir,
			PackNames:    []string{"newpack"},
			Write:        true,
		})
		require.NoError(t, err)
		assert.Len(t, result.FilesWritten, 1)

		// Check directory and file were created
		_, err = os.Stat(filepath.Join(tmpDir, "newpack", ".dodot.toml"))
		assert.NoError(t, err)
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
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := commentOutConfigValues(tt.input)
			assert.Equal(t, tt.expected, result)
		})
	}
}
