package initialize

import (
	"fmt"
	"time"

	"github.com/arthur-debert/dodot/pkg/commands/status"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/pack"
	"github.com/arthur-debert/dodot/pkg/types"
)

// InitPackOptions defines the options for the InitPack command.
type InitPackOptions struct {
	// DotfilesRoot is the path to the root of the dotfiles directory.
	DotfilesRoot string
	// PackName is the name of the new pack to create.
	PackName string
	// FileSystem is the filesystem to use (optional, defaults to OS filesystem)
	FileSystem types.FS
}

func InitPack(opts InitPackOptions) (*types.PackCommandResult, error) {
	log := logging.GetLogger("core.commands")
	log.Debug().Str("command", "InitPack").Str("pack", opts.PackName).Msg("Executing command")

	// Use provided filesystem or default
	fs := opts.FileSystem
	if fs == nil {
		fs = filesystem.NewOS()
	}

	// Delegate to pack.Initialize
	initResult, err := pack.Initialize(fs, pack.InitOptions{
		PackName:     opts.PackName,
		DotfilesRoot: opts.DotfilesRoot,
	})
	if err != nil {
		return nil, err
	}

	// Get current pack status
	statusOpts := status.StatusPacksOptions{
		DotfilesRoot: opts.DotfilesRoot,
		PackNames:    []string{opts.PackName},
		FileSystem:   fs,
	}
	packStatus, statusErr := status.StatusPacks(statusOpts)
	if statusErr != nil {
		log.Error().Err(statusErr).Msg("Failed to get pack status")
	}

	// Build result
	result := &types.PackCommandResult{
		Command:   "init",
		Timestamp: time.Now(),
		DryRun:    false,
		Packs:     []types.DisplayPack{},
		Metadata: types.CommandMetadata{
			FilesCreated: len(initResult.FilesCreated),
			CreatedPaths: initResult.FilesCreated,
		},
	}

	// Copy packs from status if available
	if packStatus != nil {
		result.Packs = packStatus.Packs
	}

	// Generate message
	result.Message = fmt.Sprintf("The pack %s has been initialized with %d files.", opts.PackName, len(initResult.FilesCreated))

	return result, nil
}
