package deprovision

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
)

// DeprovisionPacksOptions defines the options for the DeprovisionPacks command.
type DeprovisionPacksOptions struct {
	DotfilesRoot string
	PackNames    []string
	DryRun       bool
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
// It uses the DataStore abstraction properly, without knowing handler internals.
func DeprovisionPacks(opts DeprovisionPacksOptions) (*DeprovisionResult, error) {
	logger := logging.GetLogger("commands.deprovision")
	logger.Info().
		Strs("packs", opts.PackNames).
		Bool("dryRun", opts.DryRun).
		Msg("Deprovisioning packs")

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

	// Provisioning handlers that deprovision removes
	provisioningHandlers := []string{"homebrew", "install"}

	result := &DeprovisionResult{
		DryRun: opts.DryRun,
	}

	// Process each pack
	for _, pack := range packs {
		packResult := PackResult{
			Name: pack.Name,
		}

		// Remove state for each provisioning handler
		for _, handlerName := range provisioningHandlers {
			handlerResult := HandlerResult{
				HandlerName: handlerName,
			}

			if opts.DryRun {
				// In dry run, just check if state exists
				stateDir := p.PackHandlerDir(pack.Name, handlerName)
				if _, err := fs.Stat(stateDir); err == nil {
					handlerResult.StateRemoved = true
					packResult.TotalCleared++
				}
			} else {
				// Actually remove the state
				err := ds.RemoveState(pack.Name, handlerName)
				if err != nil {
					handlerResult.Error = err
					packResult.Error = fmt.Errorf("%s handler failed: %w", handlerName, err)
					logger.Error().
						Err(err).
						Str("pack", pack.Name).
						Str("handler", handlerName).
						Msg("Failed to remove handler state")
				} else {
					handlerResult.StateRemoved = true
					packResult.TotalCleared++
				}
			}

			packResult.HandlersRun = append(packResult.HandlersRun, handlerResult)
		}

		result.TotalCleared += packResult.TotalCleared
		result.Packs = append(result.Packs, packResult)
	}

	logger.Info().
		Int("packsProcessed", len(result.Packs)).
		Int("totalCleared", result.TotalCleared).
		Bool("dryRun", opts.DryRun).
		Msg("Deprovision command completed")

	return result, nil
}
