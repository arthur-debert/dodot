package types_test

import (
	"fmt"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestActionCheckStatus_BrokenStates(t *testing.T) {
	t.Run("install script modified after execution", func(t *testing.T) {
		// Setup
		fs := testutil.NewTestFS()
		dataDir := "data/dodot"
		testutil.CreateDirT(t, fs, dataDir)

		action := types.Action{
			Type:   types.ActionTypeInstall,
			Source: "dotfiles/tools/install.sh",
			Pack:   "tools",
		}

		// Create script with original content
		originalContent := "#!/bin/bash\necho 'Installing tools...'"
		testutil.CreateFileT(t, fs, action.Source, originalContent)

		// Create sentinel with checksum of original content
		originalChecksum := "d9014c4624844aa5bac314773d6b689ad467fa4e1d1a50a1b8a99d5a95f72ff5"
		sentinelPath := filepath.Join(dataDir, "provision", "tools_install.sh.sentinel")
		testutil.CreateDirT(t, fs, filepath.Dir(sentinelPath))
		sentinelContent := fmt.Sprintf("%s:2025-01-15T10:00:00Z", originalChecksum)
		testutil.CreateFileT(t, fs, sentinelPath, sentinelContent)

		// Modify the script content
		modifiedContent := "#!/bin/bash\necho 'Installing tools v2...'"
		require.NoError(t, fs.WriteFile(action.Source, []byte(modifiedContent), 0644))

		// Execute
		mockPaths := &testutil.MockPaths{
			DataDirPath: dataDir,
		}
		status, err := action.CheckStatus(fs, mockPaths)

		// Assert
		require.NoError(t, err)
		assert.Equal(t, types.StatusStateError, status.State)
		assert.Contains(t, status.Message, "source file modified")
		assert.Contains(t, status.Message, "2025-01-15")
		assert.NotNil(t, status.Timestamp)
	})

	t.Run("brewfile modified after execution", func(t *testing.T) {
		// Setup
		fs := testutil.NewTestFS()
		dataDir := "data/dodot"
		testutil.CreateDirT(t, fs, dataDir)

		action := types.Action{
			Type:   types.ActionTypeBrew,
			Source: "dotfiles/homebrew/Brewfile",
			Pack:   "homebrew",
		}

		// Create Brewfile with original content
		originalContent := "brew 'git'\nbrew 'vim'"
		testutil.CreateFileT(t, fs, action.Source, originalContent)

		// Create sentinel with checksum
		originalChecksum := "abc123" // Different from actual checksum
		sentinelPath := filepath.Join(dataDir, "homebrew", "homebrew_Brewfile.sentinel")
		testutil.CreateDirT(t, fs, filepath.Dir(sentinelPath))
		sentinelContent := fmt.Sprintf("%s:2025-01-10", originalChecksum)
		testutil.CreateFileT(t, fs, sentinelPath, sentinelContent)

		// Execute
		mockPaths := &testutil.MockPaths{
			DataDirPath: dataDir,
		}
		status, err := action.CheckStatus(fs, mockPaths)

		// Assert
		require.NoError(t, err)
		assert.Equal(t, types.StatusStateError, status.State)
		assert.Contains(t, status.Message, "Brewfile modified")
		assert.Contains(t, status.Message, "2025-01-10")
	})

	t.Run("install script deleted after execution", func(t *testing.T) {
		// Setup
		fs := testutil.NewTestFS()
		dataDir := "data/dodot"
		testutil.CreateDirT(t, fs, dataDir)

		action := types.Action{
			Type:   types.ActionTypeInstall,
			Source: "dotfiles/tools/install.sh",
			Pack:   "tools",
		}

		// Create sentinel (but no source file)
		sentinelPath := filepath.Join(dataDir, "provision", "tools_install.sh.sentinel")
		testutil.CreateDirT(t, fs, filepath.Dir(sentinelPath))
		testutil.CreateFileT(t, fs, sentinelPath, "checksum:2025-01-15T10:00:00Z")

		// Execute
		mockPaths := &testutil.MockPaths{
			DataDirPath: dataDir,
		}
		status, err := action.CheckStatus(fs, mockPaths)

		// Assert
		require.NoError(t, err)
		assert.Equal(t, types.StatusStateSuccess, status.State)
		assert.Contains(t, status.Message, "source file removed")
	})
}

func TestSentinelNaming(t *testing.T) {
	tests := []struct {
		name         string
		action       types.Action
		expectedName string
		expectedDir  string
	}{
		{
			name: "install script sentinel",
			action: types.Action{
				Type:   types.ActionTypeInstall,
				Source: "dotfiles/tools/install.sh",
				Pack:   "tools",
			},
			expectedName: "tools_install.sh.sentinel",
			expectedDir:  "data/dodot/provision",
		},
		{
			name: "brewfile sentinel",
			action: types.Action{
				Type:   types.ActionTypeBrew,
				Source: "dotfiles/homebrew/Brewfile",
				Pack:   "homebrew",
			},
			expectedName: "homebrew_Brewfile.sentinel",
			expectedDir:  "data/dodot/homebrew",
		},
		{
			name: "install script with complex path",
			action: types.Action{
				Type:   types.ActionTypeInstall,
				Source: "dotfiles/tools/scripts/setup.sh",
				Pack:   "tools",
			},
			expectedName: "tools_setup.sh.sentinel",
			expectedDir:  "data/dodot/provision",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			mockPaths := &testutil.MockPaths{
				DataDirPath: "data/dodot",
			}

			sentinelInfo, err := tt.action.GetSentinelInfo(mockPaths)
			require.NoError(t, err)

			assert.Equal(t, tt.expectedName, sentinelInfo.Name)
			assert.Equal(t, tt.expectedDir, sentinelInfo.Dir)
			assert.Equal(t, filepath.Join(tt.expectedDir, tt.expectedName), sentinelInfo.Path)
		})
	}
}

