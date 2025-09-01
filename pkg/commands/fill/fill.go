package fill

import (
	"fmt"
	"time"

	"github.com/arthur-debert/dodot/pkg/commands/status"
	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/pack"
	"github.com/arthur-debert/dodot/pkg/types"
)

// FillPackOptions defines the options for the FillPack command.
type FillPackOptions struct {
	// DotfilesRoot is the path to the root of the dotfiles directory.
	DotfilesRoot string
	// PackName is the name of the pack to fill with template files.
	PackName string
	// FileSystem is the filesystem to use (optional, defaults to OS filesystem)
	FileSystem types.FS
}

// FillPack adds placeholder files for handlers to an existing pack.
func FillPack(opts FillPackOptions) (*types.PackCommandResult, error) {
	log := logging.GetLogger("core.commands")
	log.Debug().Str("command", "FillPack").Str("pack", opts.PackName).Msg("Executing command")

	// Use provided filesystem or default
	fs := opts.FileSystem
	if fs == nil {
		fs = filesystem.NewOS()
	}

	// Find the specific pack
	targetPack, err := core.FindPackFS(opts.DotfilesRoot, opts.PackName, fs)
	if err != nil {
		return nil, err
	}

	// Wrap in our enhanced Pack type and delegate to Fill method
	p := pack.New(targetPack)
	fillResult, err := p.Fill(fs)
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
		Command:   "fill",
		Timestamp: time.Now(),
		DryRun:    false,
		Packs:     []types.DisplayPack{},
		Metadata: types.CommandMetadata{
			FilesCreated: len(fillResult.FilesCreated),
			CreatedPaths: fillResult.FilesCreated,
		},
	}

	// Copy packs from status if available
	if packStatus != nil {
		result.Packs = packStatus.Packs
	}

	// Generate message
	if len(fillResult.FilesCreated) == 1 {
		result.Message = "The pack " + opts.PackName + " has been filled with 1 placeholder file."
	} else {
		result.Message = fmt.Sprintf("The pack %s has been filled with %d placeholder files.", opts.PackName, len(fillResult.FilesCreated))
	}

	return result, nil
}
