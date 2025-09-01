// Package handlerpipeline provides a focused pipeline for executing handlers on a single pack.
// It encapsulates the flow: match files → filter handlers → create operations → execute.
package handlerpipeline

import (
	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/rules"
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
	DataStore  types.DataStore
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

	// Step 3: Execute matches using existing rules system
	executionOpts := rules.ExecutionOptions{
		DryRun:     opts.DryRun,
		Force:      opts.Force,
		FileSystem: opts.FileSystem,
	}

	ctx, err := rules.ExecuteMatches(filteredMatches, opts.DataStore, executionOpts)
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
func getMatchesForPack(pack types.Pack, fs types.FS) ([]types.RuleMatch, error) {
	// Use existing core function but for a single pack
	packs := []types.Pack{pack}
	return core.GetMatchesFS(packs, fs)
}

// filterMatches filters matches based on the filter type
func filterMatches(matches []types.RuleMatch, filter FilterType) []types.RuleMatch {
	switch filter {
	case ConfigOnly:
		return core.FilterMatchesByHandlerCategory(matches, true, false)
	case ProvisionOnly:
		return core.FilterMatchesByHandlerCategory(matches, false, true)
	case All:
		return matches
	default:
		return matches
	}
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
				Success:        hr.Status == types.StatusReady,
				Error:          hr.Error,
			}

			result.ExecutedHandlers = append(result.ExecutedHandlers, execution)

			switch hr.Status {
			case types.StatusReady:
				result.SuccessCount++
			case types.StatusError, types.StatusConflict:
				result.FailureCount++
			case types.StatusSkipped:
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
