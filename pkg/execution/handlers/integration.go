package handlers

import (
	"fmt"
	"path/filepath"
	"sort"
	"strings"

	"github.com/arthur-debert/dodot/pkg/config"
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
	"github.com/arthur-debert/dodot/pkg/rules"
	"github.com/arthur-debert/dodot/pkg/types"
)

// GetMatches scans packs and returns all rule matches using the rule system
func GetMatches(packs []types.Pack) ([]rules.RuleMatch, error) {
	matcher := rules.NewMatcher()
	return matcher.GetMatches(packs)
}

// GetMatchesFS scans packs and returns all rule matches using the rule system with a custom filesystem
// This function is used for testing and commands that need to use a different filesystem
func GetMatchesFS(packs []types.Pack, fs types.FS) ([]rules.RuleMatch, error) {
	matcher := rules.NewMatcher()
	return matcher.GetMatchesFS(packs, fs)
}

// CreateHandler creates a handler instance by name
// This replaces the registry-based handler creation
func CreateHandler(name string, options map[string]interface{}) (interface{}, error) {
	logger := logging.GetLogger("rules.integration")
	logger.Debug().
		Str("handler", name).
		Interface("options", options).
		Msg("Creating handler")

	switch name {
	case "symlink":
		h := symlink.NewHandler()
		return h, nil
	case "shell":
		h := shell.NewHandler()
		return h, nil
	case "homebrew":
		h := homebrew.NewHandler()
		return h, nil
	case "install":
		h := install.NewHandler()
		return h, nil
	case "path":
		h := path.NewHandler()
		return h, nil
	default:
		return nil, fmt.Errorf("unknown handler: %s", name)
	}
}

// GroupMatchesByHandler groups rule matches by their handler name
func GroupMatchesByHandler(matches []rules.RuleMatch) map[string][]rules.RuleMatch {
	grouped := make(map[string][]rules.RuleMatch)
	for _, match := range matches {
		handler := match.HandlerName
		grouped[handler] = append(grouped[handler], match)
	}
	return grouped
}

// GetHandlerExecutionOrder returns handlers in the order they should be executed
// Code execution handlers run before configuration handlers
func GetHandlerExecutionOrder(handlerNames []string) []string {
	type handlerInfo struct {
		name     string
		category operations.HandlerCategory
	}

	var handlerList []handlerInfo
	for _, name := range handlerNames {
		// Validate handler exists
		_, err := CreateHandler(name, nil)
		if err != nil {
			continue
		}

		handlerList = append(handlerList, handlerInfo{
			name:     name,
			category: handlers.HandlerRegistry.GetHandlerCategory(name),
		})
	}

	// Sort: code execution handlers first, then configuration handlers
	sort.Slice(handlerList, func(i, j int) bool {
		if handlerList[i].category == handlerList[j].category {
			return handlerList[i].name < handlerList[j].name // alphabetical within same category
		}
		// Code execution comes before configuration
		return handlerList[i].category == operations.CategoryCodeExecution
	})

	// Extract sorted names
	sorted := make([]string, len(handlerList))
	for i, h := range handlerList {
		sorted[i] = h.name
	}

	return sorted
}

// GetPatternsForHandler returns all patterns that would activate a specific handler
// This includes patterns from both global and pack-specific rules
func GetPatternsForHandler(handlerName string, pack types.Pack) ([]string, error) {
	logger := logging.GetLogger("rules.integration")
	logger.Debug().
		Str("handler", handlerName).
		Str("pack", pack.Name).
		Msg("Getting patterns for handler")

	// Load global rules
	globalRules := config.GetRules()
	if len(globalRules) == 0 {
		globalRules = rules.GetDefaultRules()
	}

	// Load pack-specific rules
	packRules, err := rules.LoadPackRules(pack.Path)
	if err != nil {
		logger.Debug().
			Err(err).
			Str("pack", pack.Name).
			Msg("No pack rules found, using global rules only")
	}

	// Merge rules (pack rules have higher priority)
	effectiveRules := rules.MergeRules(globalRules, packRules)

	// Find all patterns that map to this handler
	patterns := []string{}
	seen := make(map[string]bool)

	for _, rule := range effectiveRules {
		// Skip exclusion patterns
		if strings.HasPrefix(rule.Pattern, "!") {
			continue
		}

		if rule.Handler == handlerName {
			// Avoid duplicates
			if !seen[rule.Pattern] {
				patterns = append(patterns, rule.Pattern)
				seen[rule.Pattern] = true
			}
		}
	}

	logger.Debug().
		Str("handler", handlerName).
		Int("patternCount", len(patterns)).
		Strs("patterns", patterns).
		Msg("Found patterns for handler")

	return patterns, nil
}

