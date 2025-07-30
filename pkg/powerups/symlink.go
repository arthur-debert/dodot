package powerups

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/arthur-debert/dodot/pkg/utils"
)

const (
	SymlinkPowerUpName     = "symlink"
	SymlinkPowerUpPriority = 100
)

// SymlinkPowerUp creates symbolic links from matched files to target locations
type SymlinkPowerUp struct {
	defaultTarget string
}

// NewSymlinkPowerUp creates a new SymlinkPowerUp with default target as user home
func NewSymlinkPowerUp() *SymlinkPowerUp {
	logger := logging.GetLogger("powerups.symlink")

	// Try to get home directory, but use ~ as fallback
	homeDir, err := utils.GetHomeDirectory()
	if err != nil {
		logger.Warn().Err(err).Msg("failed to get home directory, using ~ placeholder")
		homeDir = "~"
	}

	return &SymlinkPowerUp{
		defaultTarget: homeDir,
	}
}

// Name returns the unique name of this power-up
func (p *SymlinkPowerUp) Name() string {
	return SymlinkPowerUpName
}

// Description returns a human-readable description of what this power-up does
func (p *SymlinkPowerUp) Description() string {
	return "Creates symbolic links from dotfiles to target locations"
}

// RunMode returns whether this power-up runs once or many times
func (p *SymlinkPowerUp) RunMode() types.RunMode {
	return types.RunModeMany
}

// Process takes a group of trigger matches and generates symlink actions
func (p *SymlinkPowerUp) Process(matches []types.TriggerMatch) ([]types.Action, error) {
	logger := logging.GetLogger("powerups.symlink")
	actions := make([]types.Action, 0, len(matches))

	// Get target directory from options or use default
	targetDir := p.defaultTarget
	if len(matches) > 0 && matches[0].PowerUpOptions != nil {
		if target, ok := matches[0].PowerUpOptions["target"].(string); ok {
			targetDir = os.ExpandEnv(target)
		}
	}

	// Track symlink targets to detect conflicts
	targetMap := make(map[string]string)

	for _, match := range matches {
		// Calculate target path
		filename := filepath.Base(match.Path)
		targetPath := filepath.Join(targetDir, filename)

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
		action := types.Action{
			Type:        types.ActionTypeLink,
			Description: fmt.Sprintf("Symlink %s -> %s", match.Path, targetPath),
			Source:      match.AbsolutePath,
			Target:      targetPath,
			Pack:        match.Pack,
			PowerUpName: p.Name(),
			Priority:    SymlinkPowerUpPriority,
			Metadata: map[string]interface{}{
				"trigger": match.TriggerName,
			},
		}

		actions = append(actions, action)

		logger.Debug().
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

// ValidateOptions checks if the provided options are valid for this power-up
func (p *SymlinkPowerUp) ValidateOptions(options map[string]interface{}) error {
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

func init() {
	// Register a factory function that creates the symlink power-up
	err := registry.RegisterPowerUpFactory(SymlinkPowerUpName, func(config map[string]interface{}) (types.PowerUp, error) {
		return NewSymlinkPowerUp(), nil
	})
	if err != nil {
		panic(fmt.Sprintf("failed to register SymlinkPowerUp factory: %v", err))
	}
}
