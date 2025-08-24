package core

import (
	"fmt"

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
// It first calls the handler's Clear method to perform handler-specific cleanup,
// then removes the handler's state directory.
func ClearHandler(ctx types.ClearContext, handler types.Clearable, handlerName string) (*ClearResult, error) {
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
		logger.Debug().Msg("Removing handler state directory")
		if err := ctx.DataStore.DeleteProvisioningState(ctx.Pack.Name, handlerName); err != nil {
			logger.Error().Err(err).Msg("Failed to remove state directory")
			result.Error = fmt.Errorf("failed to remove state directory: %w", err)
			return result, result.Error
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
func ClearHandlers(ctx types.ClearContext, handlers map[string]types.Clearable) (map[string]*ClearResult, error) {
	logger := logging.GetLogger("core.clear").With().
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
