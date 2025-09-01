package initialize

import (
	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/types"

	// Import registry to register commands
	_ "github.com/arthur-debert/dodot/pkg/commands/registry"
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

// InitPack is a thin wrapper that delegates to the centralized command execution.
// The core logic has been moved to the command registry.
func InitPack(opts InitPackOptions) (*types.PackCommandResult, error) {
	// Use the centralized command execution
	return core.ExecuteRegisteredCommand("init", core.CommandExecuteOptions{
		DotfilesRoot: opts.DotfilesRoot,
		PackNames:    []string{opts.PackName},
		DryRun:       false,
		Force:        false,
		FileSystem:   opts.FileSystem,
	})
}
