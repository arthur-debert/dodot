// Package status provides the status command implementation for dodot.
//
// The status command shows the deployment state of packs and files,
// answering two key questions:
//   - What has already been deployed? (current state)
//   - What will happen if I deploy? (predicted state)
package status

import (
	"io/fs"
	"os"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/packs"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
)

// StatusPacksOptions contains options for the status command
type StatusPacksOptions struct {
	// DotfilesRoot is the root directory containing packs
	DotfilesRoot string

	// PackNames specifies which packs to check status for
	// If empty, all packs are checked
	PackNames []string

	// Paths provides system paths
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

	// Initialize paths if not provided
	if opts.Paths == nil {
		p, err := paths.New(opts.DotfilesRoot)
		if err != nil {
			return nil, errors.Wrapf(err, errors.ErrConfigLoad,
				"failed to initialize paths")
		}
		opts.Paths = p
	}

	// Initialize filesystem if not provided
	if opts.FileSystem == nil {
		opts.FileSystem = &osFS{}
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

// osFS implements types.FS using the OS filesystem
type osFS struct{}

func (o *osFS) Stat(name string) (fs.FileInfo, error) {
	return os.Stat(name)
}

func (o *osFS) ReadFile(name string) ([]byte, error) {
	return os.ReadFile(name)
}

func (o *osFS) WriteFile(name string, data []byte, perm fs.FileMode) error {
	return os.WriteFile(name, data, perm)
}

func (o *osFS) MkdirAll(path string, perm fs.FileMode) error {
	return os.MkdirAll(path, perm)
}

func (o *osFS) Symlink(oldname, newname string) error {
	return os.Symlink(oldname, newname)
}

func (o *osFS) Readlink(name string) (string, error) {
	return os.Readlink(name)
}

func (o *osFS) Remove(name string) error {
	return os.Remove(name)
}

func (o *osFS) RemoveAll(path string) error {
	return os.RemoveAll(path)
}

func (o *osFS) Lstat(name string) (fs.FileInfo, error) {
	return os.Lstat(name)
}

func (o *osFS) ReadDir(name string) ([]fs.DirEntry, error) {
	return os.ReadDir(name)
}
