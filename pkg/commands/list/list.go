package list

import (
	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
)

// ListPacksOptions defines the options for the ListPacks command.
type ListPacksOptions struct {
	// DotfilesRoot is the path to the root of the dotfiles directory.
	DotfilesRoot string
}

// ListPacks finds all available packs in the dotfiles root directory.
func ListPacks(opts ListPacksOptions) (*types.ListPacksResult, error) {
	log := logging.GetLogger("core.commands")
	log.Debug().Str("command", "ListPacks").Msg("Executing command")

	candidates, err := core.GetPackCandidates(opts.DotfilesRoot)
	if err != nil {
		return nil, err
	}

	packs, err := core.GetPacks(candidates)
	if err != nil {
		return nil, err
	}

	result := &types.ListPacksResult{
		Packs: make([]types.PackInfo, len(packs)),
	}

	for i, p := range packs {
		result.Packs[i] = types.PackInfo{
			Name: p.Name,
			Path: p.Path,
		}
	}

	log.Info().Str("command", "ListPacks").Int("packCount", len(result.Packs)).Msg("Command finished")
	return result, nil
}
