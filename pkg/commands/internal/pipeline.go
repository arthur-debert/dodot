package internal

import (
	"os"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/types"
)

// PipelineOptions contains options for running the execution pipeline
type PipelineOptions struct {
	DotfilesRoot       string
	PackNames          []string
	DryRun             bool
	RunMode            types.RunMode
	Force              bool
	EnableHomeSymlinks bool
}

// RunPipeline executes the core pipeline: GetPacks -> GetTriggers -> GetActions -> Execute
// This replaces the old RunExecutionPipeline but works with DirectExecutor instead of Operations
func RunPipeline(opts PipelineOptions) (*types.ExecutionContext, error) {
	logger := logging.GetLogger("commands.internal.pipeline")
	logger.Debug().
		Str("dotfilesRoot", opts.DotfilesRoot).
		Strs("packNames", opts.PackNames).
		Bool("dryRun", opts.DryRun).
		Str("runMode", string(opts.RunMode)).
		Bool("force", opts.Force).
		Msg("Starting execution pipeline")

	// 1. Initialize Paths instance
	pathsInstance, err := paths.New(opts.DotfilesRoot)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrInternal, "failed to initialize paths")
	}

	// 2. Discover and select packs using centralized helper
	selectedPacks, err := core.DiscoverAndSelectPacks(pathsInstance.DotfilesRoot(), opts.PackNames)
	if err != nil {
		// Add context about where we searched for packs
		if dodotErr, ok := err.(*errors.DodotError); ok && dodotErr.Code == errors.ErrPackNotFound {
			// Enhance error with dotfiles root search information
			dodotErr = dodotErr.WithDetail("dotfilesRoot", pathsInstance.DotfilesRoot())
			dodotErr = dodotErr.WithDetail("searchPath", pathsInstance.DotfilesRoot())
			dodotErr = dodotErr.WithDetail("usedFallback", pathsInstance.UsedFallback())

			// Add information about how dotfiles root was determined
			if envRoot := os.Getenv("DOTFILES_ROOT"); envRoot != "" {
				dodotErr = dodotErr.WithDetail("source", "DOTFILES_ROOT environment variable")
			} else if !pathsInstance.UsedFallback() {
				dodotErr = dodotErr.WithDetail("source", "git repository root")
			} else {
				dodotErr = dodotErr.WithDetail("source", "current working directory (fallback)")
			}
			err = dodotErr
		}
		return nil, err
	}

	logger.Debug().
		Int("selectedPacks", len(selectedPacks)).
		Msg("Packs selected")

	// 4. Get firing triggers for the packs
	matches, err := core.GetFiringTriggers(selectedPacks)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrInternal, "failed to get firing triggers")
	}

	logger.Debug().
		Int("triggerMatches", len(matches)).
		Msg("Triggers matched")

	// 5. Generate actions from triggers
	actions, err := core.GetActions(matches)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrInternal, "failed to generate actions")
	}

	logger.Debug().
		Int("totalActions", len(actions)).
		Msg("Actions generated")

	// 5a. Enrich run-once actions with checksums
	// This is needed so the executor can write checksums to sentinel files
	actions = core.EnrichRunOnceActionsWithChecksums(actions)

	// 6. Filter actions by run mode
	filteredActions, err := filterActionsByRunMode(actions, opts.RunMode)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrInternal, "failed to filter actions by run mode")
	}

	logger.Debug().
		Int("filteredActions", len(filteredActions)).
		Str("runMode", string(opts.RunMode)).
		Msg("Actions filtered by run mode")

	// 7. Filter run-once actions based on --force flag
	if opts.RunMode == types.RunModeOnce && !opts.Force {
		filteredActions, err = core.FilterRunOnceActions(filteredActions, opts.Force, pathsInstance)
		if err != nil {
			return nil, errors.Wrapf(err, errors.ErrInternal, "failed to filter run-once actions")
		}
		logger.Debug().
			Int("actionsAfterRunOnce", len(filteredActions)).
			Msg("Run-once actions filtered")
	}

	// 8. Create execution context
	ctx := types.NewExecutionContext(getCommandFromRunMode(opts.RunMode), opts.DryRun)

	// 9. If dry run, we still need to create pack results structure
	if opts.DryRun {
		logger.Info().Msg("Dry run mode - creating planned results")
		// Group actions by pack and create pack results
		packResultsMap := groupActionsByPack(filteredActions, selectedPacks)
		for packName, packResult := range packResultsMap {
			ctx.AddPackResult(packName, packResult)
		}
		ctx.Complete()
		return ctx, nil
	}

	// 10. Create and configure DirectExecutor
	executorOpts := &core.DirectExecutorOptions{
		Paths:             pathsInstance,
		DryRun:            opts.DryRun,
		Force:             opts.Force,
		AllowHomeSymlinks: opts.EnableHomeSymlinks,
		Config:            config.Default(),
	}

	executor := core.NewDirectExecutor(executorOpts)

	// 11. Execute actions
	logger.Info().
		Int("actionCount", len(filteredActions)).
		Msg("Executing actions")

	results, err := executor.ExecuteActions(filteredActions)
	if err != nil {
		// Still return context with partial results
		if len(results) > 0 {
			packResultsMap := convertActionResultsToPackResults(results, selectedPacks)
			for packName, packResult := range packResultsMap {
				ctx.AddPackResult(packName, packResult)
			}
		}
		ctx.Complete()
		return ctx, errors.Wrapf(err, errors.ErrActionExecute, "failed to execute actions")
	}

	// 12. Process results into execution context
	packResultsMap := convertActionResultsToPackResults(results, selectedPacks)
	for packName, packResult := range packResultsMap {
		ctx.AddPackResult(packName, packResult)
	}

	logger.Info().
		Int("totalResults", len(results)).
		Int("packsProcessed", len(selectedPacks)).
		Msg("Pipeline execution completed")

	ctx.Complete()
	return ctx, nil
}

