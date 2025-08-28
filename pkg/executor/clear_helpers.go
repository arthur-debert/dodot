package executor

import (
	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/rules"
	"github.com/arthur-debert/dodot/pkg/types"
)

// GetClearableHandlersByMode returns handlers that implement Clearable, grouped by run mode.
// This is used by commands to get only the handlers they need to clear.
func GetClearableHandlersByMode(mode types.RunMode) (map[string]handlers.Clearable, error) {
	logger := logging.GetLogger("executor.clear")
	result := make(map[string]handlers.Clearable)

	// Get handler names from known set
	handlerNames := []string{"symlink", "shell", "path", "homebrew", "install"}

	for _, name := range handlerNames {
		handler, err := rules.CreateHandler(name, nil)
		if err != nil {
			logger.Warn().
				Str("handler", name).
				Err(err).
				Msg("Failed to get handler factory")
			continue
		}
		if err != nil {
			logger.Warn().
				Str("handler", name).
				Err(err).
				Msg("Failed to create handler instance")
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
	logger := logging.GetLogger("executor.clear")
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

// FilterHandlersByState returns only handlers that have state for the given pack.
// This allows commands to skip handlers that have nothing to clear.
func FilterHandlersByState(ctx types.ClearContext, handlersMap map[string]handlers.Clearable) map[string]handlers.Clearable {
	logger := logging.GetLogger("executor.clear").With().
		Str("pack", ctx.Pack.Name).
		Logger()

	filtered := make(map[string]handlers.Clearable)

	for name, handler := range handlersMap {
		// The handler knows its own state directory structure
		// We check if any state exists for this handler/pack combination

		// For now, we check the standard locations. In the future,
		// handlers could expose a method to check for state existence.
		var stateDirName string

		// Historical convention: symlink handler uses "symlinks" directory
		if name == "symlink" {
			stateDirName = "symlinks"
		} else {
			stateDirName = name
		}

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

// GetHandlerStateDir returns the state directory name for a handler.
// This is exported for backward compatibility with commands that need
// to know the state directory structure.
//
// TODO: This should eventually be moved to the handler interface so
// each handler can declare its own state directory name.
func GetHandlerStateDir(handlerName string) string {
	switch handlerName {
	case "symlink":
		return "symlinks" // Historical: symlink handler uses "symlinks" directory
	default:
		return handlerName
	}
}
