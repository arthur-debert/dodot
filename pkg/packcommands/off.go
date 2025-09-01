package packcommands

import (
	"fmt"
	"time"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
)

// OffOptions defines the options for turning off packs
type OffOptions struct {
	// DotfilesRoot is the path to the root of the dotfiles directory
	DotfilesRoot string
	// PackNames is a list of specific packs to turn off. If empty, all packs are turned off
	PackNames []string
	// DryRun specifies whether to perform a dry run without making changes
	DryRun bool
	// FileSystem to use (optional, defaults to OS filesystem)
	FileSystem types.FS
}

// TurnOff turns off the specified packs by removing all handler state.
// This completely removes the pack deployment (both symlinks and provisioned resources).
//
// The function now directly manages pack state removal for cleaner separation.
func TurnOff(opts OffOptions) (*types.PackCommandResult, error) {
	logger := logging.GetLogger("pack.off")
	logger.Debug().
		Str("dotfilesRoot", opts.DotfilesRoot).
		Strs("packNames", opts.PackNames).
		Bool("dryRun", opts.DryRun).
		Msg("Starting turn off operation")

	// Use provided filesystem or default to OS
	fs := opts.FileSystem
	if fs == nil {
		fs = filesystem.NewOS()
	}

	// Initialize paths and datastore
	pathsInstance, err := paths.New(opts.DotfilesRoot)
	if err != nil {
		return nil, fmt.Errorf("failed to initialize paths: %w", err)
	}
	dataStore := datastore.New(fs, pathsInstance)

	// Track execution details
	var totalCleared int
	var errors []error
	handlersCleared := make(map[string]bool)

	// Step 1: Discover packs
	selectedPacks, err := core.DiscoverAndSelectPacksFS(opts.DotfilesRoot, opts.PackNames, fs)
	if err != nil {
		logger.Error().Err(err).Msg("Failed to discover packs")
		return nil, fmt.Errorf("failed to discover packs: %w", err)
	}

	logger.Debug().
		Int("packCount", len(selectedPacks)).
		Msg("Discovered packs for processing")

	// Step 2: Process each pack
	for _, pack := range selectedPacks {
		logger.Debug().
			Str("pack", pack.Name).
			Msg("Turning off pack")

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
				handlersCleared[handlerName] = true
			} else {
				// Remove the handler state
				if err := dataStore.RemoveState(pack.Name, handlerName); err != nil {
					logger.Error().Err(err).
						Str("pack", pack.Name).
						Str("handler", handlerName).
						Msg("Failed to remove handler state")
					errors = append(errors, fmt.Errorf("failed to remove %s state for pack %s: %w", handlerName, pack.Name, err))
				} else {
					logger.Info().
						Str("pack", pack.Name).
						Str("handler", handlerName).
						Msg("Removed handler state")
					totalCleared++
					handlersCleared[handlerName] = true
				}
			}
		}
	}

	logger.Info().
		Int("totalCleared", totalCleared).
		Int("errors", len(errors)).
		Bool("dryRun", opts.DryRun).
		Msg("Turn off operation completed")

	// Convert handler map to slice
	handlersRun := make([]string, 0, len(handlersCleared))
	for handler := range handlersCleared {
		handlersRun = append(handlersRun, handler)
	}

	// Step 3: Get current pack status
	statusPacks := make([]types.DisplayPack, 0)
	if len(errors) == 0 || len(errors) <= 2 { // Allow some errors but not total failure
		// Get status for each pack using pkg/pack/status.go
		for _, p := range selectedPacks {
			statusOpts := StatusOptions{
				Pack:       p,
				DataStore:  dataStore,
				FileSystem: fs,
				Paths:      pathsInstance,
			}

			packStatus, statusErr := GetStatus(statusOpts)
			if statusErr != nil {
				logger.Warn().Err(statusErr).Str("pack", p.Name).Msg("Failed to get individual pack status")
				continue
			}

			// Convert to display format using the same logic as status command
			displayPack := types.DisplayPack{
				Name:      packStatus.Name,
				HasConfig: packStatus.HasConfig,
				IsIgnored: packStatus.IsIgnored,
				Status:    packStatus.Status,
				Files:     make([]types.DisplayFile, 0, len(packStatus.Files)),
			}

			// Convert each file status
			for _, file := range packStatus.Files {
				displayFile := types.DisplayFile{
					Handler:        file.Handler,
					Path:           file.Path,
					Status:         statusStateToDisplayStatus(file.Status.State),
					Message:        file.Status.Message,
					LastExecuted:   file.Status.Timestamp,
					HandlerSymbol:  types.GetHandlerSymbol(file.Handler),
					AdditionalInfo: file.AdditionalInfo,
				}
				displayPack.Files = append(displayPack.Files, displayFile)
			}

			// Add special files if present
			if packStatus.IsIgnored {
				displayPack.Files = append([]types.DisplayFile{{
					Path:   ".dodotignore",
					Status: "ignored",
				}}, displayPack.Files...)
			}
			if packStatus.HasConfig {
				displayPack.Files = append([]types.DisplayFile{{
					Path:   ".dodot.toml",
					Status: "config",
				}}, displayPack.Files...)
			}

			statusPacks = append(statusPacks, displayPack)
		}
	}

	// Build result
	result := &types.PackCommandResult{
		Command:   "off",
		Timestamp: time.Now(),
		DryRun:    opts.DryRun,
		Packs:     statusPacks,
		Metadata: types.CommandMetadata{
			TotalCleared: totalCleared,
			HandlersRun:  handlersRun,
		},
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
