package addignore

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/rs/zerolog"
)

// AddIgnoreOptions holds options for the add-ignore command
type AddIgnoreOptions struct {
	DotfilesRoot string
	PackName     string
}

// AddIgnore creates a .dodotignore file in the specified pack
func AddIgnore(opts AddIgnoreOptions) (*types.AddIgnoreResult, error) {
	logger := logging.GetLogger("commands.addignore")
	logger.Info().
		Str("pack", opts.PackName).
		Str("dotfiles_root", opts.DotfilesRoot).
		Msg("Adding ignore file to pack")

	// Validate pack name
	if err := paths.ValidatePackName(opts.PackName); err != nil {
		return nil, errors.Wrap(err, errors.ErrPackNotFound, "invalid pack name")
	}

	// Get config for special files
	cfg := config.Default()

	// First check if the pack directory exists (even if it's ignored)
	packPath := filepath.Join(opts.DotfilesRoot, opts.PackName)
	ignoreFilePath := filepath.Join(packPath, cfg.Patterns.SpecialFiles.IgnoreFile)

	// Check if pack directory exists
	packInfo, err := os.Stat(packPath)
	if err != nil {
		if os.IsNotExist(err) {
			// Pack directory doesn't exist, try to find it through discovery
			// This will fail if pack doesn't exist
			targetPack, err := core.FindPack(opts.DotfilesRoot, opts.PackName)
			if err != nil {
				return nil, err
			}
			ignoreFilePath = filepath.Join(targetPack.Path, cfg.Patterns.SpecialFiles.IgnoreFile)
		} else {
			return nil, fmt.Errorf("failed to check pack directory: %w", err)
		}
	} else if !packInfo.IsDir() {
		return nil, fmt.Errorf("pack path exists but is not a directory: %s", packPath)
	}

	// Check if ignore file already exists
	if _, err := os.Stat(ignoreFilePath); err == nil {
		logger.Info().
			Str("pack", opts.PackName).
			Str("ignore_file", ignoreFilePath).
			Msg("Ignore file already exists")
		result := &types.AddIgnoreResult{
			PackName:       opts.PackName,
			IgnoreFilePath: ignoreFilePath,
			Created:        false,
			AlreadyExisted: true,
		}
		logAddIgnore(logger, opts, result, nil)
		return result, nil
	}

	// Create the ignore file
	err = os.WriteFile(ignoreFilePath, []byte(""), 0644)
	if err != nil {
		return nil, fmt.Errorf("failed to create ignore file: %w", err)
	}

	logger.Info().
		Str("pack", opts.PackName).
		Str("ignore_file", ignoreFilePath).
		Msg("Created ignore file")

	result := &types.AddIgnoreResult{
		PackName:       opts.PackName,
		IgnoreFilePath: ignoreFilePath,
		Created:        true,
		AlreadyExisted: false,
	}

	logAddIgnore(logger, opts, result, nil)
	return result, nil
}

// logAddIgnore logs the add-ignore command execution
func logAddIgnore(logger zerolog.Logger, opts AddIgnoreOptions, result *types.AddIgnoreResult, err error) {
	event := logger.Info()
	if err != nil {
		event = logger.Error().Err(err)
	}

	event.
		Str("command", "add-ignore").
		Str("pack", opts.PackName).
		Str("dotfiles_root", opts.DotfilesRoot)

	if result != nil {
		event.
			Str("ignore_file", result.IgnoreFilePath).
			Bool("created", result.Created).
			Bool("already_existed", result.AlreadyExisted)
	}

	if err != nil {
		event.Msg("Add-ignore command failed")
	} else {
		event.Msg("Add-ignore command completed")
	}
}
