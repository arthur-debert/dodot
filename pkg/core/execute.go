package core

import (
	"fmt"
	"strings"

	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/handlers"
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

	// Provisioning control options
	SkipProvisioning    bool // Skip all provisioning handlers (for --no-provision)
	ForceReprovisioning bool // Force re-run provisioning even if already done (for --provision-rerun)
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

		// Apply provisioning options
		if opts.SkipProvisioning {
			// Filter out all code execution handlers
			filteredMatches = filterOutProvisioningHandlers(filteredMatches)
			logger.Debug().Msg("Skipping provisioning handlers due to SkipProvisioning option")
		}
	}

	logger.Debug().
		Int("filteredMatches", len(filteredMatches)).
		Str("commandType", string(commandType)).
		Msg("Matches filtered by command type")

	// Step 4: Create DataStore
	dataStore := datastore.New(fs, pathsInstance)

	// Check for already provisioned handlers and handle accordingly
	var skippedPacks []string
	if !opts.ForceReprovisioning && commandType == CommandProvision && !opts.SkipProvisioning {
		// Check which provisioning handlers have already been run
		filteredMatches, skippedPacks = filterAlreadyProvisionedMatches(filteredMatches, selectedPacks, dataStore)
	}

	// If ForceReprovisioning is set, clear provisioning handler state first
	if opts.ForceReprovisioning && commandType == CommandProvision {
		if err := clearProvisioningState(selectedPacks, dataStore); err != nil {
			logger.Error().Err(err).Msg("Failed to clear provisioning state")
			return nil, err
		}
	}

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

	// Ensure all packs have results in the execution context, even if handlers were skipped
	for _, pack := range selectedPacks {
		if _, exists := ctx.GetPackResult(pack.Name); !exists {
			// Create an empty pack result for packs with no executed handlers
			packResult := types.NewPackExecutionResult(&pack)
			packResult.Status = types.ExecutionStatusSkipped
			packResult.Complete()
			ctx.AddPackResult(pack.Name, packResult)
		}
	}

	// Add provisioning skip message if applicable
	if len(skippedPacks) > 0 {
		if ctx.Messages == nil {
			ctx.Messages = []string{}
		}
		ctx.Messages = append(ctx.Messages, buildProvisioningSkipMessage(skippedPacks))
	}

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

// filterOutProvisioningHandlers removes all code execution handlers from matches
func filterOutProvisioningHandlers(matches []types.RuleMatch) []types.RuleMatch {
	var filtered []types.RuleMatch
	for _, match := range matches {
		// Keep only configuration handlers
		if handlers.HandlerRegistry.IsConfigurationHandler(match.HandlerName) {
			filtered = append(filtered, match)
		}
	}
	return filtered
}

// filterAlreadyProvisionedMatches filters out matches for handlers that have already been provisioned
func filterAlreadyProvisionedMatches(matches []types.RuleMatch, packs []types.Pack, store types.DataStore) ([]types.RuleMatch, []string) {
	logger := logging.GetLogger("core.execute")
	var filtered []types.RuleMatch
	skippedPacksMap := make(map[string]bool)

	// Create a map of pack names to Pack objects for easy lookup
	packMap := make(map[string]*types.Pack)
	for i := range packs {
		packMap[packs[i].Name] = &packs[i]
	}

	for _, match := range matches {
		// Only check provisioning state for code execution handlers
		if handlers.HandlerRegistry.IsCodeExecutionHandler(match.HandlerName) {
			// Find the pack object
			pack, exists := packMap[match.Pack]
			if !exists {
				logger.Warn().
					Str("pack", match.Pack).
					Str("handler", match.HandlerName).
					Msg("Pack not found in pack list, including handler")
				filtered = append(filtered, match)
				continue
			}

			isProvisioned, err := pack.IsHandlerProvisioned(store, match.HandlerName)
			if err != nil {
				logger.Warn().Err(err).
					Str("pack", match.Pack).
					Str("handler", match.HandlerName).
					Msg("Failed to check provisioning state, including handler")
				filtered = append(filtered, match)
				continue
			}

			if isProvisioned {
				logger.Debug().
					Str("pack", match.Pack).
					Str("handler", match.HandlerName).
					Msg("Skipping already provisioned handler")
				skippedPacksMap[match.Pack] = true
				continue
			}
		}

		// Include the match if it's not provisioned or is a configuration handler
		filtered = append(filtered, match)
	}

	// Convert map keys to slice
	var skippedPacks []string
	for pack := range skippedPacksMap {
		skippedPacks = append(skippedPacks, pack)
	}

	return filtered, skippedPacks
}

// clearProvisioningState removes provisioning handler state for the given packs
func clearProvisioningState(packs []types.Pack, store types.DataStore) error {
	logger := logging.GetLogger("core.execute")

	for i := range packs {
		pack := &packs[i]
		// Get all code execution handlers
		for _, handlerName := range handlers.HandlerRegistry.GetCodeExecutionHandlers() {
			// Check if handler has state before trying to remove it
			hasState, err := store.HasHandlerState(pack.Name, handlerName)
			if err != nil {
				logger.Warn().Err(err).
					Str("pack", pack.Name).
					Str("handler", handlerName).
					Msg("Failed to check handler state")
				continue
			}

			if hasState {
				logger.Debug().
					Str("pack", pack.Name).
					Str("handler", handlerName).
					Msg("Removing provisioning handler state")

				if err := store.RemoveState(pack.Name, handlerName); err != nil {
					return err
				}
			}
		}
	}

	return nil
}

// buildProvisioningSkipMessage creates a user-friendly message about skipped provisioning
func buildProvisioningSkipMessage(skippedPacks []string) string {
	if len(skippedPacks) == 0 {
		return ""
	}

	if len(skippedPacks) == 1 {
		return fmt.Sprintf("Pack %s has already been provisioned. To re-run provisioning, use --provision-rerun", skippedPacks[0])
	}

	return fmt.Sprintf("Packs %s have already been provisioned. To re-run provisioning, use --provision-rerun", strings.Join(skippedPacks, ", "))
}
