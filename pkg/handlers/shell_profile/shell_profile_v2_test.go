package shell_profile

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestShellProfileHandlerV2_ProcessLinking(t *testing.T) {
	handler := NewShellProfileHandlerV2()

	tests := []struct {
		name          string
		matches       []types.TriggerMatch
		expectedCount int
		expectedError bool
		checkActions  func(t *testing.T, actions []types.LinkingAction)
	}{
		{
			name: "single shell script",
			matches: []types.TriggerMatch{
				{
					Path:         "aliases.sh",
					AbsolutePath: "/dotfiles/shell/aliases.sh",
					Pack:         "shell",
					TriggerName:  "filename",
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
			name: "multiple shell scripts from same pack",
			matches: []types.TriggerMatch{
				{
					Path:         "aliases.sh",
					AbsolutePath: "/dotfiles/bash/aliases.sh",
					Pack:         "bash",
					TriggerName:  "filename",
				},
				{
					Path:         "functions.sh",
					AbsolutePath: "/dotfiles/bash/functions.sh",
					Pack:         "bash",
					TriggerName:  "filename",
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
			name: "shell scripts from different packs",
			matches: []types.TriggerMatch{
				{
					Path:         "aliases.sh",
					AbsolutePath: "/dotfiles/zsh/aliases.sh",
					Pack:         "zsh",
					TriggerName:  "filename",
				},
				{
					Path:         "aliases.sh",
					AbsolutePath: "/dotfiles/bash/aliases.sh",
					Pack:         "bash",
					TriggerName:  "filename",
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
			name:          "empty matches",
			matches:       []types.TriggerMatch{},
			expectedCount: 0,
			expectedError: false,
		},
		{
			name: "nested shell scripts",
			matches: []types.TriggerMatch{
				{
					Path:         "config/aliases.sh",
					AbsolutePath: "/dotfiles/shell/config/aliases.sh",
					Pack:         "shell",
					TriggerName:  "glob",
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

func TestShellProfileHandlerV2_ValidateOptions(t *testing.T) {
	handler := NewShellProfileHandlerV2()

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
			name: "any options are accepted",
			options: map[string]interface{}{
				"anything": "goes",
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

func TestShellProfileHandlerV2_Properties(t *testing.T) {
	handler := NewShellProfileHandlerV2()

	assert.Equal(t, ShellProfileHandlerName, handler.Name())
	assert.Equal(t, "Manages shell profile modifications (e.g., sourcing aliases)", handler.Description())
	assert.Equal(t, types.RunModeLinking, handler.RunMode())

	// Verify template content
	template := handler.GetTemplateContent()
	assert.NotEmpty(t, template)
	assert.Contains(t, template, "Shell aliases")
}
