package types

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
func GetShellIntegrationSnippet(shell string, customDataDir string) string {
	switch shell {
	case "fish":
		if customDataDir != "" {
			return `if test -f "` + customDataDir + `/shell/dodot-init.fish"
    source "` + customDataDir + `/shell/dodot-init.fish"
end`
		}
		return FishIntegrationSnippet
	default:
		if customDataDir != "" {
			return `[ -f "` + customDataDir + `/shell/dodot-init.sh" ] && source "` + customDataDir + `/shell/dodot-init.sh"`
		}
		return ShellIntegrationSnippet
	}
}