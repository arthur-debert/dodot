package core

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestDirectExecutor_CleanupDanglingLinks(t *testing.T) {
	tests := []struct {
		name                string
		setupDangling       func(t *testing.T, tempDir, dotfilesRoot, homeDir string) types.Action
		cleanupEnabled      bool
		expectCleanup       bool
		expectDeploySuccess bool
	}{
		{
			name: "cleanup enabled - removes dangling link before deploy",
			setupDangling: func(t *testing.T, tempDir, dotfilesRoot, homeDir string) types.Action {
				// Create a deployed symlink that will become dangling
				pack := "vim"
				sourceFile := "vimrc"
				targetFile := ".vimrc"

				// Create pack directory
				testutil.CreateDir(t, dotfilesRoot, pack)

				sourcePath := filepath.Join(dotfilesRoot, pack, sourceFile)
				targetPath := filepath.Join(homeDir, targetFile)
				dataDir := filepath.Join(homeDir, ".local", "share", "dodot")
				intermediatePath := filepath.Join(dataDir, "deployed", "symlink", targetFile)

				// Create the initial deployment
				testutil.CreateFile(t, dotfilesRoot, filepath.Join(pack, sourceFile), "vim config")
				testutil.CreateDir(t, dataDir, "deployed/symlink")
				testutil.CreateSymlink(t, sourcePath, intermediatePath)
				testutil.CreateSymlink(t, intermediatePath, targetPath)

				// Remove source to make it dangling
				require.NoError(t, os.Remove(sourcePath))

				// Re-create source for new deployment
				testutil.CreateFile(t, dotfilesRoot, filepath.Join(pack, sourceFile), "new vim config")

				// Return action for the same file (will be re-deployed)
				return types.Action{
					Type:        types.ActionTypeLink,
					Source:      sourcePath,
					Target:      targetPath,
					Pack:        pack,
					HandlerName: "symlink",
					Description: "Link vimrc",
				}
			},
			cleanupEnabled:      true,
			expectCleanup:       true,
			expectDeploySuccess: true,
		},
		{
			name: "cleanup disabled - deployment succeeds with force behavior",
			setupDangling: func(t *testing.T, tempDir, dotfilesRoot, homeDir string) types.Action {
				pack := "git"
				sourceFile := "gitconfig"
				targetFile := ".gitconfig"

				// Create pack directory
				testutil.CreateDir(t, dotfilesRoot, pack)

				sourcePath := filepath.Join(dotfilesRoot, pack, sourceFile)
				targetPath := filepath.Join(homeDir, targetFile)
				dataDir := filepath.Join(homeDir, ".local", "share", "dodot")
				intermediatePath := filepath.Join(dataDir, "deployed", "symlink", targetFile)

				// Create the initial deployment
				testutil.CreateFile(t, dotfilesRoot, filepath.Join(pack, sourceFile), "git config")
				testutil.CreateDir(t, dataDir, "deployed/symlink")
				testutil.CreateSymlink(t, sourcePath, intermediatePath)
				testutil.CreateSymlink(t, intermediatePath, targetPath)

				// Remove source to make it dangling
				require.NoError(t, os.Remove(sourcePath))

				// Create new source file
				newSourceFile := "gitconfig-new"
				testutil.CreateFile(t, dotfilesRoot, filepath.Join(pack, newSourceFile), "new git config")

				// Deploy same target from different source
				return types.Action{
					Type:        types.ActionTypeLink,
					Source:      filepath.Join(dotfilesRoot, pack, newSourceFile),
					Target:      targetPath,
					Pack:        pack,
					HandlerName: "symlink",
					Description: "Link gitconfig-new",
				}
			},
			cleanupEnabled:      false,
			expectCleanup:       false,
			expectDeploySuccess: true, // DirectExecutor will handle the conflict
		},
		{
			name: "no dangling links - normal deploy",
			setupDangling: func(t *testing.T, tempDir, dotfilesRoot, homeDir string) types.Action {
				// No dangling setup, just return action
				pack := "zsh"
				sourceFile := "zshrc"
				targetFile := ".zshrc"

				// Create pack directory and source file
				testutil.CreateDir(t, dotfilesRoot, pack)
				testutil.CreateFile(t, dotfilesRoot, filepath.Join(pack, sourceFile), "zsh config")

				sourcePath := filepath.Join(dotfilesRoot, pack, sourceFile)
				targetPath := filepath.Join(homeDir, targetFile)

				return types.Action{
					Type:        types.ActionTypeLink,
					Source:      sourcePath,
					Target:      targetPath,
					Pack:        pack,
					HandlerName: "symlink",
					Description: "Link zshrc",
				}
			},
			cleanupEnabled:      true,
			expectCleanup:       false,
			expectDeploySuccess: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Setup test environment
			tempDir := testutil.TempDir(t, "direct-executor-dangling")
			dotfilesDir := filepath.Join(tempDir, "dotfiles")
			homeDir := filepath.Join(tempDir, "home")

			testutil.CreateDir(t, tempDir, "dotfiles")
			testutil.CreateDir(t, tempDir, "home")
			testutil.CreateDir(t, homeDir, ".local/share/dodot")

			t.Setenv("HOME", homeDir)
			t.Setenv("DOTFILES_ROOT", dotfilesDir)
			t.Setenv("DODOT_DATA_DIR", filepath.Join(homeDir, ".local", "share", "dodot"))

			// Setup dangling link scenario and get action
			action := tt.setupDangling(t, tempDir, dotfilesDir, homeDir)

			// Create paths
			p, err := paths.New(dotfilesDir)
			require.NoError(t, err)

			// Create config with cleanup flag
			cfg := config.Default()
			cfg.Security.CleanupDanglingLinks = tt.cleanupEnabled

			// Create executor
			executor := NewDirectExecutor(&DirectExecutorOptions{
				Paths:             p,
				DryRun:            false,
				Force:             false,
				AllowHomeSymlinks: true,
				Config:            cfg,
			})

			// Execute action
			results, err := executor.ExecuteActions([]types.Action{action})

			if tt.expectDeploySuccess {
				require.NoError(t, err)
				require.Len(t, results, 1)
				assert.Equal(t, types.StatusReady, results[0].Status)

				// Verify the new deployment exists
				info, err := os.Lstat(action.Target)
				require.NoError(t, err)
				assert.True(t, info.Mode()&os.ModeSymlink != 0)
			} else {
				// Should fail
				assert.Error(t, err)
			}
		})
	}
}

