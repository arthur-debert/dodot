package on

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/commands/internal"
	"github.com/arthur-debert/dodot/pkg/commands/off"
	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
)

// OnPacksOptions defines the options for the OnPacks command
type OnPacksOptions struct {
	// DotfilesRoot is the path to the root of the dotfiles directory
	DotfilesRoot string
	// PackNames is a list of specific packs to turn on. If empty, all off packs are turned on
	PackNames []string
	// DryRun specifies whether to perform a dry run without making changes
	DryRun bool
	// Force enables packs without checking stored state (re-deploys from scratch)
	Force bool
}

// OnResult represents the result of turning on packs
type OnResult struct {
	Packs         []PackOnResult
	TotalRestored int
	TotalDeployed int
	DryRun        bool
	Errors        []error
}

// PackOnResult represents the result of turning on a single pack
type PackOnResult struct {
	Name          string
	WasOff        bool                    // Was the pack previously turned off
	StateRestored bool                    // Was stored state successfully restored
	Redeployed    bool                    // Was the pack re-deployed from scratch
	ExecutionCtx  *types.ExecutionContext // Deployment result if redeployed
	Error         error
}

// OnPacks turns on (re-enables) the specified packs by either:
// 1. Restoring their stored state if they were turned off with 'off' command
// 2. Re-deploying them from scratch if no stored state exists or --force is used
//
// This command:
// 1. Checks if packs have stored off-state
// 2. For packs with stored state: restores their deployment state
// 3. For packs without stored state or with --force: runs normal deployment pipeline
// 4. Cleans up stored off-state files after successful restoration
func OnPacks(opts OnPacksOptions) (*OnResult, error) {
	logger := logging.GetLogger("commands.on")
	logger.Debug().
		Str("dotfilesRoot", opts.DotfilesRoot).
		Strs("packNames", opts.PackNames).
		Bool("dryRun", opts.DryRun).
		Bool("force", opts.Force).
		Msg("Starting on command")

	// Initialize paths
	p, err := paths.New(opts.DotfilesRoot)
	if err != nil {
		return nil, fmt.Errorf("failed to initialize paths: %w", err)
	}

	// Discover packs
	var packs []types.Pack
	if len(opts.PackNames) == 0 {
		// Find all packs that are turned off
		offPacks, err := findOffPacks(p)
		if err != nil {
			return nil, fmt.Errorf("failed to find off packs: %w", err)
		}

		// Convert to Pack structs
		for _, packName := range offPacks {
			// We need the actual pack directory - discover it
			allPacks, err := core.DiscoverAndSelectPacks(opts.DotfilesRoot, []string{packName})
			if err != nil {
				logger.Warn().
					Str("pack", packName).
					Err(err).
					Msg("Failed to find pack directory for off pack")
				continue
			}
			if len(allPacks) > 0 {
				packs = append(packs, allPacks[0])
			}
		}
	} else {
		// Use specified pack names
		allPacks, err := core.DiscoverAndSelectPacks(opts.DotfilesRoot, opts.PackNames)
		if err != nil {
			return nil, fmt.Errorf("failed to discover packs: %w", err)
		}
		packs = allPacks
	}

	logger.Debug().
		Int("packCount", len(packs)).
		Msg("Discovered packs to turn on")

	// Process each pack
	result := &OnResult{
		DryRun: opts.DryRun,
	}

	for _, pack := range packs {
		packResult := processPackOn(pack, p, opts)

		if packResult.Error != nil {
			result.Errors = append(result.Errors, fmt.Errorf("pack %s: %w", pack.Name, packResult.Error))
		}

		if packResult.StateRestored {
			result.TotalRestored++
		}
		if packResult.Redeployed {
			result.TotalDeployed++
		}

		result.Packs = append(result.Packs, packResult)
	}

	logger.Info().
		Int("packsProcessed", len(result.Packs)).
		Int("totalRestored", result.TotalRestored).
		Int("totalDeployed", result.TotalDeployed).
		Int("errors", len(result.Errors)).
		Bool("dryRun", opts.DryRun).
		Msg("On command completed")

	return result, nil
}

