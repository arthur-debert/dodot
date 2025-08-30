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

	// Normalize and validate pack name
	packName := strings.TrimRight(opts.PackName, "/")
	if packName == "" {
		return nil, errors.New(errors.ErrPackNotFound, "pack name cannot be empty")
	}
	if err := paths.ValidatePackName(packName); err != nil {
		return nil, errors.Wrap(err, errors.ErrPackNotFound, "invalid pack name")
	}

	// Find or create the target pack
	targetPack, err := core.FindPack(opts.DotfilesRoot, packName)
	if err != nil {
		// If pack doesn't exist, create it
		packPath := filepath.Join(opts.DotfilesRoot, packName)
		if err := fs.MkdirAll(packPath, 0755); err != nil {
			return nil, fmt.Errorf("failed to create pack directory: %w", err)
		}
		logger.Info().Str("pack", packName).Msg("Created pack directory")

		// Create a minimal pack instance
		targetPack = &types.Pack{
			Name: packName,
			Path: packPath,
		}
	}

	// Prepare result
	result := &types.AdoptResult{
		PackName:     targetPack.Name,
		AdoptedFiles: []types.AdoptedFile{},
	}

	// Process each source path
	for _, sourcePath := range opts.SourcePaths {
		adopted, err := adoptSingleFile(fs, logger, targetPack, sourcePath, opts.Force, opts.DotfilesRoot)
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

// adoptSingleFile handles the adoption of a single file using Pack guardian methods
func adoptSingleFile(fs types.FS, logger zerolog.Logger, pack *types.Pack, sourcePath string, force bool, dotfilesRoot string) (*types.AdoptedFile, error) {
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

	// Determine destination path within pack using paths mapping
	pathsInstance, err := paths.New(dotfilesRoot)
	if err != nil {
		return nil, fmt.Errorf("failed to initialize paths: %w", err)
	}

	// Map system file to pack location (this handles .gitconfig -> gitconfig mapping)
	fullDestPath := pathsInstance.MapSystemFileToPack(pack, expandedPath)
	internalPath, err := filepath.Rel(pack.Path, fullDestPath)
	if err != nil {
		return nil, fmt.Errorf("failed to determine relative path: %w", err)
	}

	// Use Pack's guardian method to adopt the file
	destPath, err := pack.AdoptFile(fs, expandedPath, internalPath, force)
	if err != nil {
		return nil, err
	}

	// Create symlink back to original location
	if err := fs.Symlink(destPath, expandedPath); err != nil {
		// If symlink fails, try to roll back by moving file back
		logger.Error().
			Err(err).
			Str("source", expandedPath).
			Str("destination", destPath).
			Msg("Failed to create symlink, attempting to roll back move")
		if rollbackErr := fs.Rename(destPath, expandedPath); rollbackErr != nil {
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
