package internal

import (
	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/handlers/homebrew"
	"github.com/arthur-debert/dodot/pkg/handlers/install"
	pathHandler "github.com/arthur-debert/dodot/pkg/handlers/path"
	"github.com/arthur-debert/dodot/pkg/handlers/shell"
	"github.com/arthur-debert/dodot/pkg/handlers/symlink"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/operations"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
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

	// Phase 3: All handlers are simplified - convert matches to operations
	var allOperations []operations.Operation

	for handlerName, handlerMatches := range matchesByHandler {
		logger.Debug().
			Str("handler", handlerName).
			Int("matches", len(handlerMatches)).
			Msg("Converting matches to operations")

		handler := getSimplifiedHandler(handlerName)
		if handler != nil {
			ops, err := handler.ToOperations(handlerMatches)
			if err != nil {
				return nil, errors.Wrapf(err, errors.ErrInternal,
					"failed to convert matches to operations for handler %s", handlerName)
			}
			allOperations = append(allOperations, ops...)
		}
	}

	// Create execution context
	ctx := types.NewExecutionContext(getCommandFromMode(opts.CommandMode), opts.DryRun)

	// Create datastore and operation executor
	fs := filesystem.NewOS()
	dataStore := datastore.New(fs, pathsInstance)

	// Create confirmer
	confirmer := &simpleConfirmer{}

	// Create operation executor
	// Phase 3: Use DataStore directly, no adapter needed
	executor := operations.NewExecutor(dataStore, fs, confirmer, opts.DryRun)

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

	ctx.Complete()
	return ctx, nil
}

// getSimplifiedHandler returns the simplified handler for the given name.
// Phase 3: All handlers are now simplified.
func getSimplifiedHandler(handlerName string) operations.Handler {
	switch handlerName {
	case operations.HandlerPath:
		return pathHandler.NewSimplifiedHandler()
	case operations.HandlerSymlink:
		return symlink.NewSimplifiedHandler()
	case operations.HandlerShell:
		return shell.NewSimplifiedHandler()
	case operations.HandlerInstall:
		return install.NewSimplifiedHandler()
	case operations.HandlerHomebrew:
		return homebrew.NewSimplifiedHandler()
	default:
		// Unknown handler
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
