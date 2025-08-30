package internal

import (
	"time"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/executor"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	pathHandler "github.com/arthur-debert/dodot/pkg/handlers/path"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/operations"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/arthur-debert/dodot/pkg/ui/confirmations"
)

// RunPipelineWithOperations runs the pipeline using the new operation system.
// This is used when DODOT_USE_OPERATIONS=true is set.
// It demonstrates how the simplified architecture works in practice.
func RunPipelineWithOperations(opts PipelineOptions) (*types.ExecutionContext, error) {
	logger := logging.GetLogger("commands.internal.pipeline_operations")
	logger.Info().Msg("Using operation-based pipeline")

	// Steps 1-5 are the same as regular pipeline
	pathsInstance, err := paths.New(opts.DotfilesRoot)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrInternal, "failed to initialize paths")
	}

	selectedPacks, err := core.DiscoverAndSelectPacks(pathsInstance.DotfilesRoot(), opts.PackNames)
	if err != nil {
		return nil, err
	}

	// Get matches
	matches, err := core.GetMatches(selectedPacks)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrInternal, "failed to get matches")
	}

	// Filter by command mode
	filteredMatches := matches
	if opts.CommandMode == CommandModeConfiguration {
		filteredMatches = core.FilterMatchesByHandlerCategory(matches, true, false)
	}

	// Group matches by handler for operation conversion
	matchesByHandler := core.GroupMatchesByHandler(filteredMatches)

	// Convert matches to operations using simplified handlers where available
	var allOperations []operations.Operation
	var fallbackActions []types.Action

	for handlerName, handlerMatches := range matchesByHandler {
		if operations.IsHandlerSimplified(handlerName) {
			// Use simplified handler to generate operations
			logger.Debug().
				Str("handler", handlerName).
				Int("matches", len(handlerMatches)).
				Msg("Using simplified handler")

			handler := getSimplifiedHandler(handlerName)
			if handler != nil {
				ops, err := handler.ToOperations(handlerMatches)
				if err != nil {
					return nil, errors.Wrapf(err, errors.ErrInternal,
						"failed to convert matches to operations for handler %s", handlerName)
				}
				allOperations = append(allOperations, ops...)
			}
		} else {
			// Fall back to regular action generation for non-migrated handlers
			logger.Debug().
				Str("handler", handlerName).
				Int("matches", len(handlerMatches)).
				Msg("Using legacy handler")

			actionResult, err := core.GetActionsWithConfirmations(handlerMatches)
			if err != nil {
				return nil, errors.Wrapf(err, errors.ErrInternal,
					"failed to generate actions for handler %s", handlerName)
			}

			// Handle confirmations if needed
			if actionResult.HasConfirmations() {
				dialog := confirmations.NewConsoleDialog()
				confirmCtx, err := core.CollectAndProcessConfirmations(actionResult.Confirmations, dialog)
				if err != nil {
					return nil, err
				}
				if !confirmCtx.AllApproved(getConfirmationIDs(actionResult.Confirmations)) {
					// User cancelled
					ctx := types.NewExecutionContext(getCommandFromMode(opts.CommandMode), opts.DryRun)
					ctx.Complete()
					return ctx, nil
				}
			}

			fallbackActions = append(fallbackActions, actionResult.Actions...)
		}
	}

	// Create execution context
	ctx := types.NewExecutionContext(getCommandFromMode(opts.CommandMode), opts.DryRun)

	// If we have operations, execute them
	if len(allOperations) > 0 {
		// Create datastore and operation executor
		fs := filesystem.NewOS()
		dataStore := datastore.New(fs, pathsInstance)

		// Create operation datastore adapter
		opDataStore := operations.NewDataStoreAdapter(dataStore, fs)

		// Create confirmer
		// For phase 1, we'll use a simple confirmer that always approves
		confirmer := &simpleConfirmer{}

		// Create operation executor
		executor := operations.NewExecutor(opDataStore, fs, confirmer, opts.DryRun)

		// Execute operations grouped by handler
		for handlerName, handlerOps := range groupOperationsByHandler(allOperations) {
			handler := getSimplifiedHandler(handlerName)
			if handler != nil {
				results, err := executor.Execute(handlerOps, handler)
				if err != nil {
					ctx.Complete()
					return ctx, err
				}

				// Convert operation results to pack results
				addOperationResultsToContext(ctx, results, selectedPacks)
			}
		}
	}

	// Execute fallback actions using regular executor
	if len(fallbackActions) > 0 {
		fs := filesystem.NewOS()
		dataStore := datastore.New(fs, pathsInstance)

		// Filter provisioning if needed
		if opts.CommandMode == CommandModeAll && !opts.Force {
			fallbackActions, err = core.FilterProvisioningActions(fallbackActions, opts.Force, dataStore)
			if err != nil {
				return nil, err
			}
		}

		// Execute with regular executor
		exec := executor.New(executor.Options{
			DataStore: dataStore,
			DryRun:    opts.DryRun,
			Logger:    logging.GetLogger("executor"),
		})

		results := exec.Execute(fallbackActions)

		// Add results to context
		packResultsMap := convertActionResultsToPackResults(results, selectedPacks)
		for packName, packResult := range packResultsMap {
			ctx.AddPackResult(packName, packResult)
		}

		// Check for errors
		for _, result := range results {
			if !result.Success && result.Error != nil {
				ctx.Complete()
				return ctx, errors.New(errors.ErrActionExecute, "some actions failed during execution")
			}
		}
	}

	ctx.Complete()
	return ctx, nil
}

