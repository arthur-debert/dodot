package path

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/types"
)

const (
	// PathPowerUpName is the unique name for the path power-up
	PathPowerUpName = "path"
)

// PathPowerUp handles directories by adding them to PATH
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
	return "Adds directories to PATH"
}

// RunMode returns when this power-up should run
func (p *PathPowerUp) RunMode() types.RunMode {
	return types.RunModeMany
}

// Process takes directories and creates path add actions
func (p *PathPowerUp) Process(matches []types.TriggerMatch) ([]types.Action, error) {
	logger := logging.GetLogger("powerups.path")
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

		// Create path add action
		cfg := config.Default()
		action := types.Action{
			Type:        types.ActionTypePathAdd,
			Description: fmt.Sprintf("Add %s/%s to PATH", match.Pack, match.Path),
			Source:      dirPath,
			Target:      dirPath, // For PATH add, target is the directory to add
			Pack:        match.Pack,
			PowerUpName: p.Name(),
			Priority:    cfg.Priorities.PowerUps["path"],
			Metadata: map[string]interface{}{
				"trigger": match.TriggerName,
				"dirName": match.Path, // Store the relative directory name (e.g., "bin")
			},
		}

		actions = append(actions, action)

		logger.Debug().
			Str("directory", dirPath).
			Str("pack", match.Pack).
			Str("path", match.Path).
			Msg("generated path add action")
	}

	logger.Info().
		Int("match_count", len(matches)).
		Int("action_count", len(actions)).
		Msg("processed directory matches for PATH")

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
