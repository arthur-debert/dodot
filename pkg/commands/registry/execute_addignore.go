package registry

import (
	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/pack"
	"github.com/arthur-debert/dodot/pkg/types"
)

// executeAddIgnore executes the add-ignore command
func executeAddIgnore(opts core.CommandExecuteOptions) (*types.PackCommandResult, error) {
	// Extract pack name from discovered packs or options
	var packName string
	if len(opts.PackNames) > 0 {
		packName = opts.PackNames[0]
	}

	fs := opts.FileSystem
	if fs == nil {
		fs = filesystem.NewOS()
	}

	// Create a status function that uses the pack module
	getPackStatus := func(packName, dotfilesRoot string, fs types.FS) ([]types.DisplayPack, error) {
		statusResult, err := pack.GetPacksStatus(pack.StatusCommandOptions{
			DotfilesRoot: dotfilesRoot,
			PackNames:    []string{packName},
			FileSystem:   fs,
		})
		if err != nil {
			return nil, err
		}
		return statusResult.Packs, nil
	}

	// Call the pack.AddIgnore function
	return pack.AddIgnore(pack.AddIgnoreOptions{
		PackName:      packName,
		DotfilesRoot:  opts.DotfilesRoot,
		FileSystem:    fs,
		GetPackStatus: getPackStatus,
	})
}

// init registers the add-ignore command
func init() {
	core.RegisterCommand(core.CommandConfig{
		Name:    "add-ignore",
		Type:    core.SimpleExecution,
		Execute: executeAddIgnore,
		Validators: []core.ValidatorFunc{
			core.ValidateSinglePack,
		},
	})
}
