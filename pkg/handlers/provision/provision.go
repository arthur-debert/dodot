package provision

import (
	_ "embed"
	"fmt"
	"os"
	"strings"

	"github.com/arthur-debert/dodot/pkg/internal/hashutil"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
)

// ProvisionScriptHandlerName is the name of the provision handler
const ProvisionScriptHandlerName = "provision"

// provisionTemplate is the template content for install.sh
const provisionTemplate = `#!/usr/bin/env bash
# dodot install script for PACK_NAME pack
# 
# This script runs ONCE during 'dodot install PACK_NAME'
# Use it for one-time setup tasks like:
# - Installing dependencies not available via Homebrew
# - Creating directories
# - Downloading external resources
# - Setting up initial configurations
#
# The script is idempotent - dodot tracks execution via checksums
# and won't run it again unless the file contents change.
#
# Safe to keep empty or remove. By keeping it, you can add
# installation steps later without redeploying the pack.

set -euo pipefail

echo "Installing PACK_NAME pack..."

# Add your installation commands below
# Examples:
# mkdir -p "$HOME/.config/PACK_NAME"
# curl -fsSL https://example.com/install.sh | bash
`

// ProvisionScriptHandler runs install.sh scripts
type ProvisionScriptHandler struct{}

// NewProvisionScriptHandler creates a new instance of the install script handler
func NewProvisionScriptHandler() *ProvisionScriptHandler {
	return &ProvisionScriptHandler{}
}

// Name returns the unique name of this handler
func (h *ProvisionScriptHandler) Name() string {
	return ProvisionScriptHandlerName
}

// Description returns a human-readable description of what this handler does
func (h *ProvisionScriptHandler) Description() string {
	return "Runs install.sh scripts for initial setup"
}

// RunMode returns whether this handler runs once or many times
func (h *ProvisionScriptHandler) RunMode() types.RunMode {
	return types.RunModeProvisioning
}

// ProcessProvisioning takes install script matches and generates RunScriptAction instances
func (h *ProvisionScriptHandler) ProcessProvisioning(matches []types.TriggerMatch) ([]types.ProvisioningAction, error) {
	result, err := h.ProcessProvisioningWithConfirmations(matches)
	if err != nil {
		return nil, err
	}

	// Convert ProcessingResult actions to ProvisioningAction slice for backward compatibility
	provisioningActions := make([]types.ProvisioningAction, 0, len(result.Actions))
	for _, action := range result.Actions {
		if provAction, ok := action.(types.ProvisioningAction); ok {
			provisioningActions = append(provisioningActions, provAction)
		}
	}

	return provisioningActions, nil
}

// ProcessProvisioningWithConfirmations implements ProvisioningHandlerWithConfirmations
func (h *ProvisionScriptHandler) ProcessProvisioningWithConfirmations(matches []types.TriggerMatch) (types.ProcessingResult, error) {
	logger := logging.GetLogger("handlers.provision")
	actions := make([]types.Action, 0, len(matches))

	for _, match := range matches {
		logger.Debug().
			Str("path", match.Path).
			Str("pack", match.Pack).
			Msg("Processing install script")

		// Calculate checksum of the script
		checksum, err := hashutil.CalculateFileChecksum(match.AbsolutePath)
		if err != nil {
			logger.Error().
				Err(err).
				Str("path", match.AbsolutePath).
				Msg("Failed to calculate checksum")
			return types.ProcessingResult{}, fmt.Errorf("failed to calculate checksum for %s: %w", match.AbsolutePath, err)
		}

		// Create RunScriptAction
		action := &types.RunScriptAction{
			PackName:     match.Pack,
			ScriptPath:   match.AbsolutePath,
			Checksum:     checksum,
			SentinelName: fmt.Sprintf("%s.sentinel", match.Path),
		}

		actions = append(actions, action)
	}

	logger.Info().
		Int("match_count", len(matches)).
		Int("action_count", len(actions)).
		Msg("processed provisioning script matches")

	// Provision scripts don't need confirmation - they're just running install scripts
	// Confirmation is only needed for clearing/uninstalling
	return types.NewProcessingResult(actions), nil
}

// ValidateOptions checks if the provided options are valid for this handler
func (h *ProvisionScriptHandler) ValidateOptions(options map[string]interface{}) error {
	// Install script handler doesn't have any options
	return nil
}

// GetTemplateContent returns the template content for this handler
func (h *ProvisionScriptHandler) GetTemplateContent() string {
	return provisionTemplate
}

// Clear prepares for provision cleanup (reads state, future: runs uninstall.sh)
func (h *ProvisionScriptHandler) Clear(ctx types.ClearContext) ([]types.ClearedItem, error) {
	logger := logging.GetLogger("handlers.provision").With().
		Str("pack", ctx.Pack.Name).
		Bool("dryRun", ctx.DryRun).
		Logger()

	clearedItems := []types.ClearedItem{}

	// Read state to understand what was provisioned
	stateDir := ctx.Paths.PackHandlerDir(ctx.Pack.Name, "provision")
	entries, err := ctx.FS.ReadDir(stateDir)
	if err != nil {
		if os.IsNotExist(err) {
			logger.Debug().Msg("No provision state directory")
			return clearedItems, nil
		}
		return nil, fmt.Errorf("failed to read provision state: %w", err)
	}

	// Find run records to understand what scripts were executed
	scriptRuns := make(map[string][]string) // script name -> run timestamps
	for _, entry := range entries {
		if entry.IsDir() || !strings.HasPrefix(entry.Name(), "run-") {
			continue
		}

		// Extract script name from run record
		// Format: run-<timestamp>-<hash> or similar
		// For now, just record that we found run records
		logger.Info().
			Str("runRecord", entry.Name()).
			Msg("Found provision run record")

		// TODO: Parse the run record to extract which script was run
		scriptName := "install.sh" // Default assumption
		scriptRuns[scriptName] = append(scriptRuns[scriptName], entry.Name())
	}

	// TODO: In a future release:
	// 1. Check if uninstall.sh exists in the pack
	// 2. If it exists, prompt user: "Run uninstall.sh for this pack?"
	// 3. Execute uninstall.sh if confirmed
	// 4. Return list of what was uninstalled

	for scriptName, runs := range scriptRuns {
		runCount := len(runs)
		if ctx.DryRun {
			clearedItems = append(clearedItems, types.ClearedItem{
				Type:        "provision_state",
				Path:        stateDir,
				Description: fmt.Sprintf("Would remove %d run record(s) for %s (uninstall.sh not implemented)", runCount, scriptName),
			})
		} else {
			clearedItems = append(clearedItems, types.ClearedItem{
				Type:        "provision_state",
				Path:        stateDir,
				Description: fmt.Sprintf("Removing %d run record(s) for %s (uninstall.sh check not implemented)", runCount, scriptName),
			})
		}
	}

	if len(clearedItems) == 0 && len(entries) > 0 {
		// Had entries but no run records
		if ctx.DryRun {
			clearedItems = append(clearedItems, types.ClearedItem{
				Type:        "provision_state",
				Path:        stateDir,
				Description: "Would remove provision state directory",
			})
		} else {
			clearedItems = append(clearedItems, types.ClearedItem{
				Type:        "provision_state",
				Path:        stateDir,
				Description: "Removing provision state directory",
			})
		}
	}

	return clearedItems, nil
}

// Verify interface compliance
var _ types.ProvisioningHandler = (*ProvisionScriptHandler)(nil)
var _ types.ProvisioningHandlerWithConfirmations = (*ProvisionScriptHandler)(nil)
var _ types.Clearable = (*ProvisionScriptHandler)(nil)
