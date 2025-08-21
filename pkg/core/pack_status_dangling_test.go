package core

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/state"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestGetPackStatus_DanglingLinks(t *testing.T) {
	tests := []struct {
		name           string
		setupDangling  func(t *testing.T, tempDir, dotfilesRoot, homeDir string) types.Action
		expectedStatus string
		expectedInMsg  string
	}{
		{
			name: "detects missing source file",
			setupDangling: func(t *testing.T, tempDir, dotfilesRoot, homeDir string) types.Action {
				pack := "vim"
				sourceFile := "vimrc"
				targetFile := ".vimrc"
				dataDir := filepath.Join(homeDir, ".local", "share", "dodot")

				// Create pack directory
				testutil.CreateDir(t, dotfilesRoot, pack)

				sourcePath := filepath.Join(dotfilesRoot, pack, sourceFile)
				targetPath := filepath.Join(homeDir, targetFile)
				intermediatePath := filepath.Join(dataDir, "deployed", "symlink", targetFile)

				// Create the initial deployment
				testutil.CreateFile(t, dotfilesRoot, filepath.Join(pack, sourceFile), "vim config")
				testutil.CreateDir(t, dataDir, "deployed/symlink")
				testutil.CreateSymlink(t, sourcePath, intermediatePath)
				testutil.CreateSymlink(t, intermediatePath, targetPath)

				// Remove source to make it dangling
				require.NoError(t, os.Remove(sourcePath))

				// Return action for status check
				return types.Action{
					Type:        types.ActionTypeLink,
					Source:      sourcePath,
					Target:      targetPath,
					Pack:        pack,
					PowerUpName: "symlink",
					Description: "Link vimrc",
				}
			},
			expectedStatus: "warning",
			expectedInMsg:  "dangling: source file removed",
		},
		{
			name: "detects missing intermediate link",
			setupDangling: func(t *testing.T, tempDir, dotfilesRoot, homeDir string) types.Action {
				pack := "git"
				sourceFile := "gitconfig"
				targetFile := ".gitconfig"
				dataDir := filepath.Join(homeDir, ".local", "share", "dodot")

				// Create pack directory
				testutil.CreateDir(t, dotfilesRoot, pack)

				sourcePath := filepath.Join(dotfilesRoot, pack, sourceFile)
				targetPath := filepath.Join(homeDir, targetFile)
				intermediatePath := filepath.Join(dataDir, "deployed", "symlink", targetFile)

				// Create the initial deployment
				testutil.CreateFile(t, dotfilesRoot, filepath.Join(pack, sourceFile), "git config")
				testutil.CreateDir(t, dataDir, "deployed/symlink")
				testutil.CreateSymlink(t, sourcePath, intermediatePath)
				testutil.CreateSymlink(t, intermediatePath, targetPath)

				// Remove intermediate to make it dangling
				require.NoError(t, os.Remove(intermediatePath))

				// Return action for status check
				return types.Action{
					Type:        types.ActionTypeLink,
					Source:      sourcePath,
					Target:      targetPath,
					Pack:        pack,
					PowerUpName: "symlink",
					Description: "Link gitconfig",
				}
			},
			expectedStatus: "warning",
			expectedInMsg:  "dangling: intermediate link missing",
		},
		{
			name: "normal deployed link shows as success",
			setupDangling: func(t *testing.T, tempDir, dotfilesRoot, homeDir string) types.Action {
				pack := "zsh"
				sourceFile := "zshrc"
				targetFile := ".zshrc"
				dataDir := filepath.Join(homeDir, ".local", "share", "dodot")

				// Create pack directory
				testutil.CreateDir(t, dotfilesRoot, pack)

				sourcePath := filepath.Join(dotfilesRoot, pack, sourceFile)
				targetPath := filepath.Join(homeDir, targetFile)
				intermediatePath := filepath.Join(dataDir, "deployed", "symlink", targetFile)

				// Create a proper deployment
				testutil.CreateFile(t, dotfilesRoot, filepath.Join(pack, sourceFile), "zsh config")
				testutil.CreateDir(t, dataDir, "deployed/symlink")
				testutil.CreateSymlink(t, sourcePath, intermediatePath)
				testutil.CreateSymlink(t, intermediatePath, targetPath)

				// Return action for status check
				return types.Action{
					Type:        types.ActionTypeLink,
					Source:      sourcePath,
					Target:      targetPath,
					Pack:        pack,
					PowerUpName: "symlink",
					Description: "Link zshrc",
				}
			},
			expectedStatus: "success",
			expectedInMsg:  "",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Setup test environment
			tempDir := testutil.TempDir(t, "pack-status-dangling")
			dotfilesRoot := filepath.Join(tempDir, "dotfiles")
			homeDir := filepath.Join(tempDir, "home")

			testutil.CreateDir(t, tempDir, "dotfiles")
			testutil.CreateDir(t, tempDir, "home")
			testutil.CreateDir(t, homeDir, ".local/share/dodot")

			t.Setenv("HOME", homeDir)
			t.Setenv("DOTFILES_ROOT", dotfilesRoot)
			t.Setenv("DODOT_DATA_DIR", filepath.Join(homeDir, ".local", "share", "dodot"))

			// Setup dangling link scenario and get action
			action := tt.setupDangling(t, tempDir, dotfilesRoot, homeDir)

			// Create paths
			p, err := paths.New(dotfilesRoot)
			require.NoError(t, err)

			// Create filesystem
			fs := filesystem.NewOS()

			// Use LinkDetector to detect dangling links
			linkDetector := state.NewLinkDetector(fs, p)
			danglingLinks, err := linkDetector.DetectDanglingLinks([]types.Action{action})
			require.NoError(t, err)

			// Create dangling links map
			danglingByPath := make(map[string]*state.DanglingLink)
			for i := range danglingLinks {
				danglingByPath[danglingLinks[i].DeployedPath] = &danglingLinks[i]
			}

			// Get display status for the action with dangling links map
			displayFile, err := getActionDisplayStatus(action, fs, p, danglingByPath)
			require.NoError(t, err)

			// Verify status
			assert.Equal(t, tt.expectedStatus, displayFile.Status)

			// Verify message contains expected text
			if tt.expectedInMsg != "" {
				assert.Contains(t, displayFile.Message, tt.expectedInMsg)
			}
		})
	}
}

