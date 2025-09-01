package off

import (
	"fmt"
	"time"

	"github.com/arthur-debert/dodot/pkg/commands/status"
	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
)

// OffPacksOptions defines the options for the OffPacks command
type OffPacksOptions struct {
	// DotfilesRoot is the path to the root of the dotfiles directory
	DotfilesRoot string
	// PackNames is a list of specific packs to turn off. If empty, all packs are turned off
	PackNames []string
	// DryRun specifies whether to perform a dry run without making changes
	DryRun bool
}

// PackRemovalResult tracks the removal results for a single pack
type PackRemovalResult struct {
	Name         string   `json:"name"`
	HandlersRun  []string `json:"handlersRun"`
	RemovedItems []string `json:"removedItems"`
	Errors       []error  `json:"errors"`
}

// OffPacks turns off the specified packs by removing all handler state.
// This completely removes the pack deployment (both symlinks and provisioned resources).
//
// The command removes all handler state from the data directory for each pack.
func OffPacks(opts OffPacksOptions) (*types.PackCommandResult, error) {
	logger := logging.GetLogger("commands.off")
	logger.Debug().
		Str("dotfilesRoot", opts.DotfilesRoot).
		Strs("packNames", opts.PackNames).
		Bool("dryRun", opts.DryRun).
		Msg("Starting off command")

	// Track execution details
	var totalCleared int
	var errors []error
	var handlersRun []string
	allHandlersMap := make(map[string]bool)

	// Initialize paths
	pathsInstance, err := paths.New(opts.DotfilesRoot)
	if err != nil {
		return nil, fmt.Errorf("failed to initialize paths: %w", err)
	}

	// Create filesystem and datastore
	fs := filesystem.NewOS()
	store := datastore.New(fs, pathsInstance)

	// Discover and select packs
	selectedPacks, err := core.DiscoverAndSelectPacksFS(pathsInstance.DotfilesRoot(), opts.PackNames, fs)
	if err != nil {
		logger.Error().Err(err).Msg("Failed to discover packs")
		return nil, err
	}

	// Process each pack
	for _, pack := range selectedPacks {
		packResult := PackRemovalResult{
			Name: pack.Name,
		}

		// Get all handler types
		allHandlers := append(
			handlers.HandlerRegistry.GetConfigurationHandlers(),
			handlers.HandlerRegistry.GetCodeExecutionHandlers()...,
		)

		// Remove state for each handler type
		for _, handlerName := range allHandlers {
			// Check if handler has state
			hasState, err := store.HasHandlerState(pack.Name, handlerName)
			if err != nil {
				logger.Warn().Err(err).
					Str("pack", pack.Name).
					Str("handler", handlerName).
					Msg("Failed to check handler state")
				continue
			}

			if hasState {
				if opts.DryRun {
					logger.Info().
						Str("pack", pack.Name).
						Str("handler", handlerName).
						Msg("[DRY RUN] Would remove handler state")
					packResult.HandlersRun = append(packResult.HandlersRun, handlerName)
					packResult.RemovedItems = append(packResult.RemovedItems, fmt.Sprintf("%s handler state", handlerName))
					allHandlersMap[handlerName] = true
				} else {
					logger.Debug().
						Str("pack", pack.Name).
						Str("handler", handlerName).
						Msg("Removing handler state")

					if err := store.RemoveState(pack.Name, handlerName); err != nil {
						packResult.Errors = append(packResult.Errors, fmt.Errorf("failed to remove %s handler state: %w", handlerName, err))
						errors = append(errors, fmt.Errorf("pack %s: failed to remove %s handler state: %w", pack.Name, handlerName, err))
					} else {
						packResult.HandlersRun = append(packResult.HandlersRun, handlerName)
						packResult.RemovedItems = append(packResult.RemovedItems, fmt.Sprintf("%s handler state", handlerName))
						totalCleared++
						allHandlersMap[handlerName] = true
					}
				}
			}
		}

		// Track pack-level errors
		if len(packResult.Errors) > 0 {
			for _, err := range packResult.Errors {
				errors = append(errors, err)
			}
		}
	}

	logger.Info().
		Int("totalCleared", totalCleared).
		Int("errors", len(errors)).
		Bool("dryRun", opts.DryRun).
		Msg("Off command completed")

	// Convert map to slice for handlers run
	for handler := range allHandlersMap {
		handlersRun = append(handlersRun, handler)
	}

	// Get current pack status
	statusOpts := status.StatusPacksOptions{
		DotfilesRoot: opts.DotfilesRoot,
		PackNames:    opts.PackNames,
		FileSystem:   fs,
	}
	packStatus, err := status.StatusPacks(statusOpts)
	if err != nil {
		logger.Error().Err(err).Msg("Failed to get pack status")
		errors = append(errors, fmt.Errorf("failed to get pack status: %w", err))
	}

	// Build result
	result := &types.PackCommandResult{
		Command:   "off",
		Timestamp: time.Now(),
		DryRun:    opts.DryRun,
		Packs:     []types.DisplayPack{},
		Metadata: types.CommandMetadata{
			TotalCleared: totalCleared,
			HandlersRun:  handlersRun,
		},
	}

	// Copy packs from status if available
	if packStatus != nil {
		result.Packs = packStatus.Packs
	}

	// Generate message
	packNames := make([]string, 0, len(result.Packs))
	for _, pack := range result.Packs {
		packNames = append(packNames, pack.Name)
	}
	result.Message = types.FormatCommandMessage("turned off", packNames)

	if len(errors) > 0 {
		return result, fmt.Errorf("off command encountered %d errors", len(errors))
	}

	return result, nil
}