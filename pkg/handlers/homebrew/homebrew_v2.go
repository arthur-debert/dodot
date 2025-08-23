package homebrew

import (
	_ "embed"
	"fmt"

	"github.com/arthur-debert/dodot/pkg/internal/hashutil"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
)

// HomebrewHandlerV2 processes Brewfiles to install packages via Homebrew
type HomebrewHandlerV2 struct{}

// NewHomebrewHandlerV2 creates a new instance of the Homebrew handler
func NewHomebrewHandlerV2() *HomebrewHandlerV2 {
	return &HomebrewHandlerV2{}
}

// Name returns the unique name of this handler
func (h *HomebrewHandlerV2) Name() string {
	return HomebrewHandlerName
}

// Description returns a human-readable description of what this handler does
func (h *HomebrewHandlerV2) Description() string {
	return "Processes Brewfiles to install Homebrew packages"
}

// RunMode returns whether this handler runs once or many times
func (h *HomebrewHandlerV2) RunMode() types.RunMode {
	return types.RunModeProvisioning
}

// Process implements the old Handler interface for compatibility
func (h *HomebrewHandlerV2) Process(matches []types.TriggerMatch) ([]types.Action, error) {
	// This method is here for compatibility but should not be used
	return nil, fmt.Errorf("Process method is deprecated, use ProcessProvisioning instead")
}

// ProcessProvisioning takes Brewfile matches and generates RunScriptAction instances
func (h *HomebrewHandlerV2) ProcessProvisioning(matches []types.TriggerMatch) ([]types.ProvisioningAction, error) {
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
func (h *HomebrewHandlerV2) ValidateOptions(options map[string]interface{}) error {
	// Homebrew handler doesn't have any options
	return nil
}

// GetTemplateContent returns the template content for this handler
func (h *HomebrewHandlerV2) GetTemplateContent() string {
	return homebrewTemplate
}

// Verify interface compliance
var _ types.Handler = (*HomebrewHandlerV2)(nil)
var _ types.ProvisioningHandlerV2 = (*HomebrewHandlerV2)(nil)
