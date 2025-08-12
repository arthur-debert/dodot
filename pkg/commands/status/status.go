// Package status provides the status command implementation for dodot.
//
// The status command shows the deployment state of packs and files,
// answering two key questions:
//   - What has already been deployed? (current state)
//   - What will happen if I deploy? (predicted state)
package status

import (
	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/packs"
	"github.com/arthur-debert/dodot/pkg/types"
)

// StatusPacksOptions contains options for the status command
type StatusPacksOptions struct {
	// DotfilesRoot is the root directory containing packs
	DotfilesRoot string

	// PackNames specifies which packs to check status for
	// If empty, all packs are checked
	PackNames []string

	// Paths provides system paths (required)
	Paths types.Pather

	// FileSystem to use (defaults to OS filesystem)
	FileSystem types.FS
}

// StatusPacks shows the deployment status of specified packs
func StatusPacks(opts StatusPacksOptions) (*types.DisplayResult, error) {
	logger := logging.GetLogger("commands.status")
	logger.Debug().
		Str("dotfilesRoot", opts.DotfilesRoot).
		Strs("packNames", opts.PackNames).
		Msg("Starting status command")

	// Initialize filesystem if not provided
	if opts.FileSystem == nil {
		opts.FileSystem = filesystem.NewOS()
	}

	// Get pack candidates using the filesystem
	candidates, err := packs.GetPackCandidatesFS(opts.Paths.DotfilesRoot(), opts.FileSystem)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrPackNotFound,
			"failed to get pack candidates")
	}

	// Get all packs
	allPacks, err := packs.GetPacksFS(candidates, opts.FileSystem)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrPackNotFound,
			"failed to get packs")
	}

	// Filter to specific packs if requested
	selectedPacks, err := packs.SelectPacks(allPacks, opts.PackNames)
	if err != nil {
		return nil, err
	}

	logger.Info().
		Int("packCount", len(selectedPacks)).
		Msg("Found packs to check")

	// Get status for all packs
	result, err := core.GetMultiPackStatus(selectedPacks, "status", opts.FileSystem, opts.Paths)
	if err != nil {
		return nil, err
	}

	logger.Info().
		Int("packCount", len(result.Packs)).
		Msg("Status check complete")

	return result, nil
}
