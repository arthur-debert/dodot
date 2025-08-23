package homebrew

import (
	"crypto/sha256"
	_ "embed"
	"fmt"
	"io"
	"os"

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
		checksum, err := calculateBrewfileChecksum(match.AbsolutePath)
		if err != nil {
			logger.Error().
				Err(err).
				Str("path", match.AbsolutePath).
				Msg("Failed to calculate checksum")
			return nil, fmt.Errorf("failed to calculate checksum for %s: %w", match.AbsolutePath, err)
		}

		// Create a RunScriptAction for the Brewfile
		// Note: The executor will need to recognize Brewfiles and run them
		// with "brew bundle --file=<path>" instead of executing directly
		// This is identified by the sentinel name pattern "homebrew-*.sentinel"
		action := &types.RunScriptAction{
			PackName:     match.Pack,
			ScriptPath:   match.AbsolutePath, // Path to the Brewfile
			Checksum:     checksum,
			SentinelName: fmt.Sprintf("homebrew-%s.sentinel", match.Pack),
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

// calculateBrewfileChecksum calculates SHA256 checksum of a Brewfile
func calculateBrewfileChecksum(path string) (string, error) {
	file, err := os.Open(path)
	if err != nil {
		return "", err
	}
	defer func() {
		_ = file.Close()
	}()

	hash := sha256.New()
	if _, err := io.Copy(hash, file); err != nil {
		return "", err
	}

	return fmt.Sprintf("sha256:%x", hash.Sum(nil)), nil
}

// Verify interface compliance
var _ types.Handler = (*HomebrewHandlerV2)(nil)
var _ types.ProvisioningHandlerV2 = (*HomebrewHandlerV2)(nil)
