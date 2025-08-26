package off

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/commands/deprovision"
	"github.com/arthur-debert/dodot/pkg/commands/unlink"
	"github.com/arthur-debert/dodot/pkg/logging"
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

// OffResult represents the result of turning off packs
type OffResult struct {
	UnlinkResult      *unlink.UnlinkResult
	DeprovisionResult *deprovision.DeprovisionResult
	TotalCleared      int
	DryRun            bool
	Errors            []error
}

// OffPacks turns off the specified packs by running unlink followed by deprovision.
// This completely removes the pack deployment (both symlinks and provisioned resources).
//
// The command:
// 1. Runs unlink to remove all symlinks and clear linking handler state
// 2. Runs deprovision to remove provisioned resources and clear provisioning handler state
func OffPacks(opts OffPacksOptions) (*OffResult, error) {
	logger := logging.GetLogger("commands.off")
	logger.Debug().
		Str("dotfilesRoot", opts.DotfilesRoot).
		Strs("packNames", opts.PackNames).
		Bool("dryRun", opts.DryRun).
		Msg("Starting off command")

	result := &OffResult{
		DryRun: opts.DryRun,
	}

	// Step 1: Run deprovision first (remove provisioned resources)
	deprovisionOpts := deprovision.DeprovisionPacksOptions{
		DotfilesRoot: opts.DotfilesRoot,
		PackNames:    opts.PackNames,
		DryRun:       opts.DryRun,
	}

	deprovisionResult, err := deprovision.DeprovisionPacks(deprovisionOpts)
	if err != nil {
		logger.Error().Err(err).Msg("Failed to deprovision packs")
		result.Errors = append(result.Errors, fmt.Errorf("deprovision failed: %w", err))
	} else {
		result.DeprovisionResult = deprovisionResult
		result.TotalCleared += deprovisionResult.TotalCleared
		if len(deprovisionResult.Errors) > 0 {
			result.Errors = append(result.Errors, deprovisionResult.Errors...)
		}
	}

	// Step 2: Run unlink (remove symlinks)
	unlinkOpts := unlink.UnlinkPacksOptions{
		DotfilesRoot: opts.DotfilesRoot,
		PackNames:    opts.PackNames,
		DryRun:       opts.DryRun,
	}

	unlinkResult, err := unlink.UnlinkPacks(unlinkOpts)
	if err != nil {
		logger.Error().Err(err).Msg("Failed to unlink packs")
		result.Errors = append(result.Errors, fmt.Errorf("unlink failed: %w", err))
	} else {
		result.UnlinkResult = unlinkResult
		result.TotalCleared += unlinkResult.TotalRemoved
	}

	logger.Info().
		Int("totalCleared", result.TotalCleared).
		Int("errors", len(result.Errors)).
		Bool("dryRun", opts.DryRun).
		Msg("Off command completed")

	if len(result.Errors) > 0 {
		return result, fmt.Errorf("off command encountered %d errors", len(result.Errors))
	}

	return result, nil
}
