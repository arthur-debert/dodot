package core

import (
	"path/filepath"
	"testing"
	"time"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestGetPackStatus(t *testing.T) {
	tests := []struct {
		name           string
		pack           types.Pack
		actions        []types.Action
		setupFS        func(fs types.FS, dataDir string)
		expectedStatus string
		expectedFiles  int
		checkResult    func(t *testing.T, result *types.DisplayPack)
	}{
		{
			name: "empty pack returns queue status",
			pack: types.Pack{
				Name: "empty",
				Path: "dotfiles/empty",
			},
			actions:        []types.Action{},
			setupFS:        func(fs types.FS, dataDir string) {},
			expectedStatus: "queue",
			expectedFiles:  0,
		},
		{
			name: "pack with all successful actions",
			pack: types.Pack{
				Name: "vim",
				Path: "dotfiles/vim",
			},
			actions: []types.Action{
				{
					Type:        types.ActionTypeLink,
					Description: "Link .vimrc",
					Source:      "dotfiles/vim/.vimrc",
					Target:      "home/user/.vimrc",
					Pack:        "vim",
					HandlerName: "symlink",
				},
			},
			setupFS: func(fs types.FS, dataDir string) {
				// Setup deployed symlink - note: no pack name in path
				deployedPath := filepath.Join(dataDir, "deployed", "symlink", ".vimrc")

				testutil.CreateDirT(t, fs, filepath.Dir(deployedPath))
				require.NoError(t, fs.Symlink("dotfiles/vim/.vimrc", deployedPath))

				// Create source file
				testutil.CreateFileT(t, fs, "dotfiles/vim/.vimrc", "vim config")

				// Create target symlink
				testutil.CreateDirT(t, fs, "home/user")
				require.NoError(t, fs.Symlink(deployedPath, "home/user/.vimrc"))
			},
			expectedStatus: "success",
			expectedFiles:  1,
			checkResult: func(t *testing.T, result *types.DisplayPack) {
				assert.Equal(t, "success", result.Files[0].Status)
				assert.Contains(t, result.Files[0].Message, "linked to")
				// Verify display path uses target basename
				assert.Equal(t, ".vimrc", result.Files[0].Path)
			},
		},
		{
			name: "pack with pending actions",
			pack: types.Pack{
				Name: "zsh",
				Path: "dotfiles/zsh",
			},
			actions: []types.Action{
				{
					Type:        types.ActionTypeLink,
					Description: "Link .zshrc",
					Source:      "dotfiles/zsh/.zshrc",
					Target:      "home/user/.zshrc",
					Pack:        "zsh",
					HandlerName: "symlink",
				},
			},
			setupFS: func(fs types.FS, dataDir string) {
				// Only create source file, no deployment
				testutil.CreateFileT(t, fs, "dotfiles/zsh/.zshrc", "zsh config")
			},
			expectedStatus: "queue",
			expectedFiles:  1,
			checkResult: func(t *testing.T, result *types.DisplayPack) {
				assert.Equal(t, "queue", result.Files[0].Status)
				assert.Contains(t, result.Files[0].Message, "→")
				// Verify display path uses target basename
				assert.Equal(t, ".zshrc", result.Files[0].Path)
			},
		},
		{
			name: "pack with error (broken symlink)",
			pack: types.Pack{
				Name: "broken",
				Path: "dotfiles/broken",
			},
			actions: []types.Action{
				{
					Type:        types.ActionTypeLink,
					Description: "Link config",
					Source:      "dotfiles/broken/config",
					Target:      "home/user/.config",
					Pack:        "broken",
					HandlerName: "symlink",
				},
			},
			setupFS: func(fs types.FS, dataDir string) {
				// Setup deployed symlink but missing source - note: no pack name in path
				deployedPath := filepath.Join(dataDir, "deployed", "symlink", ".config")

				testutil.CreateDirT(t, fs, filepath.Dir(deployedPath))
				require.NoError(t, fs.Symlink("dotfiles/broken/config", deployedPath))

				// Source file doesn't exist - broken link
				testutil.CreateDirT(t, fs, "home/user")
				require.NoError(t, fs.Symlink(deployedPath, "home/user/.config"))
			},
			expectedStatus: "partial",
			expectedFiles:  1,
			checkResult: func(t *testing.T, result *types.DisplayPack) {
				assert.Equal(t, "warning", result.Files[0].Status)
				assert.Contains(t, result.Files[0].Message, "dangling")
				// Verify display path uses source basename
				assert.Equal(t, "config", result.Files[0].Path)
			},
		},
		{
			name: "pack with .dodotignore",
			pack: types.Pack{
				Name: "ignored",
				Path: "dotfiles/ignored",
			},
			actions: []types.Action{}, // Should be empty for ignored packs
			setupFS: func(fs types.FS, dataDir string) {
				testutil.CreateFileT(t, fs, "dotfiles/ignored/.dodotignore", "")
			},
			expectedStatus: "ignored",
			expectedFiles:  1,
			checkResult: func(t *testing.T, result *types.DisplayPack) {
				assert.True(t, result.IsIgnored)
				assert.Equal(t, ".dodotignore", result.Files[0].Path)
				assert.Equal(t, "ignored", result.Files[0].Status)
			},
		},
		{
			name: "pack with .dodot.toml",
			pack: types.Pack{
				Name: "configured",
				Path: "dotfiles/configured",
			},
			actions: []types.Action{
				{
					Type:        types.ActionTypeLink,
					Description: "Link file",
					Source:      "dotfiles/configured/file",
					Target:      "home/user/file",
					Pack:        "configured",
					HandlerName: "symlink",
					Metadata:    map[string]interface{}{"override": true},
				},
			},
			setupFS: func(fs types.FS, dataDir string) {
				testutil.CreateFileT(t, fs, "dotfiles/configured/.dodot.toml", "")
				testutil.CreateFileT(t, fs, "dotfiles/configured/file", "content")
			},
			expectedStatus: "queue",
			expectedFiles:  2, // .dodot.toml + file
			checkResult: func(t *testing.T, result *types.DisplayPack) {
				assert.True(t, result.HasConfig)

				// Find config file
				var configFile *types.DisplayFile
				for i := range result.Files {
					if result.Files[i].Path == ".dodot.toml" {
						configFile = &result.Files[i]
						break
					}
				}
				require.NotNil(t, configFile)
				assert.Equal(t, "config", configFile.Status)
				assert.Equal(t, "config", configFile.Handler)

				// Find overridden file
				var overrideFile *types.DisplayFile
				for i := range result.Files {
					if result.Files[i].Path == "file" {
						overrideFile = &result.Files[i]
						break
					}
				}
				require.NotNil(t, overrideFile)
				assert.True(t, overrideFile.IsOverride)
			},
		},
		{
			name: "pack with mixed statuses becomes queue",
			pack: types.Pack{
				Name: "mixed",
				Path: "dotfiles/mixed",
			},
			actions: []types.Action{
				{
					Type:        types.ActionTypeLink,
					Description: "Link deployed",
					Source:      "dotfiles/mixed/deployed",
					Target:      "home/user/deployed",
					Pack:        "mixed",
					HandlerName: "symlink",
				},
				{
					Type:        types.ActionTypeLink,
					Description: "Link pending",
					Source:      "dotfiles/mixed/pending",
					Target:      "home/user/pending",
					Pack:        "mixed",
					HandlerName: "symlink",
				},
			},
			setupFS: func(fs types.FS, dataDir string) {
				// Setup first as deployed - note: target basename is used for deployed path
				deployedPath := filepath.Join(dataDir, "deployed", "symlink", "deployed")

				testutil.CreateDirT(t, fs, filepath.Dir(deployedPath))
				require.NoError(t, fs.Symlink("dotfiles/mixed/deployed", deployedPath))

				// Create source files
				testutil.CreateFileT(t, fs, "dotfiles/mixed/deployed", "deployed")
				testutil.CreateFileT(t, fs, "dotfiles/mixed/pending", "pending")

				// Create target for deployed
				testutil.CreateDirT(t, fs, "home/user")
				require.NoError(t, fs.Symlink(deployedPath, "home/user/deployed"))
			},
			expectedStatus: "queue",
			expectedFiles:  2,
			checkResult: func(t *testing.T, result *types.DisplayPack) {
				successCount := 0
				queueCount := 0
				for _, file := range result.Files {
					switch file.Status {
					case "success":
						successCount++
					case "queue":
						queueCount++
					}
				}
				assert.Equal(t, 1, successCount)
				assert.Equal(t, 1, queueCount)
			},
		},
		{
			name: "pack with different action types",
			pack: types.Pack{
				Name: "multi",
				Path: "dotfiles/multi",
			},
			actions: []types.Action{
				{
					Type:        types.ActionTypeBrew,
					Description: "Install homebrew packages",
					Source:      "dotfiles/multi/Brewfile",
					Pack:        "multi",
					HandlerName: "homebrew",
				},
				{
					Type:        types.ActionTypeInstall,
					Description: "Run install script",
					Source:      "dotfiles/multi/install.sh",
					Pack:        "multi",
					HandlerName: "install",
				},
				{
					Type:        types.ActionTypePathAdd,
					Description: "Add bin to PATH",
					Source:      "dotfiles/multi/bin",
					Pack:        "multi",
					HandlerName: "path",
				},
				{
					Type:        types.ActionTypeShellSource,
					Description: "Source aliases",
					Source:      "dotfiles/multi/aliases.sh",
					Pack:        "multi",
					HandlerName: "shell_profile",
				},
			},
			setupFS: func(fs types.FS, dataDir string) {
				// Create all source files
				brewContent := "brew 'git'"
				installContent := "#!/bin/sh"
				testutil.CreateFileT(t, fs, "dotfiles/multi/Brewfile", brewContent)
				testutil.CreateFileT(t, fs, "dotfiles/multi/install.sh", installContent)
				testutil.CreateDirT(t, fs, "dotfiles/multi/bin")
				testutil.CreateFileT(t, fs, "dotfiles/multi/aliases.sh", "alias ll='ls -l'")

				// Calculate checksums for the files
				brewChecksum := calculateTestChecksum([]byte(brewContent))
				installChecksum := calculateTestChecksum([]byte(installContent))

				// Setup some as deployed
				// Homebrew sentinel - uses pack_Brewfile.sentinel format with checksum:timestamp
				brewSentinel := filepath.Join(dataDir, "homebrew", "multi_Brewfile.sentinel")
				testutil.CreateFileT(t, fs, brewSentinel, brewChecksum+":2024-01-15T10:00:00Z")

				// Install script sentinel - uses pack_scriptname.sentinel format with checksum:timestamp
				installSentinel := filepath.Join(dataDir, "install", "multi_install.sh.sentinel")
				timestamp := time.Now().Format(time.RFC3339)
				testutil.CreateFileT(t, fs, installSentinel, installChecksum+":"+timestamp)
			},
			expectedStatus: "queue", // Mixed success and pending
			expectedFiles:  4,
			checkResult: func(t *testing.T, result *types.DisplayPack) {
				// Verify each action type is represented correctly
				handlerTypes := make(map[string]bool)
				statusCounts := make(map[string]int)
				for _, file := range result.Files {
					handlerTypes[file.Handler] = true
					statusCounts[file.Status]++
					t.Logf("File %s (%s): status=%s, message=%s", file.Path, file.Handler, file.Status, file.Message)
				}

				assert.True(t, handlerTypes["homebrew"])
				assert.True(t, handlerTypes["install_script"])
				assert.True(t, handlerTypes["path"])
				assert.True(t, handlerTypes["shell_profile"])

				// Should have 2 success (brew, install) and 2 pending (path, shell)
				assert.Equal(t, 2, statusCounts["success"], "Should have 2 successful actions")
				assert.Equal(t, 2, statusCounts["queue"], "Should have 2 pending actions")
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Setup filesystem
			fs := testutil.NewTestFS()
			dataDir := "data/dodot"

			// Setup paths
			paths := NewTestPaths(dataDir)

			// Run test setup
			if tt.setupFS != nil {
				tt.setupFS(fs, dataDir)
			}

			// Get pack status
			result, err := GetPackStatus(tt.pack, tt.actions, fs, paths)
			require.NoError(t, err)
			require.NotNil(t, result)

			// Check basic results
			assert.Equal(t, tt.pack.Name, result.Name)
			assert.Equal(t, tt.expectedStatus, result.Status)
			assert.Equal(t, tt.expectedFiles, len(result.Files))

			// Run additional checks
			if tt.checkResult != nil {
				tt.checkResult(t, result)
			}
		})
	}
}
