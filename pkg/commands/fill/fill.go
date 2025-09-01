package fill

import (
	"github.com/arthur-debert/dodot/pkg/commands/status"
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

	// Create status function that wraps the status command
	getStatusFunc := func(packName, dotfilesRoot string, fs types.FS) ([]types.DisplayPack, error) {
		statusOpts := status.StatusPacksOptions{
			DotfilesRoot: dotfilesRoot,
			PackNames:    []string{packName},
			FileSystem:   fs,
		}
		result, err := status.StatusPacks(statusOpts)
		if err != nil {
			return nil, err
		}
		return result.Packs, nil
	}

	// Delegate to pack.Fill
	return pack.Fill(pack.FillOptions{
		PackName:      opts.PackName,
		DotfilesRoot:  opts.DotfilesRoot,
		FileSystem:    opts.FileSystem,
		GetPackStatus: getStatusFunc,
	})
}
