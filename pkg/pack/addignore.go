package pack

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
)

// AddIgnore creates a .dodotignore file for the pack if it doesn't already exist.
// Returns information about whether the file was created or already existed.
func (p *Pack) AddIgnore(fs types.FS, cfg *config.Config) (*types.AddIgnoreResult, error) {
	logger := logging.GetLogger("pack.addignore")
	logger.Debug().
		Str("pack", p.Name).
		Msg("Checking for ignore file")

	// Use default config if not provided
	if cfg == nil {
		cfg = config.Default()
	}

	// Check if ignore file already exists using the embedded Pack's method
	exists, err := p.HasIgnoreFile(fs, cfg)
	if err != nil {
		return nil, fmt.Errorf("failed to check for ignore file: %w", err)
	}

	ignoreFilePath := p.GetFilePath(cfg.Patterns.SpecialFiles.IgnoreFile)

	if exists {
		logger.Info().
			Str("pack", p.Name).
			Str("path", ignoreFilePath).
			Msg("Ignore file already exists")

		return &types.AddIgnoreResult{
			PackName:       p.Name,
			IgnoreFilePath: ignoreFilePath,
			Created:        false,
			AlreadyExisted: true,
		}, nil
	}

	// Create the ignore file using the embedded Pack's method
	if err := p.CreateIgnoreFile(fs, cfg); err != nil {
		return nil, fmt.Errorf("failed to create ignore file: %w", err)
	}

	logger.Info().
		Str("pack", p.Name).
		Str("path", ignoreFilePath).
		Msg("Successfully created ignore file")

	return &types.AddIgnoreResult{
		PackName:       p.Name,
		IgnoreFilePath: ignoreFilePath,
		Created:        true,
		AlreadyExisted: false,
	}, nil
}
