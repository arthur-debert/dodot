package shell_add_path

import (
	_ "embed"
	"fmt"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/types"
)

const (
	ShellAddPathHandlerName = "shell_add_path"
)

//go:embed path-template.txt
var pathTemplate string

// ShellAddPathHandler manages adding directories to the PATH
type ShellAddPathHandler struct{}

// NewShellAddPathHandler creates a new instance of the ShellAddPathHandler
func NewShellAddPathHandler() types.Handler {
	return &ShellAddPathHandler{}
}

func (p *ShellAddPathHandler) Name() string {
	return ShellAddPathHandlerName
}

func (p *ShellAddPathHandler) Description() string {
	return "Adds directories to the PATH environment variable"
}

func (p *ShellAddPathHandler) RunMode() types.RunMode {
	return types.RunModeLinking
}

func (p *ShellAddPathHandler) Process(matches []types.TriggerMatch) ([]types.Action, error) {
	logger := logging.GetLogger("handlers.shell_add_path")
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
			HandlerName: p.Name(),
			Priority:    match.Priority,
		}
		actions = append(actions, action)
	}

	return actions, nil
}

func (p *ShellAddPathHandler) ValidateOptions(options map[string]interface{}) error {
	return nil // No options to validate yet
}

// GetTemplateContent returns the template content for this handler
func (p *ShellAddPathHandler) GetTemplateContent() string {
	return pathTemplate
}

func init() {
	err := registry.RegisterHandlerFactory(ShellAddPathHandlerName, func(config map[string]interface{}) (types.Handler, error) {
		return NewShellAddPathHandler(), nil
	})
	if err != nil {
		panic(fmt.Sprintf("failed to register %s handler: %v", ShellAddPathHandlerName, err))
	}
}
