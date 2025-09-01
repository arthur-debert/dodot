package adopt

import (
	"github.com/arthur-debert/dodot/pkg/commands/status"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/pack"
	"github.com/arthur-debert/dodot/pkg/types"
)

// AdoptFilesOptions holds options for the adopt command
type AdoptFilesOptions struct {
	DotfilesRoot string
	PackName     string
	SourcePaths  []string
	Force        bool
	FileSystem   types.FS // Allow injecting a filesystem for testing
}

// AdoptFiles moves existing files into a pack and creates symlinks back to their original locations
func AdoptFiles(opts AdoptFilesOptions) (*types.PackCommandResult, error) {
	logger := logging.GetLogger("commands.adopt")
	logger.Info().
		Str("pack", opts.PackName).
		Str("dotfiles_root", opts.DotfilesRoot).
		Strs("source_paths", opts.SourcePaths).
		Bool("force", opts.Force).
		Msg("Adopting files into pack")

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

	// Delegate to pack.Adopt
	return pack.Adopt(pack.AdoptOptions{
		SourcePaths:   opts.SourcePaths,
		Force:         opts.Force,
		DotfilesRoot:  opts.DotfilesRoot,
		PackName:      opts.PackName,
		FileSystem:    opts.FileSystem,
		GetPackStatus: getStatusFunc,
	})
}
