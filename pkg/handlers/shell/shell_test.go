// Test Type: Unit Test
// Description: Tests for the shell handler - manages shell profile modifications

package shell_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/handlers/shell"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestShellHandler_ProcessLinking(t *testing.T) {
	handler := shell.NewShellHandler()

	tests := []struct {
		name          string
		matches       []types.RuleMatch
		expectedCount int
		expectedError bool
		checkActions  func(t *testing.T, actions []types.LinkingAction)
	}{
		{
			name: "single_shell_script",
			matches: []types.RuleMatch{
				{
					Path:         "aliases.sh",
					AbsolutePath: "/dotfiles/shell/aliases.sh",
					Pack:         "shell",
					RuleName:     "filename",
				},
			},
			expectedCount: 1,
			expectedError: false,
			checkActions: func(t *testing.T, actions []types.LinkingAction) {
				action, ok := actions[0].(*types.AddToShellProfileAction)
				require.True(t, ok, "action should be AddToShellProfileAction")
				assert.Equal(t, "shell", action.PackName)
				assert.Equal(t, "/dotfiles/shell/aliases.sh", action.ScriptPath)
			},
		},
		{
			name: "multiple_shell_scripts_from_same_pack",
			matches: []types.RuleMatch{
				{
					Path:         "aliases.sh",
					AbsolutePath: "/dotfiles/bash/aliases.sh",
					Pack:         "bash",
					RuleName:     "filename",
				},
				{
					Path:         "functions.sh",
					AbsolutePath: "/dotfiles/bash/functions.sh",
					Pack:         "bash",
					RuleName:     "filename",
				},
			},
			expectedCount: 2,
			expectedError: false,
			checkActions: func(t *testing.T, actions []types.LinkingAction) {
				// Check first action
				action1, ok := actions[0].(*types.AddToShellProfileAction)
				require.True(t, ok)
				assert.Equal(t, "bash", action1.PackName)
				assert.Equal(t, "/dotfiles/bash/aliases.sh", action1.ScriptPath)

				// Check second action
				action2, ok := actions[1].(*types.AddToShellProfileAction)
				require.True(t, ok)
				assert.Equal(t, "bash", action2.PackName)
				assert.Equal(t, "/dotfiles/bash/functions.sh", action2.ScriptPath)
			},
		},
		{
			name: "shell_scripts_from_different_packs",
			matches: []types.RuleMatch{
				{
					Path:         "aliases.sh",
					AbsolutePath: "/dotfiles/zsh/aliases.sh",
					Pack:         "zsh",
					RuleName:     "filename",
				},
				{
					Path:         "aliases.sh",
					AbsolutePath: "/dotfiles/bash/aliases.sh",
					Pack:         "bash",
					RuleName:     "filename",
				},
			},
			expectedCount: 2,
			expectedError: false,
			checkActions: func(t *testing.T, actions []types.LinkingAction) {
				action1, ok := actions[0].(*types.AddToShellProfileAction)
				require.True(t, ok)
				assert.Equal(t, "zsh", action1.PackName)
				assert.Equal(t, "/dotfiles/zsh/aliases.sh", action1.ScriptPath)

				action2, ok := actions[1].(*types.AddToShellProfileAction)
				require.True(t, ok)
				assert.Equal(t, "bash", action2.PackName)
				assert.Equal(t, "/dotfiles/bash/aliases.sh", action2.ScriptPath)
			},
		},
		{
			name:          "empty_matches",
			matches:       []types.RuleMatch{},
			expectedCount: 0,
			expectedError: false,
		},
		{
			name: "nested_shell_scripts",
			matches: []types.RuleMatch{
				{
					Path:         "config/aliases.sh",
					AbsolutePath: "/dotfiles/shell/config/aliases.sh",
					Pack:         "shell",
					RuleName:     "glob",
				},
			},
			expectedCount: 1,
			expectedError: false,
			checkActions: func(t *testing.T, actions []types.LinkingAction) {
				action, ok := actions[0].(*types.AddToShellProfileAction)
				require.True(t, ok)
				assert.Equal(t, "/dotfiles/shell/config/aliases.sh", action.ScriptPath)
			},
		},
		{
			name: "shell_script_with_handler_options",
			matches: []types.RuleMatch{
				{
					Path:         "profile.sh",
					AbsolutePath: "/dotfiles/shell/profile.sh",
					Pack:         "shell",
					RuleName:     "filename",
					HandlerOptions: map[string]interface{}{
						"placement": "environment",
					},
				},
			},
			expectedCount: 1,
			expectedError: false,
			checkActions: func(t *testing.T, actions []types.LinkingAction) {
				action, ok := actions[0].(*types.AddToShellProfileAction)
				require.True(t, ok)
				assert.Equal(t, "shell", action.PackName)
				assert.Equal(t, "/dotfiles/shell/profile.sh", action.ScriptPath)
				// Note: Current implementation doesn't use options yet
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
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

func TestShellHandler_ProcessLinkingWithConfirmations(t *testing.T) {
	handler := shell.NewShellHandler()

	tests := []struct {
		name          string
		matches       []types.RuleMatch
		expectedCount int
		expectedError bool
	}{
		{
			name: "shell_scripts_no_confirmations_needed",
			matches: []types.RuleMatch{
				{
					Path:         "aliases.sh",
					AbsolutePath: "/dotfiles/bash/aliases.sh",
					Pack:         "bash",
					RuleName:     "filename",
				},
			},
			expectedCount: 1,
			expectedError: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, err := handler.ProcessLinkingWithConfirmations(tt.matches)

			if tt.expectedError {
				assert.Error(t, err)
				return
			}

			require.NoError(t, err)
			assert.Len(t, result.Actions, tt.expectedCount)
			assert.Empty(t, result.Confirmations, "shell handler should not need confirmations for linking")
		})
	}
}

func TestShellHandler_ValidateOptions(t *testing.T) {
	handler := shell.NewShellHandler()

	tests := []struct {
		name          string
		options       map[string]interface{}
		expectedError bool
	}{
		{
			name:          "nil_options",
			options:       nil,
			expectedError: false,
		},
		{
			name:          "empty_options",
			options:       map[string]interface{}{},
			expectedError: false,
		},
		{
			name: "placement_option",
			options: map[string]interface{}{
				"placement": "environment",
			},
			expectedError: false, // Currently no options are validated
		},
		{
			name: "any_options_accepted",
			options: map[string]interface{}{
				"anything": "goes",
				"foo":      123,
			},
			expectedError: false, // Currently no options are validated
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

func TestShellHandler_Properties(t *testing.T) {
	handler := shell.NewShellHandler()

	assert.Equal(t, shell.ShellHandlerName, handler.Name())
	assert.Equal(t, "Manages shell profile modifications (e.g., sourcing aliases)", handler.Description())
	assert.Equal(t, types.HandlerTypeConfiguration, handler.Type())

	// Verify template content
	template := handler.GetTemplateContent()
	assert.NotEmpty(t, template)
	assert.Contains(t, template, "Shell aliases")
}

func TestShellHandler_Clear(t *testing.T) {
	handler := shell.NewShellHandler()

	tests := []struct {
		name        string
		dryRun      bool
		expectedLen int
	}{
		{
			name:        "clear_dry_run",
			dryRun:      true,
			expectedLen: 1,
		},
		{
			name:        "clear_actual",
			dryRun:      false,
			expectedLen: 1,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			ctx := types.ClearContext{
				Pack: types.Pack{
					Name: "bash",
				},
				DryRun: tt.dryRun,
				Paths: &mockPaths{
					packHandlerDir: "/data/state/packs/bash/shell",
				},
			}

			cleared, err := handler.Clear(ctx)
			require.NoError(t, err)
			assert.Len(t, cleared, tt.expectedLen)

			item := cleared[0]
			assert.Equal(t, "shell_state", item.Type)
			assert.Equal(t, "/data/state/packs/bash/shell", item.Path)

			if tt.dryRun {
				assert.Contains(t, item.Description, "Would remove")
			} else {
				assert.Contains(t, item.Description, "will be removed")
			}
		})
	}
}

// mockPaths implements the minimal Paths interface for testing
type mockPaths struct {
	packHandlerDir string
}

func (m *mockPaths) PackHandlerDir(packName, handlerName string) string {
	return m.packHandlerDir
}

func (m *mockPaths) DataDir() string {
	panic("not implemented")
}

func (m *mockPaths) StateRoot() string {
	panic("not implemented")
}

func (m *mockPaths) PacksRoot() string {
	panic("not implemented")
}

func (m *mockPaths) PackRoot(packName string) string {
	panic("not implemented")
}

func (m *mockPaths) DeployedRoot() string {
	panic("not implemented")
}

func (m *mockPaths) HandlerDir(handlerName string) string {
	panic("not implemented")
}

func (m *mockPaths) IntermediateFile(packName, sourceFile string) string {
	panic("not implemented")
}

func (m *mockPaths) HandlerDataFile(handlerName, filename string) string {
	panic("not implemented")
}

func (m *mockPaths) MapPackFileToSystem(pack *types.Pack, relPath string) string {
	panic("not implemented")
}