func TestDeployedPathNaming(t *testing.T) {
	tests := []struct {
		name         string
		action       types.Action
		expectedPath string
	}{
		{
			name: "symlink deployed path",
			action: types.Action{
				Type:   types.ActionTypeLink,
				Source: "dotfiles/vim/vimrc",
				Target: "home/user/.vimrc",
			},
			expectedPath: "data/dodot/deployed/symlink/.vimrc",
		},
		{
			name: "path deployed path",
			action: types.Action{
				Type:   types.ActionTypePathAdd,
				Source: "dotfiles/tools/bin",
				Pack:   "tools",
			},
			expectedPath: "data/dodot/deployed/path/tools_bin",
		},
		{
			name: "shell profile deployed path",
			action: types.Action{
				Type:   types.ActionTypeShellSource,
				Source: "dotfiles/zsh/aliases.sh",
				Pack:   "zsh",
			},
			expectedPath: "data/dodot/deployed/shell_profile/zsh_aliases.sh",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			mockPaths := &testutil.MockPaths{
				DataDirPath: "data/dodot",
			}

			var path string
			var err error

			switch tt.action.Type {
			case types.ActionTypeLink:
				path, err = tt.action.GetDeployedSymlinkPath(mockPaths)
			case types.ActionTypePathAdd:
				path, err = tt.action.GetDeployedPathPath(mockPaths)
			case types.ActionTypeShellSource:
				path, err = tt.action.GetDeployedShellProfilePath(mockPaths)
			}

			require.NoError(t, err)
			assert.Equal(t, tt.expectedPath, path)
		})
	}
}

func TestParseSentinelData(t *testing.T) {
	// Note: parseSentinelData is not exported, so we test it indirectly
	// through the status checking functions

	t.Run("legacy sentinel format", func(t *testing.T) {
		fs := testutil.NewTestFS()
		dataDir := "data/dodot"
		testutil.CreateDirT(t, fs, dataDir)

		action := types.Action{
			Type:   types.ActionTypeInstall,
			Source: "dotfiles/tools/install.sh",
			Pack:   "tools",
		}

		// Create script
		testutil.CreateFileT(t, fs, action.Source, "script content")

		// Create legacy sentinel with just timestamp
		sentinelPath := filepath.Join(dataDir, "provision", "tools_install.sh.sentinel")
		testutil.CreateDirT(t, fs, filepath.Dir(sentinelPath))
		testutil.CreateFileT(t, fs, sentinelPath, "2025-01-15")

		// Execute
		mockPaths := &testutil.MockPaths{
			DataDirPath: dataDir,
		}
		status, err := action.CheckStatus(fs, mockPaths)

		// Assert - should still work with legacy format
		require.NoError(t, err)
		assert.Equal(t, types.StatusStateSuccess, status.State)
	})
}
