package pack

import (
	"fmt"
	"time"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/filesystem"
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
// The function uses core.Execute with unlink and deprovision commands to ensure consistent behavior.
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

	// Track execution details for metadata
	var totalCleared int
	var errors []error

	// Step 1: Unlink configuration handlers (symlink, shell, path)
	unlinkResult, err := core.Execute(core.CommandUnlink, core.ExecuteOptions{
		DotfilesRoot: opts.DotfilesRoot,
		PackNames:    opts.PackNames,
		DryRun:       opts.DryRun,
		Force:        false,
		FileSystem:   fs,
	})
	if err != nil {
		logger.Error().Err(err).Msg("Failed to unlink packs")
		errors = append(errors, fmt.Errorf("unlink failed: %w", err))
	} else {
		totalCleared += unlinkResult.CompletedHandlers
		// Check for errors in pack results
		for packName, packResult := range unlinkResult.PackResults {
			if packResult.FailedHandlers > 0 {
				errors = append(errors, fmt.Errorf("pack %s had %d failed handlers during unlink", packName, packResult.FailedHandlers))
			}
		}
	}

	// Step 2: Deprovision code execution handlers (install, homebrew)
	deprovisionResult, err := core.Execute(core.CommandDeprovision, core.ExecuteOptions{
		DotfilesRoot: opts.DotfilesRoot,
		PackNames:    opts.PackNames,
		DryRun:       opts.DryRun,
		Force:        false,
		FileSystem:   fs,
	})
	if err != nil {
		logger.Error().Err(err).Msg("Failed to deprovision packs")
		errors = append(errors, fmt.Errorf("deprovision failed: %w", err))
	} else {
		totalCleared += deprovisionResult.CompletedHandlers
		// Check for errors in pack results
		for packName, packResult := range deprovisionResult.PackResults {
			if packResult.FailedHandlers > 0 {
				errors = append(errors, fmt.Errorf("pack %s had %d failed handlers during deprovision", packName, packResult.FailedHandlers))
			}
		}
	}

	logger.Info().
		Int("totalCleared", totalCleared).
		Int("errors", len(errors)).
		Bool("dryRun", opts.DryRun).
		Msg("Turn off operation completed")

	// Collect handler names that were processed
	handlersRun := make([]string, 0)
	handlerMap := make(map[string]bool)

	// Add handlers from unlink results
	if unlinkResult != nil {
		for _, packResult := range unlinkResult.PackResults {
			for _, handlerResult := range packResult.HandlerResults {
				handlerMap[handlerResult.HandlerName] = true
			}
		}
	}

	// Add handlers from deprovision results
	if deprovisionResult != nil {
		for _, packResult := range deprovisionResult.PackResults {
			for _, handlerResult := range packResult.HandlerResults {
				handlerMap[handlerResult.HandlerName] = true
			}
		}
	}

	// Convert map to slice
	for handler := range handlerMap {
		handlersRun = append(handlersRun, handler)
	}

	// Get current pack status using pkg/pack/status.go functionality
	statusPacks := make([]types.DisplayPack, 0)
	if len(errors) == 0 || len(errors) <= 2 { // Allow some errors but not total failure
		pathsInstance, pathErr := paths.New(opts.DotfilesRoot)
		if pathErr != nil {
			logger.Warn().Err(pathErr).Msg("Failed to create paths for status check")
			// Don't add this to errors since status is supplementary
		} else {
			// Use core pack discovery - this will return empty list if no packs found
			selectedPacks, discErr := core.DiscoverAndSelectPacksFS(opts.DotfilesRoot, opts.PackNames, fs)
			if discErr != nil {
				logger.Warn().Err(discErr).Msg("Failed to discover packs for status")
				// Don't add this to errors since status is supplementary
			} else {
				// Create datastore for status checking
				dataStore := datastore.New(fs, pathsInstance)

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
