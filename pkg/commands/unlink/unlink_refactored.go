package unlink

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
)

// getHandlerStateDir returns the actual state directory name for a handler
// Some handlers use different directory names than their handler names
func getHandlerStateDir(handlerName string) string {
	switch handlerName {
	case "symlink":
		return "symlinks" // Historical: symlink handler uses "symlinks" directory
	default:
		return handlerName
	}
}

// UnlinkPacksV2Options contains options for the unlink command using clearable
type UnlinkPacksV2Options struct {
	// DotfilesRoot is the path to the dotfiles directory
	DotfilesRoot string

	// PackNames is the list of pack names to unlink (empty = all)
	PackNames []string

	// Force skips confirmation prompts (unused in v2, kept for compatibility)
	Force bool

	// DryRun shows what would be removed without actually removing
	DryRun bool
}

// UnlinkPacksV2 removes linking deployments using the clearable infrastructure
func UnlinkPacksV2(opts UnlinkPacksV2Options) (*UnlinkResult, error) {
	logger := logging.GetLogger("commands.unlink.v2")
	logger.Debug().
		Str("dotfilesRoot", opts.DotfilesRoot).
		Strs("packNames", opts.PackNames).
		Bool("dryRun", opts.DryRun).
		Msg("Starting unlink v2 command")

	// Initialize paths
	p, err := paths.New(opts.DotfilesRoot)
	if err != nil {
		return nil, fmt.Errorf("failed to initialize paths: %w", err)
	}

	// Initialize filesystem
	fs := filesystem.NewOS()

	// Initialize datastore
	ds := datastore.New(fs, p)

	// Discover and select packs
	packs, err := core.DiscoverAndSelectPacks(opts.DotfilesRoot, opts.PackNames)
	if err != nil {
		return nil, fmt.Errorf("failed to discover packs: %w", err)
	}

	// Get linking handlers only
	linkingHandlers, err := core.GetClearableHandlersByMode(types.RunModeLinking)
	if err != nil {
		return nil, fmt.Errorf("failed to get linking handlers: %w", err)
	}

	logger.Debug().
		Int("packCount", len(packs)).
		Int("handlerCount", len(linkingHandlers)).
		Msg("Discovered packs and handlers")

	// Process each pack
	result := &UnlinkResult{
		DryRun: opts.DryRun,
	}

	for _, pack := range packs {
		packResult := PackUnlinkResult{
			Name: pack.Name,
		}

		// Create clear context for this pack
		ctx := types.ClearContext{
			Pack:      pack,
			DataStore: ds,
			FS:        fs,
			Paths:     p,
			DryRun:    opts.DryRun,
		}

		// Filter handlers to only those with state
		handlersWithState := core.FilterHandlersByState(ctx, linkingHandlers)

		logger.Debug().
			Str("pack", pack.Name).
			Int("handlersWithState", len(handlersWithState)).
			Msg("Filtered handlers by state")

		if len(handlersWithState) == 0 {
			logger.Debug().
				Str("pack", pack.Name).
				Msg("No linking state to clear")
			result.Packs = append(result.Packs, packResult)
			continue
		}

		// Clear handlers for this pack using enhanced method that handles linking handlers
		clearResults, err := core.ClearHandlersEnhanced(ctx, handlersWithState)
		if err != nil {
			packResult.Errors = append(packResult.Errors, err.Error())
			logger.Error().
				Err(err).
				Str("pack", pack.Name).
				Msg("Failed to clear handlers")
		}

		// Convert clear results to unlink format
		for handlerName, clearResult := range clearResults {
			if clearResult.Error != nil {
				packResult.Errors = append(packResult.Errors,
					fmt.Sprintf("%s: %v", handlerName, clearResult.Error))
			}

			// Convert cleared items to removed items
			for _, item := range clearResult.ClearedItems {
				removedItem := RemovedItem{
					Type:    item.Type,
					Path:    item.Path,
					Success: clearResult.Error == nil,
				}

				// For symlinks, the description contains target info
				// We don't have direct access to the target, but that's OK
				// The important part is that the symlink was removed

				packResult.RemovedItems = append(packResult.RemovedItems, removedItem)
			}

			// Add state directory removal as a separate item
			// In dry run mode, we still want to report what would be removed
			if clearResult.StateRemoved || ctx.DryRun {
				// Use the actual state directory name (e.g., "symlinks" for "symlink" handler)
				stateDirName := getHandlerStateDir(handlerName)
				packResult.RemovedItems = append(packResult.RemovedItems, RemovedItem{
					Type:    handlerName + "_directory",
					Path:    p.PackHandlerDir(pack.Name, stateDirName),
					Success: true,
				})
			}
		}

		result.TotalRemoved += len(packResult.RemovedItems)
		result.Packs = append(result.Packs, packResult)
	}

	logger.Info().
		Int("packsProcessed", len(result.Packs)).
		Int("totalRemoved", result.TotalRemoved).
		Bool("dryRun", opts.DryRun).
		Msg("Unlink v2 command completed")

	return result, nil
}
