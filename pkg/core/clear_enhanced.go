package core

import (
	"fmt"
	"os"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
)

// ClearHandlerEnhanced orchestrates the clearing of a handler's deployments and state.
// It handles provisioning and linking handlers differently:
// - Provisioning handlers: Use DeleteProvisioningState to remove state directory
// - Linking handlers: Manually remove the state directory since DeleteProvisioningState rejects them
func ClearHandlerEnhanced(ctx types.ClearContext, handler types.Clearable, handlerName string) (*ClearResult, error) {
	logger := logging.GetLogger("core.clear").With().
		Str("pack", ctx.Pack.Name).
		Str("handler", handlerName).
		Bool("dryRun", ctx.DryRun).
		Logger()

	result := &ClearResult{
		HandlerName:  handlerName,
		ClearedItems: []types.ClearedItem{},
		StateRemoved: false,
	}

	// Step 1: Let handler perform its specific cleanup
	logger.Debug().Msg("Calling handler Clear method")
	clearedItems, err := handler.Clear(ctx)
	if err != nil {
		logger.Error().Err(err).Msg("Handler Clear failed")
		result.Error = fmt.Errorf("handler clear failed: %w", err)
		return result, result.Error
	}
	result.ClearedItems = clearedItems

	// Step 2: Remove state directory (unless dry run)
	if !ctx.DryRun {
		// Check if this is a provisioning handler
		isProvisioning := isProvisioningHandler(handlerName)

		if isProvisioning {
			// Use datastore method for provisioning handlers
			logger.Debug().Msg("Removing provisioning handler state directory")
			if err := ctx.DataStore.DeleteProvisioningState(ctx.Pack.Name, handlerName); err != nil {
				logger.Error().Err(err).Msg("Failed to remove state directory")
				result.Error = fmt.Errorf("failed to remove state directory: %w", err)
				return result, result.Error
			}
		} else {
			// For linking handlers, remove directory directly
			logger.Debug().Msg("Removing linking handler state directory")
			// Use the actual state directory name (e.g., "symlinks" for "symlink" handler)
			stateDirName := GetHandlerStateDir(handlerName)
			stateDir := ctx.Paths.PackHandlerDir(ctx.Pack.Name, stateDirName)

			// Check if directory exists
			if _, err := ctx.FS.Stat(stateDir); err != nil {
				if os.IsNotExist(err) {
					// Directory doesn't exist, nothing to do
					logger.Debug().Msg("State directory doesn't exist")
				} else {
					logger.Error().Err(err).Msg("Failed to check state directory")
					result.Error = fmt.Errorf("failed to check state directory: %w", err)
					return result, result.Error
				}
			} else {
				// Remove the directory
				if err := ctx.FS.RemoveAll(stateDir); err != nil {
					logger.Error().Err(err).Msg("Failed to remove state directory")
					result.Error = fmt.Errorf("failed to remove state directory: %w", err)
					return result, result.Error
				}
				logger.Info().Str("stateDir", stateDir).Msg("Removed linking handler state directory")
			}
		}
		result.StateRemoved = true
	} else {
		logger.Debug().Msg("Dry run - would remove handler state directory")
	}

	logger.Info().
		Int("clearedItems", len(clearedItems)).
		Bool("stateRemoved", result.StateRemoved).
		Msg("Handler cleared successfully")

	return result, nil
}

// ClearHandlersEnhanced clears multiple handlers for a pack using the enhanced method
func ClearHandlersEnhanced(ctx types.ClearContext, handlers map[string]types.Clearable) (map[string]*ClearResult, error) {
	logger := logging.GetLogger("core.clear").With().
		Str("pack", ctx.Pack.Name).
		Int("handlerCount", len(handlers)).
		Logger()

	results := make(map[string]*ClearResult)
	var firstError error

	for handlerName, handler := range handlers {
		result, err := ClearHandlerEnhanced(ctx, handler, handlerName)
		results[handlerName] = result

		if err != nil && firstError == nil {
			firstError = err
		}
	}

	if firstError != nil {
		logger.Error().
			Err(firstError).
			Msg("One or more handlers failed to clear")
	} else {
		logger.Info().
			Int("handlersCleared", len(results)).
			Msg("All handlers cleared successfully")
	}

	return results, firstError
}

// isProvisioningHandler returns true if the handler is a provisioning handler
func isProvisioningHandler(handlerName string) bool {
	switch handlerName {
	case "install", "homebrew":
		return true
	default:
		return false
	}
}
