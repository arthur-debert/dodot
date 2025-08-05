package path

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestPathPowerUp_Basic(t *testing.T) {
	powerup := NewPathPowerUp()

	// Test basic properties
	testutil.AssertEqual(t, PathPowerUpName, powerup.Name())
	testutil.AssertEqual(t, "Creates symlinks for executable files in ~/bin", powerup.Description())
	testutil.AssertEqual(t, types.RunModeMany, powerup.RunMode())
}

func TestPathPowerUp_Process(t *testing.T) {
	tests := []struct {
		name          string
		matches       []types.TriggerMatch
		expectedCount int
		expectError   bool
		validate      func(t *testing.T, actions []types.Action)
	}{
		{
			name: "single executable",
			matches: []types.TriggerMatch{
				{
					Pack:         "test-pack",
					Path:         "bin/script.sh",
					AbsolutePath: "/home/user/dotfiles/test-pack/bin/script.sh",
					TriggerName:  "filename",
					PowerUpName:  PathPowerUpName,
				},
			},
			expectedCount: 1,
			validate: func(t *testing.T, actions []types.Action) {
				action := actions[0]
				testutil.AssertEqual(t, types.ActionTypeLink, action.Type)
				testutil.AssertEqual(t, "/home/user/dotfiles/test-pack/bin/script.sh", action.Source)
				testutil.AssertEqual(t, "~/bin/script.sh", action.Target)
				testutil.AssertEqual(t, "test-pack", action.Pack)
				testutil.AssertEqual(t, PathPowerUpName, action.PowerUpName)
				testutil.AssertEqual(t, config.Default().Priorities.PowerUps["path"], action.Priority)
			},
		},
		{
			name: "multiple executables",
			matches: []types.TriggerMatch{
				{
					Pack:         "pack1",
					Path:         "bin/tool1",
					AbsolutePath: "/dotfiles/pack1/bin/tool1",
					TriggerName:  "filename",
					PowerUpName:  PathPowerUpName,
				},
				{
					Pack:         "pack2",
					Path:         ".local/bin/tool2",
					AbsolutePath: "/dotfiles/pack2/.local/bin/tool2",
					TriggerName:  "filename",
					PowerUpName:  PathPowerUpName,
				},
			},
			expectedCount: 2,
			validate: func(t *testing.T, actions []types.Action) {
				// First action
				testutil.AssertEqual(t, "~/bin/tool1", actions[0].Target)
				// Second action - note it takes just the basename
				testutil.AssertEqual(t, "~/bin/tool2", actions[1].Target)
			},
		},
		{
			name: "custom target directory",
			matches: []types.TriggerMatch{
				{
					Pack:         "test-pack",
					Path:         "bin/script",
					AbsolutePath: "/dotfiles/test-pack/bin/script",
					TriggerName:  "filename",
					PowerUpName:  PathPowerUpName,
					PowerUpOptions: map[string]interface{}{
						"target": "~/.local/bin",
					},
				},
			},
			expectedCount: 1,
			validate: func(t *testing.T, actions []types.Action) {
				testutil.AssertEqual(t, "~/.local/bin/script", actions[0].Target)
			},
		},
		{
			name: "conflict detection",
			matches: []types.TriggerMatch{
				{
					Pack:         "pack1",
					Path:         "bin/tool",
					AbsolutePath: "/dotfiles/pack1/bin/tool",
					TriggerName:  "filename",
					PowerUpName:  PathPowerUpName,
				},
				{
					Pack:         "pack2",
					Path:         "scripts/tool",
					AbsolutePath: "/dotfiles/pack2/scripts/tool",
					TriggerName:  "filename",
					PowerUpName:  PathPowerUpName,
				},
			},
			expectError: true,
		},
		{
			name:          "empty matches",
			matches:       []types.TriggerMatch{},
			expectedCount: 0,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			powerup := NewPathPowerUp()
			actions, err := powerup.Process(tt.matches)

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

func TestPathPowerUp_ValidateOptions(t *testing.T) {
	powerup := NewPathPowerUp()

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
			err := powerup.ValidateOptions(tt.options)
			if tt.expectError {
				testutil.AssertError(t, err)
			} else {
				testutil.AssertNoError(t, err)
			}
		})
	}
}
