package symlink

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/types"
)

// SymlinkHandlerName is the name of the symlink handler
const SymlinkHandlerName = "symlink"

// SymlinkHandler creates symbolic links from matched files to target locations
type SymlinkHandler struct {
	defaultTarget string
	paths         paths.Paths
}

// NewSymlinkHandler creates a new SymlinkHandler with default target as user home
func NewSymlinkHandler() *SymlinkHandler {
	logger := logging.GetLogger("handlers.symlink")

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

	return &SymlinkHandler{
		defaultTarget: homeDir,
		paths:         pathsInstance,
	}
}

// Name returns the unique name of this handler
func (h *SymlinkHandler) Name() string {
	return SymlinkHandlerName
}

// Description returns a human-readable description of what this handler does
func (h *SymlinkHandler) Description() string {
	return "Creates symbolic links from dotfiles to target locations"
}

// RunMode returns whether this handler runs once or many times
func (h *SymlinkHandler) RunMode() types.RunMode {
	return types.RunModeLinking
}

// ProcessLinking takes a group of trigger matches and generates LinkAction instances
func (h *SymlinkHandler) ProcessLinking(matches []types.TriggerMatch) ([]types.LinkingAction, error) {
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
func (h *SymlinkHandler) ProcessLinkingWithConfirmations(matches []types.TriggerMatch) (types.ProcessingResult, error) {
	logger := logging.GetLogger("handlers.symlink")
	actions := make([]types.Action, 0, len(matches))

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
			return types.ProcessingResult{}, fmt.Errorf("symlink conflict: both %s and %s want to link to %s",
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

	// Symlink operations don't need confirmation - they're just creating links
	// Confirmation is only needed for clearing/unlinking
	return types.NewProcessingResult(actions), nil
}

// ValidateOptions checks if the provided options are valid for this handler
func (h *SymlinkHandler) ValidateOptions(options map[string]interface{}) error {
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
func (h *SymlinkHandler) GetTemplateContent() string {
	// Symlink handler doesn't provide templates - it symlinks any file
	return ""
}

// Clear removes user-facing symlinks before state removal
func (h *SymlinkHandler) Clear(ctx types.ClearContext) ([]types.ClearedItem, error) {
	logger := logging.GetLogger("handlers.symlink").With().
		Str("pack", ctx.Pack.Name).
		Bool("dryRun", ctx.DryRun).
		Logger()

	clearedItems := []types.ClearedItem{}

	// Get the symlinks directory for this pack
	symlinksDir := ctx.Paths.PackHandlerDir(ctx.Pack.Name, "symlinks")

	// Read all intermediate symlinks
	entries, err := ctx.FS.ReadDir(symlinksDir)
	if err != nil {
		if os.IsNotExist(err) {
			logger.Debug().Msg("No symlinks directory, nothing to clear")
			return clearedItems, nil
		}
		return nil, fmt.Errorf("failed to read symlinks directory: %w", err)
	}

	// Process each intermediate symlink
	for _, entry := range entries {
		if entry.IsDir() {
			continue
		}

		intermediatePath := filepath.Join(symlinksDir, entry.Name())

		// Read where the intermediate link points (source file)
		sourceFile, err := ctx.FS.Readlink(intermediatePath)
		if err != nil {
			logger.Warn().
				Err(err).
				Str("intermediate", intermediatePath).
				Msg("failed to read intermediate symlink")
			continue
		}

		// Determine the user-facing symlink path
		targetPath := ctx.Paths.MapPackFileToSystem(&ctx.Pack, entry.Name())

		// Check if the user-facing symlink exists and points to our intermediate
		linkTarget, err := ctx.FS.Readlink(targetPath)
		if err != nil {
			if !os.IsNotExist(err) {
				logger.Debug().
					Err(err).
					Str("target", targetPath).
					Msg("failed to read user-facing symlink")
			}
			continue
		}

		// Only remove if it points to our intermediate link
		if linkTarget == intermediatePath {
			if ctx.DryRun {
				clearedItems = append(clearedItems, types.ClearedItem{
					Type:        "symlink",
					Path:        targetPath,
					Description: fmt.Sprintf("Would remove symlink to %s", filepath.Base(sourceFile)),
				})
			} else {
				if err := ctx.FS.Remove(targetPath); err != nil {
					logger.Error().
						Err(err).
						Str("target", targetPath).
						Msg("failed to remove user-facing symlink")
					clearedItems = append(clearedItems, types.ClearedItem{
						Type:        "symlink_error",
						Path:        targetPath,
						Description: fmt.Sprintf("Failed to remove symlink: %v", err),
					})
				} else {
					logger.Info().
						Str("target", targetPath).
						Str("source", sourceFile).
						Msg("removed user-facing symlink")
					clearedItems = append(clearedItems, types.ClearedItem{
						Type:        "symlink",
						Path:        targetPath,
						Description: fmt.Sprintf("Removed symlink to %s", filepath.Base(sourceFile)),
					})
				}
			}
		} else {
			logger.Warn().
				Str("target", targetPath).
				Str("expected", intermediatePath).
				Str("actual", linkTarget).
				Msg("user-facing symlink points elsewhere, not removing")
		}
	}

	return clearedItems, nil
}

// init registers the symlink handler factory
func init() {
	handlerFactoryRegistry := registry.GetRegistry[registry.HandlerFactory]()
	registry.MustRegister(handlerFactoryRegistry, SymlinkHandlerName, func(options map[string]interface{}) (interface{}, error) {
		handler := NewSymlinkHandler()

		// Apply options if provided
		if options != nil {
			if err := handler.ValidateOptions(options); err != nil {
				return nil, err
			}
			// For symlink handler, options typically include target directory
			// The specific options are applied during ProcessLinking
		}

		return handler, nil
	})
}

// Verify interface compliance
var _ handlers.LinkingHandler = (*SymlinkHandler)(nil)
var _ handlers.LinkingHandlerWithConfirmations = (*SymlinkHandler)(nil)
var _ handlers.Clearable = (*SymlinkHandler)(nil)
