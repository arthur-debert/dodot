package adopt

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/arthur-debert/synthfs/pkg/synthfs"
	"github.com/arthur-debert/synthfs/pkg/synthfs/filesystem"
	"github.com/rs/zerolog"
)

// AdoptFilesOptions holds options for the adopt command
type AdoptFilesOptions struct {
	DotfilesRoot string
	PackName     string
	SourcePaths  []string
	Force        bool
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
	targetPack, err = core.FindPack(pathsInstance.DotfilesRoot(), packName)
	if err != nil {
		// If pack doesn't exist, create it
		if dodotErr, ok := err.(*errors.DodotError); ok && dodotErr.Code == errors.ErrPackNotFound {
			// Create the pack directory
			if err := os.MkdirAll(packPath, 0755); err != nil {
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

	// Create synthfs instance for operations
	osfs := filesystem.NewOSFileSystem("/")
	pathAwareFS := synthfs.NewPathAwareFileSystem(osfs, "/").WithAbsolutePaths()
	sfs := synthfs.New()

	// Process each source path and collect operations
	var operations []synthfs.Operation
	for _, sourcePath := range opts.SourcePaths {
		ops, adopted, err := createAdoptOperations(sfs, logger, targetPack, sourcePath, opts.Force)
		if err != nil {
			logAdopt(logger, opts, result, err)
			return nil, fmt.Errorf("failed to prepare adoption of %s: %w", sourcePath, err)
		}
		if adopted != nil {
			result.AdoptedFiles = append(result.AdoptedFiles, *adopted)
			operations = append(operations, ops...)
		}
	}

	// Execute all operations atomically through synthfs
	if len(operations) > 0 {
		ctx := context.Background()
		synthfsOpts := synthfs.DefaultPipelineOptions()
		synthfsOpts.RollbackOnError = true

		_, err := synthfs.RunWithOptions(ctx, pathAwareFS, synthfsOpts, operations...)
		if err != nil {
			return nil, fmt.Errorf("failed to execute adopt operations: %w", err)
		}
	}

	logAdopt(logger, opts, result, nil)
	return result, nil
}

// createAdoptOperations creates synthfs operations for adopting a single file
func createAdoptOperations(sfs *synthfs.SynthFS, logger zerolog.Logger, pack *types.Pack, sourcePath string, force bool) ([]synthfs.Operation, *types.AdoptedFile, error) {
	// Expand the source path
	expandedPath := paths.ExpandHome(sourcePath)

	// Check if source exists
	sourceInfo, err := os.Lstat(expandedPath)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil, fmt.Errorf("source file does not exist: %s", expandedPath)
		}
		return nil, nil, fmt.Errorf("failed to stat source file: %w", err)
	}

	// Check if source is already a symlink managed by dodot
	if sourceInfo.Mode()&os.ModeSymlink != 0 {
		target, err := os.Readlink(expandedPath)
		if err != nil {
			return nil, nil, fmt.Errorf("failed to read symlink: %w", err)
		}

		// Check if it points to a file within dotfiles
		if strings.Contains(target, pack.Path) || strings.Contains(target, filepath.Dir(pack.Path)) {
			logger.Info().
				Str("source", expandedPath).
				Str("target", target).
				Msg("File is already managed by dodot")
			// Idempotent operation - not an error, just skip
			return nil, nil, nil
		}
	}

	// Determine destination path using smart path handling
	destPath := determineDestinationPath(pack.Path, expandedPath)

	// Check if destination already exists
	if _, err := os.Stat(destPath); err == nil && !force {
		return nil, nil, fmt.Errorf("destination already exists: %s (use --force to overwrite)", destPath)
	}

	// Create operations for the adoption
	var operations []synthfs.Operation

	// 1. Create destination directory if needed
	destDir := filepath.Dir(destPath)
	if _, err := os.Stat(destDir); os.IsNotExist(err) {
		mkdirOp := sfs.CreateDirWithID(fmt.Sprintf("adopt-mkdir-%s", filepath.Base(destDir)), destDir, 0755)
		operations = append(operations, mkdirOp)
	}

	// 2. Create a custom operation for the atomic move and symlink
	// This ensures the operation is atomic - either both succeed or both fail
	adoptOp := sfs.CustomOperationWithID(
		fmt.Sprintf("adopt-file-%s", filepath.Base(expandedPath)),
		func(ctx context.Context, fs filesystem.FileSystem) error {
			// Move the file
			if err := os.Rename(expandedPath, destPath); err != nil {
				// Handle cross-device moves
				if strings.Contains(err.Error(), "cross-device") {
					return fmt.Errorf("cross-device move not supported in initial implementation: %w", err)
				}
				return fmt.Errorf("failed to move file: %w", err)
			}

			// Create symlink back to original location
			if err := fs.Symlink(destPath, expandedPath); err != nil {
				// Try to move the file back on failure
				_ = os.Rename(destPath, expandedPath)
				return fmt.Errorf("failed to create symlink: %w", err)
			}

			logger.Info().
				Str("source", expandedPath).
				Str("destination", destPath).
				Msg("Successfully adopted file")
			return nil
		},
	)
	operations = append(operations, adoptOp)

	return operations, &types.AdoptedFile{
		OriginalPath: expandedPath,
		NewPath:      destPath,
		SymlinkPath:  expandedPath,
	}, nil
}

// determineDestinationPath applies smart path handling rules
func determineDestinationPath(packPath, sourcePath string) string {
	homeDir := os.Getenv("HOME")
	if homeDir == "" {
		homeDir = filepath.Dir(sourcePath) // Fallback
	}

	// Get XDG paths
	xdgConfigHome := os.Getenv("XDG_CONFIG_HOME")
	if xdgConfigHome == "" {
		xdgConfigHome = filepath.Join(homeDir, ".config")
	}

	// Get the base name of the file
	baseName := filepath.Base(sourcePath)

	// If file is directly in $HOME, place it at pack root
	if filepath.Dir(sourcePath) == homeDir {
		// Remove leading dot for cleaner pack organization
		if strings.HasPrefix(baseName, ".") && len(baseName) > 1 {
			baseName = baseName[1:]
		}
		return filepath.Join(packPath, baseName)
	}

	// If file is in XDG config path, preserve directory structure
	if strings.HasPrefix(sourcePath, xdgConfigHome) {
		// Get relative path from XDG_CONFIG_HOME
		relPath, err := filepath.Rel(xdgConfigHome, sourcePath)
		if err == nil {
			return filepath.Join(packPath, relPath)
		}
	}

	// For other paths, try to preserve some structure
	// This is a simple heuristic - could be improved
	if strings.Contains(sourcePath, "/.") {
		// Hidden directory somewhere in the path
		parts := strings.Split(sourcePath, "/")
		for i, part := range parts {
			if strings.HasPrefix(part, ".") && part != "." && i < len(parts)-1 {
				// Found a hidden directory, use everything after it
				subPath := filepath.Join(parts[i+1:]...)
				return filepath.Join(packPath, subPath)
			}
		}
	}

	// Default: just use the base name
	if strings.HasPrefix(baseName, ".") && len(baseName) > 1 {
		baseName = baseName[1:]
	}
	return filepath.Join(packPath, baseName)
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
