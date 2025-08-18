package addignore

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/config"
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
	if opts.PackName == "" {
		return nil, errors.New(errors.ErrInvalidInput, "pack name cannot be empty")
	}

	// Initialize paths
	pathsInst, err := paths.New(opts.DotfilesRoot)
	if err != nil {
		return nil, fmt.Errorf("failed to initialize paths: %w", err)
	}

	// Use standard os package for file operations since we're working with absolute paths
	// The synthfs OSFileSystem expects relative paths, not absolute ones

	// Check if pack exists
	packPath := filepath.Join(pathsInst.DotfilesRoot(), opts.PackName)
	if info, err := os.Stat(packPath); err != nil {
		if os.IsNotExist(err) {
			return nil, errors.New(errors.ErrPackNotFound, "pack not found").WithDetail("notFound", []string{opts.PackName})
		}
		return nil, fmt.Errorf("failed to check pack directory: %w", err)
	} else if !info.IsDir() {
		return nil, errors.New(errors.ErrPackInvalid, "pack path is not a directory").WithDetail("pack", opts.PackName)
	}

	// Get ignore file path from config
	cfg := config.Default()
	ignoreFilePath := filepath.Join(packPath, cfg.Patterns.SpecialFiles.IgnoreFile)

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
