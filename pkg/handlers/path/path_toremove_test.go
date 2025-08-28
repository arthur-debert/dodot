package path

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestPathHandler_ProcessLinking(t *testing.T) {
	handler := NewPathHandler()

	tests := []struct {
		name          string
		matches       []types.RuleMatch
		expectedCount int
		expectedError bool
		checkActions  func(t *testing.T, actions []types.LinkingAction)
	}{
		{
			name: "single directory",
			matches: []types.RuleMatch{
				{
					Path:         "bin",
					AbsolutePath: "/dotfiles/tools/bin",
					Pack:         "tools",
					RuleName:     "directory",
				},
			},
			expectedCount: 1,
			expectedError: false,
			checkActions: func(t *testing.T, actions []types.LinkingAction) {
				addPathAction, ok := actions[0].(*types.AddToPathAction)
				require.True(t, ok, "action should be AddToPathAction")
				assert.Equal(t, "tools", addPathAction.PackName)
				assert.Equal(t, "/dotfiles/tools/bin", addPathAction.DirPath)
			},
		},
		{
			name: "multiple directories from same pack",
			matches: []types.RuleMatch{
				{
					Path:         "bin",
					AbsolutePath: "/dotfiles/dev/bin",
					Pack:         "dev",
					RuleName:     "directory",
				},
				{
					Path:         "scripts",
					AbsolutePath: "/dotfiles/dev/scripts",
					Pack:         "dev",
					RuleName:     "directory",
				},
			},
			expectedCount: 2,
			expectedError: false,
			checkActions: func(t *testing.T, actions []types.LinkingAction) {
				// Check first action
				action1, ok := actions[0].(*types.AddToPathAction)
				require.True(t, ok)
				assert.Equal(t, "dev", action1.PackName)
				assert.Equal(t, "/dotfiles/dev/bin", action1.DirPath)

				// Check second action
				action2, ok := actions[1].(*types.AddToPathAction)
				require.True(t, ok)
				assert.Equal(t, "dev", action2.PackName)
				assert.Equal(t, "/dotfiles/dev/scripts", action2.DirPath)
			},
		},
		{
			name: "duplicate directory detection",
			matches: []types.RuleMatch{
				{
					Path:         "bin",
					AbsolutePath: "/dotfiles/tools/bin",
					Pack:         "tools",
					RuleName:     "directory",
				},
				{
					Path:         "bin",
					AbsolutePath: "/dotfiles/tools/bin",
					Pack:         "tools",
					RuleName:     "directory",
				},
			},
			expectedCount: 1, // Second duplicate should be skipped
			expectedError: false,
			checkActions: func(t *testing.T, actions []types.LinkingAction) {
				assert.Len(t, actions, 1)
				addPathAction, ok := actions[0].(*types.AddToPathAction)
				require.True(t, ok)
				assert.Equal(t, "tools", addPathAction.PackName)
				assert.Equal(t, "/dotfiles/tools/bin", addPathAction.DirPath)
			},
		},
		{
			name: "different packs with same directory name",
			matches: []types.RuleMatch{
				{
					Path:         "bin",
					AbsolutePath: "/dotfiles/tools/bin",
					Pack:         "tools",
					RuleName:     "directory",
				},
				{
					Path:         "bin",
					AbsolutePath: "/dotfiles/dev/bin",
					Pack:         "dev",
					RuleName:     "directory",
				},
			},
			expectedCount: 2, // Both should be included since they're from different packs
			expectedError: false,
			checkActions: func(t *testing.T, actions []types.LinkingAction) {
				action1, ok := actions[0].(*types.AddToPathAction)
				require.True(t, ok)
				assert.Equal(t, "tools", action1.PackName)
				assert.Equal(t, "/dotfiles/tools/bin", action1.DirPath)

				action2, ok := actions[1].(*types.AddToPathAction)
				require.True(t, ok)
				assert.Equal(t, "dev", action2.PackName)
				assert.Equal(t, "/dotfiles/dev/bin", action2.DirPath)
			},
		},
		{
			name:          "empty matches",
			matches:       []types.RuleMatch{},
			expectedCount: 0,
			expectedError: false,
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

func TestPathHandler_ValidateOptions(t *testing.T) {
	handler := NewPathHandler()

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

func TestPathHandler_Properties(t *testing.T) {
	handler := NewPathHandler()

	assert.Equal(t, PathHandlerName, handler.Name())
	assert.Equal(t, "Adds directories to PATH", handler.Description())
	assert.Equal(t, types.HandlerTypeConfiguration, handler.Type())
	assert.Empty(t, handler.GetTemplateContent())
}
