package core

import (
	"strings"

	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/packs"
	"github.com/arthur-debert/dodot/pkg/types"
)

// DiscoverAndSelectPacks is a helper that combines pack discovery and selection.
// It discovers all packs in dotfilesRoot and optionally filters by packNames.
// If packNames is empty, all discovered packs are returned.
// Pack names are normalized to handle trailing slashes from shell completion.
func DiscoverAndSelectPacks(dotfilesRoot string, packNames []string) ([]types.Pack, error) {
	// Get pack candidates
	candidates, err := packs.GetPackCandidates(dotfilesRoot)
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
		normalized := normalizePackNames(packNames)
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
		// Normalize pack names by removing trailing slashes
		normalized := normalizePackNames(packNames)
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
	// Normalize pack name
	normalized := normalizePackName(packName)

	packs, err := DiscoverAndSelectPacks(dotfilesRoot, []string{normalized})
	if err != nil {
		return nil, err
	}

	if len(packs) == 0 {
		return nil, errors.Newf(errors.ErrPackNotFound, "pack %q not found", normalized)
	}

	return &packs[0], nil
}

// ValidateDotfilesRoot checks if the dotfiles root exists and is a directory.
// This centralizes the validation that multiple commands need.
func ValidateDotfilesRoot(dotfilesRoot string) error {
	if dotfilesRoot == "" {
		return errors.New(errors.ErrInvalidInput, "dotfiles root cannot be empty")
	}

	// The actual validation happens in packs.GetPackCandidates
	// We just need to check if we can discover packs
	_, err := packs.GetPackCandidates(dotfilesRoot)
	return err
}

// normalizePackName removes trailing slashes from a pack name.
// This handles cases where shell completion adds a trailing slash to directory names.
func normalizePackName(name string) string {
	return strings.TrimRight(name, "/")
}

// normalizePackNames removes trailing slashes from all pack names in the slice.
func normalizePackNames(names []string) []string {
	normalized := make([]string, len(names))
	for i, name := range names {
		normalized[i] = normalizePackName(name)
	}
	return normalized
}