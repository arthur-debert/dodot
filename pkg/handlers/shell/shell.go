package shell

import (
	_ "embed"

	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
)

// ShellHandlerName is the name of the shell profile handler
const ShellHandlerName = "shell"

//go:embed aliases-template.txt
var aliasesTemplate string

// ShellHandler manages shell profile modifications
type ShellHandler struct{}

// NewShellHandler creates a new instance of the ShellHandler
func NewShellHandler() *ShellHandler {
	return &ShellHandler{}
}

// Name returns the unique name of this handler
func (h *ShellHandler) Name() string {
	return ShellHandlerName
}

// Description returns a human-readable description of what this handler does
func (h *ShellHandler) Description() string {
	return "Manages shell profile modifications (e.g., sourcing aliases)"
}

// RunMode returns when this handler should run
func (h *ShellHandler) RunMode() types.RunMode {
	return types.RunModeLinking
}

// ProcessLinking takes shell script files and creates AddToShellProfileAction instances
func (h *ShellHandler) ProcessLinking(matches []types.TriggerMatch) ([]types.LinkingAction, error) {
	result, err := h.ProcessLinkingWithConfirmations(matches)
	if err != nil {
		return nil, err
	}

	// Convert ProcessingResult actions to LinkingAction slice for backward compatibility
	linkingActions := make([]types.LinkingAction, 0, len(result.Actions))
	for _, action := range result.Actions {
		if linkAction, ok := action.(types.LinkingAction); ok {
			linkingActions = append(linkingActions, linkAction)
		}
	}

	return linkingActions, nil
}

// ProcessLinkingWithConfirmations implements LinkingHandlerWithConfirmations
func (h *ShellHandler) ProcessLinkingWithConfirmations(matches []types.TriggerMatch) (types.ProcessingResult, error) {
	logger := logging.GetLogger("handlers.shell")
	actions := make([]types.Action, 0, len(matches))

	for _, match := range matches {
		logger.Debug().
			Str("path", match.Path).
			Str("pack", match.Pack).
			Msg("Processing shell profile file")

		// Create AddToShellProfileAction
		action := &types.AddToShellProfileAction{
			PackName:   match.Pack,
			ScriptPath: match.AbsolutePath,
		}

		actions = append(actions, action)
	}

	logger.Info().
		Int("match_count", len(matches)).
		Int("action_count", len(actions)).
		Msg("processed shell profile matches")

	// Shell profile operations don't need confirmation - they're just adding sources
	// Confirmation is only needed for clearing/removing shell profile entries
	return types.NewProcessingResult(actions), nil
}

// ValidateOptions checks if the provided options are valid
func (h *ShellHandler) ValidateOptions(options map[string]interface{}) error {
	return nil // No options to validate yet
}

// GetTemplateContent returns the template content for this handler
func (h *ShellHandler) GetTemplateContent() string {
	return aliasesTemplate
}

// Clear performs no additional cleanup for shell profile handler
// The state directory removal is sufficient
func (h *ShellHandler) Clear(ctx types.ClearContext) ([]types.ClearedItem, error) {
	logger := logging.GetLogger("handlers.shell").With().
		Str("pack", ctx.Pack.Name).
		Bool("dryRun", ctx.DryRun).
		Logger()

	// Shell profile handler doesn't need to do anything special
	// Removing the state directory is sufficient - shell integration will stop sourcing scripts
	logger.Debug().Msg("Shell profile handler clear - state removal is sufficient")

	if ctx.DryRun {
		return []types.ClearedItem{
			{
				Type:        "shell_state",
				Path:        ctx.Paths.PackHandlerDir(ctx.Pack.Name, "shell"),
				Description: "Would remove shell profile sources",
			},
		}, nil
	}

	return []types.ClearedItem{
		{
			Type:        "shell_state",
			Path:        ctx.Paths.PackHandlerDir(ctx.Pack.Name, "shell"),
			Description: "Shell profile sources will be removed",
		},
	}, nil
}

// init registers the shell handler factory
// func init() {
// 	handlerFactoryRegistry := registry.GetRegistry[registry.HandlerFactory]()
// 	registry.MustRegister(handlerFactoryRegistry, ShellHandlerName, func(options map[string]interface{}) (interface{}, error) {
// 		handler := NewShellHandler()
//
// 		// Apply options if provided
// 		if options != nil {
// 			if err := handler.ValidateOptions(options); err != nil {
// 				return nil, err
// 			}
// 		}
//
// 		return handler, nil
// 	})
// }

// Verify interface compliance
var _ handlers.LinkingHandler = (*ShellHandler)(nil)
var _ handlers.LinkingHandlerWithConfirmations = (*ShellHandler)(nil)
var _ handlers.Clearable = (*ShellHandler)(nil)
