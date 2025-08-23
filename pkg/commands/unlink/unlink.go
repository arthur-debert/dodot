package unlink

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
)

// UnlinkPacksOptions contains options for the off command
type UnlinkPacksOptions struct {
	// DotfilesRoot is the path to the dotfiles directory
	DotfilesRoot string

	// DataDir is the dodot data directory
	DataDir string

	// PackNames is the list of pack names to turn off (empty = all)
	PackNames []string

	// Force skips confirmation prompts
	Force bool

	// DryRun shows what would be removed without actually removing
	DryRun bool
}

// UnlinkResult contains the result of the off command
type UnlinkResult struct {
	// Packs that were processed
	Packs []PackUnlinkResult `json:"packs"`

	// Total number of items removed
	TotalRemoved int `json:"totalRemoved"`

	// Whether this was a dry run
	DryRun bool `json:"dryRun"`
}

// PackUnlinkResult contains the result for a single pack
type PackUnlinkResult struct {
	// Name of the pack
	Name string `json:"name"`

	// Items that were removed
	RemovedItems []RemovedItem `json:"removedItems"`

	// Any errors encountered
	Errors []string `json:"errors,omitempty"`
}

// RemovedItem represents a single removed deployment
type RemovedItem struct {
	// Type of item (symlink, path, shell_profile, etc.)
	Type string `json:"type"`

	// Path that was removed
	Path string `json:"path"`

	// Target it pointed to (for symlinks)
	Target string `json:"target,omitempty"`

	// Whether removal succeeded
	Success bool `json:"success"`

	// Error if removal failed
	Error string `json:"error,omitempty"`
}

// UnlinkPacks removes deployments for the specified packs
//
// This command undoes the effects of linking handlers (symlink, path, shell_profile)
// but deliberately leaves provisioning handlers (provision, homebrew) untouched.
//
// The unlink process works as follows:
//
// 1. For symlink handlers:
//   - First reads all intermediate symlinks in packs/<pack>/symlinks/
//   - Removes the user-facing symlinks (e.g., ~/.vimrc) that point to them
//   - Then removes the intermediate symlinks
//
// 2. For path and shell_profile handlers:
//   - Simply removes their directories (packs/<pack>/path/, packs/<pack>/shell_profile/)
//   - The shell integration will automatically stop including them
//
// 3. For provisioning handlers:
//   - Leaves packs/<pack>/sentinels/ untouched
//   - These track what has been installed and should persist
//
// This design allows users to "turn off" a pack's active configurations
// without forgetting what software was installed by that pack.
func UnlinkPacks(opts UnlinkPacksOptions) (*UnlinkResult, error) {
	logger := logging.GetLogger("commands.unlink")
	logger.Debug().
		Str("dotfilesRoot", opts.DotfilesRoot).
		Strs("packNames", opts.PackNames).
		Bool("dryRun", opts.DryRun).
		Bool("force", opts.Force).
		Msg("Starting unlink command")

	// Initialize paths
	pathsInstance, err := paths.New(opts.DotfilesRoot)
	if err != nil {
		return nil, fmt.Errorf("failed to initialize paths: %w", err)
	}

	// Initialize filesystem
	fs := filesystem.NewOS()

	// Create datastore
	dataStore := datastore.New(fs, pathsInstance)

	// Discover packs
	selectedPacks, err := core.DiscoverAndSelectPacks(opts.DotfilesRoot, opts.PackNames)
	if err != nil {
		return nil, err
	}

	logger.Info().
		Int("packCount", len(selectedPacks)).
		Msg("Found packs to unlink")

	// Initialize result
	result := &UnlinkResult{
		DryRun:       opts.DryRun,
		Packs:        []PackUnlinkResult{},
		TotalRemoved: 0,
	}

	// Process each pack
	for _, pack := range selectedPacks {
		packResult, err := unlinkPack(pack, dataStore, fs, pathsInstance, opts.DryRun)
		if err != nil {
			logger.Error().
				Err(err).
				Str("pack", pack.Name).
				Msg("Failed to unlink pack")
			// Add error to result but continue with other packs
			packResult = PackUnlinkResult{
				Name:   pack.Name,
				Errors: []string{err.Error()},
			}
		}
		result.Packs = append(result.Packs, packResult)
		result.TotalRemoved += len(packResult.RemovedItems)
	}

	return result, nil
}

