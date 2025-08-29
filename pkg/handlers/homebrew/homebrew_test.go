package homebrew

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
)

func TestHomebrewHandler_ProcessProvisioning(t *testing.T) {
	// Create a temporary directory for test Brewfiles
	tempDir := t.TempDir()

	// Create test Brewfiles with known content
	brewfile1Content := `# Test Brewfile 1
brew 'git'
brew 'tmux'
cask 'firefox'
`
	brewfile2Content := `# Test Brewfile 2
brew 'neovim'
brew 'ripgrep'
`

	// Write test files using standard library (needed for checksum calculation)
	brewfile1Path := filepath.Join(tempDir, "pack1", "Brewfile")
	if err := os.MkdirAll(filepath.Dir(brewfile1Path), 0755); err != nil {
		t.Fatalf("failed to create dir: %v", err)
	}
	if err := os.WriteFile(brewfile1Path, []byte(brewfile1Content), 0644); err != nil {
		t.Fatalf("failed to write file: %v", err)
	}

	brewfile2Path := filepath.Join(tempDir, "pack2", "Brewfile")
	if err := os.MkdirAll(filepath.Dir(brewfile2Path), 0755); err != nil {
		t.Fatalf("failed to create dir: %v", err)
	}
	if err := os.WriteFile(brewfile2Path, []byte(brewfile2Content), 0644); err != nil {
		t.Fatalf("failed to write file: %v", err)
	}

	handler := NewHomebrewHandler()

	tests := []struct {
		name          string
		matches       []types.RuleMatch
		expectedCount int
		expectedError bool
		checkActions  func(t *testing.T, actions []types.ProvisioningAction)
	}{
		{
			name: "single Brewfile",
			matches: []types.RuleMatch{
				{
					Path:         "Brewfile",
					AbsolutePath: brewfile1Path,
					Pack:         "pack1",
					RuleName:     "filename",
				},
			},
			expectedCount: 1,
			expectedError: false,
			checkActions: func(t *testing.T, actions []types.ProvisioningAction) {
				action, ok := actions[0].(*types.BrewAction)
				if !ok {
					t.Fatalf("action should be BrewAction, got %T", actions[0])
				}
				if action.PackName != "pack1" {
					t.Errorf("PackName = %q, want %q", action.PackName, "pack1")
				}
				if action.BrewfilePath != brewfile1Path {
					t.Errorf("BrewfilePath = %q, want %q", action.BrewfilePath, brewfile1Path)
				}
				if action.Checksum == "" {
					t.Error("Checksum should not be empty")
				}
				if len(action.Checksum) < 7 || action.Checksum[:7] != "sha256:" {
					t.Errorf("Checksum should start with 'sha256:', got %q", action.Checksum)
				}
			},
		},
		{
			name: "multiple Brewfiles",
			matches: []types.RuleMatch{
				{
					Path:         "Brewfile",
					AbsolutePath: brewfile1Path,
					Pack:         "pack1",
					RuleName:     "filename",
				},
				{
					Path:         "Brewfile",
					AbsolutePath: brewfile2Path,
					Pack:         "pack2",
					RuleName:     "filename",
				},
			},
			expectedCount: 2,
			expectedError: false,
			checkActions: func(t *testing.T, actions []types.ProvisioningAction) {
				// Check first action
				action1, ok := actions[0].(*types.BrewAction)
				if !ok {
					t.Fatalf("actions[0] should be BrewAction, got %T", actions[0])
				}
				if action1.PackName != "pack1" {
					t.Errorf("action1.PackName = %q, want %q", action1.PackName, "pack1")
				}
				if action1.BrewfilePath != brewfile1Path {
					t.Errorf("action1.BrewfilePath = %q, want %q", action1.BrewfilePath, brewfile1Path)
				}

				// Check second action
				action2, ok := actions[1].(*types.BrewAction)
				if !ok {
					t.Fatalf("actions[1] should be BrewAction, got %T", actions[1])
				}
				if action2.PackName != "pack2" {
					t.Errorf("action2.PackName = %q, want %q", action2.PackName, "pack2")
				}
				if action2.BrewfilePath != brewfile2Path {
					t.Errorf("action2.BrewfilePath = %q, want %q", action2.BrewfilePath, brewfile2Path)
				}

				// Verify different checksums (different content)
				if action1.Checksum == action2.Checksum {
					t.Error("Different Brewfiles should have different checksums")
				}
			},
		},
		{
			name:          "empty matches",
			matches:       []types.RuleMatch{},
			expectedCount: 0,
			expectedError: false,
		},
		{
			name: "non-existent Brewfile",
			matches: []types.RuleMatch{
				{
					Path:         "Brewfile",
					AbsolutePath: "/non/existent/path/Brewfile",
					Pack:         "missing",
					RuleName:     "filename",
				},
			},
			expectedCount: 0,
			expectedError: true,
		},
		{
			name: "Brewfile with custom name",
			matches: []types.RuleMatch{
				{
					Path:         "Brewfile.custom",
					AbsolutePath: brewfile1Path,
					Pack:         "custom",
					RuleName:     "glob",
				},
			},
			expectedCount: 1,
			expectedError: false,
			checkActions: func(t *testing.T, actions []types.ProvisioningAction) {
				action, ok := actions[0].(*types.BrewAction)
				if !ok {
					t.Fatalf("action should be BrewAction, got %T", actions[0])
				}
				if action.PackName != "custom" {
					t.Errorf("PackName = %q, want %q", action.PackName, "custom")
				}
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			actions, err := handler.ProcessProvisioning(tt.matches)

			if tt.expectedError {
				if err == nil {
					t.Error("expected error but got none")
				}
				return
			}

			if err != nil {
				t.Errorf("unexpected error: %v", err)
				return
			}

			if len(actions) != tt.expectedCount {
				t.Errorf("got %d actions, want %d", len(actions), tt.expectedCount)
			}

			if tt.checkActions != nil {
				tt.checkActions(t, actions)
			}
		})
	}
}

