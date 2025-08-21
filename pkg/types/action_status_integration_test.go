package types_test

import (
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestActionCheckStatus_Symlink(t *testing.T) {
	tests := []struct {
		name          string
		action        types.Action
		setupFS       func(fs types.FS, dataDir string)
		expectedState types.StatusState
		expectedMsg   string
		containsMsg   string // Use when exact match isn't needed
	}{
		{
			name: "symlink not deployed",
			action: types.Action{
				Type:   types.ActionTypeLink,
				Source: "dotfiles/vim/vimrc",
				Target: "home/user/.vimrc",
			},
			setupFS: func(fs types.FS, dataDir string) {
				// No intermediate symlink exists
			},
			expectedState: types.StatusStatePending,
			expectedMsg:   "→ <Filename>.vimrc</Filename>",
		},
		{
			name: "symlink deployed successfully",
			action: types.Action{
				Type:   types.ActionTypeLink,
				Source: "dotfiles/vim/vimrc",
				Target: "home/user/.vimrc",
			},
			setupFS: func(fs types.FS, dataDir string) {
				// Create source file
				testutil.CreateFileT(t, fs, "dotfiles/vim/vimrc", "vim config")
				// Create intermediate symlink
				deployedPath := filepath.Join(dataDir, "deployed", "symlink", ".vimrc")
				testutil.CreateDirT(t, fs, filepath.Dir(deployedPath))
				require.NoError(t, fs.Symlink("dotfiles/vim/vimrc", deployedPath))
				// Create target symlink pointing to intermediate
				testutil.CreateDirT(t, fs, "home/user")
				require.NoError(t, fs.Symlink(deployedPath, "home/user/.vimrc"))
			},
			expectedState: types.StatusStateSuccess,
			expectedMsg:   "linked to .vimrc",
		},
		{
			name: "symlink deployed but source deleted (broken)",
			action: types.Action{
				Type:   types.ActionTypeLink,
				Source: "dotfiles/vim/vimrc",
				Target: "home/user/.vimrc",
			},
			setupFS: func(fs types.FS, dataDir string) {
				// Create intermediate symlink but no source file
				deployedPath := filepath.Join(dataDir, "deployed", "symlink", ".vimrc")
				testutil.CreateDirT(t, fs, filepath.Dir(deployedPath))
				require.NoError(t, fs.Symlink("dotfiles/vim/vimrc", deployedPath))
				// Create target symlink pointing to intermediate
				testutil.CreateDirT(t, fs, "home/user")
				require.NoError(t, fs.Symlink(deployedPath, "home/user/.vimrc"))
			},
			expectedState: types.StatusStateError,
			containsMsg:   "broken - source file missing",
		},
		{
			name: "target file exists but is not a symlink",
			action: types.Action{
				Type:   types.ActionTypeLink,
				Source: "dotfiles/vim/vimrc",
				Target: "home/user/.vimrc",
			},
			setupFS: func(fs types.FS, dataDir string) {
				// Create source file
				testutil.CreateFileT(t, fs, "dotfiles/vim/vimrc", "vim config")
				// Create intermediate symlink
				deployedPath := filepath.Join(dataDir, "deployed", "symlink", ".vimrc")
				testutil.CreateDirT(t, fs, filepath.Dir(deployedPath))
				require.NoError(t, fs.Symlink("dotfiles/vim/vimrc", deployedPath))
				// Create target as regular file instead of symlink
				testutil.CreateFileT(t, fs, "home/user/.vimrc", "regular file")
			},
			expectedState: types.StatusStatePending,
			expectedMsg:   "→ <Filename>.vimrc</Filename>",
		},
		{
			name: "target symlink points somewhere else",
			action: types.Action{
				Type:   types.ActionTypeLink,
				Source: "dotfiles/vim/vimrc",
				Target: "home/user/.vimrc",
			},
			setupFS: func(fs types.FS, dataDir string) {
				// Create source file
				testutil.CreateFileT(t, fs, "dotfiles/vim/vimrc", "vim config")
				// Create intermediate symlink
				deployedPath := filepath.Join(dataDir, "deployed", "symlink", ".vimrc")
				testutil.CreateDirT(t, fs, filepath.Dir(deployedPath))
				require.NoError(t, fs.Symlink("dotfiles/vim/vimrc", deployedPath))
				// Create target symlink pointing elsewhere
				testutil.CreateDirT(t, fs, "home/user")
				testutil.CreateFileT(t, fs, "home/other/.vimrc", "other config")
				require.NoError(t, fs.Symlink("home/other/.vimrc", "home/user/.vimrc"))
			},
			expectedState: types.StatusStatePending,
			expectedMsg:   "→ <Filename>.vimrc</Filename>",
		},
		{
			name: "target points to intermediate but intermediate is missing",
			action: types.Action{
				Type:   types.ActionTypeLink,
				Source: "dotfiles/vim/vimrc",
				Target: "home/user/.vimrc",
			},
			setupFS: func(fs types.FS, dataDir string) {
				// Create source file
				testutil.CreateFileT(t, fs, "dotfiles/vim/vimrc", "vim config")
				// Don't create intermediate symlink
				deployedPath := filepath.Join(dataDir, "deployed", "symlink", ".vimrc")
				// Create target symlink pointing to (missing) intermediate
				testutil.CreateDirT(t, fs, "home/user")
				require.NoError(t, fs.Symlink(deployedPath, "home/user/.vimrc"))
			},
			expectedState: types.StatusStateError,
			containsMsg:   "broken - intermediate symlink missing",
		},
		{
			name: "intermediate points to wrong source",
			action: types.Action{
				Type:   types.ActionTypeLink,
				Source: "dotfiles/vim/vimrc",
				Target: "home/user/.vimrc",
			},
			setupFS: func(fs types.FS, dataDir string) {
				// Create source file
				testutil.CreateFileT(t, fs, "dotfiles/vim/vimrc", "vim config")
				// Create another file
				testutil.CreateFileT(t, fs, "dotfiles/vim/other", "other config")
				// Create intermediate symlink pointing to wrong file
				deployedPath := filepath.Join(dataDir, "deployed", "symlink", ".vimrc")
				testutil.CreateDirT(t, fs, filepath.Dir(deployedPath))
				require.NoError(t, fs.Symlink("dotfiles/vim/other", deployedPath))
				// Create target symlink pointing to intermediate
				testutil.CreateDirT(t, fs, "home/user")
				require.NoError(t, fs.Symlink(deployedPath, "home/user/.vimrc"))
			},
			expectedState: types.StatusStateError,
			containsMsg:   "broken - intermediate points to wrong file",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Setup
			fs := testutil.NewTestFS()
			dataDir := "data/dodot"
			testutil.CreateDirT(t, fs, dataDir)

			if tt.setupFS != nil {
				tt.setupFS(fs, dataDir)
			}

			// Create paths mock
			mockPaths := &testutil.MockPaths{
				DataDirPath: dataDir,
			}

			// Execute
			status, err := tt.action.CheckStatus(fs, mockPaths)

			// Assert
			require.NoError(t, err)
			assert.Equal(t, tt.expectedState, status.State, "status state mismatch")

			if tt.expectedMsg != "" {
				assert.Equal(t, tt.expectedMsg, status.Message, "status message mismatch")
			} else if tt.containsMsg != "" {
				assert.Contains(t, status.Message, tt.containsMsg, "status message missing expected text")
			}
		})
	}
}

