package off

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
)

// OffPacksOptions contains options for the off command
type OffPacksOptions struct {
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

// OffResult contains the result of the off command
type OffResult struct {
	// Packs that were processed
	Packs []PackOffResult `json:"packs"`

	// Total number of items removed
	TotalRemoved int `json:"totalRemoved"`

	// Whether this was a dry run
	DryRun bool `json:"dryRun"`
}

// PackOffResult contains the result for a single pack
type PackOffResult struct {
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

// OffPacks removes deployments for the specified packs
func OffPacks(opts OffPacksOptions) (*OffResult, error) {
	logger := logging.GetLogger("commands.off")
	logger.Info().
		Strs("packs", opts.PackNames).
		Bool("dryRun", opts.DryRun).
		Msg("Starting off command")

	// Create filesystem
	fs := filesystem.NewOS()

	// Discover and select packs
	selectedPacks, err := core.DiscoverAndSelectPacks(opts.DotfilesRoot, opts.PackNames)
	if err != nil {
		return nil, err
	}

	if len(selectedPacks) == 0 {
		return nil, errors.New(errors.ErrPackNotFound, "no packs found to process")
	}

	// Process each pack
	result := &OffResult{
		Packs:  []PackOffResult{},
		DryRun: opts.DryRun,
	}

	for _, pack := range selectedPacks {
		packResult := processPackOff(pack, fs, opts)
		result.Packs = append(result.Packs, packResult)
		result.TotalRemoved += len(packResult.RemovedItems)
	}

	logger.Info().
		Int("totalRemoved", result.TotalRemoved).
		Int("packCount", len(result.Packs)).
		Msg("Off command completed")

	return result, nil
}

// processPackOff removes all deployments for a single pack
func processPackOff(pack types.Pack, fs types.FS, opts OffPacksOptions) PackOffResult {
	logger := logging.GetLogger("commands.off").With().
		Str("pack", pack.Name).
		Logger()

	result := PackOffResult{
		Name:         pack.Name,
		RemovedItems: []RemovedItem{},
		Errors:       []string{},
	}

	// Get all possible actions for this pack to know what to look for
	triggers, err := core.GetFiringTriggersFS([]types.Pack{pack}, fs)
	if err != nil {
		result.Errors = append(result.Errors, fmt.Sprintf("failed to get triggers: %v", err))
		return result
	}

	actions, err := core.GetActions(triggers)
	if err != nil {
		result.Errors = append(result.Errors, fmt.Sprintf("failed to get actions: %v", err))
		return result
	}

	// Process each action to find and remove its deployments
	for _, action := range actions {
		items := findAndRemoveDeployments(action, fs, opts)
		result.RemovedItems = append(result.RemovedItems, items...)
	}

	// Clean up pack-specific state files (sentinels, etc.)
	stateItems := cleanupPackState(pack, fs, opts)
	result.RemovedItems = append(result.RemovedItems, stateItems...)

	logger.Info().
		Int("removedCount", len(result.RemovedItems)).
		Msg("Pack off completed")

	return result
}

// findAndRemoveDeployments finds and removes deployments for an action
func findAndRemoveDeployments(action types.Action, fs types.FS, opts OffPacksOptions) []RemovedItem {
	logger := logging.GetLogger("commands.off")
	items := []RemovedItem{}

	switch action.Type {
	case types.ActionTypeLink:
		// Remove deployed symlink
		if action.Target != "" {
			target := paths.ExpandHome(action.Target)
			if item := removeIfExists(target, "symlink", fs, opts); item != nil {
				items = append(items, *item)
			}
		}

		// Remove intermediate symlink
		intermediatePath := filepath.Join(opts.DataDir, "deployed", "symlink", filepath.Base(action.Target))
		if item := removeIfExists(intermediatePath, "intermediate", fs, opts); item != nil {
			items = append(items, *item)
		}

	case types.ActionTypePathAdd:
		// Remove from deployed/path
		deployedPath := filepath.Join(opts.DataDir, "deployed", "path", filepath.Base(action.Source))
		if item := removeIfExists(deployedPath, "path", fs, opts); item != nil {
			items = append(items, *item)
		}

	case types.ActionTypeShellSource:
		// Remove from deployed/shell_profile or shell_source
		baseName := filepath.Base(action.Source)
		for _, subdir := range []string{"shell_profile", "shell_source"} {
			deployedPath := filepath.Join(opts.DataDir, "deployed", subdir, baseName)
			if item := removeIfExists(deployedPath, subdir, fs, opts); item != nil {
				items = append(items, *item)
			}
		}

	default:
		logger.Debug().
			Str("actionType", string(action.Type)).
			Msg("No deployment removal needed for action type")
	}

	return items
}

// cleanupPackState removes pack-specific state files
func cleanupPackState(pack types.Pack, fs types.FS, opts OffPacksOptions) []RemovedItem {
	items := []RemovedItem{}

	// Remove install script sentinels
	installSentinel := filepath.Join(opts.DataDir, "install", "sentinels", pack.Name)
	if item := removeIfExists(installSentinel, "install_sentinel", fs, opts); item != nil {
		items = append(items, *item)
	}

	// Remove homebrew sentinels
	brewSentinel := filepath.Join(opts.DataDir, "homebrew", pack.Name)
	if item := removeIfExists(brewSentinel, "brew_sentinel", fs, opts); item != nil {
		items = append(items, *item)
	}

	return items
}

// removeIfExists removes a file/directory if it exists
func removeIfExists(path, itemType string, fs types.FS, opts OffPacksOptions) *RemovedItem {
	logger := logging.GetLogger("commands.off")

	// Check if item exists
	info, err := fs.Lstat(path)
	if err != nil {
		if os.IsNotExist(err) {
			return nil // Nothing to remove
		}
		return &RemovedItem{
			Type:    itemType,
			Path:    path,
			Success: false,
			Error:   fmt.Sprintf("failed to stat: %v", err),
		}
	}

	// Get target for symlinks
	var target string
	if info.Mode()&os.ModeSymlink != 0 {
		target, _ = fs.Readlink(path)
	}

	item := &RemovedItem{
		Type:    itemType,
		Path:    path,
		Target:  target,
		Success: true,
	}

	// Skip actual removal if dry run
	if opts.DryRun {
		logger.Info().
			Str("path", path).
			Str("type", itemType).
			Msg("Would remove (dry run)")
		return item
	}

	// Remove the item
	err = fs.Remove(path)
	if err != nil {
		item.Success = false
		item.Error = err.Error()
		logger.Error().
			Err(err).
			Str("path", path).
			Msg("Failed to remove item")
	} else {
		logger.Info().
			Str("path", path).
			Str("type", itemType).
			Msg("Removed item")
	}

	return item
}
