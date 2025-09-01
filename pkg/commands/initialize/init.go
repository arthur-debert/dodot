package initialize

import (
	"github.com/arthur-debert/dodot/pkg/commands/status"
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

	// Delegate to pack.Initialize
	return pack.Initialize(pack.InitOptions{
		PackName:      opts.PackName,
		DotfilesRoot:  opts.DotfilesRoot,
		FileSystem:    opts.FileSystem,
		GetPackStatus: getStatusFunc,
	})
}
