// Package status provides the status command implementation for dodot.
//
// The status command shows the deployment state of packs and files,
// answering two key questions:
//   - What has already been deployed? (current state)
//   - What will happen if I deploy? (predicted state)
package status

import (
	"github.com/arthur-debert/dodot/pkg/pack"
	"github.com/arthur-debert/dodot/pkg/types"
)

// StatusPacksOptions contains options for the status command
type StatusPacksOptions struct {
	// DotfilesRoot is the root directory containing packs
	DotfilesRoot string

	// PackNames specifies which packs to check status for
	// If empty, all packs are checked
	PackNames []string

	// Paths provides system paths (optional, will be created if not provided)
	Paths types.Pather

	// FileSystem to use (optional, defaults to OS filesystem)
	FileSystem types.FS
}

// StatusPacks is a thin wrapper around pack.GetPacksStatus for backward compatibility.
// The core logic has been moved to pkg/pack/status_command.go following the established pattern.
func StatusPacks(opts StatusPacksOptions) (*types.PackCommandResult, error) {
	// Convert command options to pack options
	packOpts := pack.StatusCommandOptions{
		DotfilesRoot: opts.DotfilesRoot,
		PackNames:    opts.PackNames,
		Paths:        opts.Paths,
		FileSystem:   opts.FileSystem,
	}

	return pack.GetPacksStatus(packOpts)
}
