package provision

import (
	_ "embed"
	"fmt"

	"github.com/arthur-debert/dodot/pkg/internal/hashutil"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
)

// ProvisionScriptHandlerV2 runs install.sh scripts
type ProvisionScriptHandlerV2 struct{}

// NewProvisionScriptHandlerV2 creates a new instance of the install script handler
func NewProvisionScriptHandlerV2() *ProvisionScriptHandlerV2 {
	return &ProvisionScriptHandlerV2{}
}

// Name returns the unique name of this handler
func (h *ProvisionScriptHandlerV2) Name() string {
	return ProvisionScriptHandlerName
}

// Description returns a human-readable description of what this handler does
func (h *ProvisionScriptHandlerV2) Description() string {
	return "Runs install.sh scripts for initial setup"
}

// RunMode returns whether this handler runs once or many times
func (h *ProvisionScriptHandlerV2) RunMode() types.RunMode {
	return types.RunModeProvisioning
}

// Process implements the old Handler interface for compatibility
func (h *ProvisionScriptHandlerV2) Process(matches []types.TriggerMatch) ([]types.Action, error) {
	// This method is here for compatibility but should not be used
	return nil, fmt.Errorf("Process method is deprecated, use ProcessProvisioning instead")
}

// ProcessProvisioning takes install script matches and generates RunScriptAction instances
func (h *ProvisionScriptHandlerV2) ProcessProvisioning(matches []types.TriggerMatch) ([]types.ProvisioningAction, error) {
	logger := logging.GetLogger("handlers.provision.v2")
	actions := make([]types.ProvisioningAction, 0, len(matches))

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
			return nil, fmt.Errorf("failed to calculate checksum for %s: %w", match.AbsolutePath, err)
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

	return actions, nil
}

// ValidateOptions checks if the provided options are valid for this handler
func (h *ProvisionScriptHandlerV2) ValidateOptions(options map[string]interface{}) error {
	// Install script handler doesn't have any options
	return nil
}

// GetTemplateContent returns the template content for this handler
func (h *ProvisionScriptHandlerV2) GetTemplateContent() string {
	return provisionTemplate
}

// Verify interface compliance
var _ types.Handler = (*ProvisionScriptHandlerV2)(nil)
var _ types.ProvisioningHandlerV2 = (*ProvisionScriptHandlerV2)(nil)
