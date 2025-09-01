package fill

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestFillPack(t *testing.T) {
	tests := []struct {
		name          string
		existingFiles map[string]string
		expectedFiles []string
		expectedError string
	}{
		{
			name:          "empty pack gets all handler files",
			existingFiles: map[string]string{},
			expectedFiles: []string{
				"profile.sh", // First shell file pattern from defaults
				"install.sh",
				"Brewfile",
				"bin", // Directory name without trailing slash
			},
		},
		{
			name: "pack with shell file gets remaining handlers",
			existingFiles: map[string]string{
				"profile.sh": "# profile",
			},
			expectedFiles: []string{
				"install.sh",
				"Brewfile",
				"bin", // Directory name without trailing slash
			},
		},
		{
			name: "pack with all handlers gets no new files",
			existingFiles: map[string]string{
				"aliases.sh": "# aliases",
				"install.sh": "#!/bin/bash",
				"Brewfile":   "brew 'git'",
				"bin/test":   "#!/bin/bash",
			},
			expectedFiles: []string{},
		},
		{
			name: "symlink-only files still need handler files",
			existingFiles: map[string]string{
				"vimrc":     "\" vim config",
				"gitconfig": "[user]",
			},
			expectedFiles: []string{
				"profile.sh", // First shell file pattern from defaults
				"install.sh",
				"Brewfile",
				"bin", // Directory name without trailing slash
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Create test environment
			env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
			defer env.Cleanup()

			// Create pack with existing files
			packPath := filepath.Join(env.DotfilesRoot, "testpack")
			fs := env.FS
			require.NoError(t, fs.MkdirAll(packPath, 0755))

			// Create existing files
			for filename, content := range tt.existingFiles {
				fullPath := filepath.Join(packPath, filename)
				if strings.HasSuffix(filename, "/") {
					// It's a directory
					require.NoError(t, fs.MkdirAll(fullPath, 0755))
				} else {
					// Ensure parent directory exists
					dir := filepath.Dir(fullPath)
					if dir != packPath {
						require.NoError(t, fs.MkdirAll(dir, 0755))
					}
					require.NoError(t, fs.WriteFile(fullPath, []byte(content), 0644))
				}
			}

			// Run fill command
			opts := FillPackOptions{
				DotfilesRoot: env.DotfilesRoot,
				PackName:     "testpack",
				FileSystem:   fs,
			}

			result, err := FillPack(opts)

			// Check error
			if tt.expectedError != "" {
				require.Error(t, err)
				assert.Contains(t, err.Error(), tt.expectedError)
				return
			}

			require.NoError(t, err)
			require.NotNil(t, result)

			// Check result
			assert.Equal(t, "fill", result.Command, "command should be fill")
			assert.Equal(t, len(tt.expectedFiles), result.Metadata.FilesCreated,
				"Expected %d files, got %d: %v", len(tt.expectedFiles), result.Metadata.FilesCreated, result.Metadata.CreatedPaths)
			assert.True(t, len(result.Packs) > 0, "should have pack status")
			if len(result.Packs) > 0 {
				assert.Equal(t, "testpack", result.Packs[0].Name, "pack name should match")
			}

			// Verify files were created
			for _, expectedFile := range tt.expectedFiles {
				found := false
				for _, createdFile := range result.Metadata.CreatedPaths {
					if createdFile == expectedFile {
						found = true
						break
					}
				}
				assert.True(t, found, "Expected file %s was not created", expectedFile)

				// Check file exists
				fullPath := filepath.Join(packPath, expectedFile)
				if expectedFile == "bin" { // Special case for path handler directory
					// Check directory exists
					info, err := fs.Stat(fullPath)
					require.NoError(t, err)
					assert.True(t, info.IsDir(), "Expected %s to be a directory", expectedFile)
				} else {
					// Check file exists and has content
					content, err := fs.ReadFile(fullPath)
					require.NoError(t, err)
					assert.NotEmpty(t, content, "Template file %s should have content", expectedFile)

					// Check executable permission for install.sh
					if expectedFile == "install.sh" {
						info, err := fs.Stat(fullPath)
						require.NoError(t, err)
						assert.Equal(t, os.FileMode(0755), info.Mode().Perm(),
							"install.sh should be executable")
					}
				}
			}
		})
	}
}

func TestFillPackErrors(t *testing.T) {
	tests := []struct {
		name          string
		packName      string
		setupFunc     func(*testutil.TestEnvironment)
		expectedError string
	}{
		{
			name:          "non-existent pack",
			packName:      "nonexistent",
			expectedError: "pack(s) not found",
		},
		{
			name:     "invalid pack name",
			packName: "",
			setupFunc: func(env *testutil.TestEnvironment) {
				// Create a pack with empty name (shouldn't be possible)
			},
			expectedError: "pack(s) not found",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Create test environment
			env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
			defer env.Cleanup()

			// Run setup if provided
			if tt.setupFunc != nil {
				tt.setupFunc(env)
			}

			// Run fill command
			opts := FillPackOptions{
				DotfilesRoot: env.DotfilesRoot,
				PackName:     tt.packName,
				FileSystem:   env.FS,
			}

			result, err := FillPack(opts)

			// Should error
			require.Error(t, err)
			assert.Contains(t, err.Error(), tt.expectedError)
			assert.Nil(t, result)
		})
	}
}

// TestFillPackWithCustomRules will be implemented when pack-specific rules are supported
// func TestFillPackWithCustomRules(t *testing.T) {
// 	// TODO: Implement when pack rules loading is implemented
// }
