package powerups

import (
	"fmt"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

const (
	// BrewfilePowerUpName is the unique name for the Brewfile power-up
	BrewfilePowerUpName = "brewfile"
)

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

		// Calculate checksum of the Brewfile
		checksum, err := testutil.CalculateFileChecksum(match.AbsolutePath)
		if err != nil {
			logger.Error().Err(err).
				Str("path", match.AbsolutePath).
				Msg("Failed to calculate Brewfile checksum")
			return nil, fmt.Errorf("failed to calculate checksum for %s: %w", match.Path, err)
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
				"checksum": checksum,
				"pack":     match.Pack,
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

// GetSentinelPath returns the path to the sentinel file for a pack
func GetBrewfileSentinelPath(pack string) string {
	return filepath.Join(types.GetBrewfileDir(), pack)
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
