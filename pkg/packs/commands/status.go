package commands

import (
	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/packs/execution"
	"github.com/arthur-debert/dodot/pkg/packs/operations"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
)

// StatusCommand implements the "status" command using the pack execution.
type StatusCommand struct{}

// Name returns the command name.
func (c *StatusCommand) Name() string {
	return "status"
}

// ExecuteForPack executes the "status" command for a single pack.
func (c *StatusCommand) ExecuteForPack(pack types.Pack, opts execution.Options) (*execution.PackResult, error) {
	logger := logging.GetLogger("execution.status")
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
		return &execution.PackResult{
			Pack:    pack,
			Success: false,
			Error:   err,
		}, err
	}

	// Create datastore for status checking
	dataStore := datastore.New(fs, pathsInstance)

	// Get pack status using the centralized GetStatus function
	statusOpts := operations.StatusOptions{
		Pack:       pack,
		DataStore:  dataStore,
		FileSystem: fs,
		Paths:      pathsInstance,
	}

	packStatus, err := operations.GetStatus(statusOpts)
	if err != nil {
		logger.Error().
			Err(err).
			Str("pack", pack.Name).
			Msg("Failed to get pack status")
		return &execution.PackResult{
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

	return &execution.PackResult{
		Pack:                  pack,
		Success:               true,
		Error:                 nil,
		CommandSpecificResult: packStatus,
	}, nil
}