func TestGetMultiPackStatus_WithDanglingLinks(t *testing.T) {
	// Setup test environment
	tempDir := testutil.TempDir(t, "multi-pack-status-dangling")
	dotfilesRoot := filepath.Join(tempDir, "dotfiles")
	homeDir := filepath.Join(tempDir, "home")
	dataDir := filepath.Join(homeDir, ".local", "share", "dodot")

	testutil.CreateDir(t, tempDir, "dotfiles")
	testutil.CreateDir(t, tempDir, "home")
	testutil.CreateDir(t, homeDir, ".local/share/dodot")
	testutil.CreateDir(t, dataDir, "deployed/symlink")

	t.Setenv("HOME", homeDir)
	t.Setenv("DOTFILES_ROOT", dotfilesRoot)
	t.Setenv("DODOT_DATA_DIR", dataDir)

	// Create multiple packs
	testutil.CreateDir(t, dotfilesRoot, "vim")
	testutil.CreateDir(t, dotfilesRoot, "git")
	testutil.CreateDir(t, dotfilesRoot, "zsh")

	// Pack 1: vim with dangling link
	// First create with different name to make intermediate point to non-existent file
	vimOldSource := filepath.Join(dotfilesRoot, "vim", "vimrc.old")
	vimTarget := filepath.Join(homeDir, ".vimrc")
	vimIntermediate := filepath.Join(dataDir, "deployed", "symlink", ".vimrc")

	// Create deployment pointing to vimrc.old
	testutil.CreateFile(t, dotfilesRoot, "vim/vimrc.old", "old vim config")
	testutil.CreateSymlink(t, vimOldSource, vimIntermediate)
	testutil.CreateSymlink(t, vimIntermediate, vimTarget)

	// Remove the old source
	require.NoError(t, os.Remove(vimOldSource))

	// Create new source file with standard name so triggers will pick it up
	testutil.CreateFile(t, dotfilesRoot, "vim/vimrc", "new vim config")

	// Pack 2: git - normal deployment
	gitSource := filepath.Join(dotfilesRoot, "git", "gitconfig")
	gitTarget := filepath.Join(homeDir, ".gitconfig")
	gitIntermediate := filepath.Join(dataDir, "deployed", "symlink", ".gitconfig")

	testutil.CreateFile(t, dotfilesRoot, "git/gitconfig", "git config")
	testutil.CreateSymlink(t, gitSource, gitIntermediate)
	testutil.CreateSymlink(t, gitIntermediate, gitTarget)

	// Pack 3: zsh - not deployed
	testutil.CreateFile(t, dotfilesRoot, "zsh/zshrc", "zsh config")

	// Create packs
	packs := []types.Pack{
		{Name: "vim", Path: filepath.Join(dotfilesRoot, "vim")},
		{Name: "git", Path: filepath.Join(dotfilesRoot, "git")},
		{Name: "zsh", Path: filepath.Join(dotfilesRoot, "zsh")},
	}

	// Create paths and filesystem
	p, err := paths.New(dotfilesRoot)
	require.NoError(t, err)
	fs := filesystem.NewOS()

	// Get multi-pack status
	result, err := GetMultiPackStatus(packs, "status", fs, p)
	require.NoError(t, err)
	require.Len(t, result.Packs, 3)

	// Check vim pack - should have warning
	vimPack := findPackByName(result.Packs, "vim")
	require.NotNil(t, vimPack)
	assert.Equal(t, "partial", vimPack.Status) // Pack with warnings

	// Find the vimrc file
	vimrcFile := findFileByPath(vimPack.Files, "vimrc")
	require.NotNil(t, vimrcFile)
	assert.Equal(t, "warning", vimrcFile.Status)
	assert.Contains(t, vimrcFile.Message, "dangling")

	// Check git pack - should be success
	gitPack := findPackByName(result.Packs, "git")
	require.NotNil(t, gitPack)
	assert.Equal(t, "success", gitPack.Status)

	// Check zsh pack - should be queue (not deployed yet)
	zshPack := findPackByName(result.Packs, "zsh")
	require.NotNil(t, zshPack)
	assert.Equal(t, "queue", zshPack.Status)
}

// Helper functions
func findPackByName(packs []types.DisplayPack, name string) *types.DisplayPack {
	for i := range packs {
		if packs[i].Name == name {
			return &packs[i]
		}
	}
	return nil
}

func findFileByPath(files []types.DisplayFile, path string) *types.DisplayFile {
	for i := range files {
		if files[i].Path == path {
			return &files[i]
		}
	}
	return nil
}
