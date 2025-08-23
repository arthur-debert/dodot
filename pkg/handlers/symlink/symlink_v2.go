package symlink

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
)

// SymlinkHandlerName is the name of the symlink handler
const SymlinkHandlerName = "symlink"

// SymlinkHandlerV2 creates symbolic links from matched files to target locations
type SymlinkHandlerV2 struct {
	defaultTarget string
	paths         paths.Paths
}

// NewSymlinkHandlerV2 creates a new SymlinkHandlerV2 with default target as user home
func NewSymlinkHandlerV2() *SymlinkHandlerV2 {
	logger := logging.GetLogger("handlers.symlink.v2")

	// Try to get home directory, preferring HOME env var for testability
	homeDir := os.Getenv("HOME")
	if homeDir == "" {
		var err error
		homeDir, err = os.UserHomeDir()
		if err != nil || homeDir == "" {
			logger.Warn().Err(err).Msg("failed to get home directory, using ~ placeholder")
			homeDir = "~"
		}
	}

	// Initialize paths instance
	// Skip paths initialization in tests for consistent behavior
	var pathsInstance paths.Paths
	if os.Getenv("DODOT_TEST_MODE") != "true" {
		var err error
		pathsInstance, err = paths.New("")
		if err != nil {
			logger.Warn().Err(err).Msg("failed to initialize paths, using fallback")
			// Continue without paths instance - we'll use the simple logic
			pathsInstance = nil
		}
	}

	return &SymlinkHandlerV2{
		defaultTarget: homeDir,
		paths:         pathsInstance,
	}
}

// Name returns the unique name of this handler
func (h *SymlinkHandlerV2) Name() string {
	return SymlinkHandlerName
}

// Description returns a human-readable description of what this handler does
func (h *SymlinkHandlerV2) Description() string {
	return "Creates symbolic links from dotfiles to target locations"
}

// RunMode returns whether this handler runs once or many times
func (h *SymlinkHandlerV2) RunMode() types.RunMode {
	return types.RunModeLinking
}

// ProcessLinking takes a group of trigger matches and generates LinkAction instances
func (h *SymlinkHandlerV2) ProcessLinking(matches []types.TriggerMatch) ([]types.LinkingAction, error) {
	logger := logging.GetLogger("handlers.symlink.v2")
	actions := make([]types.LinkingAction, 0, len(matches))

	// Get target directory from options or use default
	targetDir := h.defaultTarget
	if len(matches) > 0 && matches[0].HandlerOptions != nil {
		if target, ok := matches[0].HandlerOptions["target"].(string); ok {
			targetDir = os.ExpandEnv(target)
		}
	}

	// Track symlink targets to detect conflicts
	targetMap := make(map[string]string)

	for _, match := range matches {
		// Use centralized path mapping
		var targetPath string
		if targetDir != h.defaultTarget {
			// Custom target directory specified - use simple path joining
			targetPath = filepath.Join(targetDir, match.Path)
		} else if h.paths != nil && match.Pack != "" {
			// Use centralized mapping for default target
			// Note: We approximate the pack path from the absolute path. This works
			// for Release A but may need refinement in future releases.
			pack := &types.Pack{
				Name: match.Pack,
				Path: filepath.Dir(match.AbsolutePath),
			}
			targetPath = h.paths.MapPackFileToSystem(pack, match.Path)
		} else {
			// Fallback to simple logic if paths not initialized
			targetPath = filepath.Join(targetDir, match.Path)
		}

		// Check for conflicts
		if existingSource, exists := targetMap[targetPath]; exists {
			logger.Error().
				Str("target", targetPath).
				Str("source1", existingSource).
				Str("source2", match.AbsolutePath).
				Msg("symlink conflict detected - multiple files want same target")
			return nil, fmt.Errorf("symlink conflict: both %s and %s want to link to %s",
				existingSource, match.AbsolutePath, targetPath)
		}

		targetMap[targetPath] = match.AbsolutePath

		// Create LinkAction
		action := &types.LinkAction{
			PackName:   match.Pack,
			SourceFile: match.AbsolutePath,
			TargetFile: targetPath,
		}

		actions = append(actions, action)

		logger.Trace().
			Str("source", match.AbsolutePath).
			Str("target", targetPath).
			Str("pack", match.Pack).
			Msg("generated link action")
	}

	logger.Info().
		Int("match_count", len(matches)).
		Int("action_count", len(actions)).
		Msg("processed trigger matches")

	return actions, nil
}

// ValidateOptions checks if the provided options are valid for this handler
func (h *SymlinkHandlerV2) ValidateOptions(options map[string]interface{}) error {
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
func (h *SymlinkHandlerV2) GetTemplateContent() string {
	// Symlink handler doesn't provide templates - it symlinks any file
	return ""
}

// Verify interface compliance
var _ types.LinkingHandler = (*SymlinkHandlerV2)(nil)
