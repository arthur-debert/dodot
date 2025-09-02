// Package handlerpipeline provides a focused pipeline for executing handlers on a single pack.
// It encapsulates the flow: match files → filter handlers → create operations → execute.
package pipeline

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/errors"
	exec "github.com/arthur-debert/dodot/pkg/execution"
	"github.com/arthur-debert/dodot/pkg/execution/context"
	execresults "github.com/arthur-debert/dodot/pkg/execution/results"
	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/handlers/lib/homebrew"
	"github.com/arthur-debert/dodot/pkg/handlers/lib/install"
	"github.com/arthur-debert/dodot/pkg/handlers/lib/path"
	"github.com/arthur-debert/dodot/pkg/handlers/lib/shell"
	"github.com/arthur-debert/dodot/pkg/handlers/lib/symlink"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/operations"
	"github.com/arthur-debert/dodot/pkg/types"
)

// FilterType determines which handlers to execute
type FilterType int

const (
	// ConfigOnly executes only configuration handlers (symlink, shell, path)
	ConfigOnly FilterType = iota
	// ProvisionOnly executes only code execution handlers (homebrew, install)
	ProvisionOnly
	// All executes all handlers
	All
)

// Options contains execution options for the handler pipeline
type Options struct {
	DryRun     bool
	Force      bool
	FileSystem types.FS
	DataStore  datastore.DataStore
}

// Result contains the execution results for a single pack
type Result struct {
	Pack             types.Pack
	TotalHandlers    int
	SuccessCount     int
	FailureCount     int
	SkippedCount     int
	ExecutedHandlers []HandlerExecution
}

// HandlerExecution represents the result of executing a single handler
type HandlerExecution struct {
	HandlerName    string
	OperationCount int
	Success        bool
	Error          error
}

// ExecuteHandlersForPack executes the handler pipeline for a single pack.
// This is the minimal starting point that orchestrates existing code.
func ExecuteHandlersForPack(pack types.Pack, filter FilterType, opts Options) (*Result, error) {
	logger := logging.GetLogger("handlerpipeline")
	logger.Debug().
		Str("pack", pack.Name).
		Str("filter", filterTypeString(filter)).
		Bool("dryRun", opts.DryRun).
		Bool("force", opts.Force).
		Msg("Starting handler pipeline for pack")

	// Step 1: Get matches for this pack
	matches, err := getMatchesForPack(pack, opts.FileSystem)
	if err != nil {
		logger.Error().Err(err).Str("pack", pack.Name).Msg("Failed to get matches")
		return nil, err
	}

	// Step 2: Filter matches based on filter type
	filteredMatches := filterMatches(matches, filter)
	logger.Debug().
		Int("totalMatches", len(matches)).
		Int("filteredMatches", len(filteredMatches)).
		Msg("Filtered matches")

	// Step 3: Execute matches internally
	ctx, err := executeMatchesInternal(filteredMatches, opts)
	if err != nil {
		logger.Error().Err(err).Str("pack", pack.Name).Msg("Failed to execute matches")
		return buildResultFromContext(pack, ctx), err
	}

	// Step 4: Build result from execution context
	result := buildResultFromContext(pack, ctx)

	logger.Info().
		Str("pack", pack.Name).
		Int("totalHandlers", result.TotalHandlers).
		Int("success", result.SuccessCount).
		Int("failed", result.FailureCount).
		Int("skipped", result.SkippedCount).
		Msg("Handler pipeline completed")

	return result, nil
}

// getMatchesForPack gets rule matches for a single pack
func getMatchesForPack(pack types.Pack, fs types.FS) ([]RuleMatch, error) {
	// Call our own GetMatchesFS directly
	packs := []types.Pack{pack}
	return GetMatchesFS(packs, fs)
}

// filterMatches filters matches based on the filter type
func filterMatches(matches []RuleMatch, filter FilterType) []RuleMatch {
	switch filter {
	case ConfigOnly:
		return FilterMatchesByHandlerCategory(matches, true, false)
	case ProvisionOnly:
		return FilterMatchesByHandlerCategory(matches, false, true)
	case All:
		return matches
	default:
		return matches
	}
}

// FilterMatchesByHandlerCategory filters rule matches based on handler category
func FilterMatchesByHandlerCategory(matches []RuleMatch, allowConfiguration, allowCodeExecution bool) []RuleMatch {
	var filtered []RuleMatch

	for _, match := range matches {
		// Check if handler is configuration type
		if allowConfiguration && handlers.HandlerRegistry.IsConfigurationHandler(match.HandlerName) {
			filtered = append(filtered, match)
		}
		// Check if handler is code execution type
		if allowCodeExecution && handlers.HandlerRegistry.IsCodeExecutionHandler(match.HandlerName) {
			filtered = append(filtered, match)
		}
	}

	return filtered
}

