package handlers

import (
	"github.com/arthur-debert/dodot/pkg/handlers/homebrew"
	"github.com/arthur-debert/dodot/pkg/handlers/path"
	"github.com/arthur-debert/dodot/pkg/handlers/provision"
	"github.com/arthur-debert/dodot/pkg/handlers/shell_profile"
	"github.com/arthur-debert/dodot/pkg/handlers/symlink"
)

// GetV2Handler returns a V2 handler instance by name
func GetV2Handler(name string) interface{} {
	switch name {
	case symlink.SymlinkHandlerName:
		return symlink.NewSymlinkHandler()
	case homebrew.HomebrewHandlerName:
		return homebrew.NewHomebrewHandlerV2()
	case provision.ProvisionScriptHandlerName:
		return provision.NewProvisionScriptHandlerV2()
	case path.PathHandlerName:
		return path.NewPathHandler()
	case shell_profile.ShellProfileHandlerName:
		return shell_profile.NewShellProfileHandler()
	default:
		return nil
	}
}
