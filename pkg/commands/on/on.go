package on

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/commands/link"
	"github.com/arthur-debert/dodot/pkg/commands/provision"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
)

// OnPacksOptions defines the options for the OnPacks command
type OnPacksOptions struct {
	// DotfilesRoot is the path to the root of the dotfiles directory
	DotfilesRoot string
	// PackNames is a list of specific packs to turn on. If empty, all packs are turned on
	PackNames []string
	// DryRun specifies whether to perform a dry run without making changes
	DryRun bool
	// Force forces operations even if there are conflicts
	Force bool
	// NoProvision skips provisioning handlers (only link files)
	NoProvision bool
	// ProvisionRerun forces re-run provisioning even if already done
	ProvisionRerun bool
}

// OnResult represents the result of turning on packs
type OnResult struct {
	LinkResult      *types.ExecutionContext
	ProvisionResult *types.ExecutionContext
	TotalDeployed   int
	DryRun          bool
	Errors          []error
}

// OnPacks turns on the specified packs by running link followed by provision.
// This deploys the pack (creates symlinks and runs provisioning).
//
// The command:
// 1. Runs link to create all symlinks and setup linking handlers
// 2. Runs provision to install resources and setup provisioning handlers
func OnPacks(opts OnPacksOptions) (*OnResult, error) {
	logger := logging.GetLogger("commands.on")
	logger.Debug().
		Str("dotfilesRoot", opts.DotfilesRoot).
		Strs("packNames", opts.PackNames).
		Bool("dryRun", opts.DryRun).
		Bool("force", opts.Force).
		Bool("noProvision", opts.NoProvision).
		Bool("provisionRerun", opts.ProvisionRerun).
		Msg("Starting on command")

	result := &OnResult{
		DryRun: opts.DryRun,
	}

	// Step 1: Run link (create symlinks)
	linkOpts := link.LinkPacksOptions{
		DotfilesRoot:       opts.DotfilesRoot,
		PackNames:          opts.PackNames,
		DryRun:             opts.DryRun,
		EnableHomeSymlinks: false, // Could be made configurable if needed
	}

	linkResult, err := link.LinkPacks(linkOpts)
	if err != nil {
		logger.Error().Err(err).Msg("Failed to link packs")
		result.Errors = append(result.Errors, fmt.Errorf("link failed: %w", err))
	} else {
		result.LinkResult = linkResult
		result.TotalDeployed += linkResult.CompletedHandlers
		// Check for errors in pack results
		for packName, packResult := range linkResult.PackResults {
			if packResult.FailedHandlers > 0 {
				result.Errors = append(result.Errors, fmt.Errorf("pack %s had %d failed handlers", packName, packResult.FailedHandlers))
			}
		}
	}

	// Step 2: Run provision (unless --no-provision was specified)
	if !opts.NoProvision {
		provisionOpts := provision.ProvisionPacksOptions{
			DotfilesRoot: opts.DotfilesRoot,
			PackNames:    opts.PackNames,
			DryRun:       opts.DryRun,
			Force:        opts.ProvisionRerun, // --provision-rerun maps to Force flag
		}

		provisionResult, err := provision.ProvisionPacks(provisionOpts)
		if err != nil {
			logger.Error().Err(err).Msg("Failed to provision packs")
			result.Errors = append(result.Errors, fmt.Errorf("provision failed: %w", err))
		} else {
			result.ProvisionResult = provisionResult
			result.TotalDeployed += provisionResult.CompletedHandlers
			// Check for errors in pack results
			for packName, packResult := range provisionResult.PackResults {
				if packResult.FailedHandlers > 0 {
					result.Errors = append(result.Errors, fmt.Errorf("pack %s had %d failed handlers during provisioning", packName, packResult.FailedHandlers))
				}
			}
		}
	} else {
		logger.Info().Msg("Skipping provision step due to --no-provision flag")
	}

	logger.Info().
		Int("totalDeployed", result.TotalDeployed).
		Int("errors", len(result.Errors)).
		Bool("dryRun", opts.DryRun).
		Msg("On command completed")

	if len(result.Errors) > 0 {
		return result, fmt.Errorf("on command encountered %d errors", len(result.Errors))
	}

	return result, nil
}