// filterActionsByRunMode filters actions based on the RunMode of their power-ups
func filterActionsByRunMode(actions []types.Action, mode types.RunMode) ([]types.Action, error) {
	logger := logging.GetLogger("commands.internal.pipeline")
	var filtered []types.Action

	for _, action := range actions {
		// Get the power-up factory to check its run mode
		factory, err := registry.GetHandlerFactory(action.HandlerName)
		if err != nil {
			logger.Warn().
				Str("powerUp", action.HandlerName).
				Err(err).
				Msg("Failed to get power-up factory, including action anyway")
			// Include the action if we can't determine its run mode
			filtered = append(filtered, action)
			continue
		}

		// Create a temporary instance to check RunMode
		powerUp, err := factory(nil)
		if err != nil {
			logger.Warn().
				Str("powerUp", action.HandlerName).
				Err(err).
				Msg("Failed to create power-up instance, including action anyway")
			filtered = append(filtered, action)
			continue
		}
		powerUpMode := powerUp.RunMode()

		// Include action if it matches the requested mode
		if powerUpMode == mode {
			filtered = append(filtered, action)
		}
	}

	logger.Debug().
		Int("input", len(actions)).
		Int("output", len(filtered)).
		Str("mode", string(mode)).
		Msg("Filtered actions by run mode")

	return filtered, nil
}

// getCommandFromRunMode returns the command name based on run mode
func getCommandFromRunMode(mode types.RunMode) string {
	switch mode {
	case types.RunModeOnce:
		return "install"
	case types.RunModeMany:
		return "deploy"
	default:
		return "execute"
	}
}

