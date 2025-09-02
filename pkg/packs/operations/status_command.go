package operations

import (
	"fmt"
	"time"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/arthur-debert/dodot/pkg/ui/display"
)

// StatusCommandOptions contains options for the status command
type StatusCommandOptions struct {
	// DotfilesRoot is the root directory containing packs
	DotfilesRoot string

	// PackNames specifies which packs to check status for
	// If empty, all packs are checked
	PackNames []string

	// Paths provides system paths (optional, will be created if not provided)
	Paths types.Pather

	// FileSystem to use (optional, defaults to OS filesystem)
	FileSystem types.FS
}

// GetPacksStatus shows the deployment status of specified packs
// This is a query operation that uses core pack discovery but doesn't execute handlers.
func GetPacksStatus(opts StatusCommandOptions) (*display.PackCommandResult, error) {
	logger := logging.GetLogger("pack.status")
	logger.Debug().
		Str("dotfilesRoot", opts.DotfilesRoot).
		Strs("packNames", opts.PackNames).
		Msg("Starting status command")

	// Track any errors encountered
	var errors []error

	// Initialize filesystem if not provided
	if opts.FileSystem == nil {
		opts.FileSystem = filesystem.NewOS()
	}

	// Initialize paths if not provided
	if opts.Paths == nil {
		p, err := paths.New(opts.DotfilesRoot)
		if err != nil {
			return nil, fmt.Errorf("failed to initialize paths: %w", err)
		}
		opts.Paths = p
	}

	// Use core pack discovery (consistent with on/off commands)
	selectedPacks, err := core.DiscoverAndSelectPacksFS(opts.DotfilesRoot, opts.PackNames, opts.FileSystem)
	if err != nil {
		logger.Error().Err(err).Msg("Failed to discover packs")
		return nil, err
	}

	logger.Info().
		Int("packCount", len(selectedPacks)).
		Msg("Found packs to check")

	// Create datastore for status checking
	dataStore := datastore.New(opts.FileSystem, opts.Paths.(paths.Paths))

	// Build command result
	result := &display.PackCommandResult{
		Command:   "status",
		DryRun:    false, // Status is always a query, never a dry run
		Timestamp: time.Now(),
		Packs:     make([]display.DisplayPack, 0, len(selectedPacks)),
		// Status command doesn't have a message
		Message: "",
	}

	// Process each pack using centralized status logic
	for _, p := range selectedPacks {
		// Get pack status using the centralized pack.GetStatus function
		statusOpts := StatusOptions{
			Pack:       p,
			DataStore:  dataStore,
			FileSystem: opts.FileSystem,
			Paths:      opts.Paths.(paths.Paths),
		}

		packStatus, err := GetStatus(statusOpts)
		if err != nil {
			logger.Error().
				Err(err).
				Str("pack", p.Name).
				Msg("Failed to get pack status")
			errors = append(errors, fmt.Errorf("pack %s: status check failed: %w", p.Name, err))
			// Continue with other packs even if one fails
			continue
		}

		// Convert to display format using existing conversion logic
		displayPack := convertStatusToDisplayPack(packStatus)
		result.Packs = append(result.Packs, displayPack)
	}

	logger.Info().
		Int("packsProcessed", len(result.Packs)).
		Int("errors", len(errors)).
		Msg("Status command completed")

	// Return error if any packs failed (but still return partial results)
	if len(errors) > 0 {
		return result, fmt.Errorf("status command encountered %d errors", len(errors))
	}

	return result, nil
}

// convertStatusToDisplayPack converts pack.StatusResult to display.DisplayPack
func convertStatusToDisplayPack(status *StatusResult) display.DisplayPack {
	displayPack := display.DisplayPack{
		Name:      status.Name,
		HasConfig: status.HasConfig,
		IsIgnored: status.IsIgnored,
		Status:    status.Status,
		Files:     make([]display.DisplayFile, 0, len(status.Files)),
	}

	// Convert each file status
	for _, file := range status.Files {
		displayFile := display.DisplayFile{
			Handler:        file.Handler,
			Path:           file.Path,
			Status:         statusStateToDisplayStatus(file.Status.State),
			Message:        file.Status.Message,
			LastExecuted:   file.Status.Timestamp,
			HandlerSymbol:  display.GetHandlerSymbol(file.Handler),
			AdditionalInfo: file.AdditionalInfo,
		}
		displayPack.Files = append(displayPack.Files, displayFile)
	}

	// Add special files if present
	if status.IsIgnored {
		displayPack.Files = append([]display.DisplayFile{{
			Path:   ".dodotignore",
			Status: "ignored",
		}}, displayPack.Files...)
	}
	if status.HasConfig {
		displayPack.Files = append([]display.DisplayFile{{
			Path:   ".dodot.toml",
			Status: "config",
		}}, displayPack.Files...)
	}

	return displayPack
}