func TestActionCheckStatus_Install(t *testing.T) {
	tests := []struct {
		name          string
		action        types.Action
		setupFS       func(fs types.FS, dataDir string)
		expectedState types.StatusState
		expectedMsg   string
	}{
		{
			name: "install script not executed",
			action: types.Action{
				Type:   types.ActionTypeInstall,
				Source: "dotfiles/tools/install.sh",
				Pack:   "tools",
			},
			setupFS: func(fs types.FS, dataDir string) {
				// No sentinel file
			},
			expectedState: types.StatusStatePending,
			expectedMsg:   "⏵ <Filename>install.sh</Filename>",
		},
		{
			name: "install script executed",
			action: types.Action{
				Type:   types.ActionTypeInstall,
				Source: "dotfiles/tools/install.sh",
				Pack:   "tools",
			},
			setupFS: func(fs types.FS, dataDir string) {
				// Create source file
				testutil.CreateFileT(t, fs, "dotfiles/tools/install.sh", "install script content")
				// Create sentinel file with matching checksum
				sentinelPath := filepath.Join(dataDir, "install", "tools_install.sh.sentinel")
				testutil.CreateDirT(t, fs, filepath.Dir(sentinelPath))
				// Use actual checksum of the content
				checksum := "8fd3ca7d6ce2b983eca4fe5cd5c33de49c05c6ce4aa2c9b13e9851a3cef006fe"
				testutil.CreateFileT(t, fs, sentinelPath, checksum+":2025-01-15T10:00:00Z")
			},
			expectedState: types.StatusStateSuccess,
			expectedMsg:   "executed during installation",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Setup
			fs := testutil.NewTestFS()
			dataDir := "data/dodot"
			testutil.CreateDirT(t, fs, dataDir)

			if tt.setupFS != nil {
				tt.setupFS(fs, dataDir)
			}

			mockPaths := &testutil.MockPaths{
				DataDirPath: dataDir,
			}

			// Execute
			status, err := tt.action.CheckStatus(fs, mockPaths)

			// Assert
			require.NoError(t, err)
			assert.Equal(t, tt.expectedState, status.State)
			assert.Equal(t, tt.expectedMsg, status.Message)
		})
	}
}

