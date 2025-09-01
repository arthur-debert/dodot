package off

import (
	"fmt"
	"time"

	"github.com/arthur-debert/dodot/pkg/commands/status"
	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/logging"
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

// OffPacks turns off the specified packs by removing all handler state.
// This completely removes the pack deployment (both symlinks and provisioned resources).
//
// The command uses core.Execute with unlink and deprovision commands to ensure consistent behavior.
func OffPacks(opts OffPacksOptions) (*types.PackCommandResult, error) {
	logger := logging.GetLogger("commands.off")
	logger.Debug().
		Str("dotfilesRoot", opts.DotfilesRoot).
		Strs("packNames", opts.PackNames).
		Bool("dryRun", opts.DryRun).
		Msg("Starting off command")

	// Track execution details for metadata
	var totalCleared int
	var errors []error

	// Step 1: Unlink configuration handlers (symlink, shell, path)
	unlinkResult, err := core.Execute(core.CommandUnlink, core.ExecuteOptions{
		DotfilesRoot: opts.DotfilesRoot,
		PackNames:    opts.PackNames,
		DryRun:       opts.DryRun,
		Force:        false,
		FileSystem:   filesystem.NewOS(),
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
		FileSystem:   filesystem.NewOS(),
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
		Msg("Off command completed")

	// Get current pack status
	statusOpts := status.StatusPacksOptions{
		DotfilesRoot: opts.DotfilesRoot,
		PackNames:    opts.PackNames,
		FileSystem:   filesystem.NewOS(),
	}
	packStatus, err := status.StatusPacks(statusOpts)
	if err != nil {
		logger.Error().Err(err).Msg("Failed to get pack status")
		errors = append(errors, fmt.Errorf("failed to get pack status: %w", err))
	}

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
