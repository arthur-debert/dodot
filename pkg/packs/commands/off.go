package commands

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/packs/execution"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
)

// OffCommand implements the "off" command using the pack execution.
type OffCommand struct{}

// Name returns the command name.
func (c *OffCommand) Name() string {
	return "off"
}

// ExecuteForPack executes the "off" command for a single pack.
func (c *OffCommand) ExecuteForPack(pack types.Pack, opts execution.Options) (*execution.PackResult, error) {
	logger := logging.GetLogger("execution.off")
	logger.Debug().
		Str("pack", pack.Name).
		Bool("dryRun", opts.DryRun).
		Msg("Executing off command for pack")

	// Initialize filesystem
	fs := opts.FileSystem
	if fs == nil {
		fs = filesystem.NewOS()
	}

	// Initialize paths and datastore
	pathsInstance, err := paths.New(opts.DotfilesRoot)
	if err != nil {
		return &execution.PackResult{
			Pack:    pack,
			Success: false,
			Error:   err,
		}, err
	}
	dataStore := datastore.New(fs, pathsInstance)

	// Track execution details
	var totalCleared int
	var errors []error

	// Get all handler names (both configuration and code execution)
	allHandlers := append(
		handlers.HandlerRegistry.GetConfigurationHandlers(),
		handlers.HandlerRegistry.GetCodeExecutionHandlers()...,
	)

	// Remove state for each handler
	for _, handlerName := range allHandlers {
		// Check if handler has state
		hasState, err := dataStore.HasHandlerState(pack.Name, handlerName)
		if err != nil {
			logger.Warn().Err(err).
				Str("pack", pack.Name).
				Str("handler", handlerName).
				Msg("Failed to check handler state")
			continue
		}

		if !hasState {
			continue
		}

		if opts.DryRun {
			logger.Info().
				Str("pack", pack.Name).
				Str("handler", handlerName).
				Msg("Would remove handler state (dry run)")
			totalCleared++
		} else {
			// Remove the handler state
			if err := dataStore.RemoveState(pack.Name, handlerName); err != nil {
				logger.Error().Err(err).
					Str("pack", pack.Name).
					Str("handler", handlerName).
					Msg("Failed to remove handler state")
				errors = append(errors, fmt.Errorf("failed to remove %s state: %w", handlerName, err))
			} else {
				logger.Info().
					Str("pack", pack.Name).
					Str("handler", handlerName).
					Msg("Removed handler state")
				totalCleared++
			}
		}
	}

	// Get pack status for result
	var packStatus *StatusResult
	if len(errors) == 0 || len(errors) <= 2 { // Allow some errors but not total failure
		statusOpts := StatusOptions{
			Pack:       pack,
			DataStore:  dataStore,
			FileSystem: fs,
			Paths:      pathsInstance,
		}
		packStatus, err = GetStatus(statusOpts)
		if err != nil {
			logger.Warn().Err(err).Str("pack", pack.Name).Msg("Failed to get pack status")
		}
	}

	// Determine overall success
	success := len(errors) == 0

	logger.Info().
		Str("pack", pack.Name).
		Int("totalCleared", totalCleared).
		Int("errors", len(errors)).
		Bool("success", success).
		Msg("Off command completed for pack")

	// Aggregate errors if any
	var finalError error
	if len(errors) > 0 {
		finalError = fmt.Errorf("encountered %d errors while turning off pack", len(errors))
	}

	return &execution.PackResult{
		Pack:                  pack,
		Success:               success,
		Error:                 finalError,
		CommandSpecificResult: packStatus,
	}, nil
}