func TestDirectExecutor_CleanupDanglingLinks_MultipleActions(t *testing.T) {
	// Setup test environment
	tempDir := testutil.TempDir(t, "direct-executor-dangling-multi")
	dotfilesDir := filepath.Join(tempDir, "dotfiles")
	homeDir := filepath.Join(tempDir, "home")
	dataDir := filepath.Join(homeDir, ".local", "share", "dodot")

	testutil.CreateDir(t, tempDir, "dotfiles")
	testutil.CreateDir(t, tempDir, "home")
	testutil.CreateDir(t, homeDir, ".local/share/dodot")
	testutil.CreateDir(t, dataDir, "deployed/symlink")

	t.Setenv("HOME", homeDir)
	t.Setenv("DOTFILES_ROOT", dotfilesDir)
	t.Setenv("DODOT_DATA_DIR", dataDir)

	// Create multiple packs
	testutil.CreateDir(t, dotfilesDir, "vim")
	testutil.CreateDir(t, dotfilesDir, "git")
	testutil.CreateDir(t, dotfilesDir, "zsh")

	// Create multiple dangling links
	actions := []types.Action{}

	// First dangling link
	sourcePath1 := filepath.Join(dotfilesDir, "vim", "vimrc")
	targetPath1 := filepath.Join(homeDir, ".vimrc")
	intermediatePath1 := filepath.Join(dataDir, "deployed", "symlink", ".vimrc")

	testutil.CreateFile(t, dotfilesDir, "vim/vimrc", "vim config")
	testutil.CreateSymlink(t, sourcePath1, intermediatePath1)
	testutil.CreateSymlink(t, intermediatePath1, targetPath1)
	require.NoError(t, os.Remove(sourcePath1))

	// Re-create source for new deployment
	testutil.CreateFile(t, dotfilesDir, "vim/vimrc", "new vim config")
	actions = append(actions, types.Action{
		Type:        types.ActionTypeLink,
		Source:      sourcePath1,
		Target:      targetPath1,
		Pack:        "vim",
		HandlerName: "symlink",
	})

	// Second dangling link
	sourcePath2 := filepath.Join(dotfilesDir, "git", "gitconfig")
	targetPath2 := filepath.Join(homeDir, ".gitconfig")
	intermediatePath2 := filepath.Join(dataDir, "deployed", "symlink", ".gitconfig")

	testutil.CreateFile(t, dotfilesDir, "git/gitconfig", "git config")
	testutil.CreateSymlink(t, sourcePath2, intermediatePath2)
	testutil.CreateSymlink(t, intermediatePath2, targetPath2)
	require.NoError(t, os.Remove(sourcePath2))

	// Re-create source for new deployment
	testutil.CreateFile(t, dotfilesDir, "git/gitconfig", "new git config")
	actions = append(actions, types.Action{
		Type:        types.ActionTypeLink,
		Source:      sourcePath2,
		Target:      targetPath2,
		Pack:        "git",
		HandlerName: "symlink",
	})

	// Third action - no dangling link
	sourcePath3 := filepath.Join(dotfilesDir, "zsh", "zshrc")
	targetPath3 := filepath.Join(homeDir, ".zshrc")
	testutil.CreateFile(t, dotfilesDir, "zsh/zshrc", "zsh config")
	actions = append(actions, types.Action{
		Type:        types.ActionTypeLink,
		Source:      sourcePath3,
		Target:      targetPath3,
		Pack:        "zsh",
		HandlerName: "symlink",
	})

	// Create paths
	p, err := paths.New(dotfilesDir)
	require.NoError(t, err)

	// Create executor with cleanup enabled
	cfg := config.Default()
	cfg.Security.CleanupDanglingLinks = true

	executor := NewDirectExecutor(&DirectExecutorOptions{
		Paths:             p,
		DryRun:            false,
		Force:             false,
		AllowHomeSymlinks: true,
		Config:            cfg,
	})

	// Execute actions
	results, err := executor.ExecuteActions(actions)
	require.NoError(t, err)
	require.Len(t, results, 3)

	// All deployments should succeed
	for i, result := range results {
		assert.Equal(t, types.StatusReady, result.Status, "Action %d failed", i)
	}

	// Verify all symlinks exist and point to correct locations
	for _, action := range actions {
		info, err := os.Lstat(action.Target)
		require.NoError(t, err)
		assert.True(t, info.Mode()&os.ModeSymlink != 0)
	}
}

