package provision

import (
	"errors"
	"fmt"
	"os"

	"github.com/arthur-debert/dodot/pkg/core"
	doerrors "github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/shell"
	"github.com/arthur-debert/dodot/pkg/types"
)

// ProvisionPacksOptions defines the options for the ProvisionPacks command.
type ProvisionPacksOptions struct {
	// DotfilesRoot is the path to the root of the dotfiles directory.
	DotfilesRoot string
	// PackNames is a list of specific packs to install. If empty, all packs are installed.
	PackNames []string
	// DryRun specifies whether to perform a dry run without making changes.
	DryRun bool
	// Force re-runs handlers that normally only run once.
	Force bool
	// EnableHomeSymlinks allows symlink operations to target the user's home directory.
	EnableHomeSymlinks bool
}

// ProvisionPacks runs the installation + deployment using the unified core execution approach.
// It executes both code execution handlers (install scripts, brewfiles) and configuration
// handlers (symlinks, shell profiles, path) in the correct order.
func ProvisionPacks(opts ProvisionPacksOptions) (*types.ExecutionContext, error) {
	log := logging.GetLogger("commands.provision")
	log.Debug().Str("command", "ProvisionPacks").Msg("Executing command")

	// Create confirmer that always approves (matches internal pipeline behavior)
	confirmer := &alwaysApproveConfirmer{}

	// Use the unified core execution flow (runs all handlers in correct order)
	ctx, err := core.Execute(core.CommandProvision, core.ExecuteOptions{
		DotfilesRoot: opts.DotfilesRoot,
		PackNames:    opts.PackNames,
		DryRun:       opts.DryRun,
		Force:        opts.Force,
		Confirmer:    confirmer,
		FileSystem:   filesystem.NewOS(),
	})

	if err != nil {
		log.Error().Err(err).Msg("Provision failed")
		// Check if this is a pack not found error and propagate it directly
		var dodotErr *doerrors.DodotError
		if errors.As(err, &dodotErr) && dodotErr.Code == doerrors.ErrPackNotFound {
			return ctx, err // Return the original error to preserve error code
		}
		return ctx, doerrors.Wrapf(err, doerrors.ErrOperationExecute, "failed to execute provisioning operations")
	}

	// Set up shell integration after successful execution (not in dry-run mode)
	if !opts.DryRun && (ctx.CompletedHandlers > 0 || ctx.SkippedHandlers > 0) {
		log.Debug().Msg("Installing shell integration")

		// Create paths instance to get data directory
		p, pathErr := paths.New(opts.DotfilesRoot)
		if pathErr != nil {
			log.Warn().Err(pathErr).Msg("Could not create paths instance for shell integration")
			fmt.Fprintf(os.Stderr, "Warning: Could not set up shell integration: %v\n", pathErr)
		} else {
			dataDir := p.DataDir()
			if err := shell.InstallShellIntegration(dataDir); err != nil {
				log.Warn().Err(err).Msg("Could not install shell integration")
				fmt.Fprintf(os.Stderr, "Warning: Could not install shell integration: %v\n", err)
			} else {
				log.Info().Str("dataDir", dataDir).Msg("Shell integration installed successfully")

				// Show user what was installed and how to enable it
				snippet := types.GetShellIntegrationSnippet("bash", dataDir)

				fmt.Println("✅ Shell integration installed successfully!")
				fmt.Printf("📁 Scripts installed to: %s/shell/\n", dataDir)
				fmt.Println("🔧 To enable, add this line to your shell config (~/.bashrc, ~/.zshrc, etc.):")
				fmt.Printf("   %s\n", snippet)
				fmt.Println("🔄 Then reload your shell or run: source ~/.bashrc")
			}
		}
	}

	log.Info().
		Int("totalActions", ctx.TotalHandlers).
		Str("command", "ProvisionPacks").
		Msg("Command finished")

	return ctx, nil
}

// alwaysApproveConfirmer matches the behavior of internal.simpleConfirmer
type alwaysApproveConfirmer struct{}

func (a *alwaysApproveConfirmer) RequestConfirmation(id, title, description string, items ...string) bool {
	return true
}
