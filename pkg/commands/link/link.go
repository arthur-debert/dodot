package link

import (
	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/filesystem"
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

// LinkPacks runs the linking logic using the unified core execution approach.
// It executes configuration handlers only (symlinks, shell profiles, path) while
// skipping code execution handlers (install scripts, brewfiles).
func LinkPacks(opts LinkPacksOptions) (*types.ExecutionContext, error) {
	log := logging.GetLogger("commands.link")
	log.Debug().Str("command", "LinkPacks").Msg("Executing command")

	// Create confirmer that always approves (matches internal pipeline behavior)
	confirmer := &alwaysApproveConfirmer{}

	// Use the unified core execution flow
	ctx, err := core.Execute(core.CommandLink, core.ExecuteOptions{
		DotfilesRoot: opts.DotfilesRoot,
		PackNames:    opts.PackNames,
		DryRun:       opts.DryRun,
		Force:        false, // Link doesn't use force flag
		Confirmer:    confirmer,
		FileSystem:   filesystem.NewOS(),
	})

	if err != nil {
		log.Error().Err(err).Msg("Link failed")
		return ctx, err
	}

	log.Info().Str("command", "LinkPacks").Msg("Command finished")
	return ctx, nil
}

// alwaysApproveConfirmer matches the behavior of internal.simpleConfirmer
type alwaysApproveConfirmer struct{}

func (a *alwaysApproveConfirmer) RequestConfirmation(id, title, description string, items ...string) bool {
	return true
}
