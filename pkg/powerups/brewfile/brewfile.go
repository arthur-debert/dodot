package brewfile

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
	// BrewfilePowerUpName is the unique name for the Brewfile power-up
	BrewfilePowerUpName = "brewfile"
)

//go:embed brewfile-template.txt
var brewfileTemplate string

// BrewfilePowerUp processes Brewfiles to install packages via Homebrew
type BrewfilePowerUp struct{}

// NewBrewfilePowerUp creates a new instance of the Brewfile power-up
func NewBrewfilePowerUp() types.PowerUp {
	return &BrewfilePowerUp{}
}

// Name returns the unique name of this power-up
func (p *BrewfilePowerUp) Name() string {
	return BrewfilePowerUpName
}

// Description returns a human-readable description of what this power-up does
func (p *BrewfilePowerUp) Description() string {
	return "Processes Brewfiles to install Homebrew packages"
}

// RunMode returns whether this power-up runs once or many times
func (p *BrewfilePowerUp) RunMode() types.RunMode {
	return types.RunModeOnce
}

// Process takes Brewfile matches and generates brew actions
func (p *BrewfilePowerUp) Process(matches []types.TriggerMatch) ([]types.Action, error) {
	logger := logging.GetLogger("powerups.brewfile")
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
func (p *BrewfilePowerUp) ValidateOptions(options map[string]interface{}) error {
	// Brewfile power-up doesn't have any options
	return nil
}

// GetTemplateContent returns the template content for this power-up
func (p *BrewfilePowerUp) GetTemplateContent() string {
	return brewfileTemplate
}

// GetSentinelPath returns the path to the sentinel file for a pack
func GetBrewfileSentinelPath(pack string) string {
	return filepath.Join(paths.GetBrewfileDir(), pack)
}

func init() {
	// Register factory in the global registry
	RegisterBrewfilePowerUpFactory()
}

// RegisterBrewfilePowerUpFactory registers the Brewfile power-up factory
func RegisterBrewfilePowerUpFactory() {
	err := registry.RegisterPowerUpFactory(BrewfilePowerUpName, func(config map[string]interface{}) (types.PowerUp, error) {
		return NewBrewfilePowerUp(), nil
	})
	if err != nil {
		panic(fmt.Sprintf("failed to register %s power-up: %v", BrewfilePowerUpName, err))
	}
}