// getSimplifiedHandler returns the simplified handler for the given name.
// This is where we instantiate simplified handlers during phase 1.
func getSimplifiedHandler(handlerName string) operations.Handler {
	switch handlerName {
	case "path":
		return pathHandler.NewSimplifiedHandler()
	default:
		// Other handlers not yet migrated
		return nil
	}
}

// groupOperationsByHandler groups operations by their handler name.
func groupOperationsByHandler(ops []operations.Operation) map[string][]operations.Operation {
	grouped := make(map[string][]operations.Operation)
	for _, op := range ops {
		grouped[op.Handler] = append(grouped[op.Handler], op)
	}
	return grouped
}

// addOperationResultsToContext converts operation results to pack results.
func addOperationResultsToContext(ctx *types.ExecutionContext, results []operations.OperationResult, packs []types.Pack) {
	// Group results by pack
	handlerResultsByPack := make(map[string][]*types.HandlerResult)

	for _, result := range results {
		packName := result.Operation.Pack
		handlerName := result.Operation.Handler

		// Find or create handler result
		var handlerResult *types.HandlerResult
		for _, hr := range handlerResultsByPack[packName] {
			if hr.HandlerName == handlerName {
				handlerResult = hr
				break
			}
		}

		if handlerResult == nil {
			handlerResult = &types.HandlerResult{
				HandlerName: handlerName,
				Files:       []string{},
				Status:      types.StatusReady,
			}
			handlerResultsByPack[packName] = append(handlerResultsByPack[packName], handlerResult)
		}

		// Add file to handler result
		if result.Operation.Source != "" {
			handlerResult.Files = append(handlerResult.Files, result.Operation.Source)
		}

		// Update status based on result
		if !result.Success {
			handlerResult.Status = types.StatusError
			handlerResult.Error = result.Error
		}
	}

	// Create pack execution results
	for _, pack := range packs {
		if handlerResults, ok := handlerResultsByPack[pack.Name]; ok {
			packResult := &types.PackExecutionResult{
				Pack:           &pack,
				HandlerResults: handlerResults,
				Status:         types.ExecutionStatusSuccess,
				StartTime:      time.Now(),
				EndTime:        time.Now(),
				TotalHandlers:  len(handlerResults),
			}

			// Count completed/failed handlers
			for _, hr := range handlerResults {
				switch hr.Status {
				case types.StatusReady:
					packResult.CompletedHandlers++
				case types.StatusError:
					packResult.FailedHandlers++
					packResult.Status = types.ExecutionStatusPartial
				}
			}

			ctx.AddPackResult(pack.Name, packResult)
		}
	}
}

// simpleConfirmer is a basic confirmer for phase 1 testing.
type simpleConfirmer struct{}

func (s *simpleConfirmer) RequestConfirmation(id, title, description string, items ...string) bool {
	// For phase 1, always approve
	// In phase 2, we'll integrate with the real confirmation system
	return true
}
