package off

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
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

// OffResult represents the result of turning off packs
type OffResult struct {
	Packs        []PackRemovalResult
	TotalCleared int
	DryRun       bool
	Errors       []error
}

// PackRemovalResult represents the result of removing a single pack
type PackRemovalResult struct {
	Name         string
	HandlersRun  []string
	RemovedItems []string
	Errors       []error
}

// OffPacks turns off the specified packs by removing all handler state.
// This completely removes the pack deployment (both symlinks and provisioned resources).
//
// The command removes all handler state from the data directory for each pack.
func OffPacks(opts OffPacksOptions) (*OffResult, error) {
	logger := logging.GetLogger("commands.off")
	logger.Debug().
		Str("dotfilesRoot", opts.DotfilesRoot).
		Strs("packNames", opts.PackNames).
		Bool("dryRun", opts.DryRun).
		Msg("Starting off command")

	result := &OffResult{
		DryRun: opts.DryRun,
		Packs:  []PackRemovalResult{},
	}

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
				} else {
					logger.Debug().
						Str("pack", pack.Name).
						Str("handler", handlerName).
						Msg("Removing handler state")

					if err := store.RemoveState(pack.Name, handlerName); err != nil {
						packResult.Errors = append(packResult.Errors, fmt.Errorf("failed to remove %s handler state: %w", handlerName, err))
						result.Errors = append(result.Errors, fmt.Errorf("pack %s: failed to remove %s handler state: %w", pack.Name, handlerName, err))
					} else {
						packResult.HandlersRun = append(packResult.HandlersRun, handlerName)
						packResult.RemovedItems = append(packResult.RemovedItems, fmt.Sprintf("%s handler state", handlerName))
						result.TotalCleared++
					}
				}
			}
		}

		result.Packs = append(result.Packs, packResult)
	}

	logger.Info().
		Int("totalCleared", result.TotalCleared).
		Int("errors", len(result.Errors)).
		Bool("dryRun", opts.DryRun).
		Msg("Off command completed")

	if len(result.Errors) > 0 {
		return result, fmt.Errorf("off command encountered %d errors", len(result.Errors))
	}

	return result, nil
}
