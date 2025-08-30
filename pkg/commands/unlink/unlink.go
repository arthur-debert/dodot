package unlink

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
)

// UnlinkPacksOptions contains options for the unlink command
type UnlinkPacksOptions struct {
	DotfilesRoot string
	PackNames    []string
	DryRun       bool
}

// UnlinkResult contains the result of the unlink command
type UnlinkResult struct {
	Packs        []PackUnlinkResult `json:"packs"`
	TotalRemoved int                `json:"totalRemoved"`
	DryRun       bool               `json:"dryRun"`
}

// PackUnlinkResult contains the result for a single pack
type PackUnlinkResult struct {
	Name         string        `json:"name"`
	RemovedItems []RemovedItem `json:"removedItems"`
	Errors       []string      `json:"errors,omitempty"`
}

// RemovedItem represents an item that was removed
type RemovedItem struct {
	Type    string `json:"type"`
	Path    string `json:"path"`
	Success bool   `json:"success"`
}

// UnlinkPacks removes configuration state for the specified packs.
// It uses the DataStore abstraction properly, without knowing handler internals.
func UnlinkPacks(opts UnlinkPacksOptions) (*UnlinkResult, error) {
	logger := logging.GetLogger("commands.unlink")
	logger.Info().
		Strs("packs", opts.PackNames).
		Bool("dryRun", opts.DryRun).
		Msg("Unlinking packs")

	// Initialize paths
	p, err := paths.New(opts.DotfilesRoot)
	if err != nil {
		return nil, fmt.Errorf("failed to initialize paths: %w", err)
	}

	// Initialize filesystem and datastore
	fs := filesystem.NewOS()
	ds := datastore.New(fs, p)

	// Discover and select packs
	packs, err := core.DiscoverAndSelectPacks(opts.DotfilesRoot, opts.PackNames)
	if err != nil {
		return nil, fmt.Errorf("failed to discover packs: %w", err)
	}

	// Configuration handlers that unlink removes
	configHandlers := []string{"symlink", "shell", "path"}

	result := &UnlinkResult{
		DryRun: opts.DryRun,
	}

	// Process each pack
	for _, pack := range packs {
		packResult := PackUnlinkResult{
			Name: pack.Name,
		}

		// Remove state for each configuration handler
		for _, handlerName := range configHandlers {
			if opts.DryRun {
				// In dry run, just check if state exists
				stateDir := p.PackHandlerDir(pack.Name, handlerName)
				if _, err := fs.Stat(stateDir); err == nil {
					packResult.RemovedItems = append(packResult.RemovedItems, RemovedItem{
						Type:    handlerName + "_state",
						Path:    stateDir,
						Success: true,
					})
				}
			} else {
				// Actually remove the state
				err := ds.RemoveState(pack.Name, handlerName)
				if err != nil {
					packResult.Errors = append(packResult.Errors, fmt.Sprintf("%s: %v", handlerName, err))
					logger.Error().
						Err(err).
						Str("pack", pack.Name).
						Str("handler", handlerName).
						Msg("Failed to remove handler state")
				} else {
					// Only count as removed if there was actually state to remove
					stateDir := p.PackHandlerDir(pack.Name, handlerName)
					packResult.RemovedItems = append(packResult.RemovedItems, RemovedItem{
						Type:    handlerName + "_state",
						Path:    stateDir,
						Success: true,
					})
				}
			}
		}

		result.TotalRemoved += len(packResult.RemovedItems)
		result.Packs = append(result.Packs, packResult)
	}

	logger.Info().
		Int("packsProcessed", len(result.Packs)).
		Int("totalRemoved", result.TotalRemoved).
		Bool("dryRun", opts.DryRun).
		Msg("Unlink command completed")

	return result, nil
}
