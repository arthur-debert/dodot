package registry

import (
	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/handlers/homebrew"
	"github.com/arthur-debert/dodot/pkg/handlers/install"
	"github.com/arthur-debert/dodot/pkg/handlers/path"
	"github.com/arthur-debert/dodot/pkg/handlers/shell"
	"github.com/arthur-debert/dodot/pkg/handlers/symlink"
)

// GetHandler returns a handler instance by name
// Returns nil if the handler is not found
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

// GetLinkingHandler returns a linking handler instance by name
// Returns nil if the handler is not found or is not a linking handler
func GetLinkingHandler(name string) handlers.LinkingHandler {
	handler := GetHandler(name)
	if handler == nil {
		return nil
	}
	linkingHandler, _ := handler.(handlers.LinkingHandler)
	return linkingHandler
}

// GetProvisioningHandler returns a provisioning handler instance by name
// Returns nil if the handler is not found or is not a provisioning handler
func GetProvisioningHandler(name string) handlers.ProvisioningHandler {
	handler := GetHandler(name)
	if handler == nil {
		return nil
	}
	provisioningHandler, _ := handler.(handlers.ProvisioningHandler)
	return provisioningHandler
}

// GetClearableHandler returns a clearable handler instance by name
// Returns nil if the handler is not found or is not clearable
func GetClearableHandler(name string) handlers.Clearable {
	handler := GetHandler(name)
	if handler == nil {
		return nil
	}
	clearable, _ := handler.(handlers.Clearable)
	return clearable
}
