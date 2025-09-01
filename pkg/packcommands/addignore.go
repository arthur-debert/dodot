package packcommands

import (
	"fmt"
	"time"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/statustype"
	"github.com/arthur-debert/dodot/pkg/types"
)

// AddIgnoreOptions contains options for the AddIgnore operation
type AddIgnoreOptions struct {
	// PackName is the name of the pack to add ignore file to
	PackName string
	// DotfilesRoot is the root directory for dotfiles
	DotfilesRoot string
	// FileSystem is the filesystem to use (optional, defaults to OS filesystem)
	FileSystem types.FS
	// GetPackStatus is a function to get pack status to avoid circular imports
	GetPackStatus statustype.GetPackStatusFunc
}

// AddIgnore creates a .dodotignore file for the pack if it doesn't already exist.
// Returns information about whether the file was created or already existed.
func AddIgnore(opts AddIgnoreOptions) (*types.PackCommandResult, error) {
	logger := logging.GetLogger("pack.addignore")
	logger.Debug().
		Str("pack", opts.PackName).
		Msg("Checking for ignore file")

	// Use provided filesystem or default
	fs := opts.FileSystem
	if fs == nil {
		fs = filesystem.NewOS()
	}

	// Validate pack name
	if opts.PackName == "" {
		return nil, errors.New(errors.ErrPackNotFound, "pack name cannot be empty")
	}

	// Find the pack
	packPath := opts.DotfilesRoot + "/" + opts.PackName
	if _, err := fs.Stat(packPath); err != nil {
		return nil, errors.New(errors.ErrPackNotFound, fmt.Sprintf("pack %s not found", opts.PackName))
	}

	// Create pack instance
	p := &Pack{
		Pack: &types.Pack{
			Name: opts.PackName,
			Path: packPath,
		},
	}

	// Get default configuration
	cfg := config.Default()

	// Check if ignore file already exists using the embedded Pack's method
	exists, err := p.HasIgnoreFile(fs, cfg)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrInternal, "failed to check for ignore file")
	}

	ignoreFilePath := p.GetFilePath(cfg.Patterns.SpecialFiles.IgnoreFile)

	if exists {
		logger.Info().
			Str("pack", p.Name).
			Str("path", ignoreFilePath).
			Msg("Ignore file already exists")

		// File already exists, build result
		result := buildAddIgnoreResult(opts, ignoreFilePath, false, true, fs)
		return result, nil
	}

	// Create the ignore file using the embedded Pack's method
	if err := p.CreateIgnoreFile(fs, cfg); err != nil {
		return nil, errors.Wrapf(err, errors.ErrInternal, "failed to create ignore file")
	}

	logger.Info().
		Str("pack", p.Name).
		Str("path", ignoreFilePath).
		Msg("Successfully created ignore file")

	// File created, build result
	result := buildAddIgnoreResult(opts, ignoreFilePath, true, false, fs)
	return result, nil
}

// buildAddIgnoreResult creates a PackCommandResult for addignore operations
func buildAddIgnoreResult(opts AddIgnoreOptions, ignoreFilePath string, created, alreadyExisted bool, fs types.FS) *types.PackCommandResult {
	logger := logging.GetLogger("pack.addignore")

	// Get current pack status if function provided
	var packStatus []types.DisplayPack
	if opts.GetPackStatus != nil {
		var statusErr error
		packStatus, statusErr = opts.GetPackStatus(opts.PackName, opts.DotfilesRoot, fs)
		if statusErr != nil {
			logger.Error().Err(statusErr).Msg("Failed to get pack status")
		}
	}

	// Build result
	result := &types.PackCommandResult{
		Command:   "add-ignore",
		Timestamp: time.Now(),
		DryRun:    false,
		Packs:     []types.DisplayPack{},
		Metadata: types.CommandMetadata{
			IgnoreCreated:  created,
			AlreadyExisted: alreadyExisted,
		},
	}

	// Copy packs from status if available
	if packStatus != nil {
		result.Packs = packStatus
	}

	// Generate message
	if alreadyExisted {
		result.Message = "A .dodotignore file already exists in the pack " + opts.PackName + "."
	} else {
		result.Message = "A .dodotignore file has been added to the pack " + opts.PackName + "."
	}

	return result
}
