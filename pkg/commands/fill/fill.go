package fill

import (
	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/types"

	// Import registry to register commands
	_ "github.com/arthur-debert/dodot/pkg/commands/registry"
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

// FillPack is a thin wrapper that delegates to the centralized command execution.
// The core logic has been moved to the command registry.
func FillPack(opts FillPackOptions) (*types.PackCommandResult, error) {
	// Use the centralized command execution
	return core.ExecuteRegisteredCommand("fill", core.CommandExecuteOptions{
		DotfilesRoot: opts.DotfilesRoot,
		PackNames:    []string{opts.PackName},
		DryRun:       false,
		Force:        false,
		FileSystem:   opts.FileSystem,
	})
}
