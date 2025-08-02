package packs

import (
	"sort"

	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
)

// SelectPacks filters a list of packs by name
func SelectPacks(allPacks []types.Pack, selectedNames []string) ([]types.Pack, error) {
	logger := logging.GetLogger("packs.selection")

	if len(selectedNames) == 0 {
		// No selection means all packs
		return allPacks, nil
	}

	// Create a map for quick lookup
	packMap := make(map[string]types.Pack)
	for _, pack := range allPacks {
		packMap[pack.Name] = pack
	}

	var selected []types.Pack
	var notFound []string

	for _, name := range selectedNames {
		if pack, exists := packMap[name]; exists {
			selected = append(selected, pack)
			logger.Trace().Str("name", name).Msg("Selected pack")
		} else {
			notFound = append(notFound, name)
		}
	}

	if len(notFound) > 0 {
		return nil, errors.New(errors.ErrPackNotFound, "pack(s) not found").
			WithDetail("notFound", notFound).
			WithDetail("available", GetPackNames(allPacks))
	}

	// Sort by name for consistent ordering
	sort.Slice(selected, func(i, j int) bool {
		return selected[i].Name < selected[j].Name
	})

	logger.Info().
		Int("selected", len(selected)).
		Int("total", len(allPacks)).
		Msg("Selected packs")

	return selected, nil
}

// GetPackNames returns a list of pack names
func GetPackNames(packs []types.Pack) []string {
	names := make([]string, len(packs))
	for i, pack := range packs {
		names[i] = pack.Name
	}
	return names
}
