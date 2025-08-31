package core

import (
	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/operations"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/rules"
	"github.com/arthur-debert/dodot/pkg/types"
)

// CommandType represents the type of command being executed
type CommandType string

const (
	// CommandLink runs only configuration handlers (symlinks, shell, path)
	CommandLink CommandType = "link"
	// CommandProvision runs all handlers (both configuration and code execution)
	CommandProvision CommandType = "provision"
	// CommandUnlink removes configuration handler state
	CommandUnlink CommandType = "unlink"
	// CommandDeprovision removes code execution handler state
	CommandDeprovision CommandType = "deprovision"
)

// ExecuteOptions contains options for executing commands via the core execution flow
type ExecuteOptions struct {
	DotfilesRoot string
	PackNames    []string
	DryRun       bool
	Force        bool
	Confirmer    operations.Confirmer
	FileSystem   types.FS
}

// Execute runs the unified execution flow for any command type.
// This replaces the internal pipeline approach with a clean architecture:
// core.Execute() → rules.ExecuteMatches() → handlers → DataStore
func Execute(commandType CommandType, opts ExecuteOptions) (*types.ExecutionContext, error) {
	logger := logging.GetLogger("core.execute")
	logger.Info().
		Str("commandType", string(commandType)).
		Str("dotfilesRoot", opts.DotfilesRoot).
		Strs("packNames", opts.PackNames).
		Bool("dryRun", opts.DryRun).
		Bool("force", opts.Force).
		Msg("Starting core execution")

	// Initialize paths
	pathsInstance, err := paths.New(opts.DotfilesRoot)
	if err != nil {
		return nil, err
	}

	// Use provided filesystem or default to OS
	fs := opts.FileSystem
	if fs == nil {
		fs = filesystem.NewOS()
	}

	// Step 1: Discover and select packs
	selectedPacks, err := DiscoverAndSelectPacksFS(pathsInstance.DotfilesRoot(), opts.PackNames, fs)
	if err != nil {
		logger.Error().Err(err).Msg("Failed to discover packs")
		return nil, err
	}

	logger.Debug().
		Int("packCount", len(selectedPacks)).
		Msg("Packs discovered and selected")

	// Step 2: Get matches using rules system (skip for unlink/deprovision)
	var filteredMatches []types.RuleMatch
	if commandType == CommandUnlink || commandType == CommandDeprovision {
		// Unlink and deprovision use datastore.RemoveState() directly
		// They don't need to process rule matches
		logger.Debug().Msg("Skipping rule matching for unlink/deprovision command")
		filteredMatches = []types.RuleMatch{}
	} else {
		matches, err := GetMatchesFS(selectedPacks, fs)
		if err != nil {
			logger.Error().Err(err).Msg("Failed to get matches")
			return nil, err
		}

		logger.Debug().
			Int("matchCount", len(matches)).
			Msg("Rule matches generated")

		// Step 3: Filter matches based on command type
		filteredMatches = filterMatchesByCommandType(matches, commandType)
	}

	logger.Debug().
		Int("filteredMatches", len(filteredMatches)).
		Str("commandType", string(commandType)).
		Msg("Matches filtered by command type")

	// Step 4: Create DataStore
	dataStore := datastore.New(fs, pathsInstance)

	// Step 5: Execute matches using rules system
	rulesOpts := rules.ExecutionOptions{
		DryRun:     opts.DryRun,
		Force:      opts.Force,
		Confirmer:  opts.Confirmer,
		FileSystem: fs,
	}

	ctx, err := rules.ExecuteMatches(filteredMatches, dataStore, rulesOpts)
	if err != nil {
		logger.Error().Err(err).Msg("Failed to execute matches")
		return ctx, err
	}

	// Update context command name to reflect the actual command
	ctx.Command = string(commandType)

	logger.Info().
		Str("commandType", string(commandType)).
		Int("totalHandlers", ctx.TotalHandlers).
		Int("completedHandlers", ctx.CompletedHandlers).
		Int("failedHandlers", ctx.FailedHandlers).
		Msg("Core execution completed")

	return ctx, nil
}

// filterMatchesByCommandType filters rule matches based on the command type
func filterMatchesByCommandType(matches []types.RuleMatch, commandType CommandType) []types.RuleMatch {
	switch commandType {
	case CommandLink:
		// Link only runs configuration handlers
		return FilterMatchesByHandlerCategory(matches, true, false)
	case CommandProvision:
		// Provision runs all handlers
		return FilterMatchesByHandlerCategory(matches, true, true)
	case CommandUnlink, CommandDeprovision:
		// Unlink and deprovision are handled differently - they use datastore.RemoveState()
		// These command types shouldn't use this execution flow
		return []types.RuleMatch{}
	default:
		// Unknown command type - run all handlers as fallback
		return FilterMatchesByHandlerCategory(matches, true, true)
	}
}