// processPackOn handles turning on a single pack
func processPackOn(pack types.Pack, p paths.Paths, opts OnPacksOptions) PackOnResult {
	logger := logging.GetLogger("commands.on")

	packResult := PackOnResult{
		Name: pack.Name,
	}

	// Check if pack has stored off-state
	if !opts.Force && off.IsPackOff(p, pack.Name) {
		packResult.WasOff = true

		// Attempt to restore from stored state
		if err := restorePackState(pack, p, opts.DryRun); err != nil {
			logger.Warn().
				Str("pack", pack.Name).
				Err(err).
				Msg("Failed to restore pack state, falling back to re-deployment")
		} else {
			packResult.StateRestored = true

			// Clean up stored state file (unless dry run)
			if !opts.DryRun {
				if err := removePackOffState(p, pack.Name); err != nil {
					logger.Warn().
						Str("pack", pack.Name).
						Err(err).
						Msg("Failed to remove off-state file")
				}
			}

			return packResult
		}
	}

	// Fall back to re-deployment using the pipeline
	logger.Debug().
		Str("pack", pack.Name).
		Bool("wasOff", packResult.WasOff).
		Bool("force", opts.Force).
		Msg("Re-deploying pack using pipeline")

	// Use the internal pipeline to deploy the pack
	pipelineOpts := internal.PipelineOptions{
		DotfilesRoot:       opts.DotfilesRoot,
		PackNames:          []string{pack.Name},
		DryRun:             opts.DryRun,
		RunMode:            types.RunModeLinking, // Start with linking
		Force:              false,
		EnableHomeSymlinks: false,
	}

	// Deploy linking first
	linkCtx, err := internal.RunPipeline(pipelineOpts)
	if err != nil {
		packResult.Error = fmt.Errorf("failed to deploy linking for pack: %w", err)
		return packResult
	}

	// Then deploy provisioning
	pipelineOpts.RunMode = types.RunModeProvisioning
	provisionCtx, err := internal.RunPipeline(pipelineOpts)
	if err != nil {
		packResult.Error = fmt.Errorf("failed to deploy provisioning for pack: %w", err)
		return packResult
	}

	// Use the provisioning context as the main result (it includes both phases)
	_ = linkCtx // We ran both phases but only return the provision context
	packResult.ExecutionCtx = provisionCtx
	packResult.Redeployed = true

	// Clean up off-state file if it existed (unless dry run)
	if !opts.DryRun && packResult.WasOff {
		if err := removePackOffState(p, pack.Name); err != nil {
			logger.Warn().
				Str("pack", pack.Name).
				Err(err).
				Msg("Failed to remove off-state file after re-deployment")
		}
	}

	return packResult
}

// restorePackState restores a pack's state from stored off-state
func restorePackState(pack types.Pack, p paths.Paths, dryRun bool) error {
	logger := logging.GetLogger("commands.on")

	// Load stored state
	state, err := off.LoadPackState(p, pack.Name)
	if err != nil {
		return fmt.Errorf("failed to load pack state: %w", err)
	}

	logger.Debug().
		Str("pack", pack.Name).
		Int("handlers", len(state.Handlers)).
		Str("version", state.Version).
		Msg("Loaded pack state for restoration")

	// For now, state restoration is not implemented
	// This would require:
	// 1. Recreating symlinks based on stored ClearedItems
	// 2. Restoring PATH entries
	// 3. Re-enabling shell profile sources
	// 4. Re-installing homebrew packages (if user previously approved)
	// 5. Re-running provision scripts (if needed)

	// TODO: Implement state restoration
	// For now, we'll return an error to force re-deployment
	return fmt.Errorf("state restoration not yet implemented")
}

// findOffPacks returns the names of all packs that are currently turned off
func findOffPacks(p paths.Paths) ([]string, error) {
	offStateDir := filepath.Join(p.DataDir(), "off-state")

	// Check if off-state directory exists
	if _, err := os.Stat(offStateDir); os.IsNotExist(err) {
		return []string{}, nil
	}

	// Read off-state directory
	entries, err := os.ReadDir(offStateDir)
	if err != nil {
		return nil, fmt.Errorf("failed to read off-state directory: %w", err)
	}

	var offPacks []string
	for _, entry := range entries {
		if !entry.IsDir() && filepath.Ext(entry.Name()) == ".json" {
			packName := entry.Name()[:len(entry.Name())-5] // Remove .json extension
			offPacks = append(offPacks, packName)
		}
	}

	return offPacks, nil
}

// removePackOffState removes the stored off-state file for a pack
func removePackOffState(p paths.Paths, packName string) error {
	stateFile := filepath.Join(p.DataDir(), "off-state", packName+".json")

	if err := os.Remove(stateFile); err != nil && !os.IsNotExist(err) {
		return fmt.Errorf("failed to remove off-state file: %w", err)
	}

	return nil
}