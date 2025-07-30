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
	// InstallScriptPowerUpName is the unique name for the install script power-up
	InstallScriptPowerUpName = "install_script"
)

// InstallScriptPowerUp runs install.sh scripts
type InstallScriptPowerUp struct{}

// NewInstallScriptPowerUp creates a new instance of the install script power-up
func NewInstallScriptPowerUp() types.PowerUp {
	return &InstallScriptPowerUp{}
}

// Name returns the unique name of this power-up
func (p *InstallScriptPowerUp) Name() string {
	return InstallScriptPowerUpName
}

// Description returns a human-readable description of what this power-up does
func (p *InstallScriptPowerUp) Description() string {
	return "Runs install.sh scripts for initial setup"
}

// RunMode returns whether this power-up runs once or many times
func (p *InstallScriptPowerUp) RunMode() types.RunMode {
	return types.RunModeOnce
}

// Process takes install script matches and generates install actions
func (p *InstallScriptPowerUp) Process(matches []types.TriggerMatch) ([]types.Action, error) {
	logger := logging.GetLogger("powerups.install")
	actions := make([]types.Action, 0, len(matches))

	for _, match := range matches {
		logger.Debug().
			Str("path", match.Path).
			Str("pack", match.Pack).
			Msg("Processing install script")

		// Calculate checksum of the install script
		checksum, err := testutil.CalculateFileChecksum(match.AbsolutePath)
		if err != nil {
			logger.Error().Err(err).
				Str("path", match.AbsolutePath).
				Msg("Failed to calculate install script checksum")
			return nil, fmt.Errorf("failed to calculate checksum for %s: %w", match.Path, err)
		}

		action := types.Action{
			Type:        types.ActionTypeInstall,
			Description: fmt.Sprintf("Run install script %s", match.Path),
			Source:      match.AbsolutePath,
			Target:      "", // Not used for install scripts
			Pack:        match.Pack,
			PowerUpName: p.Name(),
			Priority:    match.Priority,
			Command:     match.AbsolutePath,
			Args:        []string{}, // Could be extended to support arguments
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
func (p *InstallScriptPowerUp) ValidateOptions(options map[string]interface{}) error {
	// Install script power-up doesn't have any options
	return nil
}

// GetSentinelPath returns the path to the sentinel file for a pack
func GetInstallSentinelPath(pack string) string {
	return filepath.Join(types.GetInstallDir(), pack)
}

func init() {
	// Register factory in the global registry
	RegisterInstallScriptPowerUpFactory()
}

// RegisterInstallScriptPowerUpFactory registers the install script power-up factory
func RegisterInstallScriptPowerUpFactory() {
	err := registry.RegisterPowerUpFactory(InstallScriptPowerUpName, func(config map[string]interface{}) (types.PowerUp, error) {
		return NewInstallScriptPowerUp(), nil
	})
	if err != nil {
		panic(fmt.Sprintf("failed to register %s power-up: %v", InstallScriptPowerUpName, err))
	}
}
