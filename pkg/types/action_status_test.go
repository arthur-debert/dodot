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
			expectedMsg:   "will symlink to .vimrc",
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
			},
			expectedState: types.StatusStateError,
			containsMsg:   "broken - source file missing",
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
			expectedMsg:   "will execute install script",
		},
		{
			name: "install script executed",
			action: types.Action{
				Type:   types.ActionTypeInstall,
				Source: "dotfiles/tools/install.sh",
				Pack:   "tools",
			},
			setupFS: func(fs types.FS, dataDir string) {
				// Create sentinel file
				sentinelPath := filepath.Join(dataDir, "install", "tools_install.sh.sentinel")
				testutil.CreateDirT(t, fs, filepath.Dir(sentinelPath))
				testutil.CreateFileT(t, fs, sentinelPath, "checksum:timestamp")
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
			expectedMsg:   "will run homebrew install",
		},
		{
			name: "brewfile processed",
			action: types.Action{
				Type:   types.ActionTypeBrew,
				Source: "dotfiles/homebrew/Brewfile",
				Pack:   "homebrew",
			},
			setupFS: func(fs types.FS, dataDir string) {
				// Create sentinel
				sentinelPath := filepath.Join(dataDir, "homebrew", "homebrew_Brewfile.sentinel")
				testutil.CreateDirT(t, fs, filepath.Dir(sentinelPath))
				testutil.CreateFileT(t, fs, sentinelPath, "timestamp")
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
			expectedMsg:   "will add to PATH",
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
			expectedMsg:   "will be sourced in shell init",
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
