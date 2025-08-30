package deprovision

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/handlers/homebrew"
	"github.com/arthur-debert/dodot/pkg/handlers/install"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/operations"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
)

// DeprovisionPacksOptions defines the options for the DeprovisionPacks command.
type DeprovisionPacksOptions struct {
	// DotfilesRoot is the path to the root of the dotfiles directory.
	DotfilesRoot string
	// PackNames is a list of specific packs to deprovision. If empty, all packs are deprovisioned.
	PackNames []string
	// DryRun specifies whether to perform a dry run without making changes.
	DryRun bool
}

// DeprovisionResult represents the result of deprovisioning operations
type DeprovisionResult struct {
	Packs        []PackResult
	TotalCleared int
	DryRun       bool
	Errors       []error
}

// PackResult represents the result of deprovisioning a single pack
type PackResult struct {
	Name         string
	HandlersRun  []HandlerResult
	TotalCleared int
	Error        error
}

// HandlerResult represents the result of clearing a single handler
type HandlerResult struct {
	HandlerName  string
	ClearedItems []types.ClearedItem
	StateRemoved bool
	Error        error
}

// DeprovisionPacks removes provisioning state for the specified packs.
// It only clears provisioning handlers (homebrew, provision) while preserving
// linking handler state (symlinks, path, shell).
func DeprovisionPacks(opts DeprovisionPacksOptions) (*DeprovisionResult, error) {
	logger := logging.GetLogger("commands.deprovision")
	logger.Debug().
		Str("dotfilesRoot", opts.DotfilesRoot).
		Strs("packNames", opts.PackNames).
		Bool("dryRun", opts.DryRun).
		Msg("Starting deprovision command")

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

	// Create code execution handlers (require user consent)
	codeExecHandlers := []operations.Handler{
		homebrew.NewHandler(),
		install.NewHandler(),
	}

	logger.Debug().
		Int("packCount", len(packs)).
		Int("handlerCount", len(codeExecHandlers)).
		Msg("Discovered packs and handlers")

	// Process each pack
	result := &DeprovisionResult{
		DryRun: opts.DryRun,
	}

	for _, pack := range packs {
		packResult := PackResult{
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
		for _, handler := range codeExecHandlers {
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
				Msg("No provisioning state to clear")
			result.Packs = append(result.Packs, packResult)
			continue
		}

		// Create executor and clear handlers for this pack
		executor := operations.NewExecutor(ds, fs, nil, opts.DryRun)
		var clearResults []types.ClearedItem

		for _, handler := range handlersWithState {
			items, err := executor.ExecuteClear(handler, ctx)
			if err != nil {
				packResult.Error = err
				result.Errors = append(result.Errors, fmt.Errorf("pack %s handler %s: %w", pack.Name, handler.Name(), err))
				continue
			}
			clearResults = append(clearResults, items...)
		}

		// Group cleared items by handler
		handlerItems := make(map[string][]types.ClearedItem)
		for _, item := range clearResults {
			// Extract handler name from the item (you might need to add HandlerName to ClearedItem)
			// For now, we'll group by the handler that cleared it
			handlerItems[item.Type] = append(handlerItems[item.Type], item)
		}

		// Convert to handler results
		for _, handler := range handlersWithState {
			items := handlerItems[handler.Name()]
			handlerResult := HandlerResult{
				HandlerName:  handler.Name(),
				ClearedItems: items,
				StateRemoved: len(items) > 0,
			}
			packResult.HandlersRun = append(packResult.HandlersRun, handlerResult)
			packResult.TotalCleared += len(items)
		}

		result.TotalCleared += packResult.TotalCleared
		result.Packs = append(result.Packs, packResult)
	}

	logger.Info().
		Int("packsProcessed", len(result.Packs)).
		Int("totalCleared", result.TotalCleared).
		Int("errors", len(result.Errors)).
		Bool("dryRun", opts.DryRun).
		Msg("Deprovision command completed")

	return result, nil
}
