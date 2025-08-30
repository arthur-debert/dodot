package install

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
)

func TestInstallHandler_ProcessProvisioning(t *testing.T) {
	// Create a temporary directory for test scripts
	tempDir := t.TempDir()

	// Create test scripts with known content
	script1Content := "#!/bin/sh\necho 'Installing pack1'\n"
	script1Path := filepath.Join(tempDir, "pack1", "install.sh")
	if err := os.MkdirAll(filepath.Dir(script1Path), 0755); err != nil {
		t.Fatalf("failed to create dir: %v", err)
	}
	if err := os.WriteFile(script1Path, []byte(script1Content), 0755); err != nil {
		t.Fatalf("failed to write script: %v", err)
	}

	script2Content := "#!/bin/sh\necho 'Installing pack2'\n"
	script2Path := filepath.Join(tempDir, "pack2", "provision.sh")
	if err := os.MkdirAll(filepath.Dir(script2Path), 0755); err != nil {
		t.Fatalf("failed to create dir: %v", err)
	}
	if err := os.WriteFile(script2Path, []byte(script2Content), 0755); err != nil {
		t.Fatalf("failed to write script: %v", err)
	}

	handler := NewInstallHandler()

	tests := []struct {
		name          string
		matches       []types.RuleMatch
		expectedCount int
		expectedError bool
		checkActions  func(t *testing.T, actions []types.ProvisioningAction)
	}{
		{
			name: "single install script",
			matches: []types.RuleMatch{
				{
					Path:         "install.sh",
					AbsolutePath: script1Path,
					Pack:         "pack1",
					RuleName:     "filename",
				},
			},
			expectedCount: 1,
			expectedError: false,
			checkActions: func(t *testing.T, actions []types.ProvisioningAction) {
				action, ok := actions[0].(*types.RunScriptAction)
				if !ok {
					t.Fatalf("action should be RunScriptAction, got %T", actions[0])
				}
				if action.PackName != "pack1" {
					t.Errorf("PackName = %q, want %q", action.PackName, "pack1")
				}
				if action.ScriptPath != script1Path {
					t.Errorf("ScriptPath = %q, want %q", action.ScriptPath, script1Path)
				}
				if action.Checksum == "" {
					t.Error("Checksum should not be empty")
				}
				if !strings.Contains(action.Checksum, "sha256:") {
					t.Errorf("Checksum should contain 'sha256:', got %q", action.Checksum)
				}
				if action.SentinelName != "install.sh.sentinel" {
					t.Errorf("SentinelName = %q, want %q", action.SentinelName, "install.sh.sentinel")
				}
			},
		},
		{
			name: "multiple provision scripts",
			matches: []types.RuleMatch{
				{
					Path:         "install.sh",
					AbsolutePath: script1Path,
					Pack:         "pack1",
					RuleName:     "filename",
				},
				{
					Path:         "provision.sh",
					AbsolutePath: script2Path,
					Pack:         "pack2",
					RuleName:     "filename",
				},
			},
			expectedCount: 2,
			expectedError: false,
			checkActions: func(t *testing.T, actions []types.ProvisioningAction) {
				// Check first action
				action1, ok := actions[0].(*types.RunScriptAction)
				if !ok {
					t.Fatalf("actions[0] should be RunScriptAction, got %T", actions[0])
				}
				if action1.PackName != "pack1" {
					t.Errorf("action1.PackName = %q, want %q", action1.PackName, "pack1")
				}
				if action1.ScriptPath != script1Path {
					t.Errorf("action1.ScriptPath = %q, want %q", action1.ScriptPath, script1Path)
				}
				if action1.SentinelName != "install.sh.sentinel" {
					t.Errorf("action1.SentinelName = %q, want %q", action1.SentinelName, "install.sh.sentinel")
				}

				// Check second action
				action2, ok := actions[1].(*types.RunScriptAction)
				if !ok {
					t.Fatalf("actions[1] should be RunScriptAction, got %T", actions[1])
				}
				if action2.PackName != "pack2" {
					t.Errorf("action2.PackName = %q, want %q", action2.PackName, "pack2")
				}
				if action2.ScriptPath != script2Path {
					t.Errorf("action2.ScriptPath = %q, want %q", action2.ScriptPath, script2Path)
				}
				if action2.SentinelName != "provision.sh.sentinel" {
					t.Errorf("action2.SentinelName = %q, want %q", action2.SentinelName, "provision.sh.sentinel")
				}

				// Verify different checksums (different content)
				if action1.Checksum == action2.Checksum {
					t.Error("Different scripts should have different checksums")
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
			name: "non-existent script",
			matches: []types.RuleMatch{
				{
					Path:         "missing.sh",
					AbsolutePath: "/non/existent/path/missing.sh",
					Pack:         "missing",
					RuleName:     "filename",
				},
			},
			expectedCount: 0,
			expectedError: true,
		},
		{
			name: "nested provision script",
			matches: []types.RuleMatch{
				{
					Path:         "scripts/setup.sh",
					AbsolutePath: script1Path,
					Pack:         "complex",
					RuleName:     "glob",
				},
			},
			expectedCount: 1,
			expectedError: false,
			checkActions: func(t *testing.T, actions []types.ProvisioningAction) {
				action, ok := actions[0].(*types.RunScriptAction)
				if !ok {
					t.Fatalf("action should be RunScriptAction, got %T", actions[0])
				}
				if action.PackName != "complex" {
					t.Errorf("PackName = %q, want %q", action.PackName, "complex")
				}
				if action.SentinelName != "scripts/setup.sh.sentinel" {
					t.Errorf("SentinelName = %q, want %q", action.SentinelName, "scripts/setup.sh.sentinel")
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

func TestInstallHandler_ValidateOptions(t *testing.T) {
	handler := NewInstallHandler()

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

func TestInstallHandler_Properties(t *testing.T) {
	handler := NewInstallHandler()

	if got := handler.Name(); got != InstallHandlerName {
		t.Errorf("Name() = %q, want %q", got, InstallHandlerName)
	}

	expectedDesc := "Runs install.sh scripts for initial setup"
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
	if !strings.Contains(template, "dodot install script") {
		t.Error("Template should contain 'dodot install script'")
	}
}

func TestInstallHandler_ProcessProvisioningWithConfirmations(t *testing.T) {
	// Create test script
	tempDir := t.TempDir()
	scriptPath := filepath.Join(tempDir, "install.sh")
	scriptContent := "#!/bin/sh\necho 'Installing'\n"

	if err := os.WriteFile(scriptPath, []byte(scriptContent), 0755); err != nil {
		t.Fatalf("failed to write script: %v", err)
	}

	handler := NewInstallHandler()

	matches := []types.RuleMatch{
		{
			Path:         "install.sh",
			AbsolutePath: scriptPath,
			Pack:         "test-pack",
			RuleName:     "filename",
		},
	}

	result, err := handler.ProcessProvisioningWithConfirmations(matches)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	// Should have actions but no confirmations (provisioning doesn't need confirmation)
	if len(result.Actions) != 1 {
		t.Errorf("got %d actions, want 1", len(result.Actions))
	}
	if len(result.Confirmations) != 0 {
		t.Errorf("got %d confirmations, want 0", len(result.Confirmations))
	}
	if result.HasConfirmations() {
		t.Error("HasConfirmations() = true, want false")
	}

	// Action should be RunScriptAction
	scriptAction, ok := result.Actions[0].(*types.RunScriptAction)
	if !ok {
		t.Fatalf("action should be RunScriptAction, got %T", result.Actions[0])
	}
	if scriptAction.PackName != "test-pack" {
		t.Errorf("PackName = %q, want %q", scriptAction.PackName, "test-pack")
	}
	if scriptAction.ScriptPath != scriptPath {
		t.Errorf("ScriptPath = %q, want %q", scriptAction.ScriptPath, scriptPath)
	}
	if scriptAction.Checksum == "" {
		t.Error("Checksum should not be empty")
	}
	if scriptAction.SentinelName != "install.sh.sentinel" {
		t.Errorf("SentinelName = %q, want %q", scriptAction.SentinelName, "install.sh.sentinel")
	}
}

// TODO: Add tests for Clear functionality once implemented
func TestInstallHandler_Clear_TODO(t *testing.T) {
	t.Skip("Clear functionality tests to be implemented when Clear method is complete")
}
