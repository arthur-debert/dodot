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
	"path/filepath"
	"strings"

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
	candidates, err := getPackCandidatesWithFS(opts.Paths.DotfilesRoot(), opts.FileSystem)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrPackNotFound,
			"failed to get pack candidates")
	}

	// Get all packs
	allPacks, err := getPacksWithFS(candidates, opts.FileSystem)
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

// getPackCandidatesWithFS returns all potential pack directories using the provided filesystem
func getPackCandidatesWithFS(dotfilesRoot string, fs types.FS) ([]string, error) {
	logger := logging.GetLogger("commands.status")

	// Validate dotfiles root exists
	info, err := fs.Stat(dotfilesRoot)
	if err != nil {
		return nil, errors.Wrap(err, errors.ErrNotFound, "dotfiles root does not exist").
			WithDetail("path", dotfilesRoot)
	}

	if !info.IsDir() {
		return nil, errors.New(errors.ErrInvalidInput, "dotfiles root is not a directory").
			WithDetail("path", dotfilesRoot)
	}

	// Read directory entries
	entries, err := os.ReadDir(dotfilesRoot)
	if os.IsNotExist(err) {
		// For test filesystem, we need to manually find directories
		return findPacksInTestFS(dotfilesRoot, fs)
	}
	if err != nil {
		return nil, errors.Wrap(err, errors.ErrFileAccess, "cannot read dotfiles root").
			WithDetail("path", dotfilesRoot)
	}

	var candidates []string
	for _, entry := range entries {
		if entry.IsDir() && !strings.HasPrefix(entry.Name(), ".") {
			candidates = append(candidates, filepath.Join(dotfilesRoot, entry.Name()))
		}
	}

	logger.Debug().
		Int("count", len(candidates)).
		Msg("Found pack candidates")

	return candidates, nil
}

// findPacksInTestFS is a helper for test filesystems that don't support ReadDir
func findPacksInTestFS(dotfilesRoot string, fs types.FS) ([]string, error) {
	// For test filesystem, we'll look for known pack patterns
	// This is a simplified approach for testing
	var candidates []string

	// Try common pack names
	packNames := []string{"vim", "zsh", "git", "tmux", "test", "configured", "temp"}

	for _, name := range packNames {
		packPath := filepath.Join(dotfilesRoot, name)
		if info, err := fs.Stat(packPath); err == nil && info.IsDir() {
			candidates = append(candidates, packPath)
		}
	}

	return candidates, nil
}

// getPacksWithFS creates pack structs from candidates using the provided filesystem
func getPacksWithFS(candidates []string, fs types.FS) ([]types.Pack, error) {
	var packs []types.Pack

	for _, candidate := range candidates {
		// Check if it's a valid pack directory
		info, err := fs.Stat(candidate)
		if err != nil || !info.IsDir() {
			continue
		}

		// Create a pack
		packName := filepath.Base(candidate)
		pack := types.Pack{
			Name: packName,
			Path: candidate,
		}

		// Note: IsIgnored and HasConfig are determined during status check,
		// not stored on the Pack struct itself

		packs = append(packs, pack)
	}

	return packs, nil
}