// buildResultFromContext converts execution context to our result type
func buildResultFromContext(pack types.Pack, ctx *types.ExecutionContext) *Result {
	result := &Result{
		Pack:             pack,
		ExecutedHandlers: make([]HandlerExecution, 0),
	}

	if ctx == nil {
		return result
	}

	// Extract pack-specific results
	if packResult, exists := ctx.GetPackResult(pack.Name); exists {
		result.TotalHandlers = len(packResult.HandlerResults)

		for _, hr := range packResult.HandlerResults {
			execution := HandlerExecution{
				HandlerName:    hr.HandlerName,
				OperationCount: len(hr.Files),
				Success:        hr.Status == exec.StatusReady,
				Error:          hr.Error,
			}

			result.ExecutedHandlers = append(result.ExecutedHandlers, execution)

			switch hr.Status {
			case exec.StatusReady:
				result.SuccessCount++
			case exec.StatusError, exec.StatusConflict:
				result.FailureCount++
			case exec.StatusSkipped:
				result.SkippedCount++
			}
		}
	}

	return result
}

// filterTypeString returns a string representation of the filter type
func filterTypeString(filter FilterType) string {
	switch filter {
	case ConfigOnly:
		return "ConfigOnly"
	case ProvisionOnly:
		return "ProvisionOnly"
	case All:
		return "All"
	default:
		return "Unknown"
	}
}

// transformMatches converts RuleMatch to FileInput for handlers
func transformMatches(matches []RuleMatch) []operations.FileInput {
	inputs := make([]operations.FileInput, len(matches))
	for i, match := range matches {
		inputs[i] = operations.FileInput{
			PackName:     match.Pack,
			SourcePath:   match.AbsolutePath,
			RelativePath: match.Path,
			Options:      match.HandlerOptions,
		}
	}
	return inputs
}

// executeMatchesInternal handles the internal execution of matches
func executeMatchesInternal(matches []RuleMatch, opts Options) (*types.ExecutionContext, error) {
	logger := logging.GetLogger("handlerpipeline.execute")
	logger.Debug().
		Int("matches", len(matches)).
		Bool("dryRun", opts.DryRun).
		Bool("force", opts.Force).
		Msg("Executing matches internally")

	// Create execution context using manager
	ctxManager := context.NewManager()
	ctx := ctxManager.CreateContext("execute", opts.DryRun)

	// Group matches by handler type
	groupedMatches := groupMatchesByHandler(matches)
	if len(groupedMatches) == 0 {
		logger.Info().Msg("No matches to execute")
		ctxManager.CompleteContext(ctx)
		return ctx, nil
	}

	// Get execution order (code execution handlers before configuration handlers)
	handlerNames := getHandlerExecutionOrder(getHandlerNames(groupedMatches))
	logger.Debug().
		Strs("executionOrder", handlerNames).
		Msg("Handler execution order determined")

	// Create operations executor
	executor := operations.NewExecutor(opts.DataStore, opts.FileSystem, opts.DryRun)

	// Execute each handler in order
	for _, handlerName := range handlerNames {
		handlerMatches := groupedMatches[handlerName]
		logger.Debug().
			Str("handler", handlerName).
			Int("matchCount", len(handlerMatches)).
			Msg("Executing handler")

		// Create handler instance
		handler, err := createOperationsHandler(handlerName)
		if err != nil {
			logger.Error().
				Err(err).
				Str("handler", handlerName).
				Msg("Failed to create handler")
			return ctx, errors.Wrapf(err, errors.ErrInternal,
				"failed to create handler %s", handlerName)
		}

		// Convert matches to file inputs for the handler
		fileInputs := transformMatches(handlerMatches)

		// Convert file inputs to operations
		ops, err := handler.ToOperations(fileInputs)
		if err != nil {
			logger.Error().
				Err(err).
				Str("handler", handlerName).
				Msg("Failed to convert matches to operations")
			return ctx, errors.Wrapf(err, errors.ErrInternal,
				"failed to convert matches to operations for handler %s", handlerName)
		}

		if len(ops) == 0 {
			logger.Debug().
				Str("handler", handlerName).
				Msg("No operations generated, skipping")
			continue
		}

		// Execute operations
		results, err := executor.Execute(ops, handler)
		if err != nil {
			logger.Error().
				Err(err).
				Str("handler", handlerName).
				Msg("Handler execution failed")
			return ctx, errors.Wrapf(err, errors.ErrOperationExecute,
				"failed to execute operations for handler %s", handlerName)
		}

		// Add results to execution context
		addOperationResultsToContext(ctx, results, handlerMatches, ctxManager)

		logger.Info().
			Str("handler", handlerName).
			Int("operationCount", len(ops)).
			Int("successCount", countSuccessfulResults(results)).
			Msg("Handler execution completed")
	}

	ctxManager.CompleteContext(ctx)
	logger.Info().
		Int("totalHandlers", ctx.TotalHandlers).
		Int("completedHandlers", ctx.CompletedHandlers).
		Int("failedHandlers", ctx.FailedHandlers).
		Msg("Handler pipeline execution completed")

	return ctx, nil
}

