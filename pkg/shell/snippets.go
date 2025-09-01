package shell

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/config"
)

// GetShellIntegrationSnippet returns the appropriate shell integration snippet
// If dataDir is provided, it uses that path; otherwise it uses the default snippet
func GetShellIntegrationSnippet(shell string, dataDir string) string {
	shellIntegration := config.GetShellIntegration()

	switch shell {
	case "fish":
		if dataDir != "" {
			return fmt.Sprintf(`if test -f "%s/shell/dodot-init.fish"
    source "%s/shell/dodot-init.fish"
end`, dataDir, dataDir)
		}
		return shellIntegration.FishSnippet
	default:
		// bash/zsh
		if dataDir != "" {
			return fmt.Sprintf(shellIntegration.BashZshSnippetWithCustom, dataDir, dataDir)
		}
		return shellIntegration.BashZshSnippet
	}
}
