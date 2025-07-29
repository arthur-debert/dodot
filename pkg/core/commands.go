package core

import (
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/types"
)

// ListPacksOptions defines the options for the ListPacks command.
type ListPacksOptions struct {
	// DotfilesRoot is the path to the root of the dotfiles directory.
	DotfilesRoot string
}

// DeployPacksOptions defines the options for the DeployPacks command.
type DeployPacksOptions struct {
	// DotfilesRoot is the path to the root of the dotfiles directory.
	DotfilesRoot string
	// PackNames is a list of specific packs to deploy. If empty, all packs are deployed.
	PackNames []string
	// DryRun specifies whether to perform a dry run without making changes.
	DryRun bool
}

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
}

// ListPacks finds all available packs in the dotfiles root.
func ListPacks(opts ListPacksOptions) (*types.ListPacksResult, error) {
	log := logging.GetLogger("core.commands")
	log.Debug().Str("command", "ListPacks").Msg("Executing command")

	candidates, err := GetPackCandidates(opts.DotfilesRoot)
	if err != nil {
		return nil, err
	}

	packs, err := GetPacks(candidates)
	if err != nil {
		return nil, err
	}

	result := &types.ListPacksResult{
		Packs: make([]types.PackInfo, len(packs)),
	}

	for i, p := range packs {
		result.Packs[i] = types.PackInfo{
			Name: p.Name,
			Path: p.Path,
		}
	}

	log.Info().Str("command", "ListPacks").Int("packCount", len(result.Packs)).Msg("Command finished")
	return result, nil
}

// DeployPacks runs the deployment logic for the specified packs.
// This executes power-ups with RunModeMany.
func DeployPacks(opts DeployPacksOptions) (*types.ExecutionResult, error) {
	log := logging.GetLogger("core.commands")
	log.Debug().Str("command", "DeployPacks").Msg("Executing command")

	execOpts := executionOptions{
		DotfilesRoot: opts.DotfilesRoot,
		PackNames:    opts.PackNames,
		DryRun:       opts.DryRun,
		RunMode:      types.RunModeMany,
	}

	result, err := runExecutionPipeline(execOpts)
	if err != nil {
		return nil, err
	}

	log.Info().Str("command", "DeployPacks").Msg("Command finished")
	return result, nil
}

// InstallPacks runs the installation and deployment logic for the specified packs.
// It first executes power-ups with RunModeOnce, then those with RunModeMany.
func InstallPacks(opts InstallPacksOptions) (*types.ExecutionResult, error) {
	log := logging.GetLogger("core.commands")
	log.Debug().Str("command", "InstallPacks").Msg("Executing command")

	// Step 1: Run "once" power-ups
	onceOpts := executionOptions{
		DotfilesRoot: opts.DotfilesRoot,
		PackNames:    opts.PackNames,
		DryRun:       opts.DryRun,
		RunMode:      types.RunModeOnce,
		Force:        opts.Force,
	}
	onceResult, err := runExecutionPipeline(onceOpts)
	if err != nil {
		return nil, err
	}

	// Step 2: Run "many" power-ups (deploy)
	manyOpts := executionOptions{
		DotfilesRoot: opts.DotfilesRoot,
		PackNames:    opts.PackNames,
		DryRun:       opts.DryRun,
		RunMode:      types.RunModeMany,
	}
	manyResult, err := runExecutionPipeline(manyOpts)
	if err != nil {
		return nil, err
	}

	// Step 3: Merge results
	mergedResult := &types.ExecutionResult{
		Packs:      onceResult.Packs,
		Operations: append(onceResult.Operations, manyResult.Operations...),
		DryRun:     opts.DryRun,
	}

	log.Info().Str("command", "InstallPacks").Msg("Command finished")
	return mergedResult, nil
}

// executionOptions is an internal struct to pass to the pipeline runner.
type executionOptions struct {
	DotfilesRoot string
	PackNames    []string
	DryRun       bool
	RunMode      types.RunMode
	Force        bool
}

// runExecutionPipeline is the core logic for deploy and install.
func runExecutionPipeline(opts executionOptions) (*types.ExecutionResult, error) {
	// 1. Get all packs
	candidates, err := GetPackCandidates(opts.DotfilesRoot)
	if err != nil {
		return nil, err
	}
	allPacks, err := GetPacks(candidates)
	if err != nil {
		return nil, err
	}

	// 2. Filter to selected packs
	selectedPacks, err := SelectPacks(allPacks, opts.PackNames)
	if err != nil {
		return nil, err
	}

	// 3. Get all trigger matches for the selected packs
	matches, err := GetFiringTriggers(selectedPacks)
	if err != nil {
		return nil, err
	}

	// 4. Get all actions from the matches
	actions, err := GetActions(matches)
	if err != nil {
		return nil, err
	}

	// 5. Filter actions by the desired RunMode
	filteredActions, err := filterActionsByRunMode(actions, opts.RunMode)
	if err != nil {
		return nil, err
	}

	// 6. Convert the filtered actions to filesystem operations
	ops, err := GetFsOps(filteredActions)
	if err != nil {
		return nil, err
	}

	// 7. Construct and return the result
	result := &types.ExecutionResult{
		Packs:      getPackNames(selectedPacks),
		Operations: ops,
		DryRun:     opts.DryRun,
	}

	return result, nil
}

// filterActionsByRunMode filters a slice of actions based on the RunMode of the
// power-up that generated them.
func filterActionsByRunMode(actions []types.Action, mode types.RunMode) ([]types.Action, error) {
	var filtered []types.Action
	for _, action := range actions {
		// The PowerUpName is stored in the action. We need to get the factory,
		// create a temporary instance (without options) just to check its RunMode.
		factory, err := registry.GetPowerUpFactory(action.PowerUpName)
		if err != nil {
			// This should be rare, as the power-up must have existed to create the action
			return nil, err
		}
		powerUp, err := factory(nil) // Options don't affect the RunMode
		if err != nil {
			return nil, err
		}

		if powerUp.RunMode() == mode {
			filtered = append(filtered, action)
		}
	}
	return filtered, nil
}
