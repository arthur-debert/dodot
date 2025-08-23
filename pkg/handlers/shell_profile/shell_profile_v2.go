package shell_profile

import (
	_ "embed"
	"fmt"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
)

// ShellProfileHandlerV2 manages shell profile modifications
type ShellProfileHandlerV2 struct{}

// NewShellProfileHandlerV2 creates a new instance of the ShellProfileHandlerV2
func NewShellProfileHandlerV2() *ShellProfileHandlerV2 {
	return &ShellProfileHandlerV2{}
}

// Name returns the unique name of this handler
func (h *ShellProfileHandlerV2) Name() string {
	return ShellProfileHandlerName
}

// Description returns a human-readable description of what this handler does
func (h *ShellProfileHandlerV2) Description() string {
	return "Manages shell profile modifications (e.g., sourcing aliases)"
}

// RunMode returns when this handler should run
func (h *ShellProfileHandlerV2) RunMode() types.RunMode {
	return types.RunModeLinking
}

// Process implements the old Handler interface for compatibility
func (h *ShellProfileHandlerV2) Process(matches []types.TriggerMatch) ([]types.Action, error) {
	// This method is here for compatibility but should not be used
	return nil, fmt.Errorf("Process method is deprecated, use ProcessLinking instead")
}

// ProcessLinking takes shell script files and creates AddToShellProfileAction instances
func (h *ShellProfileHandlerV2) ProcessLinking(matches []types.TriggerMatch) ([]types.LinkingAction, error) {
	logger := logging.GetLogger("handlers.shell_profile.v2")
	actions := make([]types.LinkingAction, 0, len(matches))

	for _, match := range matches {
		logger.Debug().
			Str("path", match.Path).
			Str("pack", match.Pack).
			Msg("Processing shell profile file")

		// Create AddToShellProfileAction
		action := &types.AddToShellProfileAction{
			PackName:   match.Pack,
			ScriptPath: match.AbsolutePath,
		}

		actions = append(actions, action)
	}

	logger.Info().
		Int("match_count", len(matches)).
		Int("action_count", len(actions)).
		Msg("processed shell profile matches")

	return actions, nil
}

// ValidateOptions checks if the provided options are valid
func (h *ShellProfileHandlerV2) ValidateOptions(options map[string]interface{}) error {
	return nil // No options to validate yet
}

// GetTemplateContent returns the template content for this handler
func (h *ShellProfileHandlerV2) GetTemplateContent() string {
	return aliasesTemplate
}

// Verify interface compliance
var _ types.Handler = (*ShellProfileHandlerV2)(nil)
var _ types.LinkingHandlerV2 = (*ShellProfileHandlerV2)(nil)
