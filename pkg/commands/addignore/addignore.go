package addignore

import (
	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/types"

	// Import registry to register commands
	_ "github.com/arthur-debert/dodot/pkg/commands/registry"
)

// AddIgnoreOptions holds options for the add-ignore command
type AddIgnoreOptions struct {
	DotfilesRoot string
	PackName     string
	FileSystem   types.FS // Optional, for testing
}

// AddIgnore is a thin wrapper that delegates to the centralized command execution.
// The core logic has been moved to the command registry.
func AddIgnore(opts AddIgnoreOptions) (*types.PackCommandResult, error) {
	// Use the centralized command execution
	return core.ExecuteRegisteredCommand("add-ignore", core.CommandExecuteOptions{
		DotfilesRoot: opts.DotfilesRoot,
		PackNames:    []string{opts.PackName},
		DryRun:       false,
		Force:        false,
		FileSystem:   opts.FileSystem, // Pass through filesystem
	})
}
