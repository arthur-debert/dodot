package template

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestTemplatePowerUp_Basic(t *testing.T) {
	powerup := NewTemplatePowerUp()

	// Test basic properties
	testutil.AssertEqual(t, TemplatePowerUpName, powerup.Name())
	testutil.AssertEqual(t, "Processes template files with variable substitution", powerup.Description())
	testutil.AssertEqual(t, types.RunModeMany, powerup.RunMode())

	// Test default variables are set
	testutil.AssertNotNil(t, powerup.variables)
	testutil.AssertTrue(t, len(powerup.variables) > 0)
}

func TestTemplatePowerUp_Process(t *testing.T) {
	tests := []struct {
		name          string
		matches       []types.TriggerMatch
		expectedCount int
		expectError   bool
		validate      func(t *testing.T, actions []types.Action)
	}{
		{
			name: "single template file",
			matches: []types.TriggerMatch{
				{
					Pack:         "test-pack",
					Path:         "config.tmpl",
					AbsolutePath: "/home/user/dotfiles/test-pack/config.tmpl",
					TriggerName:  "filename",
					PowerUpName:  TemplatePowerUpName,
				},
			},
			expectedCount: 1,
			validate: func(t *testing.T, actions []types.Action) {
				action := actions[0]
				testutil.AssertEqual(t, types.ActionTypeTemplate, action.Type)
				testutil.AssertEqual(t, "/home/user/dotfiles/test-pack/config.tmpl", action.Source)
				testutil.AssertEqual(t, "~/config", action.Target) // .tmpl removed
				testutil.AssertEqual(t, "test-pack", action.Pack)
				testutil.AssertEqual(t, TemplatePowerUpName, action.PowerUpName)
				testutil.AssertEqual(t, config.Default().Priorities.PowerUps["template"], action.Priority)

				// Check metadata contains variables
				vars, ok := action.Metadata["variables"].(map[string]string)
				testutil.AssertTrue(t, ok)
				testutil.AssertTrue(t, len(vars) > 0)
			},
		},
		{
			name: "template without .tmpl extension",
			matches: []types.TriggerMatch{
				{
					Pack:         "test-pack",
					Path:         "bashrc",
					AbsolutePath: "/dotfiles/test-pack/bashrc",
					TriggerName:  "filename",
					PowerUpName:  TemplatePowerUpName,
				},
			},
			expectedCount: 1,
			validate: func(t *testing.T, actions []types.Action) {
				testutil.AssertEqual(t, "~/bashrc", actions[0].Target)
			},
		},
		{
			name: "custom target directory",
			matches: []types.TriggerMatch{
				{
					Pack:         "test-pack",
					Path:         "gitconfig.tmpl",
					AbsolutePath: "/dotfiles/test-pack/gitconfig.tmpl",
					TriggerName:  "filename",
					PowerUpName:  TemplatePowerUpName,
					PowerUpOptions: map[string]interface{}{
						"target": "~/.config",
					},
				},
			},
			expectedCount: 1,
			validate: func(t *testing.T, actions []types.Action) {
				testutil.AssertEqual(t, "~/.config/gitconfig", actions[0].Target)
			},
		},
		{
			name: "custom variables",
			matches: []types.TriggerMatch{
				{
					Pack:         "test-pack",
					Path:         "config.tmpl",
					AbsolutePath: "/dotfiles/test-pack/config.tmpl",
					TriggerName:  "filename",
					PowerUpName:  TemplatePowerUpName,
					PowerUpOptions: map[string]interface{}{
						"variables": map[string]interface{}{
							"EMAIL": "user@example.com",
							"NAME":  "Test User",
						},
					},
				},
			},
			expectedCount: 1,
			validate: func(t *testing.T, actions []types.Action) {
				vars, ok := actions[0].Metadata["variables"].(map[string]string)
				testutil.AssertTrue(t, ok)
				testutil.AssertEqual(t, "user@example.com", vars["EMAIL"])
				testutil.AssertEqual(t, "Test User", vars["NAME"])
			},
		},
		{
			name: "multiple templates",
			matches: []types.TriggerMatch{
				{
					Pack:         "pack1",
					Path:         "config1.tmpl",
					AbsolutePath: "/dotfiles/pack1/config1.tmpl",
					TriggerName:  "filename",
					PowerUpName:  TemplatePowerUpName,
				},
				{
					Pack:         "pack2",
					Path:         "config2.tmpl",
					AbsolutePath: "/dotfiles/pack2/config2.tmpl",
					TriggerName:  "filename",
					PowerUpName:  TemplatePowerUpName,
				},
			},
			expectedCount: 2,
		},
		{
			name:          "empty matches",
			matches:       []types.TriggerMatch{},
			expectedCount: 0,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			powerup := NewTemplatePowerUp()
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

func TestTemplatePowerUp_ValidateOptions(t *testing.T) {
	powerup := NewTemplatePowerUp()

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
				"target": "~/.config",
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
			name: "valid variables option",
			options: map[string]interface{}{
				"variables": map[string]interface{}{
					"VAR1": "value1",
					"VAR2": "value2",
				},
			},
		},
		{
			name: "invalid variables type",
			options: map[string]interface{}{
				"variables": "not a map",
			},
			expectError: true,
		},
		{
			name: "invalid variable value type",
			options: map[string]interface{}{
				"variables": map[string]interface{}{
					"VAR1": 123, // not a string
				},
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
