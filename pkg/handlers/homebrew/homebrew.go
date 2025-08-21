package homebrew

import (
	_ "embed"
	"fmt"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/types"
)

const (
	// HomebrewHandlerName is the unique name for the Homebrew handler
	HomebrewHandlerName = "homebrew"
)

//go:embed homebrew-template.txt
var homebrewTemplate string

// HomebrewHandler processes Brewfiles to install packages via Homebrew
type HomebrewHandler struct{}

// NewHomebrewHandler creates a new instance of the Homebrew handler
func NewHomebrewHandler() types.Handler {
	return &HomebrewHandler{}
}

// Name returns the unique name of this handler
func (p *HomebrewHandler) Name() string {
	return HomebrewHandlerName
}

// Description returns a human-readable description of what this handler does
func (p *HomebrewHandler) Description() string {
	return "Processes Brewfiles to install Homebrew packages"
}

// RunMode returns whether this handler runs once or many times
func (p *HomebrewHandler) RunMode() types.RunMode {
	return types.RunModeOnce
}

// Process takes Brewfile matches and generates brew actions
func (p *HomebrewHandler) Process(matches []types.TriggerMatch) ([]types.Action, error) {
	logger := logging.GetLogger("handlers.homebrew")
	actions := make([]types.Action, 0, len(matches))

	for _, match := range matches {
		logger.Debug().
			Str("path", match.Path).
			Str("pack", match.Pack).
			Msg("Processing Brewfile")

		action := types.Action{
			Type:        types.ActionTypeBrew,
			Description: fmt.Sprintf("Install packages from %s", match.Path),
			Source:      match.AbsolutePath,
			Target:      "", // Not used for brew
			Pack:        match.Pack,
			HandlerName: p.Name(),
			Priority:    match.Priority,
			Metadata: map[string]interface{}{
				"pack": match.Pack,
			},
		}

		actions = append(actions, action)
	}

	return actions, nil
}

// ValidateOptions checks if the provided options are valid for this handler
func (p *HomebrewHandler) ValidateOptions(options map[string]interface{}) error {
	// Homebrew handler doesn't have any options
	return nil
}

// GetTemplateContent returns the template content for this handler
func (p *HomebrewHandler) GetTemplateContent() string {
	return homebrewTemplate
}

// GetHomebrewSentinelPath returns the path to the sentinel file for a pack
// Deprecated: Use pathsInstance.SentinelPath("homebrew", pack) instead
func GetHomebrewSentinelPath(pack string, pathsInstance *paths.Paths) string {
	return pathsInstance.SentinelPath("homebrew", pack)
}

func init() {
	// Register factory in the global registry
	RegisterHomebrewHandlerFactory()
}

// RegisterHomebrewHandlerFactory registers the Homebrew handler factory
func RegisterHomebrewHandlerFactory() {
	err := registry.RegisterHandlerFactory(HomebrewHandlerName, func(config map[string]interface{}) (types.Handler, error) {
		return NewHomebrewHandler(), nil
	})
	if err != nil {
		panic(fmt.Sprintf("failed to register %s handler: %v", HomebrewHandlerName, err))
	}
}
