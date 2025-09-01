package initialize

import (
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

func InitPack(opts InitPackOptions) (*types.InitResult, error) {
	log := logging.GetLogger("core.commands")
	log.Debug().Str("command", "InitPack").Str("pack", opts.PackName).Msg("Executing command")

	// Use provided filesystem or default
	fs := opts.FileSystem
	if fs == nil {
		fs = filesystem.NewOS()
	}

	// Delegate to pack.Initialize
	return pack.Initialize(fs, pack.InitOptions{
		PackName:     opts.PackName,
		DotfilesRoot: opts.DotfilesRoot,
	})
}
