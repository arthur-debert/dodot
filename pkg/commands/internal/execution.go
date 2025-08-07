package internal

import (
	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/types"
)

// ExecutionOptions is an internal struct to pass to the pipeline runner.
type ExecutionOptions struct {
	DotfilesRoot       string
	PackNames          []string
	DryRun             bool
	RunMode            types.RunMode
	Force              bool
	EnableHomeSymlinks bool
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

// RunExecutionPipeline is the core logic that executes actions directly using DirectExecutor
func RunExecutionPipeline(opts ExecutionOptions) (*types.ExecutionContext, error) {
	logger := logging.GetLogger("core.commands")
	logger.Debug().
		Str("dotfilesRoot", opts.DotfilesRoot).
		Strs("packNames", opts.PackNames).
		Bool("dryRun", opts.DryRun).
		Str("runMode", string(opts.RunMode)).
		Bool("force", opts.Force).
		Msg("Starting execution pipeline")

	// 0. Initialize Paths instance
	pathsInstance, err := paths.New(opts.DotfilesRoot)
	if err != nil {
		return nil, err
	}

	// 1. Get Pack Candidates
	candidates, err := core.GetPackCandidates(opts.DotfilesRoot)
	if err != nil {
		return nil, err
	}

	allPacks, err := core.GetPacks(candidates)
	if err != nil {
		return nil, err
	}

	// 2. Filter to specific packs if requested
	selectedPacks, err := core.SelectPacks(allPacks, opts.PackNames)
	if err != nil {
		return nil, err
	}

	// 3. Get firing triggers for the packs
	matches, err := core.GetFiringTriggers(selectedPacks)
	if err != nil {
		return nil, err
	}

	// 4. Generate actions from triggers
	actions, err := core.GetActions(matches)
	if err != nil {
		return nil, err
	}

	// 5. Filter actions by run mode
	filteredActions, err := filterActionsByRunMode(actions, opts.RunMode)
	if err != nil {
		return nil, err
	}

	// 6. Filter run-once actions based on --force flag
	if opts.RunMode == types.RunModeOnce {
		filteredActions, err = core.FilterRunOnceActions(filteredActions, opts.Force, pathsInstance)
		if err != nil {
			return nil, err
		}
	}

	// 7. Create execution context for results tracking
	executionContext := types.NewExecutionContext("deploy", opts.DryRun)

	// 8. Execute actions directly using DirectExecutor
	if len(filteredActions) > 0 {
		// Create DirectExecutor with options
		directExecutorOpts := &core.DirectExecutorOptions{
			Paths:             pathsInstance,
			DryRun:            opts.DryRun,
			Force:             opts.Force,
			AllowHomeSymlinks: opts.EnableHomeSymlinks,
			Config:            config.Default(),
		}

		executor := core.NewDirectExecutor(directExecutorOpts)

		// Execute actions directly
		results, err := executor.ExecuteActions(filteredActions)
		if err != nil {
			logger.Error().Err(err).Msg("Failed to execute actions")
			return executionContext, err
		}

		// Convert results to pack execution results for context
		packResults := groupResultsByPack(results, selectedPacks)
		for packName, packResult := range packResults {
			executionContext.AddPackResult(packName, packResult)
		}

		logger.Info().
			Int("totalOperations", len(results)).
			Int("packsProcessed", len(packResults)).
			Msg("Execution completed")
	}

	executionContext.Complete()
	return executionContext, nil
}

// groupResultsByPack groups operation results by pack for execution context
func groupResultsByPack(results []types.OperationResult, packs []types.Pack) map[string]*types.PackExecutionResult {
	packMap := make(map[string]*types.Pack)
	for i := range packs {
		packMap[packs[i].Name] = &packs[i]
	}

	packResults := make(map[string]*types.PackExecutionResult)

	for _, result := range results {
		packName := result.Operation.Pack
		if packName == "" {
			packName = "unknown"
		}

		// Get or create pack result
		packResult, exists := packResults[packName]
		if !exists {
			pack := packMap[packName]
			if pack == nil {
				// Create a minimal pack for unknown results
				pack = &types.Pack{Name: packName}
			}
			packResult = types.NewPackExecutionResult(pack)
			packResults[packName] = packResult
		}

		// Add operation result to pack
		packResult.AddOperationResult(&result)
	}

	// Complete all pack results
	for _, packResult := range packResults {
		packResult.Complete()
	}

	return packResults
}
