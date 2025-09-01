package fill

import (
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
func FillPack(opts FillPackOptions) (*types.FillResult, error) {
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
	return p.Fill(fs)
}
