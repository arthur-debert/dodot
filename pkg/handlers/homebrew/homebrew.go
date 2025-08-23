package homebrew

import (
	_ "embed"
	"fmt"

	"github.com/arthur-debert/dodot/pkg/internal/hashutil"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
)

// HomebrewHandlerName is the name of the homebrew handler
const HomebrewHandlerName = "homebrew"

// homebrewTemplate is the template content for Brewfile
const homebrewTemplate = `# Homebrew dependencies for PACK_NAME pack
# 
# This file is processed by 'dodot install PACK_NAME' to install
# packages using Homebrew. Each package is installed once during
# initial deployment. The deployment is tracked by checksum, so
# modifying this file will trigger a re-run.
#
# Safe to keep empty or remove. By keeping it, you can add
# homebrew packages later without redeploying the pack.

# Examples:
# brew "git"
# brew "vim"
# cask "visual-studio-code"`

// HomebrewHandler processes Brewfiles to install packages via Homebrew
type HomebrewHandler struct{}

// NewHomebrewHandler creates a new instance of the Homebrew handler
func NewHomebrewHandler() *HomebrewHandler {
	return &HomebrewHandler{}
}

// Name returns the unique name of this handler
func (h *HomebrewHandler) Name() string {
	return HomebrewHandlerName
}

// Description returns a human-readable description of what this handler does
func (h *HomebrewHandler) Description() string {
	return "Processes Brewfiles to install Homebrew packages"
}

// RunMode returns whether this handler runs once or many times
func (h *HomebrewHandler) RunMode() types.RunMode {
	return types.RunModeProvisioning
}

// ProcessProvisioning takes Brewfile matches and generates RunScriptAction instances
func (h *HomebrewHandler) ProcessProvisioning(matches []types.TriggerMatch) ([]types.ProvisioningAction, error) {
	logger := logging.GetLogger("handlers.homebrew.v2")
	actions := make([]types.ProvisioningAction, 0, len(matches))

	for _, match := range matches {
		logger.Debug().
			Str("path", match.Path).
			Str("pack", match.Pack).
			Msg("Processing Brewfile")

		// Calculate checksum of the Brewfile
		checksum, err := hashutil.CalculateFileChecksum(match.AbsolutePath)
		if err != nil {
			logger.Error().
				Err(err).
				Str("path", match.AbsolutePath).
				Msg("Failed to calculate checksum")
			return nil, fmt.Errorf("failed to calculate checksum for %s: %w", match.AbsolutePath, err)
		}

		// Create a BrewAction for the Brewfile
		action := &types.BrewAction{
			PackName:     match.Pack,
			BrewfilePath: match.AbsolutePath,
			Checksum:     checksum,
		}

		actions = append(actions, action)
	}

	logger.Info().
		Int("match_count", len(matches)).
		Int("action_count", len(actions)).
		Msg("processed Brewfile matches")

	return actions, nil
}

// ValidateOptions checks if the provided options are valid for this handler
func (h *HomebrewHandler) ValidateOptions(options map[string]interface{}) error {
	// Homebrew handler doesn't have any options
	return nil
}

// GetTemplateContent returns the template content for this handler
func (h *HomebrewHandler) GetTemplateContent() string {
	return homebrewTemplate
}

// Verify interface compliance
var _ types.ProvisioningHandler = (*HomebrewHandler)(nil)
