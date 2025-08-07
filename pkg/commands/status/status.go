package status

import (
	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/packs"
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
func StatusPacks(opts StatusPacksOptions) (*types.DisplayResult, error) {
	log := logging.GetLogger("core.commands")
	log.Debug().Str("command", "StatusPacks").Msg("Executing command")

	// Initialize Paths instance
	pathsInstance, err := paths.New(opts.DotfilesRoot)
	if err != nil {
		return nil, err
	}

	// 1. Get all packs using the core pipeline
	candidates, err := core.GetPackCandidates(opts.DotfilesRoot)
	if err != nil {
		return nil, err
	}
	allPacks, err := core.GetPacks(candidates)
	if err != nil {
		return nil, err
	}

	// 2. Filter to selected packs
	selectedPacks, err := packs.SelectPacks(allPacks, opts.PackNames)
	if err != nil {
		return nil, err
	}

	// 3. Run the full pipeline to get operations (same as deploy command)
	triggerMatches, err := core.GetFiringTriggers(selectedPacks)
	if err != nil {
		return nil, err
	}

	actions, err := core.GetActions(triggerMatches)
	if err != nil {
		return nil, err
	}

	// Create execution context with home symlinks enabled to match deploy/install behavior
	ctx := core.NewExecutionContextWithHomeSymlinks(false, pathsInstance, true, nil)
	operations, err := core.ConvertActionsToOperationsWithContext(actions, ctx)
	if err != nil {
		return nil, err
	}

	// 4. Transform operations into DisplayResult
	result := CreateDisplayResultFromOperations(operations, selectedPacks, "status")

	log.Info().Str("command", "StatusPacks").Int("packCount", len(result.Packs)).Msg("Command finished")
	return result, nil
}

// StatusPacksDirect checks the deployment status using the direct action-based approach.
func StatusPacksDirect(opts StatusPacksOptions) (*types.DisplayResult, error) {
	log := logging.GetLogger("core.commands")
	log.Debug().Str("command", "StatusPacksDirect").Msg("Executing command")

	// 1. Get all packs using the core pipeline
	candidates, err := core.GetPackCandidates(opts.DotfilesRoot)
	if err != nil {
		return nil, err
	}
	allPacks, err := core.GetPacks(candidates)
	if err != nil {
		return nil, err
	}

	// 2. Filter to selected packs
	selectedPacks, err := packs.SelectPacks(allPacks, opts.PackNames)
	if err != nil {
		return nil, err
	}

	// 3. Get actions directly (no conversion to operations)
	triggerMatches, err := core.GetFiringTriggers(selectedPacks)
	if err != nil {
		return nil, err
	}

	actions, err := core.GetActions(triggerMatches)
	if err != nil {
		return nil, err
	}

	// 4. Transform actions into DisplayResult (new function)
	result := CreateDisplayResultFromActions(actions, selectedPacks, "status")

	log.Info().Str("command", "StatusPacksDirect").Int("packCount", len(result.Packs)).Msg("Command finished")
	return result, nil
}
