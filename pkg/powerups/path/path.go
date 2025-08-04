package path

import (
	"fmt"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/types"
)

const (
	// PathPowerUpName is the unique name for the path power-up
	PathPowerUpName = "path"

	// PathPowerUpPriority is the priority for path operations
	PathPowerUpPriority = 90
)

// PathPowerUp handles executable files by creating symlinks in ~/bin
type PathPowerUp struct {
	targetDir string
}

// NewPathPowerUp creates a new PathPowerUp
func NewPathPowerUp() *PathPowerUp {
	return &PathPowerUp{
		targetDir: "~/bin",
	}
}

// Name returns the unique name of this power-up
func (p *PathPowerUp) Name() string {
	return PathPowerUpName
}

// Description returns a human-readable description
func (p *PathPowerUp) Description() string {
	return "Creates symlinks for executable files in ~/bin"
}

// RunMode returns when this power-up should run
func (p *PathPowerUp) RunMode() types.RunMode {
	return types.RunModeMany
}

// Process takes executable files and creates symlink actions to ~/bin
func (p *PathPowerUp) Process(matches []types.TriggerMatch) ([]types.Action, error) {
	logger := logging.GetLogger("powerups.path")
	actions := make([]types.Action, 0, len(matches))

	// Get target directory from options or use default
	targetDir := p.targetDir
	if len(matches) > 0 && matches[0].PowerUpOptions != nil {
		if target, ok := matches[0].PowerUpOptions["target"].(string); ok {
			targetDir = target
		}
	}

	// Track targets to detect conflicts
	targetMap := make(map[string]string)

	for _, match := range matches {
		// For bin files, we want just the filename without any directory structure
		filename := filepath.Base(match.Path)
		targetPath := filepath.Join(targetDir, filename)

		// Check for conflicts
		if existingSource, exists := targetMap[targetPath]; exists {
			logger.Error().
				Str("target", targetPath).
				Str("source1", existingSource).
				Str("source2", match.AbsolutePath).
				Msg("path conflict detected - multiple files want same target")
			return nil, fmt.Errorf("path conflict: both %s and %s want to link to %s",
				existingSource, match.AbsolutePath, targetPath)
		}

		targetMap[targetPath] = match.AbsolutePath

		// Create symlink action
		action := types.Action{
			Type:        types.ActionTypeLink,
			Description: fmt.Sprintf("Link executable %s -> %s", match.Path, targetPath),
			Source:      match.AbsolutePath,
			Target:      targetPath,
			Pack:        match.Pack,
			PowerUpName: p.Name(),
			Priority:    PathPowerUpPriority,
			Metadata: map[string]interface{}{
				"trigger":    match.TriggerName,
				"executable": true,
			},
		}

		actions = append(actions, action)

		logger.Debug().
			Str("source", match.AbsolutePath).
			Str("target", targetPath).
			Str("pack", match.Pack).
			Msg("generated path symlink action")
	}

	logger.Info().
		Int("match_count", len(matches)).
		Int("action_count", len(actions)).
		Msg("processed executable matches")

	return actions, nil
}

// ValidateOptions checks if the provided options are valid
func (p *PathPowerUp) ValidateOptions(options map[string]interface{}) error {
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

// GetTemplateContent returns the template content for this power-up
func (p *PathPowerUp) GetTemplateContent() string {
	return ""
}

func init() {
	// Register the factory
	err := registry.RegisterPowerUpFactory(PathPowerUpName, func(config map[string]interface{}) (types.PowerUp, error) {
		return NewPathPowerUp(), nil
	})
	if err != nil {
		panic(fmt.Sprintf("failed to register %s power-up: %v", PathPowerUpName, err))
	}

	// Default matchers will be registered separately to avoid import cycles
}
