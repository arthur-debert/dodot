package internal

import (
	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/types"
)

// ExecutionOptions is an internal struct to pass to the pipeline runner.
type ExecutionOptions struct {
	DotfilesRoot string
	PackNames    []string
	DryRun       bool
	RunMode      types.RunMode
	Force        bool
}

// RunExecutionPipeline is the core logic for deploy and install.
func RunExecutionPipeline(opts ExecutionOptions) (*types.ExecutionResult, error) {
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
		filteredActions, err = core.FilterRunOnceActions(filteredActions, opts.Force)
		if err != nil {
			return nil, err
		}
	}

	// 7. Create execution context
	ctx := core.NewExecutionContext(opts.Force, pathsInstance)

	// 8. Convert actions to operations (PLANNING PHASE)
	// This converts high-level actions into low-level operations
	// No actual file system changes happen at this stage
	var ops []types.Operation
	if opts.DryRun {
		// For dry run, convert actions to operations for display
		initialOps, err := core.ConvertActionsToOperationsWithContext(filteredActions, ctx)
		if err != nil {
			return nil, err
		}
		ops = initialOps
	} else {
		// For actual execution, convert actions to operations
		// Note: This is still just planning - execution happens later
		if opts.RunMode == types.RunModeOnce {
			// For RunModeOnce, operations will include checkpoint/sentinel files
			ops, err = core.ConvertActionsToOperationsWithContext(filteredActions, ctx)
		} else {
			// For RunModeMany, convert without checkpoint files
			ops, err = core.ConvertActionsToOperationsWithContext(filteredActions, ctx)
		}
		if err != nil {
			return nil, err
		}
	}

	// 9. Construct and return the result
	// Note: At this point we have PLANNED operations but have NOT EXECUTED them
	// Execution happens in the command handlers using executors
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

// getPackNames returns a list of pack names
func getPackNames(packs []types.Pack) []string {
	names := make([]string, len(packs))
	for i, pack := range packs {
		names[i] = pack.Name
	}
	return names
}
