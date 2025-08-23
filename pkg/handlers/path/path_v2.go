package path

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
)

// PathHandlerV2 handles directories by adding them to PATH
type PathHandlerV2 struct {
	targetDir string
}

// NewPathHandlerV2 creates a new PathHandlerV2
func NewPathHandlerV2() *PathHandlerV2 {
	return &PathHandlerV2{
		targetDir: "~/bin",
	}
}

// Name returns the unique name of this handler
func (h *PathHandlerV2) Name() string {
	return PathHandlerName
}

// Description returns a human-readable description
func (h *PathHandlerV2) Description() string {
	return "Adds directories to PATH"
}

// RunMode returns when this handler should run
func (h *PathHandlerV2) RunMode() types.RunMode {
	return types.RunModeLinking
}

// Process implements the old Handler interface for compatibility
func (h *PathHandlerV2) Process(matches []types.TriggerMatch) ([]types.Action, error) {
	// This method is here for compatibility but should not be used
	return nil, fmt.Errorf("Process method is deprecated, use ProcessLinking instead")
}

// ProcessLinking takes directories and creates AddToPathAction instances
func (h *PathHandlerV2) ProcessLinking(matches []types.TriggerMatch) ([]types.LinkingAction, error) {
	logger := logging.GetLogger("handlers.path.v2")
	actions := make([]types.LinkingAction, 0, len(matches))

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

	return actions, nil
}

// ValidateOptions checks if the provided options are valid
func (h *PathHandlerV2) ValidateOptions(options map[string]interface{}) error {
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
func (h *PathHandlerV2) GetTemplateContent() string {
	return ""
}

// Verify interface compliance
var _ types.Handler = (*PathHandlerV2)(nil)
var _ types.LinkingHandlerV2 = (*PathHandlerV2)(nil)
