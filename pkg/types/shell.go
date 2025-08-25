package types

import (
	"fmt"
)

// TODO: These should be moved to config once we resolve the import cycle
const (
	bashZshSnippet           = `[ -f "$HOME/.local/share/dodot/shell/dodot-init.sh" ] && source "$HOME/.local/share/dodot/shell/dodot-init.sh"`
	bashZshSnippetWithCustom = `[ -f "%s/shell/dodot-init.sh" ] && source "%s/shell/dodot-init.sh"`
	fishSnippet              = `if test -f "$HOME/.local/share/dodot/shell/dodot-init.fish"
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
		return fishSnippet
	default:
		// bash/zsh
		if dataDir != "" {
			return fmt.Sprintf(bashZshSnippetWithCustom, dataDir, dataDir)
		}
		return bashZshSnippet
	}
}
