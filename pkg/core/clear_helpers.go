package core

import (
	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/handlers/homebrew"
	"github.com/arthur-debert/dodot/pkg/handlers/install"
	"github.com/arthur-debert/dodot/pkg/handlers/path"
	"github.com/arthur-debert/dodot/pkg/handlers/registry"
	"github.com/arthur-debert/dodot/pkg/handlers/shell"
	"github.com/arthur-debert/dodot/pkg/handlers/symlink"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
)

// GetClearableHandlersByMode returns handlers that implement Clearable, grouped by run mode
func GetClearableHandlersByMode(mode types.RunMode) (map[string]handlers.Clearable, error) {
	logger := logging.GetLogger("core.clear")
	result := make(map[string]handlers.Clearable)

	// List of all handler names
	handlerNames := []string{
		symlink.SymlinkHandlerName,
		path.PathHandlerName,
		shell.ShellHandlerName,
		homebrew.HomebrewHandlerName,
		install.InstallHandlerName,
	}

	for _, name := range handlerNames {
		handler := registry.GetHandler(name)
		if handler == nil {
			logger.Warn().
				Str("handler", name).
				Msg("Failed to get handler")
			continue
		}

		// Check if handler matches the requested mode
		var handlerMode types.RunMode
		switch h := handler.(type) {
		case handlers.LinkingHandler:
			handlerMode = h.RunMode()
		case handlers.ProvisioningHandler:
			handlerMode = h.RunMode()
		default:
			continue
		}

		if handlerMode != mode {
			continue
		}

		// Check if handler implements Clearable
		if clearable, ok := handler.(handlers.Clearable); ok {
			result[name] = clearable
		} else {
			logger.Debug().
				Str("handler", name).
				Msg("Handler does not implement Clearable")
		}
	}

	logger.Debug().
		Str("mode", string(mode)).
		Int("clearableCount", len(result)).
		Msg("Found clearable handlers")

	return result, nil
}

// GetAllClearableHandlers returns all handlers that implement Clearable
func GetAllClearableHandlers() (map[string]handlers.Clearable, error) {
	logger := logging.GetLogger("core.clear")
	handlers := make(map[string]handlers.Clearable)

	// Get linking handlers
	linkingHandlers, err := GetClearableHandlersByMode(types.RunModeLinking)
	if err != nil {
		return nil, err
	}
	for name, handler := range linkingHandlers {
		handlers[name] = handler
	}

	// Get provisioning handlers
	provisioningHandlers, err := GetClearableHandlersByMode(types.RunModeProvisioning)
	if err != nil {
		return nil, err
	}
	for name, handler := range provisioningHandlers {
		handlers[name] = handler
	}

	logger.Debug().
		Int("totalClearable", len(handlers)).
		Msg("Found all clearable handlers")

	return handlers, nil
}

// GetHandlerStateDir returns the actual state directory name for a handler
// Some handlers use different directory names than their handler names
func GetHandlerStateDir(handlerName string) string {
	switch handlerName {
	case "symlink":
		return "symlinks" // Historical: symlink handler uses "symlinks" directory
	default:
		return handlerName
	}
}

// FilterHandlersByState returns only handlers that have state for the given pack
func FilterHandlersByState(ctx types.ClearContext, handlersMap map[string]handlers.Clearable) map[string]handlers.Clearable {
	logger := logging.GetLogger("core.clear").With().
		Str("pack", ctx.Pack.Name).
		Logger()

	filtered := make(map[string]handlers.Clearable)

	for name, handler := range handlersMap {
		// Check if handler has any state
		// Note: Some handlers use different directory names than their handler names
		stateDirName := GetHandlerStateDir(name)
		handlerDir := ctx.Paths.PackHandlerDir(ctx.Pack.Name, stateDirName)
		if _, err := ctx.FS.Stat(handlerDir); err == nil {
			filtered[name] = handler
			logger.Debug().
				Str("handler", name).
				Str("stateDir", stateDirName).
				Msg("Handler has state")
		}
	}

	logger.Debug().
		Int("totalHandlers", len(handlersMap)).
		Int("withState", len(filtered)).
		Msg("Filtered handlers by state")

	return filtered
}