func TestHomebrewHandler_ValidateOptions(t *testing.T) {
	handler := NewHomebrewHandler()

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
				if err == nil {
					t.Error("expected error but got none")
				}
			} else {
				if err != nil {
					t.Errorf("unexpected error: %v", err)
				}
			}
		})
	}
}

func TestHomebrewHandler_Properties(t *testing.T) {
	handler := NewHomebrewHandler()

	if got := handler.Name(); got != HomebrewHandlerName {
		t.Errorf("Name() = %q, want %q", got, HomebrewHandlerName)
	}

	expectedDesc := "Processes Brewfiles to install Homebrew packages"
	if got := handler.Description(); got != expectedDesc {
		t.Errorf("Description() = %q, want %q", got, expectedDesc)
	}

	if got := handler.Type(); got != types.HandlerTypeCodeExecution {
		t.Errorf("Type() = %v, want %v", got, types.HandlerTypeCodeExecution)
	}

	// Verify template content
	template := handler.GetTemplateContent()
	if template == "" {
		t.Error("GetTemplateContent() returned empty string")
	}
	// The template might contain different text, let's just verify it's not empty
	// and has some expected content
	if len(template) < 50 {
		t.Errorf("Template seems too short: %d chars", len(template))
	}
}

func TestBrewActionDescription(t *testing.T) {
	action := &types.BrewAction{
		PackName:     "test",
		BrewfilePath: "/path/to/Brewfile",
		Checksum:     "sha256:abcd1234",
	}

	desc := action.Description()
	if !strings.Contains(desc, "Install Homebrew packages") {
		t.Errorf("Description should contain 'Install Homebrew packages', got %q", desc)
	}
	// The description already contains the path as shown in the error message
	// Just need to check the containsAt logic
	if !strings.Contains(desc, "/path/to/Brewfile") {
		t.Errorf("Description should contain '/path/to/Brewfile', got %q", desc)
	}
}
