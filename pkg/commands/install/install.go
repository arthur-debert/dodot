package install

import (
	"github.com/arthur-debert/dodot/pkg/commands/internal"
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

// InstallPacks runs the installation and deployment logic using the direct executor approach.
// It first executes power-ups with RunModeOnce, then those with RunModeMany.
func InstallPacks(opts InstallPacksOptions) (*types.ExecutionContext, error) {
	log := logging.GetLogger("core.commands")
	log.Debug().Str("command", "InstallPacks").Msg("Executing command")

	// Create combined execution context
	combinedContext := types.NewExecutionContext("install", opts.DryRun)

	// Step 1: Run "once" power-ups
	onceOpts := internal.ExecutionOptions{
		DotfilesRoot:       opts.DotfilesRoot,
		PackNames:          opts.PackNames,
		DryRun:             opts.DryRun,
		RunMode:            types.RunModeOnce,
		Force:              opts.Force,
		EnableHomeSymlinks: opts.EnableHomeSymlinks,
	}
	onceContext, err := internal.RunExecutionPipeline(onceOpts)
	if err != nil {
		return nil, err
	}

	// Step 2: Run "many" power-ups (deploy)
	manyOpts := internal.ExecutionOptions{
		DotfilesRoot:       opts.DotfilesRoot,
		PackNames:          opts.PackNames,
		DryRun:             opts.DryRun,
		RunMode:            types.RunModeMany,
		Force:              opts.Force,
		EnableHomeSymlinks: opts.EnableHomeSymlinks,
	}
	manyContext, err := internal.RunExecutionPipeline(manyOpts)
	if err != nil {
		return nil, err
	}

	// Step 3: Merge contexts
	// Copy results from both contexts into the combined context
	for packName, packResult := range onceContext.PackResults {
		combinedContext.AddPackResult(packName, packResult)
	}
	for packName, packResult := range manyContext.PackResults {
		if existingResult, exists := combinedContext.PackResults[packName]; exists {
			// Merge operation results from both contexts
			for _, opResult := range packResult.Operations {
				existingResult.AddOperationResult(opResult)
			}
		} else {
			combinedContext.AddPackResult(packName, packResult)
		}
	}

	combinedContext.Complete()
	log.Info().Str("command", "InstallPacks").Msg("Command finished")
	return combinedContext, nil
}

// InstallPacksDirect is an alias for InstallPacks for backward compatibility.
// Deprecated: Use InstallPacks instead.
func InstallPacksDirect(opts InstallPacksOptions) (*types.ExecutionContext, error) {
	return InstallPacks(opts)
}
