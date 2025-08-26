package executor

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
)

// ClearResult represents the result of clearing a handler
type ClearResult struct {
	HandlerName  string              // Name of the handler
	ClearedItems []types.ClearedItem // Items cleared by the handler
	StateRemoved bool                // Whether state directory was removed
	Error        error               // Any error that occurred
}

// ClearHandler orchestrates the clearing of a handler's deployments and state.
// It delegates the actual cleanup work to the handler's Clear() method and
// coordinates state removal through the appropriate channels.
//
// The process is:
// 1. Handler performs its specific cleanup (removes symlinks, paths, etc.)
// 2. State directory is cleaned up based on handler type
//
// This ensures proper separation of concerns - handlers know how to clean
// their own deployments, while the executor orchestrates the process.
func ClearHandler(ctx types.ClearContext, handler handlers.Clearable, handlerName string) (*ClearResult, error) {
	logger := logging.GetLogger("executor.clear").With().
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

	// Step 2: Clean up handler state directory
	// TODO: This is a temporary solution for backward compatibility.
	// Ideally, the DataStore should provide a unified state removal method
	// that works for all handler types.
	if !ctx.DryRun {
		logger.Debug().Msg("Removing handler state directory")
		
		// For provisioning handlers, use DeleteProvisioningState
		if !isLinkingHandler(handlerName) {
			if err := ctx.DataStore.DeleteProvisioningState(ctx.Pack.Name, handlerName); err != nil {
				logger.Error().Err(err).Msg("Failed to remove state directory")
				result.Error = fmt.Errorf("failed to remove state directory: %w", err)
				return result, result.Error
			}
		} else {
			// For linking handlers, manually remove the state directory
			// after the handler has cleaned its contents
			stateDirName := GetHandlerStateDir(handlerName)
			stateDir := ctx.Paths.PackHandlerDir(ctx.Pack.Name, stateDirName)
			
			// Check if directory exists and is empty
			if _, err := ctx.FS.Stat(stateDir); err == nil {
				// Try to remove the directory
				if err := ctx.FS.RemoveAll(stateDir); err != nil {
					logger.Error().
						Err(err).
						Str("stateDir", stateDir).
						Msg("Failed to remove state directory")
					result.Error = fmt.Errorf("failed to remove state directory: %w", err)
					return result, result.Error
				}
				logger.Debug().
					Str("stateDir", stateDir).
					Msg("Removed linking handler state directory")
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

// ClearHandlers clears multiple handlers for a pack
func ClearHandlers(ctx types.ClearContext, handlers map[string]handlers.Clearable) (map[string]*ClearResult, error) {
	logger := logging.GetLogger("executor.clear").With().
		Str("pack", ctx.Pack.Name).
		Int("handlerCount", len(handlers)).
		Logger()

	results := make(map[string]*ClearResult)
	var firstError error

	for handlerName, handler := range handlers {
		result, err := ClearHandler(ctx, handler, handlerName)
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

// isLinkingHandler checks if a handler is a linking handler based on its name.
// This is a temporary solution until we have a better way to determine handler types.
// TODO: Replace this with a proper handler type check through the registry.
func isLinkingHandler(handlerName string) bool {
	switch handlerName {
	case "symlink", "path", "shell":
		return true
	default:
		return false
	}
}