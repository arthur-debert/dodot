package deploy

import (
	"github.com/arthur-debert/dodot/pkg/commands/internal"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
)

// DeployPacksOptions defines the options for the DeployPacks command.
type DeployPacksOptions struct {
	// DotfilesRoot is the path to the root of the dotfiles directory.
	DotfilesRoot string
	// PackNames is a list of specific packs to deploy. If empty, all packs are deployed.
	PackNames []string
	// DryRun specifies whether to perform a dry run without making changes.
	DryRun bool
}

// DeployPacks runs the deployment logic for the specified packs.
// This executes power-ups with RunModeMany.
func DeployPacks(opts DeployPacksOptions) (*types.ExecutionResult, error) {
	log := logging.GetLogger("core.commands")
	log.Debug().Str("command", "DeployPacks").Msg("Executing command")

	execOpts := internal.ExecutionOptions{
		DotfilesRoot: opts.DotfilesRoot,
		PackNames:    opts.PackNames,
		DryRun:       opts.DryRun,
		RunMode:      types.RunModeMany,
	}

	result, err := internal.RunExecutionPipeline(execOpts)
	if err != nil {
		return nil, err
	}

	log.Info().Str("command", "DeployPacks").Msg("Command finished")
	return result, nil
}
