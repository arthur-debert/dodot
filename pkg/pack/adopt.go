package pack

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/rs/zerolog"
)

// AdoptOptions contains options for the Adopt operation
type AdoptOptions struct {
	// SourcePaths are the files to adopt into the pack
	SourcePaths []string
	// Force overwrites existing files in the pack
	Force bool
	// DotfilesRoot is the root directory for dotfiles
	DotfilesRoot string
}

// AdoptResult represents the result of adopting files into a pack
type AdoptResult struct {
	PackName     string        `json:"packName"`
	AdoptedFiles []AdoptedFile `json:"adoptedFiles"`
}

// AdoptedFile represents a file that was adopted into a pack
type AdoptedFile struct {
	OriginalPath string `json:"originalPath"`
	NewPath      string `json:"newPath"`
	SymlinkPath  string `json:"symlinkPath"`
}

// Adopt moves existing files into the pack and creates symlinks back to their original locations.
// This allows existing configuration files to be managed by dodot without disrupting their use.
func (p *Pack) Adopt(fs types.FS, opts AdoptOptions) (*AdoptResult, error) {
	logger := logging.GetLogger("pack.adopt")
	logger.Info().
		Str("pack", p.Name).
		Strs("source_paths", opts.SourcePaths).
		Bool("force", opts.Force).
		Msg("Adopting files into pack")

	// Prepare result
	result := &AdoptResult{
		PackName:     p.Name,
		AdoptedFiles: []AdoptedFile{},
	}

	// Process each source path
	for _, sourcePath := range opts.SourcePaths {
		adopted, err := p.adoptSingleFile(fs, logger, sourcePath, opts.Force, opts.DotfilesRoot)
		if err != nil {
			return nil, fmt.Errorf("failed to adopt %s: %w", sourcePath, err)
		}
		if adopted != nil {
			result.AdoptedFiles = append(result.AdoptedFiles, *adopted)
		}
	}

	logger.Info().
		Str("pack", p.Name).
		Int("files_adopted", len(result.AdoptedFiles)).
		Msg("Adopt operation completed")

	return result, nil
}

// adoptSingleFile handles the adoption of a single file using the embedded Pack's guardian methods
func (p *Pack) adoptSingleFile(fs types.FS, logger zerolog.Logger, sourcePath string, force bool, dotfilesRoot string) (*AdoptedFile, error) {
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
		if strings.Contains(target, p.Path) || strings.Contains(target, filepath.Dir(p.Path)) {
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
	fullDestPath := pathsInstance.MapSystemFileToPack(p.Pack, expandedPath)
	internalPath, err := filepath.Rel(p.Path, fullDestPath)
	if err != nil {
		return nil, fmt.Errorf("failed to determine relative path: %w", err)
	}

	// Use the embedded Pack's guardian method to adopt the file
	destPath, err := p.AdoptFile(fs, expandedPath, internalPath, force)
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

	return &AdoptedFile{
		OriginalPath: expandedPath,
		NewPath:      destPath,
		SymlinkPath:  expandedPath,
	}, nil
}

// AdoptOrCreate creates a pack if it doesn't exist before adopting files.
// This is a static method since we might need to create the pack first.
func AdoptOrCreate(fs types.FS, dotfilesRoot, packName string, opts AdoptOptions) (*AdoptResult, error) {
	logger := logging.GetLogger("pack.adopt")

	// Normalize and validate pack name
	packName = strings.TrimRight(packName, "/")
	if packName == "" {
		return nil, errors.New(errors.ErrPackNotFound, "pack name cannot be empty")
	}
	if err := paths.ValidatePackName(packName); err != nil {
		return nil, errors.Wrap(err, errors.ErrPackNotFound, "invalid pack name")
	}

	// Check if pack exists
	packPath := filepath.Join(dotfilesRoot, packName)
	if _, err := fs.Stat(packPath); os.IsNotExist(err) {
		// Create pack directory
		if err := fs.MkdirAll(packPath, 0755); err != nil {
			return nil, fmt.Errorf("failed to create pack directory: %w", err)
		}
		logger.Info().Str("pack", packName).Msg("Created pack directory")
	}

	// Create pack instance
	p := &Pack{
		Pack: &types.Pack{
			Name: packName,
			Path: packPath,
		},
	}

	// Delegate to Adopt method
	return p.Adopt(fs, opts)
}
