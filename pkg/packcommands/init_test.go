package packcommands_test

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/arthur-debert/dodot/pkg/packcommands"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestInitialize(t *testing.T) {
	tests := []struct {
		name          string
		packName      string
		expectedFiles []string
		expectedError string
		checkContent  bool
	}{
		{
			name:     "successful pack creation",
			packName: "testpack",
			expectedFiles: []string{
				".dodot.toml",
				"README.txt",
				"profile.sh", // From fill command
				"install.sh", // From fill command
				"Brewfile",   // From fill command
				"bin",        // Directory from fill command
			},
			checkContent: true,
		},
		{
			name:          "empty pack name",
			packName:      "",
			expectedError: "pack name cannot be empty",
		},
		{
			name:          "invalid pack name with slash",
			packName:      "test/pack",
			expectedError: "pack name contains invalid characters",
		},
		{
			name:          "invalid pack name with backslash",
			packName:      "test\\pack",
			expectedError: "pack name contains invalid characters",
		},
		{
			name:          "invalid pack name with special chars",
			packName:      "test*pack",
			expectedError: "pack name contains invalid characters",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Create test environment
			env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
			defer env.Cleanup()

			// Run init command
			opts := packcommands.InitOptions{
				DotfilesRoot: env.DotfilesRoot,
				PackName:     tt.packName,
				FileSystem:   env.FS,
			}

			result, err := packcommands.Initialize(opts)

			// Check error
			if tt.expectedError != "" {
				require.Error(t, err)
				assert.Contains(t, err.Error(), tt.expectedError)
				return
			}

			require.NoError(t, err)
			require.NotNil(t, result)

			// Check result
			assert.Equal(t, "init", result.Command, "command should be init")
			// Note: Pack status is only available if GetPackStatus function is provided
			// which is not the case in these tests

			// Check expected files were created
			assert.Equal(t, len(tt.expectedFiles), result.Metadata.FilesCreated,
				"Expected %d files, got %d: %v", len(tt.expectedFiles), result.Metadata.FilesCreated, result.Metadata.CreatedPaths)

			// Verify each expected file
			packPath := filepath.Join(env.DotfilesRoot, tt.packName)
			for _, expectedFile := range tt.expectedFiles {
				found := false
				for _, createdFile := range result.Metadata.CreatedPaths {
					if createdFile == expectedFile {
						found = true
						break
					}
				}
				assert.True(t, found, "Expected file %s was not created", expectedFile)

				// Check file/directory exists
				fullPath := filepath.Join(packPath, expectedFile)
				info, err := env.FS.Stat(fullPath)
				require.NoError(t, err, "File %s should exist", expectedFile)

				if expectedFile == "bin" {
					assert.True(t, info.IsDir(), "bin should be a directory")
				} else {
					assert.False(t, info.IsDir(), "%s should be a file", expectedFile)
				}

				// Check specific file content if requested
				if tt.checkContent && !info.IsDir() {
					content, err := env.FS.ReadFile(fullPath)
					require.NoError(t, err)

					switch expectedFile {
					case ".dodot.toml":
						// Should be commented out config
						assert.Contains(t, string(content), "[pack]")
						assert.Contains(t, string(content), "[symlink]")
						assert.Contains(t, string(content), "[mappings]")
						assert.Contains(t, string(content), "# ignore =")

					case "README.txt":
						assert.Contains(t, string(content), "dodot Pack: "+tt.packName)
						assert.Contains(t, string(content), "Getting Started:")

					case "install.sh":
						// Should be executable
						assert.Equal(t, os.FileMode(0755), info.Mode().Perm())
						assert.Contains(t, string(content), "#!/")

					case "profile.sh", "Brewfile":
						// Should have content from handler templates
						assert.NotEmpty(t, content)
					}
				}
			}
		})
	}
}

func TestInitializeExistingPack(t *testing.T) {
	// Create test environment
	env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
	defer env.Cleanup()

	packName := "existing"
	packPath := filepath.Join(env.DotfilesRoot, packName)

	// Create existing pack directory
	require.NoError(t, env.FS.MkdirAll(packPath, 0755))

	// Try to init over existing pack
	opts := packcommands.InitOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackName:     packName,
		FileSystem:   env.FS,
	}

	result, err := packcommands.Initialize(opts)

	// Should error
	require.Error(t, err)
	assert.Contains(t, err.Error(), "already exists")
	assert.Nil(t, result)
}

func TestInitializeIntegration(t *testing.T) {
	// This test verifies that init creates a valid pack that can be used with other commands
	t.Run("created pack is valid", func(t *testing.T) {
		// Create test environment
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
		defer env.Cleanup()

		// Init a new pack
		packName := "integration"
		opts := packcommands.InitOptions{
			DotfilesRoot: env.DotfilesRoot,
			PackName:     packName,
			FileSystem:   env.FS,
		}

		result, err := packcommands.Initialize(opts)
		require.NoError(t, err)
		require.NotNil(t, result)

		// Verify the pack structure is valid
		packPath := filepath.Join(env.DotfilesRoot, packName)

		// Check .dodot.toml exists and is valid TOML
		configPath := filepath.Join(packPath, ".dodot.toml")
		configContent, err := env.FS.ReadFile(configPath)
		require.NoError(t, err)

		// Should have proper TOML structure
		content := string(configContent)
		assert.True(t, strings.Contains(content, "[pack]") || strings.Contains(content, "[symlink]") || strings.Contains(content, "[mappings]"))

		// Should have comments
		assert.Contains(t, content, "#")

		// README should reference the pack name
		readmePath := filepath.Join(packPath, "README.txt")
		readmeContent, err := env.FS.ReadFile(readmePath)
		require.NoError(t, err)
		assert.Contains(t, string(readmeContent), packName)

		// All template files should exist
		expectedTemplates := []string{"profile.sh", "install.sh", "Brewfile"}
		for _, template := range expectedTemplates {
			templatePath := filepath.Join(packPath, template)
			_, err := env.FS.Stat(templatePath)
			assert.NoError(t, err, "Template %s should exist", template)
		}

		// bin directory should exist
		binPath := filepath.Join(packPath, "bin")
		info, err := env.FS.Stat(binPath)
		require.NoError(t, err)
		assert.True(t, info.IsDir())
	})
}