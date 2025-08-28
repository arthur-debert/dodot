package provision

import (
	"errors"
	"fmt"
	"os"

	"github.com/arthur-debert/dodot/pkg/commands/internal"
	doerrors "github.com/arthur-debert/dodot/pkg/errors"
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

// ProvisionPacks runs the installation + deployment using the direct executor approach.
// It executes both code execution handlers (install scripts, brewfiles) and configuration
// handlers (symlinks, shell profiles, path) in sequence.
func ProvisionPacks(opts ProvisionPacksOptions) (*types.ExecutionContext, error) {
	log := logging.GetLogger("commands.provision")
	log.Debug().Str("command", "ProvisionPacks").Msg("Executing command")

	// Phase 1: Run all handlers (both code execution and configuration)
	log.Debug().Msg("Phase 1: Executing provisioning actions (install scripts, brewfiles)")
	installCtx, err := internal.RunPipeline(internal.PipelineOptions{
		DotfilesRoot:       opts.DotfilesRoot,
		PackNames:          opts.PackNames,
		DryRun:             opts.DryRun,
		CommandMode:        internal.CommandModeAll, // Run all handler types
		Force:              opts.Force,              // Force flag applies to provisioning actions
		EnableHomeSymlinks: opts.EnableHomeSymlinks,
	})

	if err != nil {
		log.Error().Err(err).Msg("Phase 1 (provisioning) failed")
		// Check if this is a pack not found error and propagate it directly
		var dodotErr *doerrors.DodotError
		if errors.As(err, &dodotErr) && dodotErr.Code == doerrors.ErrPackNotFound {
			return installCtx, err // Return the original error to preserve error code
		}
		return installCtx, doerrors.Wrapf(err, doerrors.ErrActionExecute, "failed to execute provisioning actions")
	}

	// Phase 2: Run configuration handlers only (symlinks, profiles, etc.)
	log.Debug().Msg("Phase 2: Executing deployment actions (symlinks, profiles)")
	deployCtx, err := internal.RunPipeline(internal.PipelineOptions{
		DotfilesRoot:       opts.DotfilesRoot,
		PackNames:          opts.PackNames,
		DryRun:             opts.DryRun,
		CommandMode:        internal.CommandModeConfiguration, // Only configuration handlers
		Force:              false,                             // Force doesn't apply to deploy actions
		EnableHomeSymlinks: opts.EnableHomeSymlinks,
	})

	if err != nil {
		log.Error().Err(err).Msg("Phase 2 (linking) failed")
		// Check if this is a pack not found error and propagate it directly
		var dodotErr *doerrors.DodotError
		if errors.As(err, &dodotErr) && dodotErr.Code == doerrors.ErrPackNotFound {
			return mergeExecutionContexts(installCtx, deployCtx), err // Return the original error to preserve error code
		}
		// Return combined context with partial results from both phases
		return mergeExecutionContexts(installCtx, deployCtx), doerrors.Wrapf(err, doerrors.ErrActionExecute, "failed to execute linking actions")
	}

	// Merge results from both phases
	mergedCtx := mergeExecutionContexts(installCtx, deployCtx)

	// Set up shell integration after successful execution (not in dry-run mode)
	if !opts.DryRun && (mergedCtx.CompletedActions > 0 || mergedCtx.SkippedActions > 0) {
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
		Int("installActions", installCtx.TotalActions).
		Int("deployActions", deployCtx.TotalActions).
		Int("totalActions", mergedCtx.TotalActions).
		Str("command", "ProvisionPacks").
		Msg("Command finished")

	return mergedCtx, nil
}

// mergeExecutionContexts combines results from install and deploy phases into a single context
func mergeExecutionContexts(installCtx, deployCtx *types.ExecutionContext) *types.ExecutionContext {
	if installCtx == nil && deployCtx == nil {
		return types.NewExecutionContext("provision", false)
	}
	if installCtx == nil {
		deployCtx.Command = "provision" // Update command name
		return deployCtx
	}
	if deployCtx == nil {
		return installCtx
	}

	// Create new merged context using provisioning context as base
	merged := types.NewExecutionContext("provision", installCtx.DryRun)
	merged.StartTime = installCtx.StartTime

	// Add all pack results from install phase
	for packName, packResult := range installCtx.PackResults {
		merged.AddPackResult(packName, packResult)
	}

	// Merge in pack results from deploy phase
	for packName, deployPackResult := range deployCtx.PackResults {
		if existingPackResult, exists := merged.PackResults[packName]; exists {
			// Merge Handler results from deploy into existing pack result
			existingPackResult.HandlerResults = append(existingPackResult.HandlerResults, deployPackResult.HandlerResults...)
			existingPackResult.TotalHandlers += deployPackResult.TotalHandlers
			existingPackResult.CompletedHandlers += deployPackResult.CompletedHandlers
			existingPackResult.FailedHandlers += deployPackResult.FailedHandlers
			existingPackResult.SkippedHandlers += deployPackResult.SkippedHandlers

			// Update pack status - if either phase failed, mark as failed
			switch deployPackResult.Status {
			case types.ExecutionStatusError:
				existingPackResult.Status = types.ExecutionStatusError
			case types.ExecutionStatusPartial:
				existingPackResult.Status = types.ExecutionStatusPartial
			}
		} else {
			// Add pack result that only appeared in deploy phase
			merged.AddPackResult(packName, deployPackResult)
		}
	}

	// Use the later end time
	if deployCtx.EndTime.After(installCtx.EndTime) {
		merged.EndTime = deployCtx.EndTime
	} else {
		merged.EndTime = installCtx.EndTime
	}

	// Recalculate totals (AddPackResult should have handled this, but be explicit)
	merged.TotalActions = 0
	merged.CompletedActions = 0
	merged.FailedActions = 0
	merged.SkippedActions = 0

	for _, packResult := range merged.PackResults {
		merged.TotalActions += packResult.TotalHandlers
		merged.CompletedActions += packResult.CompletedHandlers
		merged.FailedActions += packResult.FailedHandlers
		merged.SkippedActions += packResult.SkippedHandlers
	}

	return merged
}
