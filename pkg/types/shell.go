package types

import (
	"fmt"
)

// Shell integration constants
const (
	// ShellIntegrationSnippet is the line users need to add to their shell config
	ShellIntegrationSnippet = `[ -f "$HOME/.local/share/dodot/shell/dodot-init.sh" ] && source "$HOME/.local/share/dodot/shell/dodot-init.sh"`

	// ShellIntegrationSnippetWithCustomDir is the snippet template for custom DODOT_DATA_DIR
	ShellIntegrationSnippetWithCustomDir = `[ -f "%s/shell/dodot-init.sh" ] && source "%s/shell/dodot-init.sh"`

	// FishIntegrationSnippet is the line for Fish shell users
	FishIntegrationSnippet = `if test -f "$HOME/.local/share/dodot/shell/dodot-init.fish"
    source "$HOME/.local/share/dodot/shell/dodot-init.fish"
end`
)

// GetShellIntegrationSnippet returns the appropriate shell integration snippet
// If dataDir is provided, it uses that path; otherwise it uses the default snippet
func GetShellIntegrationSnippet(shell string, dataDir string) string {
	switch shell {
	case "fish":
		if dataDir != "" {
			return fmt.Sprintf(`if test -f "%s/shell/dodot-init.fish"
    source "%s/shell/dodot-init.fish"
end`, dataDir, dataDir)
		}
		return FishIntegrationSnippet
	default:
		// bash/zsh
		if dataDir != "" {
			return fmt.Sprintf(`[ -f "%s/shell/dodot-init.sh" ] && source "%s/shell/dodot-init.sh"`, dataDir, dataDir)
		}
		return ShellIntegrationSnippet
	}
}
