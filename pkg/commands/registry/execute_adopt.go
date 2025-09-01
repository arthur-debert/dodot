package registry

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/pack"
	"github.com/arthur-debert/dodot/pkg/types"
)

// executeAdopt executes the adopt command
func executeAdopt(opts core.CommandExecuteOptions) (*types.PackCommandResult, error) {
	// Extract pack name from options
	var packName string
	if len(opts.PackNames) > 0 {
		packName = opts.PackNames[0]
	}

	// Extract source paths from options
	sourcePaths, ok := opts.Options["sourcePaths"].([]string)
	if !ok || len(sourcePaths) == 0 {
		return nil, fmt.Errorf("source paths are required")
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

	// Call the pack.Adopt function
	return pack.Adopt(pack.AdoptOptions{
		SourcePaths:   sourcePaths,
		Force:         opts.Force,
		DotfilesRoot:  opts.DotfilesRoot,
		PackName:      packName,
		FileSystem:    fs,
		GetPackStatus: getPackStatus,
	})
}

// init registers the adopt command
func init() {
	core.RegisterCommand(core.CommandConfig{
		Name:    "adopt",
		Type:    core.SimpleExecution,
		Execute: executeAdopt,
		Validators: []core.ValidatorFunc{
			core.ValidateSinglePackName,
			core.ValidateSinglePack, // Ensures the pack exists
		},
	})
}
