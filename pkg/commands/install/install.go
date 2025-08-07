package install

import (
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
)

// InstallPacksOptions defines the options for the InstallPacks command.
type InstallPacksOptions struct {
	// DotfilesRoot is the path to the root of the dotfiles directory.
	DotfilesRoot string
	// PackNames is a list of specific packs to install. If empty, all packs are installed.
	PackNames []string
	// DryRun specifies whether to perform a dry run without making changes.
	DryRun bool
	// Force re-runs power-ups that normally only run once.
	Force bool
	// EnableHomeSymlinks allows symlink operations to target the user's home directory.
	EnableHomeSymlinks bool
}

// InstallPacks runs the installation + deployment using the direct executor approach.
// TODO: Implement new DirectExecutor-based installation (internal execution pipeline removed)
func InstallPacks(opts InstallPacksOptions) (*types.ExecutionContext, error) {
	log := logging.GetLogger("core.commands")
	log.Debug().Str("command", "InstallPacks").Msg("Executing command")

	// TODO: Replace with DirectExecutor-based implementation
	log.Info().Str("command", "InstallPacks").Msg("Command finished")
	return nil, errors.New(errors.ErrNotImplemented, "InstallPacks not yet implemented with new DirectExecutor")
}

// InstallPacksDirect is an alias for InstallPacks for backward compatibility.
// Deprecated: Use InstallPacks instead.
func InstallPacksDirect(opts InstallPacksOptions) (*types.ExecutionContext, error) {
	return InstallPacks(opts)
}
