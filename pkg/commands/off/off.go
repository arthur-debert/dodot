package off

import (
	"github.com/arthur-debert/dodot/pkg/pack"
	"github.com/arthur-debert/dodot/pkg/types"
)

// OffPacksOptions defines the options for the OffPacks command
// This is a wrapper around pack.OffOptions for backward compatibility
type OffPacksOptions struct {
	// DotfilesRoot is the path to the root of the dotfiles directory
	DotfilesRoot string
	// PackNames is a list of specific packs to turn off. If empty, all packs are turned off
	PackNames []string
	// DryRun specifies whether to perform a dry run without making changes
	DryRun bool
}

// OffPacks is a thin wrapper around pack.TurnOff for backward compatibility.
// The core logic has been moved to pkg/pack/off.go following the established pattern.
func OffPacks(opts OffPacksOptions) (*types.PackCommandResult, error) {
	// Convert command options to pack options
	packOpts := pack.OffOptions{
		DotfilesRoot: opts.DotfilesRoot,
		PackNames:    opts.PackNames,
		DryRun:       opts.DryRun,
		FileSystem:   nil, // Use default OS filesystem
	}

	return pack.TurnOff(packOpts)
}
