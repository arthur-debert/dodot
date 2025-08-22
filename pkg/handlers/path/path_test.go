package path

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestPathHandler_Basic(t *testing.T) {
	handler := NewPathHandler()

	// Test basic properties
	testutil.AssertEqual(t, PathHandlerName, handler.Name())
	testutil.AssertEqual(t, "Adds directories to PATH", handler.Description())
	testutil.AssertEqual(t, types.RunModeLinking, handler.RunMode())
}

func TestPathHandler_Process(t *testing.T) {
	tests := []struct {
		name          string
		matches       []types.TriggerMatch
		expectedCount int
		expectError   bool
		validate      func(t *testing.T, actions []types.Action)
	}{
		{
			name: "single directory",
			matches: []types.TriggerMatch{
				{
					Pack:         "test-pack",
					Path:         "bin",
					AbsolutePath: "/home/user/dotfiles/test-pack/bin",
					TriggerName:  "directory",
					HandlerName:  PathHandlerName,
				},
			},
			expectedCount: 1,
			validate: func(t *testing.T, actions []types.Action) {
				action := actions[0]
				testutil.AssertEqual(t, types.ActionTypePathAdd, action.Type)
				testutil.AssertEqual(t, "/home/user/dotfiles/test-pack/bin", action.Source)
				testutil.AssertEqual(t, "/home/user/dotfiles/test-pack/bin", action.Target)
				testutil.AssertEqual(t, "test-pack", action.Pack)
				testutil.AssertEqual(t, PathHandlerName, action.HandlerName)
				testutil.AssertEqual(t, config.Default().Priorities.Handlers["path"], action.Priority)
				testutil.AssertEqual(t, "bin", action.Metadata["dirName"])
			},
		},
		{
			name: "multiple directories",
			matches: []types.TriggerMatch{
				{
					Pack:         "pack1",
					Path:         "bin",
					AbsolutePath: "/dotfiles/pack1/bin",
					TriggerName:  "directory",
					HandlerName:  PathHandlerName,
				},
				{
					Pack:         "pack2",
					Path:         ".local/bin",
					AbsolutePath: "/dotfiles/pack2/.local/bin",
					TriggerName:  "directory",
					HandlerName:  PathHandlerName,
				},
			},
			expectedCount: 2,
			validate: func(t *testing.T, actions []types.Action) {
				// First action
				testutil.AssertEqual(t, "/dotfiles/pack1/bin", actions[0].Target)
				testutil.AssertEqual(t, "bin", actions[0].Metadata["dirName"])
				// Second action
				testutil.AssertEqual(t, "/dotfiles/pack2/.local/bin", actions[1].Target)
				testutil.AssertEqual(t, ".local/bin", actions[1].Metadata["dirName"])
			},
		},
		{
			name: "duplicate directories skipped",
			matches: []types.TriggerMatch{
				{
					Pack:         "test-pack",
					Path:         "bin",
					AbsolutePath: "/dotfiles/test-pack/bin",
					TriggerName:  "directory",
					HandlerName:  PathHandlerName,
				},
				{
					Pack:         "test-pack",
					Path:         "bin",
					AbsolutePath: "/dotfiles/test-pack/bin",
					TriggerName:  "directory",
					HandlerName:  PathHandlerName,
				},
			},
			expectedCount: 1,
			validate: func(t *testing.T, actions []types.Action) {
				testutil.AssertEqual(t, "/dotfiles/test-pack/bin", actions[0].Target)
			},
		},
		{
			name:          "empty matches",
			matches:       []types.TriggerMatch{},
			expectedCount: 0,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			handler := NewPathHandler()
			actions, err := handler.Process(tt.matches)

			if tt.expectError {
				testutil.AssertError(t, err)
				return
			}

			testutil.AssertNoError(t, err)
			testutil.AssertEqual(t, tt.expectedCount, len(actions))

			if tt.validate != nil {
				tt.validate(t, actions)
			}
		})
	}
}

func TestPathHandler_ValidateOptions(t *testing.T) {
	handler := NewPathHandler()

	tests := []struct {
		name        string
		options     map[string]interface{}
		expectError bool
	}{
		{
			name:    "nil options",
			options: nil,
		},
		{
			name:    "empty options",
			options: map[string]interface{}{},
		},
		{
			name: "valid target option",
			options: map[string]interface{}{
				"target": "~/.local/bin",
			},
		},
		{
			name: "invalid target type",
			options: map[string]interface{}{
				"target": 123,
			},
			expectError: true,
		},
		{
			name: "unknown option",
			options: map[string]interface{}{
				"unknown": "value",
			},
			expectError: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := handler.ValidateOptions(tt.options)
			if tt.expectError {
				testutil.AssertError(t, err)
			} else {
				testutil.AssertNoError(t, err)
			}
		})
	}
}
