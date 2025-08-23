package handlers

import (
	"github.com/arthur-debert/dodot/pkg/handlers/homebrew"
	"github.com/arthur-debert/dodot/pkg/handlers/path"
	"github.com/arthur-debert/dodot/pkg/handlers/provision"
	"github.com/arthur-debert/dodot/pkg/handlers/shell_profile"
	"github.com/arthur-debert/dodot/pkg/handlers/symlink"
	"github.com/arthur-debert/dodot/pkg/types"
)

// GetV2Handler returns a V2 handler instance by name
func GetV2Handler(name string) types.Handler {
	switch name {
	case symlink.SymlinkHandlerName:
		return symlink.NewSymlinkHandlerV2()
	case homebrew.HomebrewHandlerName:
		return homebrew.NewHomebrewHandlerV2()
	case provision.ProvisionScriptHandlerName:
		return provision.NewProvisionScriptHandlerV2()
	case path.PathHandlerName:
		return path.NewPathHandlerV2()
	case shell_profile.ShellProfileHandlerName:
		return shell_profile.NewShellProfileHandlerV2()
	default:
		return nil
	}
}