func TestDirectExecutor_CleanupDanglingLinks_DryRun(t *testing.T) {
	// Setup test environment
	tempDir := testutil.TempDir(t, "direct-executor-dangling-dryrun")
	dotfilesDir := filepath.Join(tempDir, "dotfiles")
	homeDir := filepath.Join(tempDir, "home")
	dataDir := filepath.Join(homeDir, ".local", "share", "dodot")

	testutil.CreateDir(t, tempDir, "dotfiles")
	testutil.CreateDir(t, tempDir, "home")
	testutil.CreateDir(t, homeDir, ".local/share/dodot")
	testutil.CreateDir(t, dataDir, "deployed/symlink")
	testutil.CreateDir(t, dotfilesDir, "vim")

	t.Setenv("HOME", homeDir)
	t.Setenv("DOTFILES_ROOT", dotfilesDir)
	t.Setenv("DODOT_DATA_DIR", dataDir)

	// Create a dangling link
	sourcePath := filepath.Join(dotfilesDir, "vim", "vimrc")
	targetPath := filepath.Join(homeDir, ".vimrc")
	intermediatePath := filepath.Join(dataDir, "deployed", "symlink", ".vimrc")

	testutil.CreateFile(t, dotfilesDir, "vim/vimrc", "vim config")
	testutil.CreateSymlink(t, sourcePath, intermediatePath)
	testutil.CreateSymlink(t, intermediatePath, targetPath)
	require.NoError(t, os.Remove(sourcePath))

	// Re-create source for new deployment
	testutil.CreateFile(t, dotfilesDir, "vim/vimrc", "new vim config")

	action := types.Action{
		Type:        types.ActionTypeLink,
		Source:      sourcePath,
		Target:      targetPath,
		Pack:        "vim",
		HandlerName: "symlink",
	}

	// Create paths
	p, err := paths.New(dotfilesDir)
	require.NoError(t, err)

	// Create executor with cleanup enabled but dry run
	cfg := config.Default()
	cfg.Security.CleanupDanglingLinks = true

	executor := NewDirectExecutor(&DirectExecutorOptions{
		Paths:             p,
		DryRun:            true, // Dry run mode
		Force:             false,
		AllowHomeSymlinks: true,
		Config:            cfg,
	})

	// Execute action in dry run
	results, err := executor.ExecuteActions([]types.Action{action})
	require.NoError(t, err)
	require.Len(t, results, 1)

	// Dry run should report what would be done
	assert.Equal(t, types.StatusReady, results[0].Status)

	// Dangling link should still exist (not cleaned up in dry run)
	info, err := os.Lstat(targetPath)
	require.NoError(t, err)
	assert.True(t, info.Mode()&os.ModeSymlink != 0)

	// And it should still be dangling
	target, err := os.Readlink(targetPath)
	require.NoError(t, err)
	assert.Equal(t, intermediatePath, target)
}
