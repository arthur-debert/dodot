package unlink

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/handlers/path"
	"github.com/arthur-debert/dodot/pkg/handlers/shell"
	"github.com/arthur-debert/dodot/pkg/handlers/symlink"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/operations"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
)

// UnlinkPacksOptions contains options for the unlink command
type UnlinkPacksOptions struct {
	// DotfilesRoot is the path to the dotfiles directory
	DotfilesRoot string

	// DataDir is the dodot data directory (unused, kept for compatibility)
	DataDir string

	// PackNames is the list of pack names to unlink (empty = all)
	PackNames []string

	// Force skips confirmation prompts (unused in clearable implementation)
	Force bool

	// DryRun shows what would be removed without actually removing
	DryRun bool
}

// UnlinkResult contains the result of the unlink command
type UnlinkResult struct {
	// Packs that were processed
	Packs []PackUnlinkResult `json:"packs"`

	// Total number of items removed
	TotalRemoved int `json:"totalRemoved"`

	// Whether this was a dry run
	DryRun bool `json:"dryRun"`
}

// PackUnlinkResult contains the result for a single pack
type PackUnlinkResult struct {
	// Name of the pack
	Name string `json:"name"`

	// Items that were removed
	RemovedItems []RemovedItem `json:"removedItems"`

	// Any errors encountered
	Errors []string `json:"errors,omitempty"`
}

// RemovedItem represents a single removed deployment
type RemovedItem struct {
	// Type of item (symlink, path, shell, etc.)
	Type string `json:"type"`

	// Path that was removed
	Path string `json:"path"`

	// Target it pointed to (for symlinks)
	Target string `json:"target,omitempty"`

	// Whether removal succeeded
	Success bool `json:"success"`

	// Error if removal failed
	Error string `json:"error,omitempty"`
}

// UnlinkPacks removes deployments for the specified packs
//
// This command undoes the effects of linking handlers (symlink, path, shell)
// but deliberately leaves provisioning handlers (provision, homebrew) untouched.
//
// This implementation uses the clearable infrastructure to ensure consistency
// with other clear operations.
func UnlinkPacks(opts UnlinkPacksOptions) (*UnlinkResult, error) {
	logger := logging.GetLogger("commands.unlink")
	logger.Debug().
		Str("dotfilesRoot", opts.DotfilesRoot).
		Strs("packNames", opts.PackNames).
		Bool("dryRun", opts.DryRun).
		Msg("Starting unlink command")

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

	// Create configuration handlers only (safe to clear/rerun)
	configHandlers := []operations.Handler{
		symlink.NewHandler(),
		shell.NewHandler(),
		path.NewHandler(),
	}

	logger.Debug().
		Int("packCount", len(packs)).
		Int("handlerCount", len(configHandlers)).
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
			Pack:   pack,
			FS:     fs,
			Paths:  p,
			DryRun: opts.DryRun,
		}

		// Filter handlers to only those with state
		var handlersWithState []operations.Handler
		for _, handler := range configHandlers {
			// Check if handler has any state for this pack
			stateDir := p.PackHandlerDir(pack.Name, handler.Name())
			if _, err := fs.Stat(stateDir); err == nil {
				handlersWithState = append(handlersWithState, handler)
			}
		}

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
		// Create executor and clear handlers for this pack
		executor := operations.NewExecutor(ds, fs, nil, opts.DryRun)
		clearResults := make(map[string][]types.ClearedItem)

		for _, handler := range handlersWithState {
			items, err := executor.ExecuteClear(handler, ctx)
			if err != nil {
				packResult.Errors = append(packResult.Errors, fmt.Sprintf("%s: %v", handler.Name(), err))
				logger.Error().
					Err(err).
					Str("pack", pack.Name).
					Str("handler", handler.Name()).
					Msg("Failed to clear handler")
				continue
			}
			clearResults[handler.Name()] = items
		}

		// Convert clear results to unlink format
		for handlerName, items := range clearResults {
			// Convert cleared items to removed items
			for _, item := range items {
				removedItem := RemovedItem{
					Type:    item.Type,
					Path:    item.Path,
					Success: true,
				}
				packResult.RemovedItems = append(packResult.RemovedItems, removedItem)
			}

			// Add state directory removal as a separate item
			if len(items) > 0 || ctx.DryRun {
				packResult.RemovedItems = append(packResult.RemovedItems, RemovedItem{
					Type:    handlerName + "_directory",
					Path:    p.PackHandlerDir(pack.Name, handlerName),
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
		Msg("Unlink command completed")

	return result, nil
}
