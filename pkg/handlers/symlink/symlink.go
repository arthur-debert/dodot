package symlink

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/types"
)

const (
	SymlinkHandlerName = "symlink"
)

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
	pathsInstance, err := paths.New("")
	if err != nil {
		logger.Warn().Err(err).Msg("failed to initialize paths, using fallback")
		// Continue without paths instance - we'll use the simple logic
		pathsInstance = nil
	}

	return &SymlinkHandler{
		defaultTarget: homeDir,
		paths:         pathsInstance,
	}
}

// Name returns the unique name of this handler
func (p *SymlinkHandler) Name() string {
	return SymlinkHandlerName
}

// Description returns a human-readable description of what this handler does
func (p *SymlinkHandler) Description() string {
	return "Creates symbolic links from dotfiles to target locations"
}

// RunMode returns whether this handler runs once or many times
func (p *SymlinkHandler) RunMode() types.RunMode {
	return types.RunModeLinking
}

// Process takes a group of trigger matches and generates symlink actions
func (p *SymlinkHandler) Process(matches []types.TriggerMatch) ([]types.Action, error) {
	logger := logging.GetLogger("handlers.symlink")
	actions := make([]types.Action, 0, len(matches))

	// Get target directory from options or use default
	targetDir := p.defaultTarget
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
		if targetDir != p.defaultTarget {
			// Custom target directory specified - use simple path joining
			targetPath = filepath.Join(targetDir, match.Path)
		} else if p.paths != nil && match.Pack != "" {
			// Use centralized mapping for default target
			// Note: We approximate the pack path from the absolute path. This works
			// for Release A but may need refinement in future releases.
			pack := &types.Pack{
				Name: match.Pack,
				Path: filepath.Dir(match.AbsolutePath),
			}
			targetPath = p.paths.MapPackFileToSystem(pack, match.Path)
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

		// Create symlink action
		cfg := config.Default()
		action := types.Action{
			Type:        types.ActionTypeLink,
			Description: fmt.Sprintf("Symlink %s -> %s", match.Path, targetPath),
			Source:      match.AbsolutePath,
			Target:      targetPath,
			Pack:        match.Pack,
			HandlerName: p.Name(),
			Priority:    cfg.Priorities.Handlers["symlink"],
			Metadata: map[string]interface{}{
				"trigger": match.TriggerName,
			},
		}

		actions = append(actions, action)

		logger.Trace().
			Str("source", match.AbsolutePath).
			Str("target", targetPath).
			Str("pack", match.Pack).
			Msg("generated symlink action")
	}

	logger.Info().
		Int("match_count", len(matches)).
		Int("action_count", len(actions)).
		Msg("processed trigger matches")

	return actions, nil
}

// ValidateOptions checks if the provided options are valid for this handler
func (p *SymlinkHandler) ValidateOptions(options map[string]interface{}) error {
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
func (p *SymlinkHandler) GetTemplateContent() string {
	// Symlink handler doesn't provide templates - it symlinks any file
	return ""
}

func init() {
	// Register a factory function that creates the symlink handler
	err := registry.RegisterHandlerFactory(SymlinkHandlerName, func(config map[string]interface{}) (types.Handler, error) {
		return NewSymlinkHandler(), nil
	})
	if err != nil {
		panic(fmt.Sprintf("failed to register SymlinkHandler factory: %v", err))
	}
}