// unlinkPack removes all linking handler deployments for a single pack
func unlinkPack(pack types.Pack, dataStore datastore.DataStore, fs types.FS, pathsInstance paths.Paths, dryRun bool) (PackUnlinkResult, error) {
	logger := logging.GetLogger("commands.unlink").With().
		Str("pack", pack.Name).
		Logger()

	result := PackUnlinkResult{
		Name:         pack.Name,
		RemovedItems: []RemovedItem{},
		Errors:       []string{},
	}

	// Step 1: Handle symlinks - need to remove user-facing symlinks first
	symlinksDir := pathsInstance.PackHandlerDir(pack.Name, "symlinks")
	if entries, err := fs.ReadDir(symlinksDir); err == nil {
		logger.Debug().
			Int("count", len(entries)).
			Msg("Found symlinks to remove")

		for _, entry := range entries {
			if err := removeSymlink(pack, entry.Name(), fs, pathsInstance, &result, dryRun); err != nil {
				logger.Error().
					Err(err).
					Str("file", entry.Name()).
					Msg("Failed to remove symlink")
				result.Errors = append(result.Errors, fmt.Sprintf("symlink %s: %v", entry.Name(), err))
			}
		}
	} else if !os.IsNotExist(err) {
		logger.Debug().Err(err).Msg("Failed to read symlinks directory")
	}

	// Step 2: Remove linking handler directories
	// IMPORTANT: We do this AFTER processing symlinks so we can read their targets
	linkingHandlers := []string{"symlinks", "path", "shell_profile"}
	for _, handler := range linkingHandlers {
		handlerDir := pathsInstance.PackHandlerDir(pack.Name, handler)

		// Check if directory exists
		if _, err := fs.Stat(handlerDir); err != nil {
			if os.IsNotExist(err) {
				logger.Debug().
					Str("handler", handler).
					Msg("Handler directory doesn't exist, skipping")
				continue
			}
			logger.Error().
				Err(err).
				Str("handler", handler).
				Msg("Failed to stat handler directory")
			continue
		}

		// Remove the directory
		if !dryRun {
			if err := fs.RemoveAll(handlerDir); err != nil {
				logger.Error().
					Err(err).
					Str("handler", handler).
					Msg("Failed to remove handler directory")
				result.Errors = append(result.Errors, fmt.Sprintf("remove %s dir: %v", handler, err))
				continue
			}
		}

		logger.Info().
			Str("handler", handler).
			Bool("dryRun", dryRun).
			Msg("Removed handler directory")

		// Add to result
		result.RemovedItems = append(result.RemovedItems, RemovedItem{
			Type:    handler + "_directory",
			Path:    handlerDir,
			Success: true,
		})
	}

	return result, nil
}

// removeSymlink removes a single symlink deployment
func removeSymlink(pack types.Pack, filename string, fs types.FS, pathsInstance paths.Paths, result *PackUnlinkResult, dryRun bool) error {
	logger := logging.GetLogger("commands.unlink").With().
		Str("pack", pack.Name).
		Str("file", filename).
		Logger()

	// Get the intermediate symlink path
	intermediatePath := filepath.Join(pathsInstance.PackHandlerDir(pack.Name, "symlinks"), filename)

	// Read where it points to find the source file
	sourceFile, err := fs.Readlink(intermediatePath)
	if err != nil {
		logger.Error().
			Err(err).
			Str("intermediatePath", intermediatePath).
			Msg("Failed to read intermediate link")
		return fmt.Errorf("failed to read intermediate link: %w", err)
	}

	logger.Debug().
		Str("intermediatePath", intermediatePath).
		Str("sourceFile", sourceFile).
		Msg("Read intermediate link")

	// Find the user-facing symlink
	// We need to reconstruct what the target would have been
	// This matches the logic in the symlink handler
	var targetPath string
	if pathsInstance != nil {
		// Use centralized mapping
		targetPath = pathsInstance.MapPackFileToSystem(&pack, filename)
	} else {
		// Fallback
		homeDir, _ := paths.GetHomeDirectory()
		targetPath = filepath.Join(homeDir, filename)
	}

	// Check if the user-facing symlink exists and points to our intermediate
	if linkTarget, err := fs.Readlink(targetPath); err == nil {
		logger.Debug().
			Str("targetPath", targetPath).
			Str("linkTarget", linkTarget).
			Str("intermediatePath", intermediatePath).
			Msg("Checking if symlink points to our intermediate")

		// Verify it points to our intermediate link
		if linkTarget == intermediatePath {
			// Remove the user-facing symlink
			if !dryRun {
				if err := fs.Remove(targetPath); err != nil {
					logger.Error().
						Err(err).
						Str("target", targetPath).
						Msg("Failed to remove user-facing symlink")
					result.RemovedItems = append(result.RemovedItems, RemovedItem{
						Type:    "symlink",
						Path:    targetPath,
						Target:  sourceFile,
						Success: false,
						Error:   err.Error(),
					})
				} else {
					logger.Info().
						Str("target", targetPath).
						Bool("dryRun", dryRun).
						Msg("Removed user-facing symlink")
					result.RemovedItems = append(result.RemovedItems, RemovedItem{
						Type:    "symlink",
						Path:    targetPath,
						Target:  sourceFile,
						Success: true,
					})
				}
			} else {
				// Dry run - just record what would be removed
				result.RemovedItems = append(result.RemovedItems, RemovedItem{
					Type:    "symlink",
					Path:    targetPath,
					Target:  sourceFile,
					Success: true,
				})
			}
		} else {
			logger.Warn().
				Str("target", targetPath).
				Str("expected", intermediatePath).
				Str("actual", linkTarget).
				Msg("User-facing symlink points elsewhere, not removing")
		}
	} else if !os.IsNotExist(err) {
		logger.Debug().
			Err(err).
			Str("target", targetPath).
			Msg("Failed to read user-facing symlink")
	}

	return nil
}
