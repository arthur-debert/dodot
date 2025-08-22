package install

import (
	_ "embed"
	"fmt"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/types"
)

const (
	// InstallScriptHandlerName is the unique name for the install script handler
	InstallScriptHandlerName = "install_script"
)

//go:embed install-template.txt
var installTemplate string

// InstallScriptHandler runs install.sh scripts
type InstallScriptHandler struct{}

// NewInstallScriptHandler creates a new instance of the install script handler
func NewInstallScriptHandler() types.Handler {
	return &InstallScriptHandler{}
}

// Name returns the unique name of this handler
func (p *InstallScriptHandler) Name() string {
	return InstallScriptHandlerName
}

// Description returns a human-readable description of what this handler does
func (p *InstallScriptHandler) Description() string {
	return "Runs install.sh scripts for initial setup"
}

// RunMode returns whether this handler runs once or many times
func (p *InstallScriptHandler) RunMode() types.RunMode {
	return types.RunModeProvisioning
}

// Process takes install script matches and generates install actions
func (p *InstallScriptHandler) Process(matches []types.TriggerMatch) ([]types.Action, error) {
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
func (p *InstallScriptHandler) ValidateOptions(options map[string]interface{}) error {
	// Install script handler doesn't have any options
	return nil
}

// GetTemplateContent returns the template content for this handler
func (p *InstallScriptHandler) GetTemplateContent() string {
	return installTemplate
}

// GetInstallSentinelPath returns the path to the sentinel file for a pack
// Deprecated: Use pathsInstance.SentinelPath("install", pack) instead
func GetInstallSentinelPath(pack string, pathsInstance *paths.Paths) string {
	return pathsInstance.SentinelPath("install", pack)
}

func init() {
	// Register factory in the global registry
	RegisterInstallScriptHandlerFactory()
}

// RegisterInstallScriptHandlerFactory registers the install script handler factory
func RegisterInstallScriptHandlerFactory() {
	err := registry.RegisterHandlerFactory(InstallScriptHandlerName, func(config map[string]interface{}) (types.Handler, error) {
		return NewInstallScriptHandler(), nil
	})
	if err != nil {
		panic(fmt.Sprintf("failed to register %s handler: %v", InstallScriptHandlerName, err))
	}
}
