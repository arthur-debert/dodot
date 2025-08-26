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
	matches, err := core.GetMatches(selectedPacks)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrInternal, "failed to get firing triggers")
	}

	logger.Debug().
		Int("triggerMatches", len(matches)).
		Msg("Triggers matched")

	// 5. Generate actions and confirmations from triggers
	actionResult, err := core.GetActionsWithConfirmations(matches)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrInternal, "failed to generate actions")
	}

	logger.Debug().
		Int("totalActions", len(actionResult.Actions)).
		Int("totalConfirmations", len(actionResult.Confirmations)).
		Msg("Actions and confirmations generated")

	// 6. Handle confirmations if present
	var confirmationContext *types.ConfirmationContext
	if actionResult.HasConfirmations() {
		logger.Info().
			Int("confirmationCount", len(actionResult.Confirmations)).
			Msg("Confirmation requests found - presenting to user")

		// Use console confirmation dialog
		dialog := core.NewConsoleConfirmationDialog()

		// Collect confirmations using utility function
		confirmationContext, err = core.CollectAndProcessConfirmations(actionResult.Confirmations, dialog)
		if err != nil {
			return nil, errors.Wrapf(err, errors.ErrInternal, "failed to collect confirmations")
		}

		// Check if user cancelled (no confirmations approved)
		if confirmationContext != nil && !confirmationContext.AllApproved(getConfirmationIDs(actionResult.Confirmations)) {
			logger.Info().Msg("User declined confirmations - cancelling execution")
			// Return empty context to indicate cancellation
			ctx := types.NewExecutionContext(getCommandFromRunMode(opts.RunMode), opts.DryRun)
			ctx.Complete()
			return ctx, nil
		}

		logger.Info().Msg("All confirmations approved - proceeding with execution")
	}

	// Use the generated actions
	actions := actionResult.Actions

	// 7. Create datastore for the new executor
	fs := filesystem.NewOS()
	dataStore := datastore.New(fs, pathsInstance)

	// 8. Filter actions by run mode
	filteredActions := core.FilterActionsByRunMode(actions, opts.RunMode)

	logger.Debug().
		Int("filteredActions", len(filteredActions)).
		Str("runMode", string(opts.RunMode)).
		Msg("Actions filtered by run mode")

	// 9. Filter provisioning actions based on --force flag
	if opts.RunMode == types.RunModeProvisioning && !opts.Force {
		filteredActions, err = core.FilterProvisioningActions(filteredActions, opts.Force, dataStore)
		if err != nil {
			return nil, errors.Wrapf(err, errors.ErrInternal, "failed to filter provisioning actions")
		}
		logger.Debug().
			Int("actionsAfterProvisioning", len(filteredActions)).
			Msg("Provisioning actions filtered")
	}

	// 10. Create execution context
	ctx := types.NewExecutionContext(getCommandFromRunMode(opts.RunMode), opts.DryRun)

	// 11. If dry run, we still need to create pack results structure
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

	// 12. Create and configure new Executor
	executorOpts := executor.Options{
		DataStore: dataStore,
		DryRun:    opts.DryRun,
		Logger:    logging.GetLogger("executor"),
	}

	exec := executor.New(executorOpts)

	// 13. Execute actions
	logger.Info().
		Int("actionCount", len(filteredActions)).
		Msg("Executing actions")

	results := exec.Execute(filteredActions)

	// Check if any actions failed
	var hasErrors bool
	for _, result := range results {
		if !result.Success && result.Error != nil {
			hasErrors = true
			break
		}
	}

	// 14. Process results into execution context
	packResultsMap := convertActionResultsToPackResults(results, selectedPacks)
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

// groupActionsByPack groups actions by pack for dry run display
func groupActionsByPack(actions []types.Action, packs []types.Pack) map[string]*types.PackExecutionResult {
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
		handlerName := "handler" // actions use generic handler name
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

// convertActionResultsToPackResults converts action results to pack execution results
func convertActionResultsToPackResults(results []types.ActionResult, packs []types.Pack) map[string]*types.PackExecutionResult {
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
		handlerName := "handler" // actions don't have handler names
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

// getConfirmationIDs extracts the IDs from a list of confirmation requests
func getConfirmationIDs(confirmations []types.ConfirmationRequest) []string {
	ids := make([]string, len(confirmations))
	for i, confirmation := range confirmations {
		ids[i] = confirmation.ID
	}
	return ids
}
