package discovery

import (
	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/packs"
	"github.com/arthur-debert/dodot/pkg/types"
)

// DiscoverAndSelectPacks is a helper that combines pack discovery and selection.
// It discovers all packs in dotfilesRoot and optionally filters by packNames.
// If packNames is empty, all discovered packs are returned.
// Pack names are normalized to handle trailing slashes from shell completion.
func DiscoverAndSelectPacks(dotfilesRoot string, packNames []string) ([]types.Pack, error) {
	return DiscoverAndSelectPacksWithConfig(dotfilesRoot, packNames, nil)
}

// DiscoverAndSelectPacksWithConfig is a helper that combines pack discovery and selection using provided config.
func DiscoverAndSelectPacksWithConfig(dotfilesRoot string, packNames []string, cfg interface{}) ([]types.Pack, error) {
	// Convert interface to config if provided
	var typedConfig *config.Config
	if cfg != nil {
		typedConfig = cfg.(*config.Config)
	}

	// Get pack candidates
	candidates, err := packs.GetPackCandidatesWithConfig(dotfilesRoot, typedConfig)
	if err != nil {
		return nil, err
	}

	// Get packs
	allPacks, err := packs.GetPacks(candidates)
	if err != nil {
		return nil, err
	}

	// Select specific packs if requested
	if len(packNames) > 0 {
		// Normalize pack names by removing trailing slashes
		normalized := packs.NormalizePackNames(packNames)
		selected, err := packs.SelectPacks(allPacks, normalized)
		if err != nil {
			return nil, err
		}
		return selected, nil
	}

	return allPacks, nil
}

// DiscoverAndSelectPacksFS is a helper that combines pack discovery and selection with filesystem support.
// It discovers all packs in dotfilesRoot using the provided filesystem and optionally filters by packNames.
// If packNames is empty, all discovered packs are returned.
// Pack names are normalized to handle trailing slashes from shell completion.
func DiscoverAndSelectPacksFS(dotfilesRoot string, packNames []string, filesystem types.FS) ([]types.Pack, error) {
	return DiscoverAndSelectPacksFSWithConfig(dotfilesRoot, packNames, filesystem, nil)
}

// DiscoverAndSelectPacksFSWithConfig is a helper that combines pack discovery and selection with filesystem and config support.
func DiscoverAndSelectPacksFSWithConfig(dotfilesRoot string, packNames []string, filesystem types.FS, cfg interface{}) ([]types.Pack, error) {
	// Convert interface to config if provided
	var typedConfig *config.Config
	if cfg != nil {
		typedConfig = cfg.(*config.Config)
	}

	// Get pack candidates using filesystem
	candidates, err := packs.GetPackCandidatesFSWithConfig(dotfilesRoot, filesystem, typedConfig)
	if err != nil {
		return nil, err
	}

	// Get packs using filesystem
	allPacks, err := packs.GetPacksFS(candidates, filesystem)
	if err != nil {
		return nil, err
	}

	// Select specific packs if requested
	if len(packNames) > 0 {
		// Normalize pack names by removing trailing slashes
		normalized := packs.NormalizePackNames(packNames)
		selected, err := packs.SelectPacks(allPacks, normalized)
		if err != nil {
			return nil, err
		}
		return selected, nil
	}

	return allPacks, nil
}

// FindPack discovers all packs and returns the one with the specified name.
// Returns an error if the pack is not found.
// Pack name is normalized to handle trailing slashes from shell completion.
func FindPack(dotfilesRoot string, packName string) (*types.Pack, error) {
	return FindPackFS(dotfilesRoot, packName, filesystem.NewOS())
}

// FindPackFS discovers all packs using the given filesystem and returns the one with the specified name.
// Returns an error if the pack is not found.
// Pack name is normalized to handle trailing slashes from shell completion.
func FindPackFS(dotfilesRoot string, packName string, fs types.FS) (*types.Pack, error) {
	// Normalize pack name
	normalized := packs.NormalizePackName(packName)

	allPacks, err := DiscoverAndSelectPacksFS(dotfilesRoot, []string{normalized}, fs)
	if err != nil {
		return nil, err
	}

	if len(allPacks) == 0 {
		return nil, errors.Newf(errors.ErrPackNotFound, "pack %q not found", normalized)
	}

	return &allPacks[0], nil
}

// ValidateDotfilesRoot checks if the dotfiles root exists and is a directory.
// This centralizes the validation that multiple commands need.
func ValidateDotfilesRoot(dotfilesRoot string) error {
	return ValidateDotfilesRootWithConfig(dotfilesRoot, nil)
}

// ValidateDotfilesRootWithConfig checks if the dotfiles root exists and is a directory using provided config.
func ValidateDotfilesRootWithConfig(dotfilesRoot string, cfg interface{}) error {
	if dotfilesRoot == "" {
		return errors.New(errors.ErrInvalidInput, "dotfiles root cannot be empty")
	}

	// Convert interface to config if provided
	var typedConfig *config.Config
	if cfg != nil {
		typedConfig = cfg.(*config.Config)
	}

	// The actual validation happens in packs.GetPackCandidates
	// We just need to check if we can discover packs
	_, err := packs.GetPackCandidatesWithConfig(dotfilesRoot, typedConfig)
	return err
}
