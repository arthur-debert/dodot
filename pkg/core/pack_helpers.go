package core

import (
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/packs"
	"github.com/arthur-debert/dodot/pkg/types"
)

// DiscoverAndSelectPacks is a helper that combines pack discovery and selection.
// It discovers all packs in dotfilesRoot and optionally filters by packNames.
// If packNames is empty, all discovered packs are returned.
func DiscoverAndSelectPacks(dotfilesRoot string, packNames []string) ([]types.Pack, error) {
	// Get pack candidates
	candidates, err := GetPackCandidates(dotfilesRoot)
	if err != nil {
		return nil, err
	}

	// Get packs
	allPacks, err := GetPacks(candidates)
	if err != nil {
		return nil, err
	}

	// Select specific packs if requested
	if len(packNames) > 0 {
		selected, err := packs.SelectPacks(allPacks, packNames)
		if err != nil {
			return nil, err
		}
		return selected, nil
	}

	return allPacks, nil
}

// FindPack discovers all packs and returns the one with the specified name.
// Returns an error if the pack is not found.
func FindPack(dotfilesRoot string, packName string) (*types.Pack, error) {
	packs, err := DiscoverAndSelectPacks(dotfilesRoot, []string{packName})
	if err != nil {
		return nil, err
	}

	if len(packs) == 0 {
		return nil, errors.Newf(errors.ErrPackNotFound, "pack %q not found", packName)
	}

	return &packs[0], nil
}

// DiscoverAndSelectPacksFS is a helper that combines pack discovery and selection with filesystem support.
// It discovers all packs in dotfilesRoot using the provided filesystem and optionally filters by packNames.
// If packNames is empty, all discovered packs are returned.
func DiscoverAndSelectPacksFS(dotfilesRoot string, packNames []string, filesystem types.FS) ([]types.Pack, error) {
	// Get pack candidates using filesystem
	candidates, err := packs.GetPackCandidatesFS(dotfilesRoot, filesystem)
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
		selected, err := packs.SelectPacks(allPacks, packNames)
		if err != nil {
			return nil, err
		}
		return selected, nil
	}

	return allPacks, nil
}

// ValidateDotfilesRoot checks if the dotfiles root exists and is a directory.
// This centralizes the validation that multiple commands need.
func ValidateDotfilesRoot(dotfilesRoot string) error {
	if dotfilesRoot == "" {
		return errors.New(errors.ErrInvalidInput, "dotfiles root cannot be empty")
	}

	// The actual validation happens in GetPackCandidates
	// We just need to check if we can discover packs
	_, err := GetPackCandidates(dotfilesRoot)
	return err
}
