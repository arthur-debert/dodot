// pkg/handlers/symlink/symlink_test.go
// TEST TYPE: Unit/Integration Test
// DEPENDENCIES: Mock DataStore, Memory FS
// PURPOSE: Test symlink handler logic without real filesystem

package symlink_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/handlers/symlink"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestSymlinkHandler_ProcessLinking_Unit(t *testing.T) {
	tests := []struct {
		name          string
		matches       []types.RuleMatch
		expectedCount int
		verify        func(t *testing.T, actions []types.LinkingAction)
	}{
		{
			name: "single_file_creates_link_action",
			matches: []types.RuleMatch{
				{
					Path:         ".vimrc",
					AbsolutePath: "/dotfiles/vim/.vimrc",
					Pack:         "vim",
					RuleName:     "vimrc",
				},
			},
			expectedCount: 1,
			verify: func(t *testing.T, actions []types.LinkingAction) {
				linkAction, ok := actions[0].(*types.LinkAction)
				if !ok {
					t.Error("expected LinkAction type")
					return
				}
				if linkAction.PackName != "vim" {
					t.Errorf("expected pack vim, got %s", linkAction.PackName)
				}
			},
		},
		{
			name: "multiple_files_create_multiple_actions",
			matches: []types.RuleMatch{
				{
					Path:         ".bashrc",
					AbsolutePath: "/dotfiles/bash/.bashrc",
					Pack:         "bash",
					RuleName:     "bashrc",
				},
				{
					Path:         ".bash_profile",
					AbsolutePath: "/dotfiles/bash/.bash_profile",
					Pack:         "bash",
					RuleName:     "bash_profile",
				},
			},
			expectedCount: 2,
			verify: func(t *testing.T, actions []types.LinkingAction) {
				for _, action := range actions {
					if _, ok := action.(*types.LinkAction); !ok {
						t.Error("all actions should be LinkActions")
					}
				}
			},
		},
		{
			name: "custom_target_directory",
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
			verify: func(t *testing.T, actions []types.LinkingAction) {
				linkAction, ok := actions[0].(*types.LinkAction)
				if !ok {
					t.Error("expected LinkAction type")
					return
				}
				if linkAction.TargetFile != "/etc/app/config.json" {
					t.Errorf("expected target /etc/app/config.json, got %s", linkAction.TargetFile)
				}
			},
		},
	}

	// Set test mode to avoid paths initialization
	t.Setenv("DODOT_TEST_MODE", "true")
	t.Setenv("HOME", "/home/testuser")

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			handler := symlink.NewSymlinkHandler()
			actions, err := handler.ProcessLinking(tt.matches)

			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}

			if len(actions) != tt.expectedCount {
				t.Errorf("expected %d actions, got %d", tt.expectedCount, len(actions))
			}

			if tt.verify != nil {
				tt.verify(t, actions)
			}
		})
	}
}

func TestSymlinkHandler_ProcessLinkingWithConfirmations(t *testing.T) {
	// Set test mode
	t.Setenv("DODOT_TEST_MODE", "true")
	t.Setenv("HOME", "/home/testuser")

	handler := symlink.NewSymlinkHandler()

	matches := []types.RuleMatch{
		{
			Path:         ".vimrc",
			AbsolutePath: "/dotfiles/vim/.vimrc",
			Pack:         "vim",
			RuleName:     "vimrc",
		},
		{
			Path:         ".gvimrc",
			AbsolutePath: "/dotfiles/vim/.gvimrc",
			Pack:         "vim",
			RuleName:     "gvimrc",
		},
	}

	result, err := handler.ProcessLinkingWithConfirmations(matches)
	if err != nil {
		t.Fatalf("ProcessLinkingWithConfirmations failed: %v", err)
	}

	// Should have 2 actions
	if len(result.Actions) != 2 {
		t.Errorf("expected 2 actions, got %d", len(result.Actions))
	}

	// All actions should be LinkActions
	for i, action := range result.Actions {
		if _, ok := action.(*types.LinkAction); !ok {
			t.Errorf("action %d is not a LinkAction", i)
		}
	}

	// Should not require confirmations by default
	if len(result.Confirmations) != 0 {
		t.Errorf("expected no confirmations, got %d", len(result.Confirmations))
	}
}

func TestSymlinkHandler_ConflictDetection(t *testing.T) {
	// Set test mode
	t.Setenv("DODOT_TEST_MODE", "true")
	t.Setenv("HOME", "/home/testuser")

	handler := symlink.NewSymlinkHandler()

	// Two files targeting the same location
	matches := []types.RuleMatch{
		{
			Path:         ".config",
			AbsolutePath: "/dotfiles/app1/.config",
			Pack:         "app1",
			RuleName:     "config",
		},
		{
			Path:         ".config",
			AbsolutePath: "/dotfiles/app2/.config",
			Pack:         "app2",
			RuleName:     "config",
		},
	}

	_, err := handler.ProcessLinkingWithConfirmations(matches)
	if err == nil {
		t.Fatal("expected conflict error, got nil")
	}

	// Should have detected the conflict and returned an error
	expectedError := "symlink conflict: both /dotfiles/app1/.config and /dotfiles/app2/.config want to link to /home/testuser/.config"
	if err.Error() != expectedError {
		t.Errorf("expected error %q, got %q", expectedError, err.Error())
	}
}
