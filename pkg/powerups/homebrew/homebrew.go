package homebrew

import (
	_ "embed"
	"fmt"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

const (
	// HomebrewPowerUpName is the unique name for the Homebrew power-up
	HomebrewPowerUpName = "homebrew"
)

//go:embed homebrew-template.txt
var homebrewTemplate string

// HomebrewPowerUp processes Brewfiles to install packages via Homebrew
type HomebrewPowerUp struct{}

// NewHomebrewPowerUp creates a new instance of the Homebrew power-up
func NewHomebrewPowerUp() types.PowerUp {
	return &HomebrewPowerUp{}
}

// Name returns the unique name of this power-up
func (p *HomebrewPowerUp) Name() string {
	return HomebrewPowerUpName
}

// Description returns a human-readable description of what this power-up does
func (p *HomebrewPowerUp) Description() string {
	return "Processes Brewfiles to install Homebrew packages"
}

// RunMode returns whether this power-up runs once or many times
func (p *HomebrewPowerUp) RunMode() types.RunMode {
	return types.RunModeOnce
}

// Process takes Brewfile matches and generates brew actions
func (p *HomebrewPowerUp) Process(matches []types.TriggerMatch) ([]types.Action, error) {
	logger := logging.GetLogger("powerups.homebrew")
	actions := make([]types.Action, 0, len(matches))

	for _, match := range matches {
		logger.Debug().
			Str("path", match.Path).
			Str("pack", match.Pack).
			Msg("Processing Brewfile")

		// First, create a checksum action
		checksumAction := types.Action{
			Type:        types.ActionTypeChecksum,
			Description: fmt.Sprintf("Calculate checksum for %s", match.Path),
			Source:      match.AbsolutePath,
			Pack:        match.Pack,
			PowerUpName: p.Name(),
			Priority:    match.Priority + 1, // Higher priority to run first
		}
		actions = append(actions, checksumAction)

		// Calculate checksum now for the metadata
		// This helps with run-once filtering
		checksum, err := testutil.CalculateFileChecksum(match.AbsolutePath)
		if err != nil {
			logger.Warn().
				Err(err).
				Str("path", match.AbsolutePath).
				Msg("Failed to calculate checksum for Brewfile")
			checksum = ""
		}

		action := types.Action{
			Type:        types.ActionTypeBrew,
			Description: fmt.Sprintf("Install packages from %s", match.Path),
			Source:      match.AbsolutePath,
			Target:      "", // Not used for brew
			Pack:        match.Pack,
			PowerUpName: p.Name(),
			Priority:    match.Priority,
			Metadata: map[string]interface{}{
				"pack":     match.Pack,
				"checksum": checksum,
			},
		}

		actions = append(actions, action)
	}

	return actions, nil
}

// ValidateOptions checks if the provided options are valid for this power-up
func (p *HomebrewPowerUp) ValidateOptions(options map[string]interface{}) error {
	// Homebrew power-up doesn't have any options
	return nil
}

// GetTemplateContent returns the template content for this power-up
func (p *HomebrewPowerUp) GetTemplateContent() string {
	return homebrewTemplate
}

// GetSentinelPath returns the path to the sentinel file for a pack
func GetHomebrewSentinelPath(pack string) string {
	return filepath.Join(paths.GetHomebrewDir(), pack)
}

func init() {
	// Register factory in the global registry
	RegisterHomebrewPowerUpFactory()
}

// RegisterHomebrewPowerUpFactory registers the Homebrew power-up factory
func RegisterHomebrewPowerUpFactory() {
	err := registry.RegisterPowerUpFactory(HomebrewPowerUpName, func(config map[string]interface{}) (types.PowerUp, error) {
		return NewHomebrewPowerUp(), nil
	})
	if err != nil {
		panic(fmt.Sprintf("failed to register %s power-up: %v", HomebrewPowerUpName, err))
	}
}
