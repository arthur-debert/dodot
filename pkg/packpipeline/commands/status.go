package commands

import (
	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/packcommands"
	"github.com/arthur-debert/dodot/pkg/packpipeline"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
)

// StatusCommand implements the "status" command using the pack pipeline.
type StatusCommand struct{}

// Name returns the command name.
func (c *StatusCommand) Name() string {
	return "status"
}

// ExecuteForPack executes the "status" command for a single pack.
func (c *StatusCommand) ExecuteForPack(pack types.Pack, opts packpipeline.Options) (*packpipeline.PackResult, error) {
	logger := logging.GetLogger("packpipeline.status")
	logger.Debug().
		Str("pack", pack.Name).
		Msg("Executing status command for pack")

	// Initialize filesystem
	fs := opts.FileSystem
	if fs == nil {
		fs = filesystem.NewOS()
	}

	// Initialize paths if not provided
	pathsInstance, err := paths.New(opts.DotfilesRoot)
	if err != nil {
		return &packpipeline.PackResult{
			Pack:    pack,
			Success: false,
			Error:   err,
		}, err
	}

	// Create datastore for status checking
	dataStore := datastore.New(fs, pathsInstance)

	// Get pack status using the centralized GetStatus function
	statusOpts := packcommands.StatusOptions{
		Pack:       pack,
		DataStore:  dataStore,
		FileSystem: fs,
		Paths:      pathsInstance,
	}

	packStatus, err := packcommands.GetStatus(statusOpts)
	if err != nil {
		logger.Error().
			Err(err).
			Str("pack", pack.Name).
			Msg("Failed to get pack status")
		return &packpipeline.PackResult{
			Pack:    pack,
			Success: false,
			Error:   err,
		}, err
	}

	logger.Info().
		Str("pack", pack.Name).
		Str("status", packStatus.Status).
		Int("fileCount", len(packStatus.Files)).
		Msg("Status command completed for pack")

	return &packpipeline.PackResult{
		Pack:                  pack,
		Success:               true,
		Error:                 nil,
		CommandSpecificResult: packStatus,
	}, nil
}
