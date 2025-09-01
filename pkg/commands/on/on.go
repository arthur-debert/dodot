package on

import (
	"github.com/arthur-debert/dodot/pkg/pack"
	"github.com/arthur-debert/dodot/pkg/types"
)

// OnPacksOptions defines the options for the OnPacks command
// This is a wrapper around pack.OnOptions for backward compatibility
type OnPacksOptions struct {
	// DotfilesRoot is the path to the root of the dotfiles directory
	DotfilesRoot string
	// PackNames is a list of specific packs to turn on. If empty, all packs are turned on
	PackNames []string
	// DryRun specifies whether to perform a dry run without making changes
	DryRun bool
	// Force forces operations even if there are conflicts
	Force bool
	// NoProvision skips provisioning handlers (only link files)
	NoProvision bool
	// ProvisionRerun forces re-run provisioning even if already done
	ProvisionRerun bool
}

// OnPacks is a thin wrapper around pack.TurnOn for backward compatibility.
// The core logic has been moved to pkg/pack/on.go following the established pattern.
func OnPacks(opts OnPacksOptions) (*types.PackCommandResult, error) {
	// Convert command options to pack options
	packOpts := pack.OnOptions{
		DotfilesRoot:   opts.DotfilesRoot,
		PackNames:      opts.PackNames,
		DryRun:         opts.DryRun,
		Force:          opts.Force,
		NoProvision:    opts.NoProvision,
		ProvisionRerun: opts.ProvisionRerun,
		FileSystem:     nil, // Use default OS filesystem
	}

	return pack.TurnOn(packOpts)
}