func TestActionCheckStatus_Brew(t *testing.T) {
	tests := []struct {
		name          string
		action        types.Action
		setupFS       func(fs types.FS, dataDir string)
		expectedState types.StatusState
		expectedMsg   string
	}{
		{
			name: "brewfile not processed",
			action: types.Action{
				Type:   types.ActionTypeBrew,
				Source: "dotfiles/homebrew/Brewfile",
				Pack:   "homebrew",
			},
			setupFS: func(fs types.FS, dataDir string) {
				// No sentinel
			},
			expectedState: types.StatusStatePending,
			expectedMsg:   "brew ⯈ <Filename>homebrew/Brewfile</Filename>",
		},
		{
			name: "brewfile processed",
			action: types.Action{
				Type:   types.ActionTypeBrew,
				Source: "dotfiles/homebrew/Brewfile",
				Pack:   "homebrew",
			},
			setupFS: func(fs types.FS, dataDir string) {
				// Create Brewfile
				testutil.CreateFileT(t, fs, "dotfiles/homebrew/Brewfile", "brew 'git'\nbrew 'vim'")
				// Create sentinel with matching checksum
				sentinelPath := filepath.Join(dataDir, "homebrew", "homebrew_Brewfile.sentinel")
				testutil.CreateDirT(t, fs, filepath.Dir(sentinelPath))
				// Use actual checksum of the content
				checksum := "6800eebff486c0d9a995327105d2268377d376ff8a32c37b1afaaf5b190d7bc9"
				testutil.CreateFileT(t, fs, sentinelPath, checksum+":2025-01-15T10:00:00Z")
			},
			expectedState: types.StatusStateSuccess,
			expectedMsg:   "homebrew packages installed",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Setup
			fs := testutil.NewTestFS()
			dataDir := "data/dodot"
			testutil.CreateDirT(t, fs, dataDir)

			if tt.setupFS != nil {
				tt.setupFS(fs, dataDir)
			}

			mockPaths := &testutil.MockPaths{
				DataDirPath: dataDir,
			}

			// Execute
			status, err := tt.action.CheckStatus(fs, mockPaths)

			// Assert
			require.NoError(t, err)
			assert.Equal(t, tt.expectedState, status.State)
			assert.Equal(t, tt.expectedMsg, status.Message)
		})
	}
}

