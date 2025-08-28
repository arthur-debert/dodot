package adopt

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/rs/zerolog"
)

// AdoptFilesOptions holds options for the adopt command
type AdoptFilesOptions struct {
	DotfilesRoot string
	PackName     string
	SourcePaths  []string
	Force        bool
	FileSystem   types.FS // Allow injecting a filesystem for testing
}

// AdoptFiles moves existing files into a pack and creates symlinks back to their original locations
func AdoptFiles(opts AdoptFilesOptions) (*types.AdoptResult, error) {
	logger := logging.GetLogger("commands.adopt")
	logger.Info().
		Str("pack", opts.PackName).
		Str("dotfiles_root", opts.DotfilesRoot).
		Strs("source_paths", opts.SourcePaths).
		Bool("force", opts.Force).
		Msg("Adopting files into pack")

	// Use provided filesystem or default to OS
	fs := opts.FileSystem
	if fs == nil {
		fs = filesystem.NewOS()
	}

	// Normalize pack name (remove trailing slashes from shell completion)
	packName := strings.TrimRight(opts.PackName, "/")

	// Validate pack name
	if err := paths.ValidatePackName(packName); err != nil {
		return nil, errors.Wrap(err, errors.ErrPackNotFound, "invalid pack name")
	}

	// Initialize paths
	pathsInstance, err := paths.New(opts.DotfilesRoot)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrInternal, "failed to initialize paths")
	}

	// Try to find the pack first
	var targetPack *types.Pack
	packPath := filepath.Join(pathsInstance.DotfilesRoot(), packName)

	// Check if pack exists using core infrastructure
	targetPack, err = core.FindPackFS(pathsInstance.DotfilesRoot(), packName, fs)
	if err != nil {
		// If pack doesn't exist, create it
		if dodotErr, ok := err.(*errors.DodotError); ok && dodotErr.Code == errors.ErrPackNotFound {
			// Create the pack directory
			if err := fs.MkdirAll(packPath, 0755); err != nil {
				return nil, fmt.Errorf("failed to create pack directory: %w", err)
			}
			logger.Info().Str("pack", packName).Msg("Created pack directory")

			// Create a minimal pack instance
			targetPack = &types.Pack{
				Name: packName,
				Path: packPath,
			}
		} else {
			return nil, err
		}
	}

	// Prepare result
	result := &types.AdoptResult{
		PackName:     packName,
		AdoptedFiles: []types.AdoptedFile{},
	}

	// Process each source path
	for _, sourcePath := range opts.SourcePaths {
		adopted, err := adoptSingleFile(fs, logger, pathsInstance, targetPack, sourcePath, opts.Force)
		if err != nil {
			logAdopt(logger, opts, result, err)
			return nil, fmt.Errorf("failed to adopt %s: %w", sourcePath, err)
		}
		if adopted != nil {
			result.AdoptedFiles = append(result.AdoptedFiles, *adopted)
		}
	}

	logAdopt(logger, opts, result, nil)
	return result, nil
}

// adoptSingleFile handles the adoption of a single file, performing the move and symlink.
func adoptSingleFile(fs types.FS, logger zerolog.Logger, pathsInstance paths.Paths, pack *types.Pack, sourcePath string, force bool) (*types.AdoptedFile, error) {
	// Expand the source path
	expandedPath := paths.ExpandHome(sourcePath)

	// Check if source exists
	sourceInfo, err := fs.Lstat(expandedPath)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, fmt.Errorf("source file does not exist: %s", expandedPath)
		}
		return nil, fmt.Errorf("failed to stat source file: %w", err)
	}

	// Check if source is already a symlink managed by dodot
	if sourceInfo.Mode()&os.ModeSymlink != 0 {
		target, err := fs.Readlink(expandedPath)
		if err != nil {
			return nil, fmt.Errorf("failed to read symlink: %w", err)
		}

		// Check if it points to a file within dotfiles
		if strings.Contains(target, pack.Path) || strings.Contains(target, filepath.Dir(pack.Path)) {
			logger.Info().
				Str("source", expandedPath).
				Str("target", target).
				Msg("File is already managed by dodot, skipping")
			return nil, nil // Idempotent, not an error
		}
	}

	// Determine destination path
	destPath := pathsInstance.MapSystemFileToPack(pack, expandedPath)

	// Check if destination already exists
	if _, err := fs.Stat(destPath); err == nil && !force {
		return nil, fmt.Errorf("destination already exists: %s (use --force to overwrite)", destPath)
	}

	// Create destination directory if needed
	destDir := filepath.Dir(destPath)
	if _, err := fs.Stat(destDir); os.IsNotExist(err) {
		if err := fs.MkdirAll(destDir, 0755); err != nil {
			return nil, fmt.Errorf("failed to create destination directory %s: %w", destDir, err)
		}
	}

	// Perform the move operation
	// Note: os.Rename is used here for atomicity on POSIX systems.
	// A cross-filesystem implementation would require copy+delete.
	if err := os.Rename(expandedPath, destPath); err != nil {
		return nil, fmt.Errorf("failed to move file from %s to %s: %w", expandedPath, destPath, err)
	}

	// Create symlink back to original location
	if err := fs.Symlink(destPath, expandedPath); err != nil {
		// If symlink fails, try to roll back the move.
		logger.Error().
			Err(err).
			Str("source", expandedPath).
			Str("destination", destPath).
			Msg("Failed to create symlink, attempting to roll back move")
		if rollbackErr := os.Rename(destPath, expandedPath); rollbackErr != nil {
			logger.Error().
				Err(rollbackErr).
				Msg("Failed to roll back move operation")
			return nil, fmt.Errorf("failed to create symlink and also failed to roll back the move: %w", err)
		}
		return nil, fmt.Errorf("failed to create symlink: %w", err)
	}

	logger.Info().
		Str("source", expandedPath).
		Str("destination", destPath).
		Msg("Successfully adopted file")

	return &types.AdoptedFile{
		OriginalPath: expandedPath,
		NewPath:      destPath,
		SymlinkPath:  expandedPath,
	}, nil
}

// logAdopt logs the adopt command execution
func logAdopt(logger zerolog.Logger, opts AdoptFilesOptions, result *types.AdoptResult, err error) {
	event := logger.Info()
	if err != nil {
		event = logger.Error().Err(err)
	}

	event.
		Str("command", "adopt").
		Str("pack", opts.PackName).
		Str("dotfiles_root", opts.DotfilesRoot).
		Strs("source_paths", opts.SourcePaths).
		Bool("force", opts.Force)

	if result != nil {
		event.Int("files_adopted", len(result.AdoptedFiles))
	}

	if err != nil {
		event.Msg("Adopt command failed")
	} else {
		event.Msg("Adopt command completed")
	}
}
