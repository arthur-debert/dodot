package link

import (
	"github.com/arthur-debert/dodot/pkg/commands/internal"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
)

// LinkPacksOptions defines the options for the LinkPacks command.
type LinkPacksOptions struct {
	// DotfilesRoot is the path to the root of the dotfiles directory.
	DotfilesRoot string
	// PackNames is a list of specific packs to deploy. If empty, all packs are deployed.
	PackNames []string
	// DryRun specifies whether to perform a dry run without making changes.
	DryRun bool
	// EnableHomeSymlinks allows symlink operations to target the user's home directory.
	EnableHomeSymlinks bool
}

// LinkPacks runs the linking logic using the direct executor approach.
// It executes RunModeLinking actions only (symlinks, shell profiles, path) while
// skipping RunModeProvisioning actions (install scripts, brewfiles).
func LinkPacks(opts LinkPacksOptions) (*types.ExecutionContext, error) {
	log := logging.GetLogger("commands.link")
	log.Debug().Str("command", "LinkPacks").Msg("Executing command")

	// Use the internal pipeline with RunModeLinking (link mode)
	ctx, err := internal.RunPipeline(internal.PipelineOptions{
		DotfilesRoot:       opts.DotfilesRoot,
		PackNames:          opts.PackNames,
		DryRun:             opts.DryRun,
		RunMode:            types.RunModeLinking, // Key: only run repeatable actions
		Force:              false,                // Link doesn't use force flag
		EnableHomeSymlinks: opts.EnableHomeSymlinks,
	})

	if err != nil {
		log.Error().Err(err).Msg("Link failed")
		return ctx, err
	}

	log.Info().Str("command", "LinkPacks").Msg("Command finished")
	return ctx, nil
}

// LinkPacksDirect is an alias for LinkPacks for backward compatibility.
// Deprecated: Use LinkPacks instead.
func LinkPacksDirect(opts LinkPacksOptions) (*types.ExecutionContext, error) {
	return LinkPacks(opts)
}
