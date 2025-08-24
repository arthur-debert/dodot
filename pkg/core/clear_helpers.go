package core

import (
	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/handlers/homebrew"
	"github.com/arthur-debert/dodot/pkg/handlers/path"
	"github.com/arthur-debert/dodot/pkg/handlers/provision"
	"github.com/arthur-debert/dodot/pkg/handlers/shell_profile"
	"github.com/arthur-debert/dodot/pkg/handlers/symlink"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
)

// GetClearableHandlersByMode returns handlers that implement Clearable, grouped by run mode
func GetClearableHandlersByMode(mode types.RunMode) (map[string]types.Clearable, error) {
	logger := logging.GetLogger("core.clear")
	result := make(map[string]types.Clearable)

	// List of all handler names
	handlerNames := []string{
		symlink.SymlinkHandlerName,
		path.PathHandlerName,
		shell_profile.ShellProfileHandlerName,
		homebrew.HomebrewHandlerName,
		provision.ProvisionScriptHandlerName,
	}

	for _, name := range handlerNames {
		handler := handlers.GetHandler(name)
		if handler == nil {
			logger.Warn().
				Str("handler", name).
				Msg("Failed to get handler")
			continue
		}

		// Check if handler matches the requested mode
		var handlerMode types.RunMode
		switch h := handler.(type) {
		case types.LinkingHandler:
			handlerMode = h.RunMode()
		case types.ProvisioningHandler:
			handlerMode = h.RunMode()
		default:
			continue
		}

		if handlerMode != mode {
			continue
		}

		// Check if handler implements Clearable
		if clearable, ok := handler.(types.Clearable); ok {
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
func GetAllClearableHandlers() (map[string]types.Clearable, error) {
	logger := logging.GetLogger("core.clear")
	handlers := make(map[string]types.Clearable)

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

// getHandlerStateDir returns the actual state directory name for a handler
// Some handlers use different directory names than their handler names
func getHandlerStateDir(handlerName string) string {
	switch handlerName {
	case "symlink":
		return "symlinks" // Historical: symlink handler uses "symlinks" directory
	default:
		return handlerName
	}
}

// FilterHandlersByState returns only handlers that have state for the given pack
func FilterHandlersByState(ctx types.ClearContext, handlers map[string]types.Clearable) map[string]types.Clearable {
	logger := logging.GetLogger("core.clear").With().
		Str("pack", ctx.Pack.Name).
		Logger()

	filtered := make(map[string]types.Clearable)

	for name, handler := range handlers {
		// Check if handler has any state
		// Note: Some handlers use different directory names than their handler names
		stateDirName := getHandlerStateDir(name)
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
		Int("totalHandlers", len(handlers)).
		Int("withState", len(filtered)).
		Msg("Filtered handlers by state")

	return filtered
}
