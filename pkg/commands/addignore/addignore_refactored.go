package addignore

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
)

// AddIgnoreRefactored creates a .dodotignore file using proper abstractions
func AddIgnoreRefactored(opts AddIgnoreOptions) (*types.AddIgnoreResult, error) {
	logger := logging.GetLogger("commands.addignore")
	logger.Info().
		Str("pack", opts.PackName).
		Str("dotfiles_root", opts.DotfilesRoot).
		Msg("Creating ignore file for pack")

	// Get configuration
	cfg := config.Default()

	// Initialize filesystem
	fs := filesystem.NewOS()

	// Find the pack using core abstraction
	pack, err := core.FindPack(opts.DotfilesRoot, opts.PackName)
	if err != nil {
		return nil, err
	}

	// Check if ignore file already exists using Pack's guardian method
	exists, err := pack.HasIgnoreFile(fs, cfg)
	if err != nil {
		return nil, fmt.Errorf("failed to check for ignore file: %w", err)
	}

	if exists {
		logger.Info().
			Str("pack", pack.Name).
			Msg("Ignore file already exists")

		result := &types.AddIgnoreResult{
			PackName:       pack.Name,
			IgnoreFilePath: pack.GetFilePath(cfg.Patterns.SpecialFiles.IgnoreFile),
			Created:        false,
			AlreadyExisted: true,
		}
		return result, nil
	}

	// Create the ignore file using Pack's guardian method
	if err := pack.CreateIgnoreFile(fs, cfg); err != nil {
		return nil, fmt.Errorf("failed to create ignore file: %w", err)
	}

	result := &types.AddIgnoreResult{
		PackName:       pack.Name,
		IgnoreFilePath: pack.GetFilePath(cfg.Patterns.SpecialFiles.IgnoreFile),
		Created:        true,
		AlreadyExisted: false,
	}

	logger.Info().
		Str("pack", pack.Name).
		Str("ignore_file", result.IgnoreFilePath).
		Msg("Successfully created ignore file")

	return result, nil
}
