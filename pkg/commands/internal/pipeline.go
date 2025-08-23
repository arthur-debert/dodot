package internal

import (
	"os"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/executor"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
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

	// 5. Generate V2 actions from triggers
	actionsV2, err := core.GetActions(matches)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrInternal, "failed to generate V2 actions")
	}

	logger.Debug().
		Int("totalActions", len(actionsV2)).
		Msg("V2 Actions generated")

	// 6. Create datastore for the new executor
	fs := filesystem.NewOS()
	dataStore := datastore.New(fs, pathsInstance)

	// 7. Filter actions by run mode
	filteredActions := core.FilterActionsByRunMode(actionsV2, opts.RunMode)

	logger.Debug().
		Int("filteredActions", len(filteredActions)).
		Str("runMode", string(opts.RunMode)).
		Msg("Actions filtered by run mode")

	// 8. Filter provisioning actions based on --force flag
	if opts.RunMode == types.RunModeProvisioning && !opts.Force {
		filteredActions, err = core.FilterProvisioningActions(filteredActions, opts.Force, dataStore)
		if err != nil {
			return nil, errors.Wrapf(err, errors.ErrInternal, "failed to filter provisioning actions")
		}
		logger.Debug().
			Int("actionsAfterProvisioning", len(filteredActions)).
			Msg("Provisioning actions filtered")
	}

	// 9. Create execution context
	ctx := types.NewExecutionContext(getCommandFromRunMode(opts.RunMode), opts.DryRun)

	// 10. If dry run, we still need to create pack results structure
	if opts.DryRun {
		logger.Info().Msg("Dry run mode - creating planned results")
		// Group actions by pack and create pack results
		packResultsMap := groupActionsByPackV2(filteredActions, selectedPacks)
		for packName, packResult := range packResultsMap {
			ctx.AddPackResult(packName, packResult)
		}
		ctx.Complete()
		return ctx, nil
	}

	// 11. Create and configure new V2 Executor
	executorOpts := executor.Options{
		DataStore: dataStore,
		DryRun:    opts.DryRun,
		Logger:    logging.GetLogger("executor"),
	}

	exec := executor.New(executorOpts)

	// 12. Execute V2 actions
	logger.Info().
		Int("actionCount", len(filteredActions)).
		Msg("Executing V2 actions")

	results := exec.Execute(filteredActions)

	// Check if any actions failed
	var hasErrors bool
	for _, result := range results {
		if !result.Success && result.Error != nil {
			hasErrors = true
			break
		}
	}

	// 13. Process results into execution context
	packResultsMap := convertActionResultsToPackResultsV2(results, selectedPacks)
	for packName, packResult := range packResultsMap {
		ctx.AddPackResult(packName, packResult)
	}

	// Return error if any actions failed
	if hasErrors {
		ctx.Complete()
		return ctx, errors.New(errors.ErrActionExecute, "some actions failed during execution")
	}

	logger.Info().
		Int("totalResults", len(results)).
		Int("packsProcessed", len(selectedPacks)).
		Msg("Pipeline execution completed")

	ctx.Complete()
	return ctx, nil
}

// getCommandFromRunMode returns the command name based on run mode
func getCommandFromRunMode(mode types.RunMode) string {
	switch mode {
	case types.RunModeProvisioning:
		return "provision"
	case types.RunModeLinking:
		return "link"
	default:
		return "execute"
	}
}

// groupActionsByPackV2 groups V2 actions by pack for dry run display
func groupActionsByPackV2(actions []types.ActionV2, packs []types.Pack) map[string]*types.PackExecutionResult {
	// Create pack map for easy lookup
	packMap := make(map[string]*types.Pack)
	for i := range packs {
		packMap[packs[i].Name] = &packs[i]
	}

	packResults := make(map[string]*types.PackExecutionResult)

	// Group actions by pack
	for _, action := range actions {
		packName := action.Pack()
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

		// Create a handler result for dry run
		handlerName := "handler" // V2 actions use generic handler name
		var handlerResult *types.HandlerResult
		for _, pur := range packResult.HandlerResults {
			if pur.HandlerName == handlerName {
				handlerResult = pur
				break
			}
		}
		if handlerResult == nil {
			handlerResult = &types.HandlerResult{
				HandlerName: handlerName,
				Files:       []string{},
				Status:      types.StatusReady,
			}
			packResult.HandlerResults = append(packResult.HandlerResults, handlerResult)
			packResult.TotalHandlers++
		}

		// Add action description as a "file"
		handlerResult.Files = append(handlerResult.Files, action.Description())
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

// convertActionResultsToPackResultsV2 converts V2 action results to pack execution results
func convertActionResultsToPackResultsV2(results []types.ActionResultV2, packs []types.Pack) map[string]*types.PackExecutionResult {
	// Create pack map for easy lookup
	packMap := make(map[string]*types.Pack)
	for i := range packs {
		packMap[packs[i].Name] = &packs[i]
	}

	packResults := make(map[string]*types.PackExecutionResult)

	// Group results by pack
	for _, result := range results {
		packName := result.Action.Pack()
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

		// Create a minimal handler result
		handlerName := "handler" // V2 actions don't have handler names
		var handlerResult *types.HandlerResult
		for _, pur := range packResult.HandlerResults {
			if pur.HandlerName == handlerName {
				handlerResult = pur
				break
			}
		}

		// Determine status from result
		var status types.OperationStatus
		if result.Success {
			if result.Skipped {
				status = types.StatusSkipped
			} else {
				status = types.StatusReady
			}
		} else {
			status = types.StatusError
		}

		if handlerResult == nil {
			handlerResult = &types.HandlerResult{
				HandlerName: handlerName,
				Files:       []string{},
				Status:      status,
				Error:       result.Error,
			}
			packResult.HandlerResults = append(packResult.HandlerResults, handlerResult)
			packResult.TotalHandlers++

			// Update counts based on status
			switch status {
			case types.StatusReady:
				packResult.CompletedHandlers++
			case types.StatusError:
				packResult.FailedHandlers++
			case types.StatusSkipped:
				packResult.SkippedHandlers++
			}
		} else {
			// Update existing handler result if this one has error
			if status == types.StatusError && handlerResult.Status != types.StatusError {
				handlerResult.Status = types.StatusError
				handlerResult.Error = result.Error
				packResult.FailedHandlers++
				if packResult.CompletedHandlers > 0 {
					packResult.CompletedHandlers--
				}
			}
		}

		// Add action description as a "file"
		handlerResult.Files = append(handlerResult.Files, result.Action.Description())
	}

	// Complete all pack results and determine status
	for _, packResult := range packResults {
		packResult.Complete()

		// Determine pack status based on handler results
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
