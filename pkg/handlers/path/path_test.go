// Test Type: Unit Test
// Description: Tests for the path handler - handler logic tests with no filesystem or external dependencies

package path_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/handlers/path"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestPathHandler_ProcessLinking(t *testing.T) {
	handler := path.NewPathHandler()

	tests := []struct {
		name          string
		matches       []types.RuleMatch
		expectedCount int
		expectError   bool
		checkActions  func(t *testing.T, actions []types.LinkingAction)
	}{
		{
			name: "single_directory_creates_one_action",
			matches: []types.RuleMatch{
				{
					Path:         "bin",
					AbsolutePath: "/dotfiles/tools/bin",
					Pack:         "tools",
					RuleName:     "directory",
				},
			},
			expectedCount: 1,
			expectError:   false,
			checkActions: func(t *testing.T, actions []types.LinkingAction) {
				require.Len(t, actions, 1)
				addPathAction, ok := actions[0].(*types.AddToPathAction)
				require.True(t, ok, "action should be AddToPathAction")
				assert.Equal(t, "tools", addPathAction.PackName)
				assert.Equal(t, "/dotfiles/tools/bin", addPathAction.DirPath)
			},
		},
		{
			name: "multiple_directories_from_same_pack",
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
			expectError:   false,
			checkActions: func(t *testing.T, actions []types.LinkingAction) {
				require.Len(t, actions, 2)

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
			name: "duplicate_directory_is_deduped",
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
			expectError:   false,
			checkActions: func(t *testing.T, actions []types.LinkingAction) {
				require.Len(t, actions, 1)
				addPathAction, ok := actions[0].(*types.AddToPathAction)
				require.True(t, ok)
				assert.Equal(t, "tools", addPathAction.PackName)
				assert.Equal(t, "/dotfiles/tools/bin", addPathAction.DirPath)
			},
		},
		{
			name: "different_packs_with_same_directory_name",
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
			expectError:   false,
			checkActions: func(t *testing.T, actions []types.LinkingAction) {
				require.Len(t, actions, 2)

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
			name:          "empty_matches_returns_empty_actions",
			matches:       []types.RuleMatch{},
			expectedCount: 0,
			expectError:   false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			actions, err := handler.ProcessLinking(tt.matches)

			if tt.expectError {
				require.Error(t, err)
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

func TestPathHandler_ProcessLinkingWithConfirmations(t *testing.T) {
	handler := path.NewPathHandler()

	t.Run("returns_correct_processing_result", func(t *testing.T) {
		matches := []types.RuleMatch{
			{
				Path:         "bin",
				AbsolutePath: "/dotfiles/tools/bin",
				Pack:         "tools",
				RuleName:     "directory",
			},
		}

		result, err := handler.ProcessLinkingWithConfirmations(matches)
		require.NoError(t, err)
		require.NotNil(t, result)
		assert.Len(t, result.Actions, 1)
		assert.Empty(t, result.Confirmations)

		// Verify action is correct type
		_, ok := result.Actions[0].(*types.AddToPathAction)
		assert.True(t, ok)
	})
}

func TestPathHandler_ValidateOptions(t *testing.T) {
	handler := path.NewPathHandler()

	tests := []struct {
		name        string
		options     map[string]interface{}
		expectError bool
		errorMsg    string
	}{
		{
			name:        "nil_options_is_valid",
			options:     nil,
			expectError: false,
		},
		{
			name:        "empty_options_is_valid",
			options:     map[string]interface{}{},
			expectError: false,
		},
		{
			name: "valid_target_option",
			options: map[string]interface{}{
				"target": "/custom/path",
			},
			expectError: false,
		},
		{
			name: "invalid_target_type",
			options: map[string]interface{}{
				"target": 123,
			},
			expectError: true,
			errorMsg:    "target option must be a string",
		},
		{
			name: "unknown_option",
			options: map[string]interface{}{
				"unknown": "value",
			},
			expectError: true,
			errorMsg:    "unknown option",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := handler.ValidateOptions(tt.options)
			if tt.expectError {
				require.Error(t, err)
				if tt.errorMsg != "" {
					assert.Contains(t, err.Error(), tt.errorMsg)
				}
			} else {
				require.NoError(t, err)
			}
		})
	}
}

func TestPathHandler_Properties(t *testing.T) {
	handler := path.NewPathHandler()

	t.Run("name_returns_correct_value", func(t *testing.T) {
		assert.Equal(t, path.PathHandlerName, handler.Name())
		assert.Equal(t, "path", handler.Name())
	})

	t.Run("description_returns_correct_value", func(t *testing.T) {
		assert.Equal(t, "Adds directories to PATH", handler.Description())
	})

	t.Run("type_returns_configuration", func(t *testing.T) {
		assert.Equal(t, types.HandlerTypeConfiguration, handler.Type())
	})

	t.Run("template_content_is_empty", func(t *testing.T) {
		assert.Empty(t, handler.GetTemplateContent())
	})
}