// GetAllHandlerPatterns returns patterns for all available handlers in a pack
// Returns a map of handler name to list of patterns
func GetAllHandlerPatterns(pack types.Pack) (map[string][]string, error) {
	logger := logging.GetLogger("rules.integration")
	logger.Debug().Str("pack", pack.Name).Msg("Getting all handler patterns")

	// Get all known handlers
	handlerNames := []string{"symlink", "shell", "homebrew", "install", "path"}

	result := make(map[string][]string)
	for _, handlerName := range handlerNames {
		patterns, err := GetPatternsForHandler(handlerName, pack)
		if err != nil {
			return nil, errors.Wrapf(err, errors.ErrInternal,
				"failed to get patterns for handler %s", handlerName)
		}
		if len(patterns) > 0 {
			result[handlerName] = patterns
		}
	}

	return result, nil
}

// SuggestFilenameForHandler suggests a filename that would match the handler's patterns
// For glob patterns, it returns a reasonable example filename
func SuggestFilenameForHandler(handlerName string, patterns []string) string {
	// If no patterns, return empty
	if len(patterns) == 0 {
		return ""
	}

	// Try to find the most specific pattern (non-glob)
	for _, pattern := range patterns {
		// Skip directory patterns
		if strings.HasSuffix(pattern, "/") {
			continue
		}

		// If it's not a glob pattern, use it directly
		if !strings.ContainsAny(pattern, "*?[]") {
			return pattern
		}
	}

	// Handle common glob patterns
	for _, pattern := range patterns {
		// Skip directory patterns
		if strings.HasSuffix(pattern, "/") {
			continue
		}

		switch {
		case strings.HasPrefix(pattern, "*") && strings.Count(pattern, "*") == 1:
			// Pattern like "*aliases.sh" -> "aliases.sh"
			return strings.TrimPrefix(pattern, "*")
		case strings.HasSuffix(pattern, "*") && strings.Count(pattern, "*") == 1:
			// Pattern like "profile*" -> "profile.sh"
			base := strings.TrimSuffix(pattern, "*")
			if handlerName == "shell" {
				return base + ".sh"
			}
			return base
		}
	}

	// Handler-specific defaults
	switch handlerName {
	case "shell":
		return "shell.sh"
	case "install":
		return "install.sh"
	case "homebrew":
		return "Brewfile"
	case "path":
		return "bin/"
	case "symlink":
		// For symlink (catchall), don't suggest a filename
		return ""
	default:
		return ""
	}
}

// GetHandlersNeedingFiles returns handlers that have no matching files in the pack
// It uses the existing matching system to determine which handlers are already active
// Optionally accepts a filesystem parameter for testing
func GetHandlersNeedingFiles(pack types.Pack, optionalFS ...types.FS) ([]string, error) {
	logger := logging.GetLogger("rules.integration")
	logger.Debug().Str("pack", pack.Name).Msg("Getting handlers needing files")

	// Use provided filesystem or nil (which will use default)
	var fs types.FS
	if len(optionalFS) > 0 {
		fs = optionalFS[0]
	}

	// Get all matches for this pack
	matches, err := GetMatchesFS([]types.Pack{pack}, fs)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrInternal,
			"failed to get matches for pack %s", pack.Name)
	}

	// Track which handlers already have files
	activeHandlers := make(map[string]bool)
	for _, match := range matches {
		// Skip symlink handler as it's a catchall
		if match.HandlerName == "symlink" {
			continue
		}
		activeHandlers[match.HandlerName] = true
	}

	// Get all known handlers except symlink
	allHandlers := []string{"shell", "homebrew", "install", "path"}

	// Find handlers that need files
	var needingFiles []string
	for _, handler := range allHandlers {
		if !activeHandlers[handler] {
			needingFiles = append(needingFiles, handler)
		}
	}

	logger.Debug().
		Str("pack", pack.Name).
		Strs("handlers", needingFiles).
		Msg("Handlers needing files")

	return needingFiles, nil
}

// ExecutionOptions contains options for executing rule matches
type ExecutionOptions struct {
	DryRun     bool
	Force      bool
	FileSystem types.FS
	Config     interface{} // Pack-specific config
}

