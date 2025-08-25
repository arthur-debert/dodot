package path

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
)

// PathHandlerName is the name of the path handler
const PathHandlerName = "path"

// PathHandler handles directories by adding them to PATH
type PathHandler struct {
	targetDir string
}

// NewPathHandler creates a new PathHandler
func NewPathHandler() *PathHandler {
	return &PathHandler{
		targetDir: "~/bin",
	}
}

// Name returns the unique name of this handler
func (h *PathHandler) Name() string {
	return PathHandlerName
}

// Description returns a human-readable description
func (h *PathHandler) Description() string {
	return "Adds directories to PATH"
}

// RunMode returns when this handler should run
func (h *PathHandler) RunMode() types.RunMode {
	return types.RunModeLinking
}

// ProcessLinking takes directories and creates AddToPathAction instances
func (h *PathHandler) ProcessLinking(matches []types.TriggerMatch) ([]types.LinkingAction, error) {
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
func (h *PathHandler) ProcessLinkingWithConfirmations(matches []types.TriggerMatch) (types.ProcessingResult, error) {
	logger := logging.GetLogger("handlers.path")
	actions := make([]types.Action, 0, len(matches))

	// Track directories to avoid duplicates
	seenDirs := make(map[string]bool)

	for _, match := range matches {
		// For directory matches, we want to add the directory to PATH
		// The match.AbsolutePath should be the directory path
		dirPath := match.AbsolutePath

		// Skip if we've already processed this directory
		dirKey := fmt.Sprintf("%s:%s", match.Pack, match.Path)
		if seenDirs[dirKey] {
			logger.Debug().
				Str("directory", dirPath).
				Str("pack", match.Pack).
				Msg("skipping duplicate directory")
			continue
		}
		seenDirs[dirKey] = true

		// Create AddToPathAction
		action := &types.AddToPathAction{
			PackName: match.Pack,
			DirPath:  dirPath,
		}

		actions = append(actions, action)

		logger.Debug().
			Str("directory", dirPath).
			Str("pack", match.Pack).
			Str("path", match.Path).
			Msg("generated add to path action")
	}

	logger.Info().
		Int("match_count", len(matches)).
		Int("action_count", len(actions)).
		Msg("processed directory matches for PATH")

	// PATH operations don't need confirmation - they're just adding directories to PATH
	// Confirmation is only needed for clearing/removing PATH entries
	return types.NewProcessingResult(actions), nil
}

// ValidateOptions checks if the provided options are valid
func (h *PathHandler) ValidateOptions(options map[string]interface{}) error {
	if options == nil {
		return nil
	}

	// Check target option if provided
	if target, exists := options["target"]; exists {
		if _, ok := target.(string); !ok {
			return fmt.Errorf("target option must be a string, got %T", target)
		}
	}

	// Check for unknown options
	for key := range options {
		if key != "target" {
			return fmt.Errorf("unknown option: %s", key)
		}
	}

	return nil
}

// GetTemplateContent returns the template content for this handler
func (h *PathHandler) GetTemplateContent() string {
	return ""
}

// Clear performs no additional cleanup for path handler
// The state directory removal is sufficient
func (h *PathHandler) Clear(ctx types.ClearContext) ([]types.ClearedItem, error) {
	logger := logging.GetLogger("handlers.path").With().
		Str("pack", ctx.Pack.Name).
		Bool("dryRun", ctx.DryRun).
		Logger()

	// Path handler doesn't need to do anything special
	// Removing the state directory is sufficient - shell integration will stop including it
	logger.Debug().Msg("Path handler clear - state removal is sufficient")

	if ctx.DryRun {
		return []types.ClearedItem{
			{
				Type:        "path_state",
				Path:        ctx.Paths.PackHandlerDir(ctx.Pack.Name, "path"),
				Description: "Would remove PATH entries",
			},
		}, nil
	}

	return []types.ClearedItem{
		{
			Type:        "path_state",
			Path:        ctx.Paths.PackHandlerDir(ctx.Pack.Name, "path"),
			Description: "PATH entries will be removed",
		},
	}, nil
}

// Verify interface compliance
var _ types.LinkingHandler = (*PathHandler)(nil)
var _ types.LinkingHandlerWithConfirmations = (*PathHandler)(nil)
var _ types.Clearable = (*PathHandler)(nil)