// groupMatchesByHandler groups rule matches by their handler name
func groupMatchesByHandler(matches []RuleMatch) map[string][]RuleMatch {
	grouped := make(map[string][]RuleMatch)
	for _, match := range matches {
		grouped[match.HandlerName] = append(grouped[match.HandlerName], match)
	}
	return grouped
}

// getHandlerNames extracts handler names from grouped matches
func getHandlerNames(grouped map[string][]RuleMatch) []string {
	names := make([]string, 0, len(grouped))
	for name := range grouped {
		names = append(names, name)
	}
	return names
}

// getHandlerExecutionOrder determines the order to execute handlers
// Code execution handlers run before configuration handlers
func getHandlerExecutionOrder(handlerNames []string) []string {
	if len(handlerNames) == 0 {
		return []string{}
	}

	var codeExecution []string
	var configuration []string

	for _, name := range handlerNames {
		if handlers.HandlerRegistry.IsCodeExecutionHandler(name) {
			codeExecution = append(codeExecution, name)
		} else {
			configuration = append(configuration, name)
		}
	}

	// Code execution handlers first, then configuration handlers
	return append(codeExecution, configuration...)
}

// addOperationResultsToContext adds operation results to the execution context
func addOperationResultsToContext(ctx *types.ExecutionContext, results []operations.OperationResult, matches []RuleMatch, ctxManager *context.Manager) {
	if len(results) == 0 || len(matches) == 0 {
		return
	}

	// Group results by pack
	packResults := make(map[string]*types.HandlerResult)

	for i, result := range results {
		if i >= len(matches) {
			break
		}
		match := matches[i]

		// Get or create handler result for this pack
		key := match.Pack + ":" + match.HandlerName
		if _, exists := packResults[key]; !exists {
			packResults[key] = &types.HandlerResult{
				HandlerName: match.HandlerName,
				Pack:        match.Pack,
				Files:       []string{},
				Status:      exec.StatusReady,
			}
		}

		// Add file to handler result
		packResults[key].Files = append(packResults[key].Files, match.Path)

		// Update status based on operation result
		if result.Error != nil {
			packResults[key].Status = exec.StatusError
			packResults[key].Error = result.Error
		} else if !result.Success {
			// If not successful but no error, treat as skipped
			if packResults[key].Status == exec.StatusReady {
				packResults[key].Status = exec.StatusSkipped
			}
		}
	}

	// Create results aggregator
	aggregator := execresults.NewAggregator()

	// Add all handler results to context
	for _, handlerResult := range packResults {
		// Find or create pack result
		var packResult *types.PackExecutionResult
		if pr, exists := ctx.GetPackResult(handlerResult.Pack); exists {
			packResult = pr
		} else {
			// Create minimal pack for result
			pack := &types.Pack{Name: handlerResult.Pack}
			packResult = aggregator.CreatePackResult(pack)
			ctxManager.AddPackResult(ctx, handlerResult.Pack, packResult)
		}

		// Add handler result to pack
		aggregator.AddHandlerResult(packResult, handlerResult)
	}
}

// countSuccessfulResults counts the number of successful operation results
func countSuccessfulResults(results []operations.OperationResult) int {
	count := 0
	for _, result := range results {
		if result.Success {
			count++
		}
	}
	return count
}

// createOperationsHandler creates an operations.Handler instance by name
func createOperationsHandler(name string) (operations.Handler, error) {
	switch name {
	case "symlink":
		return symlink.NewHandler(), nil
	case "shell":
		return shell.NewHandler(), nil
	case "homebrew":
		return homebrew.NewHandler(), nil
	case "install":
		return install.NewHandler(), nil
	case "path":
		return path.NewHandler(), nil
	default:
		return nil, fmt.Errorf("unknown handler: %s", name)
	}
}
