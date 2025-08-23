package handlers

import (
	"github.com/arthur-debert/dodot/pkg/handlers/homebrew"
	"github.com/arthur-debert/dodot/pkg/handlers/path"
	"github.com/arthur-debert/dodot/pkg/handlers/provision"
	"github.com/arthur-debert/dodot/pkg/handlers/shell_profile"
	"github.com/arthur-debert/dodot/pkg/handlers/symlink"
)

// GetHandler returns a handler instance by name
func GetHandler(name string) interface{} {
	switch name {
	case symlink.SymlinkHandlerName:
		return symlink.NewSymlinkHandler()
	case homebrew.HomebrewHandlerName:
		return homebrew.NewHomebrewHandler()
	case provision.ProvisionScriptHandlerName:
		return provision.NewProvisionScriptHandler()
	case path.PathHandlerName:
		return path.NewPathHandler()
	case shell_profile.ShellProfileHandlerName:
		return shell_profile.NewShellProfileHandler()
	default:
		return nil
	}
}