func TestActionCheckStatus_Path(t *testing.T) {
	tests := []struct {
		name          string
		action        types.Action
		setupFS       func(fs types.FS, dataDir string)
		expectedState types.StatusState
		expectedMsg   string
	}{
		{
			name: "path not added",
			action: types.Action{
				Type:   types.ActionTypePathAdd,
				Source: "dotfiles/tools/bin",
				Pack:   "tools",
			},
			setupFS: func(fs types.FS, dataDir string) {
				// No path symlink
			},
			expectedState: types.StatusStatePending,
			expectedMsg:   "<Filename>tools/bin</Filename> ∊ system $PATH",
		},
		{
			name: "path added",
			action: types.Action{
				Type:   types.ActionTypePathAdd,
				Source: "dotfiles/tools/bin",
				Pack:   "tools",
			},
			setupFS: func(fs types.FS, dataDir string) {
				// Create path symlink
				pathLink := filepath.Join(dataDir, "deployed", "path", "tools_bin")
				testutil.CreateDirT(t, fs, filepath.Dir(pathLink))
				require.NoError(t, fs.Symlink("dotfiles/tools/bin", pathLink))
			},
			expectedState: types.StatusStateSuccess,
			expectedMsg:   "added to PATH",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Setup
			fs := testutil.NewTestFS()
			dataDir := "data/dodot"
			testutil.CreateDirT(t, fs, dataDir)

			if tt.setupFS != nil {
				tt.setupFS(fs, dataDir)
			}

			mockPaths := &testutil.MockPaths{
				DataDirPath: dataDir,
			}

			// Execute
			status, err := tt.action.CheckStatus(fs, mockPaths)

			// Assert
			require.NoError(t, err)
			assert.Equal(t, tt.expectedState, status.State)
			assert.Equal(t, tt.expectedMsg, status.Message)
		})
	}
}

func TestActionCheckStatus_ShellSource(t *testing.T) {
	tests := []struct {
		name          string
		action        types.Action
		setupFS       func(fs types.FS, dataDir string)
		expectedState types.StatusState
		expectedMsg   string
	}{
		{
			name: "shell script not sourced",
			action: types.Action{
				Type:   types.ActionTypeShellSource,
				Source: "dotfiles/zsh/aliases.sh",
				Pack:   "zsh",
			},
			setupFS: func(fs types.FS, dataDir string) {
				// No shell profile symlink
			},
			expectedState: types.StatusStatePending,
			expectedMsg:   "source <Filename>zsh/aliases.sh</Filename> in shell init",
		},
		{
			name: "shell script sourced",
			action: types.Action{
				Type:     types.ActionTypeShellSource,
				Source:   "dotfiles/zsh/aliases.sh",
				Pack:     "zsh",
				Metadata: map[string]interface{}{"shell": "zsh"},
			},
			setupFS: func(fs types.FS, dataDir string) {
				// Create shell profile symlink
				linkPath := filepath.Join(dataDir, "deployed", "shell_profile", "zsh_aliases.sh")
				testutil.CreateDirT(t, fs, filepath.Dir(linkPath))
				require.NoError(t, fs.Symlink("dotfiles/zsh/aliases.sh", linkPath))
			},
			expectedState: types.StatusStateSuccess,
			expectedMsg:   "sourced in zsh",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Setup
			fs := testutil.NewTestFS()
			dataDir := "data/dodot"
			testutil.CreateDirT(t, fs, dataDir)

			if tt.setupFS != nil {
				tt.setupFS(fs, dataDir)
			}

			mockPaths := &testutil.MockPaths{
				DataDirPath: dataDir,
			}

			// Execute
			status, err := tt.action.CheckStatus(fs, mockPaths)

			// Assert
			require.NoError(t, err)
			assert.Equal(t, tt.expectedState, status.State)
			assert.Equal(t, tt.expectedMsg, status.Message)
		})
	}
}
