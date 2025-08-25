// Package status provides the status command implementation for dodot.
//
// The status command shows the deployment state of packs and files,
// answering two key questions:
//   - What has already been deployed? (current state)
//   - What will happen if I deploy? (predicted state)
package status

import (
	"fmt"
	"os"
	"path/filepath"
	"time"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
)

// StatusPacksOptions contains options for the status command
type StatusPacksOptions struct {
	// DotfilesRoot is the root directory containing packs
	DotfilesRoot string

	// PackNames specifies which packs to check status for
	// If empty, all packs are checked
	PackNames []string

	// Paths provides system paths (required)
	Paths types.Pather

	// FileSystem to use (defaults to OS filesystem)
	FileSystem types.FS
}

// StatusPacks shows the deployment status of specified packs
func StatusPacks(opts StatusPacksOptions) (*types.DisplayResult, error) {
	logger := logging.GetLogger("commands.status")
	logger.Debug().
		Str("dotfilesRoot", opts.DotfilesRoot).
		Strs("packNames", opts.PackNames).
		Msg("Starting status command")

	// Initialize filesystem if not provided
	if opts.FileSystem == nil {
		opts.FileSystem = filesystem.NewOS()
	}

	// Use centralized pack discovery and selection with filesystem support
	selectedPacks, err := core.DiscoverAndSelectPacksFS(opts.DotfilesRoot, opts.PackNames, opts.FileSystem)
	if err != nil {
		return nil, err
	}

	logger.Info().
		Int("packCount", len(selectedPacks)).
		Msg("Found packs to check")

	// Create datastore for status checking
	dataStore := datastore.New(opts.FileSystem, opts.Paths.(paths.Paths))

	// Build display result
	result := &types.DisplayResult{
		Command:   "status",
		DryRun:    false,
		Timestamp: time.Now(),
		Packs:     make([]types.DisplayPack, 0, len(selectedPacks)),
	}

	// Process each pack
	for _, pack := range selectedPacks {
		displayPack, err := getPackDisplayStatus(pack, dataStore, opts.FileSystem)
		if err != nil {
			logger.Error().
				Err(err).
				Str("pack", pack.Name).
				Msg("Failed to get pack status")
			// Continue with other packs even if one fails
			continue
		}
		result.Packs = append(result.Packs, *displayPack)
	}

	return result, nil
}

// getPackDisplayStatus generates display status for a single pack
func getPackDisplayStatus(pack types.Pack, dataStore types.DataStore, fs types.FS) (*types.DisplayPack, error) {
	logger := logging.GetLogger("commands.status").With().
		Str("pack", pack.Name).
		Logger()

	displayPack := &types.DisplayPack{
		Name:      pack.Name,
		Files:     []types.DisplayFile{},
		HasConfig: false,
		IsIgnored: false,
	}

	// Check for special files (.dodot.toml, .dodotignore)
	if err := checkSpecialFiles(pack, displayPack, fs); err != nil {
		return nil, err
	}

	// If pack is ignored, no need to process actions
	if displayPack.IsIgnored {
		displayPack.Status = "ignored"
		return displayPack, nil
	}

	// Get all trigger matches for this pack
	matches, err := core.ProcessPackTriggersFS(pack, fs)
	if err != nil {
		return nil, fmt.Errorf("failed to process triggers: %w", err)
	}

	// Convert matches to actions
	actions, err := core.GetActions(matches)
	if err != nil {
		return nil, fmt.Errorf("failed to get actions: %w", err)
	}

	logger.Debug().
		Int("matchCount", len(matches)).
		Int("actionCount", len(actions)).
		Msg("Processing actions for status")

	// Process each action
	for _, action := range actions {
		// Get the deployment status from datastore
		status, err := getActionStatus(action, dataStore)
		if err != nil {
			logger.Error().
				Err(err).
				Str("action", action.Description()).
				Msg("Failed to get action status")
			// Add error status for this file
			status = types.Status{
				State:   types.StatusStateError,
				Message: fmt.Sprintf("status check failed: %v", err),
			}
		}

		// Convert to display format
		displayFile := types.DisplayFile{
			Handler:        getActionHandler(action),
			Path:           getActionFilePath(action),
			Status:         statusStateToDisplayStatus(status.State),
			Message:        status.Message,
			LastExecuted:   status.Timestamp,
			HandlerSymbol:  types.GetHandlerSymbol(getActionHandler(action)),
			AdditionalInfo: getActionAdditionalInfo(action),
		}

		displayPack.Files = append(displayPack.Files, displayFile)
	}

	// Calculate aggregated pack status
	displayPack.Status = displayPack.GetPackStatus()

	logger.Debug().
		Str("status", displayPack.Status).
		Int("fileCount", len(displayPack.Files)).
		Msg("Pack status determined")

	return displayPack, nil
}

// checkSpecialFiles checks for .dodot.toml and .dodotignore files
func checkSpecialFiles(pack types.Pack, displayPack *types.DisplayPack, fs types.FS) error {
	// Check for .dodotignore
	ignorePath := filepath.Join(pack.Path, ".dodotignore")
	if _, err := fs.Stat(ignorePath); err == nil {
		displayPack.IsIgnored = true
		displayPack.Files = append(displayPack.Files, types.DisplayFile{
			Path:   ".dodotignore",
			Status: "ignored",
		})
	}

	// Check for .dodot.toml
	configPath := filepath.Join(pack.Path, ".dodot.toml")
	if _, err := fs.Stat(configPath); err == nil {
		displayPack.HasConfig = true
		displayPack.Files = append(displayPack.Files, types.DisplayFile{
			Path:   ".dodot.toml",
			Status: "config",
		})
	} else if !os.IsNotExist(err) {
		return fmt.Errorf("failed to check config file: %w", err)
	}

	return nil
}
