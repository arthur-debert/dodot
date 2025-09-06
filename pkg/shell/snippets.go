package shell

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/paths"
)

// GetShellIntegrationSnippet returns the appropriate shell integration snippet
// It checks multiple locations in order of preference:
// 1. If dataDir is provided, use that (for custom installations)
// 2. Otherwise check installed locations (homebrew, system packages)
// 3. Fall back to default snippet
func GetShellIntegrationSnippet(shell string, dataDir string) string {
	// Determine script name based on shell
	var scriptName string
	switch shell {
	case "fish":
		scriptName = "dodot-init.fish"
	default:
		scriptName = "dodot-init.sh"
	}

	// If dataDir is provided, use it (this handles custom paths and tests)
	if dataDir != "" {
		switch shell {
		case "fish":
			return fmt.Sprintf(`if test -f "%s/shell/%s"
    source "%s/shell/%s"
end`, dataDir, scriptName, dataDir, scriptName)
		default:
			return fmt.Sprintf(`[ -f "%s/shell/%s" ] && source "%s/shell/%s"`, dataDir, scriptName, dataDir, scriptName)
		}
	}

	// No dataDir provided, check if the script exists in a system location
	if scriptPath, err := paths.ResolveShellScriptPath(scriptName); err == nil {
		// Script found in system location, use that path
		switch shell {
		case "fish":
			return fmt.Sprintf(`if test -f "%s"
    source "%s"
end`, scriptPath, scriptPath)
		default:
			return fmt.Sprintf(`[ -f "%s" ] && source "%s"`, scriptPath, scriptPath)
		}
	}

	// Use default snippets from config
	shellIntegration := config.GetShellIntegration()
	if shell == "fish" {
		return shellIntegration.FishSnippet
	}
	return shellIntegration.BashZshSnippet
}
