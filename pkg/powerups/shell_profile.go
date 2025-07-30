package powerups

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/types"
)

const (
	ShellProfilePowerUpName = "shell_profile"
)

// ShellProfilePowerUp manages shell profile modifications
type ShellProfilePowerUp struct{}

// NewShellProfilePowerUp creates a new instance of the ShellProfilePowerUp
func NewShellProfilePowerUp() types.PowerUp {
	return &ShellProfilePowerUp{}
}

func (p *ShellProfilePowerUp) Name() string {
	return ShellProfilePowerUpName
}

func (p *ShellProfilePowerUp) Description() string {
	return "Manages shell profile modifications (e.g., sourcing aliases)"
}

func (p *ShellProfilePowerUp) RunMode() types.RunMode {
	return types.RunModeMany
}

func (p *ShellProfilePowerUp) Process(matches []types.TriggerMatch) ([]types.Action, error) {
	logger := logging.GetLogger("powerups.shell_profile")
	var actions []types.Action

	for _, match := range matches {
		logger.Debug().
			Str("path", match.Path).
			Str("pack", match.Pack).
			Msg("Processing shell profile file")

		action := types.Action{
			Type:        types.ActionTypeShellSource,
			Description: fmt.Sprintf("Source shell script %s", match.Path),
			Source:      match.AbsolutePath,
			Pack:        match.Pack,
			PowerUpName: p.Name(),
			Priority:    match.Priority,
		}
		actions = append(actions, action)
	}

	return actions, nil
}

func (p *ShellProfilePowerUp) ValidateOptions(options map[string]interface{}) error {
	return nil // No options to validate yet
}

func init() {
	err := registry.RegisterPowerUpFactory(ShellProfilePowerUpName, func(config map[string]interface{}) (types.PowerUp, error) {
		return NewShellProfilePowerUp(), nil
	})
	if err != nil {
		panic(fmt.Sprintf("failed to register %s power-up: %v", ShellProfilePowerUpName, err))
	}
}
