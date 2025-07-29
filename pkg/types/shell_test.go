package types

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
)

func TestGetShellIntegrationSnippet(t *testing.T) {
	tests := []struct {
		name           string
		shell          string
		customDataDir  string
		expectedResult string
	}{
		{
			name:          "bash_default",
			shell:         "bash",
			customDataDir: "",
			expectedResult: `[ -f "$HOME/.local/share/dodot/shell/dodot-init.sh" ] && source "$HOME/.local/share/dodot/shell/dodot-init.sh"`,
		},
		{
			name:          "bash_custom_dir",
			shell:         "bash",
			customDataDir: "/custom/dodot",
			expectedResult: `[ -f "/custom/dodot/shell/dodot-init.sh" ] && source "/custom/dodot/shell/dodot-init.sh"`,
		},
		{
			name:          "zsh_default",
			shell:         "zsh",
			customDataDir: "",
			expectedResult: `[ -f "$HOME/.local/share/dodot/shell/dodot-init.sh" ] && source "$HOME/.local/share/dodot/shell/dodot-init.sh"`,
		},
		{
			name:          "zsh_custom_dir",
			shell:         "zsh",
			customDataDir: "/test/data",
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
			name:          "unknown_shell_defaults_to_bash",
			shell:         "unknown",
			customDataDir: "",
			expectedResult: `[ -f "$HOME/.local/share/dodot/shell/dodot-init.sh" ] && source "$HOME/.local/share/dodot/shell/dodot-init.sh"`,
		},
		{
			name:          "empty_shell_defaults_to_bash",
			shell:         "",
			customDataDir: "/test",
			expectedResult: `[ -f "/test/shell/dodot-init.sh" ] && source "/test/shell/dodot-init.sh"`,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := GetShellIntegrationSnippet(tt.shell, tt.customDataDir)
			testutil.AssertEqual(t, tt.expectedResult, result)
		})
	}
}

func TestGetShellIntegrationSnippet_PathsWithSpecialChars(t *testing.T) {
	tests := []struct {
		name     string
		shell    string
		dataDir  string
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
			result := GetShellIntegrationSnippet(tt.shell, tt.dataDir)
			testutil.AssertContains(t, result, tt.dataDir)
		})
	}
}

func TestGetShellIntegrationSnippet_Constants(t *testing.T) {
	// Test that the constants are used correctly
	testutil.AssertEqual(t, ShellIntegrationSnippet, GetShellIntegrationSnippet("bash", ""))
	testutil.AssertEqual(t, ShellIntegrationSnippet, GetShellIntegrationSnippet("zsh", ""))
	testutil.AssertEqual(t, FishIntegrationSnippet, GetShellIntegrationSnippet("fish", ""))
}

// Benchmark tests
func BenchmarkGetShellIntegrationSnippet_Bash(b *testing.B) {
	for i := 0; i < b.N; i++ {
		_ = GetShellIntegrationSnippet("bash", "/test/data")
	}
}

func BenchmarkGetShellIntegrationSnippet_Fish(b *testing.B) {
	for i := 0; i < b.N; i++ {
		_ = GetShellIntegrationSnippet("fish", "/test/data")
	}
}

func BenchmarkGetShellIntegrationSnippet_AllShells(b *testing.B) {
	shells := []string{"bash", "zsh", "fish"}
	
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		for _, shell := range shells {
			_ = GetShellIntegrationSnippet(shell, "/test/data")
		}
	}
}