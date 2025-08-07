package install

import (
	"github.com/arthur-debert/dodot/pkg/commands/internal"
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
)

// InstallPacksOptions defines the options for the InstallPacks command.
type InstallPacksOptions struct {
	// DotfilesRoot is the path to the root of the dotfiles directory.
	DotfilesRoot string
	// PackNames is a list of specific packs to install. If empty, all packs are installed.
	PackNames []string
	// DryRun specifies whether to perform a dry run without making changes.
	DryRun bool
	// Force re-runs power-ups that normally only run once.
	Force bool
	// EnableHomeSymlinks allows symlink operations to target the user's home directory.
	EnableHomeSymlinks bool
}

// InstallPacks runs the installation + deployment using the direct executor approach.
// It executes both RunModeOnce actions (install scripts, brewfiles) and RunModeMany
// actions (symlinks, shell profiles, path) in sequence.
func InstallPacks(opts InstallPacksOptions) (*types.ExecutionContext, error) {
	log := logging.GetLogger("commands.install")
	log.Debug().Str("command", "InstallPacks").Msg("Executing command")

	// Phase 1: Run install scripts, brewfiles, etc. (RunModeOnce actions)
	log.Debug().Msg("Phase 1: Executing run-once actions (install scripts, brewfiles)")
	installCtx, err := internal.RunPipeline(internal.PipelineOptions{
		DotfilesRoot:       opts.DotfilesRoot,
		PackNames:          opts.PackNames,
		DryRun:             opts.DryRun,
		RunMode:            types.RunModeOnce, // Only install scripts, brewfiles
		Force:              opts.Force,        // Force flag applies to run-once actions
		EnableHomeSymlinks: opts.EnableHomeSymlinks,
	})

	if err != nil {
		log.Error().Err(err).Msg("Phase 1 (install) failed")
		return installCtx, errors.Wrapf(err, errors.ErrActionExecute, "failed to execute install actions")
	}

	// Phase 2: Run symlinks, shell profiles, etc. (RunModeMany actions)
	log.Debug().Msg("Phase 2: Executing deployment actions (symlinks, profiles)")
	deployCtx, err := internal.RunPipeline(internal.PipelineOptions{
		DotfilesRoot:       opts.DotfilesRoot,
		PackNames:          opts.PackNames,
		DryRun:             opts.DryRun,
		RunMode:            types.RunModeMany, // Only symlinks, profiles, etc.
		Force:              false,             // Force doesn't apply to deploy actions
		EnableHomeSymlinks: opts.EnableHomeSymlinks,
	})

	if err != nil {
		log.Error().Err(err).Msg("Phase 2 (deploy) failed")
		// Return combined context with partial results from both phases
		return mergeExecutionContexts(installCtx, deployCtx), errors.Wrapf(err, errors.ErrActionExecute, "failed to execute deployment actions")
	}

	// Merge results from both phases
	mergedCtx := mergeExecutionContexts(installCtx, deployCtx)

	log.Info().
		Int("installActions", installCtx.TotalActions).
		Int("deployActions", deployCtx.TotalActions).
		Int("totalActions", mergedCtx.TotalActions).
		Str("command", "InstallPacks").
		Msg("Command finished")

	return mergedCtx, nil
}

// mergeExecutionContexts combines results from install and deploy phases into a single context
func mergeExecutionContexts(installCtx, deployCtx *types.ExecutionContext) *types.ExecutionContext {
	if installCtx == nil && deployCtx == nil {
		return types.NewExecutionContext("install", false)
	}
	if installCtx == nil {
		deployCtx.Command = "install" // Update command name
		return deployCtx
	}
	if deployCtx == nil {
		return installCtx
	}

	// Create new merged context using install context as base
	merged := types.NewExecutionContext("install", installCtx.DryRun)
	merged.StartTime = installCtx.StartTime

	// Add all pack results from install phase
	for packName, packResult := range installCtx.PackResults {
		merged.AddPackResult(packName, packResult)
	}

	// Merge in pack results from deploy phase
	for packName, deployPackResult := range deployCtx.PackResults {
		if existingPackResult, exists := merged.PackResults[packName]; exists {
			// Merge PowerUp results from deploy into existing pack result
			existingPackResult.PowerUpResults = append(existingPackResult.PowerUpResults, deployPackResult.PowerUpResults...)
			existingPackResult.TotalPowerUps += deployPackResult.TotalPowerUps
			existingPackResult.CompletedPowerUps += deployPackResult.CompletedPowerUps
			existingPackResult.FailedPowerUps += deployPackResult.FailedPowerUps
			existingPackResult.SkippedPowerUps += deployPackResult.SkippedPowerUps

			// Update pack status - if either phase failed, mark as failed
			switch deployPackResult.Status {
			case types.ExecutionStatusError:
				existingPackResult.Status = types.ExecutionStatusError
			case types.ExecutionStatusPartial:
				existingPackResult.Status = types.ExecutionStatusPartial
			}
		} else {
			// Add pack result that only appeared in deploy phase
			merged.AddPackResult(packName, deployPackResult)
		}
	}

	// Use the later end time
	if deployCtx.EndTime.After(installCtx.EndTime) {
		merged.EndTime = deployCtx.EndTime
	} else {
		merged.EndTime = installCtx.EndTime
	}

	// Recalculate totals (AddPackResult should have handled this, but be explicit)
	merged.TotalActions = 0
	merged.CompletedActions = 0
	merged.FailedActions = 0
	merged.SkippedActions = 0

	for _, packResult := range merged.PackResults {
		merged.TotalActions += packResult.TotalPowerUps
		merged.CompletedActions += packResult.CompletedPowerUps
		merged.FailedActions += packResult.FailedPowerUps
		merged.SkippedActions += packResult.SkippedPowerUps
	}

	return merged
}

// InstallPacksDirect is an alias for InstallPacks for backward compatibility.
// Deprecated: Use InstallPacks instead.
func InstallPacksDirect(opts InstallPacksOptions) (*types.ExecutionContext, error) {
	return InstallPacks(opts)
}
