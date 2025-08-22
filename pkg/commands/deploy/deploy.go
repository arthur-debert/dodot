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
	// EnableHomeSymlinks allows symlink operations to target the user's home directory.
	EnableHomeSymlinks bool
}

// DeployPacks runs the deployment logic using the direct executor approach.
// It executes RunModeLinking actions only (symlinks, shell profiles, path) while
// skipping RunModeProvisioning actions (install scripts, brewfiles).
func DeployPacks(opts DeployPacksOptions) (*types.ExecutionContext, error) {
	log := logging.GetLogger("commands.deploy")
	log.Debug().Str("command", "DeployPacks").Msg("Executing command")

	// Use the internal pipeline with RunModeLinking (deploy mode)
	ctx, err := internal.RunPipeline(internal.PipelineOptions{
		DotfilesRoot:       opts.DotfilesRoot,
		PackNames:          opts.PackNames,
		DryRun:             opts.DryRun,
		RunMode:            types.RunModeLinking, // Key: only run repeatable actions
		Force:              false,                // Deploy doesn't use force flag
		EnableHomeSymlinks: opts.EnableHomeSymlinks,
	})

	if err != nil {
		log.Error().Err(err).Msg("Deploy failed")
		return ctx, err
	}

	log.Info().Str("command", "DeployPacks").Msg("Command finished")
	return ctx, nil
}

// DeployPacksDirect is an alias for DeployPacks for backward compatibility.
// Deprecated: Use DeployPacks instead.
func DeployPacksDirect(opts DeployPacksOptions) (*types.ExecutionContext, error) {
	return DeployPacks(opts)
}