// ExecuteMatches executes rule matches using handlers and the DataStore abstraction.
// This is the core execution function that replaces the internal pipeline approach.
func ExecuteMatches(matches []rules.RuleMatch, dataStore datastore.DataStore, opts ExecutionOptions) (*context.ExecutionContext, error) {
	logger := logging.GetLogger("rules.integration")
	logger.Info().
		Int("matches", len(matches)).
		Bool("dryRun", opts.DryRun).
		Bool("force", opts.Force).
		Msg("Executing rule matches")

	// Create execution context using manager
	ctxManager := context.NewManager()
	ctx := ctxManager.CreateContext("execute", opts.DryRun)

	// Group matches by handler type
	groupedMatches := GroupMatchesByHandler(matches)
	if len(groupedMatches) == 0 {
		logger.Info().Msg("No matches to execute")
		ctxManager.CompleteContext(ctx)
		return ctx, nil
	}

	// Get execution order (code execution handlers before configuration handlers)
	handlerNames := GetHandlerExecutionOrder(getHandlerNames(groupedMatches))
	logger.Debug().
		Strs("executionOrder", handlerNames).
		Msg("Handler execution order determined")

	// Create operations executor
	executor := operations.NewExecutor(dataStore, opts.FileSystem, opts.DryRun)

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

		// Transform matches to file inputs
		fileInputs := transformMatches(handlerMatches)

		// Convert file inputs to operations
		operations, err := handler.ToOperations(fileInputs, opts.Config)
		if err != nil {
			logger.Error().
				Err(err).
				Str("handler", handlerName).
				Msg("Failed to convert matches to operations")
			return ctx, errors.Wrapf(err, errors.ErrInternal,
				"failed to convert matches to operations for handler %s", handlerName)
		}

		if len(operations) == 0 {
			logger.Debug().
				Str("handler", handlerName).
				Msg("No operations generated, skipping")
			continue
		}

		// Execute operations
		results, err := executor.Execute(operations, handler)
		if err != nil {
			logger.Error().
				Err(err).
				Str("handler", handlerName).
				Msg("Handler execution failed")
			return ctx, errors.Wrapf(err, errors.ErrOperationExecute,
				"failed to execute operations for handler %s", handlerName)
		}

		// Add results to execution context
		addOperationResultsToExecutionContext(ctx, results, handlerMatches, ctxManager)

		logger.Info().
			Str("handler", handlerName).
			Int("operationCount", len(operations)).
			Int("successCount", countSuccessfulResults(results)).
			Msg("Handler execution completed")
	}

	ctxManager.CompleteContext(ctx)
	logger.Info().
		Int("totalHandlers", ctx.TotalHandlers).
		Int("completedHandlers", ctx.CompletedHandlers).
		Int("failedHandlers", ctx.FailedHandlers).
		Msg("Rule match execution completed")

	return ctx, nil
}

// addOperationResultsToExecutionContext converts operation results to execution context data
func addOperationResultsToExecutionContext(ctx *context.ExecutionContext, results []operations.OperationResult, matches []rules.RuleMatch, ctxManager *context.Manager) {
	// Create results aggregator
	aggregator := execresults.NewAggregator()

	// Group results by pack
	resultsByPack := make(map[string][]operations.OperationResult)
	for _, result := range results {
		pack := result.Operation.Pack
		resultsByPack[pack] = append(resultsByPack[pack], result)
	}

	// Convert to pack results
	for packName, packResults := range resultsByPack {
		// Create handler result
		handlerResult := &context.HandlerResult{
			HandlerName: packResults[0].Operation.Handler,
			Files:       make([]string, 0, len(packResults)),
			Status:      exec.StatusReady,
		}

		// Add files and check for errors
		hasErrors := false
		for _, result := range packResults {
			if result.Operation.Source != "" {
				handlerResult.Files = append(handlerResult.Files, result.Operation.Source)
			}
			if !result.Success {
				hasErrors = true
				handlerResult.Error = result.Error
			}
		}

		if hasErrors {
			handlerResult.Status = exec.StatusError
		}

		// Create or update pack result
		packResult, exists := ctx.GetPackResult(packName)
		if !exists {
			// Find pack from matches
			var pack *types.Pack
			for _, match := range matches {
				if match.Pack == packName {
					pack = &types.Pack{Name: packName, Path: filepath.Dir(match.AbsolutePath)}
					break
				}
			}
			if pack == nil {
				pack = &types.Pack{Name: packName, Path: ""}
			}

			packResult = aggregator.CreatePackResult(pack)
			ctxManager.AddPackResult(ctx, packName, packResult)
		}

		aggregator.AddHandlerResult(packResult, handlerResult)
	}
}
