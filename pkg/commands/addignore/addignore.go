package addignore

import (
	"github.com/arthur-debert/dodot/pkg/commands/status"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/pack"
	"github.com/arthur-debert/dodot/pkg/types"
)

// AddIgnoreOptions holds options for the add-ignore command
type AddIgnoreOptions struct {
	DotfilesRoot string
	PackName     string
}

// AddIgnore creates a .dodotignore file using proper abstractions
func AddIgnore(opts AddIgnoreOptions) (*types.PackCommandResult, error) {
	logger := logging.GetLogger("commands.addignore")
	logger.Info().
		Str("pack", opts.PackName).
		Str("dotfiles_root", opts.DotfilesRoot).
		Msg("Creating ignore file for pack")

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

	// Delegate to pack.AddIgnore
	return pack.AddIgnore(pack.AddIgnoreOptions{
		PackName:      opts.PackName,
		DotfilesRoot:  opts.DotfilesRoot,
		GetPackStatus: getStatusFunc,
	})
}
