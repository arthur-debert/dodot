package provision

import (
	_ "embed"
	"fmt"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/types"
)

const (
	// ProvisionScriptHandlerName is the unique name for the install script handler
	ProvisionScriptHandlerName = "provision"
)

//go:embed provision-template.txt
var provisionTemplate string

// ProvisionScriptHandler runs install.sh scripts
type ProvisionScriptHandler struct{}

// NewProvisionScriptHandler creates a new instance of the install script handler
func NewProvisionScriptHandler() types.Handler {
	return &ProvisionScriptHandler{}
}

// Name returns the unique name of this handler
func (p *ProvisionScriptHandler) Name() string {
	return ProvisionScriptHandlerName
}

// Description returns a human-readable description of what this handler does
func (p *ProvisionScriptHandler) Description() string {
	return "Runs install.sh scripts for initial setup"
}

// RunMode returns whether this handler runs once or many times
func (p *ProvisionScriptHandler) RunMode() types.RunMode {
	return types.RunModeProvisioning
}

// Process takes install script matches and generates install actions
func (p *ProvisionScriptHandler) Process(matches []types.TriggerMatch) ([]types.Action, error) {
	logger := logging.GetLogger("handlers.install")
	actions := make([]types.Action, 0, len(matches))

	for _, match := range matches {
		logger.Debug().
			Str("path", match.Path).
			Str("pack", match.Pack).
			Msg("Processing install script")

		action := types.Action{
			Type:        types.ActionTypeInstall,
			Description: fmt.Sprintf("Run install script %s", match.Path),
			Source:      match.AbsolutePath,
			Target:      "", // Not used for install scripts
			Pack:        match.Pack,
			HandlerName: p.Name(),
			Priority:    match.Priority,
			Command:     match.AbsolutePath,
			Args:        []string{}, // Could be extended to support arguments
			Metadata: map[string]interface{}{
				"pack": match.Pack,
			},
		}

		actions = append(actions, action)
	}

	return actions, nil
}

// ValidateOptions checks if the provided options are valid for this handler
func (p *ProvisionScriptHandler) ValidateOptions(options map[string]interface{}) error {
	// Install script handler doesn't have any options
	return nil
}

// GetTemplateContent returns the template content for this handler
func (p *ProvisionScriptHandler) GetTemplateContent() string {
	return provisionTemplate
}

func init() {
	// Register factory in the global registry
	RegisterProvisionScriptHandlerFactory()
}

// RegisterProvisionScriptHandlerFactory registers the install script handler factory
func RegisterProvisionScriptHandlerFactory() {
	err := registry.RegisterHandlerFactory(ProvisionScriptHandlerName, func(config map[string]interface{}) (types.Handler, error) {
		return NewProvisionScriptHandler(), nil
	})
	if err != nil {
		panic(fmt.Sprintf("failed to register %s handler: %v", ProvisionScriptHandlerName, err))
	}
}
