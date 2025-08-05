package status

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
)

// StatusPacksOptions defines the options for the StatusPacks command.
type StatusPacksOptions struct {
	// DotfilesRoot is the path to the root of the dotfiles directory.
	DotfilesRoot string
	// PackNames is a list of specific packs to check status for. If empty, all packs are checked.
	PackNames []string
}

// StatusPacks checks the deployment status of the specified packs.
func StatusPacks(opts StatusPacksOptions) (*types.PackStatusResult, error) {
	log := logging.GetLogger("core.commands")
	log.Debug().Str("command", "StatusPacks").Msg("Executing command")

	// 0. Initialize Paths instance
	pathsInstance, err := paths.New(opts.DotfilesRoot)
	if err != nil {
		return nil, err
	}

	// 1. Get all packs
	candidates, err := core.GetPackCandidates(opts.DotfilesRoot)
	if err != nil {
		return nil, err
	}
	allPacks, err := core.GetPacks(candidates)
	if err != nil {
		return nil, err
	}

	// 2. Filter to selected packs
	selectedPacks, err := core.SelectPacks(allPacks, opts.PackNames)
	if err != nil {
		return nil, err
	}

	// 3. Check status for each pack
	result := &types.PackStatusResult{
		Packs: make([]types.PackStatus, 0, len(selectedPacks)),
	}

	for _, pack := range selectedPacks {
		packStatus := types.PackStatus{
			Name:         pack.Name,
			PowerUpState: make([]types.PowerUpStatus, 0),
		}

		// Check run-once power-up status (install, homebrew)
		installStatus, err := core.GetRunOnceStatus(pack.Path, "install", pathsInstance)
		if err == nil && installStatus != nil {
			state := "Not Installed"
			description := "Install script not yet executed"
			if installStatus.Executed {
				state = "Installed"
				description = fmt.Sprintf("Installed on %s", installStatus.ExecutedAt.Format("2006-01-02 15:04:05"))
				if installStatus.Changed {
					state = "Changed"
					description += " (script has changed since execution)"
				}
			}
			packStatus.PowerUpState = append(packStatus.PowerUpState, types.PowerUpStatus{
				Name:        "install",
				State:       state,
				Description: description,
			})
		}

		homebrewStatus, err := core.GetRunOnceStatus(pack.Path, "homebrew", pathsInstance)
		if err == nil && homebrewStatus != nil {
			state := "Not Installed"
			description := "Brewfile not yet executed"
			if homebrewStatus.Executed {
				state = "Installed"
				description = fmt.Sprintf("Installed on %s", homebrewStatus.ExecutedAt.Format("2006-01-02 15:04:05"))
				if homebrewStatus.Changed {
					state = "Changed"
					description += " (Brewfile has changed since execution)"
				}
			}
			packStatus.PowerUpState = append(packStatus.PowerUpState, types.PowerUpStatus{
				Name:        "homebrew",
				State:       state,
				Description: description,
			})
		}

		// For symlink status, we'd need to check actual symlinks in the filesystem
		// This is a simplified version - in reality we'd check if symlinks exist
		// and point to the correct locations
		packStatus.PowerUpState = append(packStatus.PowerUpState, types.PowerUpStatus{
			Name:        "symlink",
			State:       "Unknown",
			Description: "Symlink status checking not yet implemented",
		})

		result.Packs = append(result.Packs, packStatus)
	}

	log.Info().Str("command", "StatusPacks").Int("packCount", len(result.Packs)).Msg("Command finished")
	return result, nil
}
