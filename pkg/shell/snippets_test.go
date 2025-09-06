// pkg/shell/snippets_test.go
// TEST TYPE: Unit Tests
// DEPENDENCIES: None
// PURPOSE: Test shell integration snippet generation

package shell_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/shell"
	"github.com/stretchr/testify/assert"
)

func TestGetShellIntegrationSnippet(t *testing.T) {
	tests := []struct {
		name           string
		shell          string
		customDataDir  string
		expectedResult string
	}{
		{
			name:           "bash_default",
			shell:          "bash",
			customDataDir:  "",
			expectedResult: `[ -f "$HOME/.local/share/dodot/shell/dodot-init.sh" ] && source "$HOME/.local/share/dodot/shell/dodot-init.sh"`,
		},
		{
			name:           "bash_custom_dir",
			shell:          "bash",
			customDataDir:  "/custom/dodot",
			expectedResult: `[ -f "/custom/dodot/shell/dodot-init.sh" ] && source "/custom/dodot/shell/dodot-init.sh"`,
		},
		{
			name:           "zsh_default",
			shell:          "zsh",
			customDataDir:  "",
			expectedResult: `[ -f "$HOME/.local/share/dodot/shell/dodot-init.sh" ] && source "$HOME/.local/share/dodot/shell/dodot-init.sh"`,
		},
		{
			name:           "zsh_custom_dir",
			shell:          "zsh",
			customDataDir:  "/test/data",
			expectedResult: `[ -f "/test/data/shell/dodot-init.sh" ] && source "/test/data/shell/dodot-init.sh"`,
		},
		{
			name:          "fish_default",
			shell:         "fish",
			customDataDir: "",
			expectedResult: `if test -f "$HOME/.local/share/dodot/shell/dodot-init.fish"
    source "$HOME/.local/share/dodot/shell/dodot-init.fish"
end`,
		},
		{
			name:          "fish_custom_dir",
			shell:         "fish",
			customDataDir: "/home/user/.dodot",
			expectedResult: `if test -f "/home/user/.dodot/shell/dodot-init.fish"
    source "/home/user/.dodot/shell/dodot-init.fish"
end`,
		},
		{
			name:           "unknown_shell_defaults_to_bash",
			shell:          "unknown",
			customDataDir:  "",
			expectedResult: `[ -f "$HOME/.local/share/dodot/shell/dodot-init.sh" ] && source "$HOME/.local/share/dodot/shell/dodot-init.sh"`,
		},
		{
			name:           "empty_shell_defaults_to_bash",
			shell:          "",
			customDataDir:  "/test",
			expectedResult: `[ -f "/test/shell/dodot-init.sh" ] && source "/test/shell/dodot-init.sh"`,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := shell.GetShellIntegrationSnippet(tt.shell, tt.customDataDir)
			if tt.customDataDir != "" {
				// When custom dir is provided, it should use that
				assert.Equal(t, tt.expectedResult, result)
			} else {
				// When no custom dir, it may use system path or default
				// Just verify it has the right script name
				if tt.shell == "fish" {
					assert.Contains(t, result, "dodot-init.fish")
					assert.Contains(t, result, "source")
				} else {
					assert.Contains(t, result, "dodot-init.sh")
					assert.Contains(t, result, "source")
				}
			}
		})
	}
}

func TestGetShellIntegrationSnippet_PathsWithSpecialChars(t *testing.T) {
	tests := []struct {
		name    string
		shell   string
		dataDir string
		// Just check that the path appears in the result
	}{
		{
			name:    "path_with_spaces_bash",
			shell:   "bash",
			dataDir: "/path with spaces/dodot",
		},
		{
			name:    "path_with_special_chars_bash",
			shell:   "bash",
			dataDir: "/path$with'special\"chars",
		},
		{
			name:    "path_with_spaces_fish",
			shell:   "fish",
			dataDir: "/path with spaces/dodot",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Set a fake PROJECT_ROOT that doesn't exist to ensure we fall back to dataDir
			t.Setenv("PROJECT_ROOT", "/nonexistent")

			result := shell.GetShellIntegrationSnippet(tt.shell, tt.dataDir)
			// When a dataDir is provided and no installed script is found,
			// it should use the dataDir path
			if tt.dataDir != "" {
				assert.Contains(t, result, tt.dataDir)
			}
		})
	}
}

func TestGetShellIntegrationSnippet_Constants(t *testing.T) {
	// Clear PROJECT_ROOT to ensure we don't pick up development paths
	t.Setenv("PROJECT_ROOT", "")

	// Test that we get valid snippets (exact path may vary based on installation)
	bashSnippet := shell.GetShellIntegrationSnippet("bash", "")
	zshSnippet := shell.GetShellIntegrationSnippet("zsh", "")
	fishSnippet := shell.GetShellIntegrationSnippet("fish", "")

	// Check that snippets have the expected structure
	assert.Contains(t, bashSnippet, "dodot-init.sh")
	assert.Contains(t, bashSnippet, "source")

	assert.Contains(t, zshSnippet, "dodot-init.sh")
	assert.Contains(t, zshSnippet, "source")

	assert.Contains(t, fishSnippet, "dodot-init.fish")
	assert.Contains(t, fishSnippet, "source")
}
