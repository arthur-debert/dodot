package handlers

import (
	"github.com/arthur-debert/dodot/pkg/handlers/homebrew"
	"github.com/arthur-debert/dodot/pkg/handlers/install"
	"github.com/arthur-debert/dodot/pkg/handlers/path"
	"github.com/arthur-debert/dodot/pkg/handlers/shell"
	"github.com/arthur-debert/dodot/pkg/handlers/symlink"
)

// GetHandler returns a handler instance by name
func GetHandler(name string) interface{} {
	switch name {
	case symlink.SymlinkHandlerName:
		return symlink.NewSymlinkHandler()
	case homebrew.HomebrewHandlerName:
		return homebrew.NewHomebrewHandler()
	case install.InstallHandlerName:
		return install.NewInstallHandler()
	case path.PathHandlerName:
		return path.NewPathHandler()
	case shell.ShellHandlerName:
		return shell.NewShellHandler()
	default:
		return nil
	}
}
