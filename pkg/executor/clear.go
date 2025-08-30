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

	// Step 2: Clean up handler state using the unified RemoveState method
	if !ctx.DryRun {
		logger.Debug().Msg("Removing handler state")

		if err := ctx.DataStore.RemoveState(ctx.Pack.Name, handlerName); err != nil {
			logger.Error().Err(err).Msg("Failed to remove handler state")
			result.Error = fmt.Errorf("failed to remove handler state: %w", err)
			return result, result.Error
		}

		result.StateRemoved = true
		logger.Debug().Msg("Handler state removed successfully")
	} else {
		logger.Debug().Msg("Dry run - would remove handler state")
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
