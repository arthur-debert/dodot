package adopt

import (
	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/types"

	// Import registry to register commands
	_ "github.com/arthur-debert/dodot/pkg/commands/registry"
)

// AdoptFilesOptions holds options for the adopt command
type AdoptFilesOptions struct {
	DotfilesRoot string
	PackName     string
	SourcePaths  []string
	Force        bool
	FileSystem   types.FS // Allow injecting a filesystem for testing
}

// AdoptFiles is a thin wrapper that delegates to the centralized command execution.
// The core logic has been moved to the command registry.
func AdoptFiles(opts AdoptFilesOptions) (*types.PackCommandResult, error) {
	// Use the centralized command execution
	return core.ExecuteRegisteredCommand("adopt", core.CommandExecuteOptions{
		DotfilesRoot: opts.DotfilesRoot,
		PackNames:    []string{opts.PackName},
		DryRun:       false,
		Force:        opts.Force,
		FileSystem:   opts.FileSystem,
		Options: map[string]interface{}{
			"sourcePaths": opts.SourcePaths,
		},
	})
}
