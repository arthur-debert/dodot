package shell_profile

import (
	_ "embed"
	"fmt"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/types"
)

const (
	ShellProfileHandlerName = "shell_profile"
)

//go:embed aliases-template.txt
var aliasesTemplate string

// ShellProfileHandler manages shell profile modifications
type ShellProfileHandler struct{}

// NewShellProfileHandler creates a new instance of the ShellProfileHandler
func NewShellProfileHandler() types.Handler {
	return &ShellProfileHandler{}
}

func (p *ShellProfileHandler) Name() string {
	return ShellProfileHandlerName
}

func (p *ShellProfileHandler) Description() string {
	return "Manages shell profile modifications (e.g., sourcing aliases)"
}

func (p *ShellProfileHandler) RunMode() types.RunMode {
	return types.RunModeLinking
}

func (p *ShellProfileHandler) Process(matches []types.TriggerMatch) ([]types.Action, error) {
	logger := logging.GetLogger("handlers.shell_profile")
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
			HandlerName: p.Name(),
			Priority:    match.Priority,
		}
		actions = append(actions, action)
	}

	return actions, nil
}

func (p *ShellProfileHandler) ValidateOptions(options map[string]interface{}) error {
	return nil // No options to validate yet
}

// GetTemplateContent returns the template content for this handler
func (p *ShellProfileHandler) GetTemplateContent() string {
	return aliasesTemplate
}

func init() {
	err := registry.RegisterHandlerFactory(ShellProfileHandlerName, func(config map[string]interface{}) (types.Handler, error) {
		return NewShellProfileHandler(), nil
	})
	if err != nil {
		panic(fmt.Sprintf("failed to register %s handler: %v", ShellProfileHandlerName, err))
	}
}
