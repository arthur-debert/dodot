package symlink

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestSymlinkHandler_ProcessLinking(t *testing.T) {
	tests := []struct {
		name          string
		matches       []types.RuleMatch
		expectedCount int
		expectedError bool
		checkActions  func(t *testing.T, actions []types.LinkingAction)
	}{
		{
			name: "single file symlink",
			matches: []types.RuleMatch{
				{
					Path:         ".vimrc",
					AbsolutePath: "/dotfiles/vim/.vimrc",
					Pack:         "vim",
					RuleName:     "filename",
				},
			},
			expectedCount: 1,
			expectedError: false,
			checkActions: func(t *testing.T, actions []types.LinkingAction) {
				linkAction, ok := actions[0].(*types.LinkAction)
				require.True(t, ok, "action should be LinkAction")
				assert.Equal(t, "vim", linkAction.PackName)
				assert.Equal(t, "/dotfiles/vim/.vimrc", linkAction.SourceFile)
				assert.Equal(t, "/home/testuser/.vimrc", linkAction.TargetFile)
			},
		},
		{
			name: "multiple files from same pack",
			matches: []types.RuleMatch{
				{
					Path:         ".bashrc",
					AbsolutePath: "/dotfiles/bash/.bashrc",
					Pack:         "bash",
					RuleName:     "filename",
				},
				{
					Path:         ".bash_profile",
					AbsolutePath: "/dotfiles/bash/.bash_profile",
					Pack:         "bash",
					RuleName:     "filename",
				},
			},
			expectedCount: 2,
			expectedError: false,
			checkActions: func(t *testing.T, actions []types.LinkingAction) {
				// Check first action
				linkAction1, ok := actions[0].(*types.LinkAction)
				require.True(t, ok)
				assert.Equal(t, "bash", linkAction1.PackName)
				assert.Equal(t, "/dotfiles/bash/.bashrc", linkAction1.SourceFile)
				assert.Equal(t, "/home/testuser/.bashrc", linkAction1.TargetFile)

				// Check second action
				linkAction2, ok := actions[1].(*types.LinkAction)
				require.True(t, ok)
				assert.Equal(t, "bash", linkAction2.PackName)
				assert.Equal(t, "/dotfiles/bash/.bash_profile", linkAction2.SourceFile)
				assert.Equal(t, "/home/testuser/.bash_profile", linkAction2.TargetFile)
			},
		},
		{
			name: "custom target directory",
			matches: []types.RuleMatch{
				{
					Path:         "config.json",
					AbsolutePath: "/dotfiles/app/config.json",
					Pack:         "app",
					RuleName:     "filename",
					HandlerOptions: map[string]interface{}{
						"target": "/etc/app",
					},
				},
			},
			expectedCount: 1,
			expectedError: false,
			checkActions: func(t *testing.T, actions []types.LinkingAction) {
				linkAction, ok := actions[0].(*types.LinkAction)
				require.True(t, ok)
				assert.Equal(t, "/etc/app/config.json", linkAction.TargetFile)
			},
		},
		{
			name: "conflict detection",
			matches: []types.RuleMatch{
				{
					Path:         ".config",
					AbsolutePath: "/dotfiles/app1/.config",
					Pack:         "app1",
					RuleName:     "filename",
				},
				{
					Path:         ".config",
					AbsolutePath: "/dotfiles/app2/.config",
					Pack:         "app2",
					RuleName:     "filename",
				},
			},
			expectedCount: 0,
			expectedError: true,
		},
		{
			name: "nested path",
			matches: []types.RuleMatch{
				{
					Path:         ".config/nvim/init.vim",
					AbsolutePath: "/dotfiles/neovim/.config/nvim/init.vim",
					Pack:         "neovim",
					RuleName:     "glob",
				},
			},
			expectedCount: 1,
			expectedError: false,
			checkActions: func(t *testing.T, actions []types.LinkingAction) {
				linkAction, ok := actions[0].(*types.LinkAction)
				require.True(t, ok)
				assert.Equal(t, "/home/testuser/.config/nvim/init.vim", linkAction.TargetFile)
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Set HOME for consistent testing
			t.Setenv("HOME", "/home/testuser")
			t.Setenv("DODOT_TEST_MODE", "true")

			// Create handler after setting environment
			handler := NewSymlinkHandler()

			actions, err := handler.ProcessLinking(tt.matches)

			if tt.expectedError {
				assert.Error(t, err)
				return
			}

			require.NoError(t, err)
			assert.Len(t, actions, tt.expectedCount)

			if tt.checkActions != nil {
				tt.checkActions(t, actions)
			}
		})
	}
}

func TestSymlinkHandler_ValidateOptions(t *testing.T) {
	handler := NewSymlinkHandler()

	tests := []struct {
		name          string
		options       map[string]interface{}
		expectedError bool
	}{
		{
			name:          "nil options",
			options:       nil,
			expectedError: false,
		},
		{
			name:          "empty options",
			options:       map[string]interface{}{},
			expectedError: false,
		},
		{
			name: "valid target option",
			options: map[string]interface{}{
				"target": "/custom/path",
			},
			expectedError: false,
		},
		{
			name: "invalid target type",
			options: map[string]interface{}{
				"target": 123,
			},
			expectedError: true,
		},
		{
			name: "unknown option",
			options: map[string]interface{}{
				"unknown": "value",
			},
			expectedError: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := handler.ValidateOptions(tt.options)
			if tt.expectedError {
				assert.Error(t, err)
			} else {
				assert.NoError(t, err)
			}
		})
	}
}

func TestSymlinkHandler_Properties(t *testing.T) {
	t.Setenv("DODOT_TEST_MODE", "true")
	handler := NewSymlinkHandler()

	assert.Equal(t, SymlinkHandlerName, handler.Name())
	assert.Equal(t, "Creates symbolic links from dotfiles to target locations", handler.Description())
	assert.Equal(t, types.RunModeLinking, handler.RunMode())
	assert.Empty(t, handler.GetTemplateContent())
}

func TestSymlinkHandler_EnvironmentVariableExpansion(t *testing.T) {
	t.Setenv("HOME", "/home/testuser")
	t.Setenv("CONFIG_DIR", "/etc/myapp")
	t.Setenv("DODOT_TEST_MODE", "true")

	// Create handler after setting environment
	handler := NewSymlinkHandler()

	matches := []types.RuleMatch{
		{
			Path:         "config.yaml",
			AbsolutePath: "/dotfiles/app/config.yaml",
			Pack:         "app",
			RuleName:     "filename",
			HandlerOptions: map[string]interface{}{
				"target": "$CONFIG_DIR",
			},
		},
	}

	actions, err := handler.ProcessLinking(matches)
	require.NoError(t, err)
	require.Len(t, actions, 1)

	linkAction, ok := actions[0].(*types.LinkAction)
	require.True(t, ok)
	assert.Equal(t, "/etc/myapp/config.yaml", linkAction.TargetFile)
}

func TestSymlinkHandler_NoHomeDirectory(t *testing.T) {
	// Clear HOME environment variable
	t.Setenv("HOME", "")
	t.Setenv("DODOT_TEST_MODE", "true")

	// Can't mock os.UserHomeDir directly, so just test with empty HOME
	// The handler will use os.UserHomeDir internally and fall back to "~"
	handler := NewSymlinkHandler()
	// We can't guarantee the default target will be "~" because os.UserHomeDir
	// might still return a valid home directory on some systems
	assert.NotEmpty(t, handler.defaultTarget)
}
