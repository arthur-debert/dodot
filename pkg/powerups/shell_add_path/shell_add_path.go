package shell_add_path

import (
	_ "embed"
	"fmt"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/types"
)

const (
	ShellAddPathPowerUpName = "shell_add_path"
)

//go:embed path-template.txt
var pathTemplate string

// ShellAddPathPowerUp manages adding directories to the PATH
type ShellAddPathPowerUp struct{}

// NewShellAddPathPowerUp creates a new instance of the ShellAddPathPowerUp
func NewShellAddPathPowerUp() types.PowerUp {
	return &ShellAddPathPowerUp{}
}

func (p *ShellAddPathPowerUp) Name() string {
	return ShellAddPathPowerUpName
}

func (p *ShellAddPathPowerUp) Description() string {
	return "Adds directories to the PATH environment variable"
}

func (p *ShellAddPathPowerUp) RunMode() types.RunMode {
	return types.RunModeMany
}

func (p *ShellAddPathPowerUp) Process(matches []types.TriggerMatch) ([]types.Action, error) {
	logger := logging.GetLogger("powerups.shell_add_path")
	var actions []types.Action

	for _, match := range matches {
		logger.Debug().
			Str("path", match.Path).
			Str("pack", match.Pack).
			Msg("Processing path directory")

		action := types.Action{
			Type:        types.ActionTypePathAdd,
			Description: fmt.Sprintf("Add %s to PATH", match.Path),
			Source:      match.AbsolutePath,
			Pack:        match.Pack,
			PowerUpName: p.Name(),
			Priority:    match.Priority,
		}
		actions = append(actions, action)
	}

	return actions, nil
}

func (p *ShellAddPathPowerUp) ValidateOptions(options map[string]interface{}) error {
	return nil // No options to validate yet
}

// GetTemplateContent returns the template content for this power-up
func (p *ShellAddPathPowerUp) GetTemplateContent() string {
	return pathTemplate
}

func init() {
	err := registry.RegisterPowerUpFactory(ShellAddPathPowerUpName, func(config map[string]interface{}) (types.PowerUp, error) {
		return NewShellAddPathPowerUp(), nil
	})
	if err != nil {
		panic(fmt.Sprintf("failed to register %s power-up: %v", ShellAddPathPowerUpName, err))
	}
}