// groupActionsByPack groups actions by pack for dry run display
func groupActionsByPack(actions []types.Action, packs []types.Pack) map[string]*types.PackExecutionResult {
	// Create pack map for easy lookup
	packMap := make(map[string]*types.Pack)
	for i := range packs {
		packMap[packs[i].Name] = &packs[i]
	}

	packResults := make(map[string]*types.PackExecutionResult)

	// Group actions by pack and power-up
	for _, action := range actions {
		packName := action.Pack
		if packName == "" {
			packName = "unknown"
		}

		// Get or create pack result
		packResult, exists := packResults[packName]
		if !exists {
			pack := packMap[packName]
			if pack == nil {
				// Create minimal pack for unknown
				pack = &types.Pack{Name: packName}
			}
			packResult = types.NewPackExecutionResult(pack)
			packResults[packName] = packResult
		}

		// Find or create HandlerResult
		var powerUpResult *types.HandlerResult
		for _, pur := range packResult.HandlerResults {
			if pur.HandlerName == action.HandlerName {
				powerUpResult = pur
				break
			}
		}
		if powerUpResult == nil {
			powerUpResult = &types.HandlerResult{
				HandlerName: action.HandlerName,
				Files:       []string{},
				Status:      types.StatusReady, // Planned status for dry run
			}
			packResult.HandlerResults = append(packResult.HandlerResults, powerUpResult)
			packResult.TotalHandlers++
		}

		// Add file to power-up if source is specified
		if action.Source != "" {
			powerUpResult.Files = append(powerUpResult.Files, action.Source)
		}
	}

	// Complete all pack results
	for _, packResult := range packResults {
		packResult.Complete()
		// For dry run, all are "ready" to execute
		packResult.CompletedHandlers = packResult.TotalHandlers
		packResult.Status = types.ExecutionStatusSuccess
	}

	return packResults
}

// convertActionResultsToPackResults converts action results to pack execution results
func convertActionResultsToPackResults(results []types.ActionResult, packs []types.Pack) map[string]*types.PackExecutionResult {
	// Create pack map for easy lookup
	packMap := make(map[string]*types.Pack)
	for i := range packs {
		packMap[packs[i].Name] = &packs[i]
	}

	packResults := make(map[string]*types.PackExecutionResult)

	// Group results by pack and power-up
	for _, result := range results {
		packName := result.Action.Pack
		if packName == "" {
			packName = "unknown"
		}

		// Get or create pack result
		packResult, exists := packResults[packName]
		if !exists {
			pack := packMap[packName]
			if pack == nil {
				// Create minimal pack for unknown
				pack = &types.Pack{Name: packName}
			}
			packResult = types.NewPackExecutionResult(pack)
			packResults[packName] = packResult
		}

		// Find or create HandlerResult
		var powerUpResult *types.HandlerResult
		for _, pur := range packResult.HandlerResults {
			if pur.HandlerName == result.Action.HandlerName {
				powerUpResult = pur
				break
			}
		}
		if powerUpResult == nil {
			powerUpResult = &types.HandlerResult{
				HandlerName: result.Action.HandlerName,
				Files:       []string{},
				Status:      result.Status,
				Error:       result.Error,
				Actions:     []types.Action{result.Action},
			}
			packResult.HandlerResults = append(packResult.HandlerResults, powerUpResult)
			packResult.TotalHandlers++

			// Update counts based on status
			switch result.Status {
			case types.StatusReady:
				packResult.CompletedHandlers++
			case types.StatusError:
				packResult.FailedHandlers++
			case types.StatusSkipped:
				packResult.SkippedHandlers++
			}
		} else {
			// Update existing power-up result if this one has error
			if result.Status == types.StatusError && powerUpResult.Status != types.StatusError {
				powerUpResult.Status = types.StatusError
				powerUpResult.Error = result.Error
				packResult.FailedHandlers++
				if packResult.CompletedHandlers > 0 {
					packResult.CompletedHandlers--
				}
			}
		}

		// Add file to power-up if source is specified
		if result.Action.Source != "" {
			powerUpResult.Files = append(powerUpResult.Files, result.Action.Source)
		}

		// Add action to power-up
		powerUpResult.Actions = append(powerUpResult.Actions, result.Action)
	}

	// Complete all pack results and determine status
	for _, packResult := range packResults {
		packResult.Complete()

		// Determine pack status based on power-up results
		if packResult.FailedHandlers > 0 {
			if packResult.CompletedHandlers > 0 {
				packResult.Status = types.ExecutionStatusPartial
			} else {
				packResult.Status = types.ExecutionStatusError
			}
		} else if packResult.SkippedHandlers == packResult.TotalHandlers {
			packResult.Status = types.ExecutionStatusSkipped
		} else {
			packResult.Status = types.ExecutionStatusSuccess
		}
	}

	return packResults
}
