package on

import (
	"fmt"
	"time"

	"github.com/arthur-debert/dodot/pkg/commands/status"
	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/shell"
	"github.com/arthur-debert/dodot/pkg/types"
	"os"
)

// OnPacksOptions defines the options for the OnPacks command
type OnPacksOptions struct {
	// DotfilesRoot is the path to the root of the dotfiles directory
	DotfilesRoot string
	// PackNames is a list of specific packs to turn on. If empty, all packs are turned on
	PackNames []string
	// DryRun specifies whether to perform a dry run without making changes
	DryRun bool
	// Force forces operations even if there are conflicts
	Force bool
	// NoProvision skips provisioning handlers (only link files)
	NoProvision bool
	// ProvisionRerun forces re-run provisioning even if already done
	ProvisionRerun bool
}

// OnPacks turns on the specified packs by deploying all handlers.
// This creates symlinks, sets up shell integrations, and runs provisioning.
//
// The command uses core.Execute with appropriate options to control behavior.
func OnPacks(opts OnPacksOptions) (*types.PackCommandResult, error) {
	logger := logging.GetLogger("commands.on")
	logger.Debug().
		Str("dotfilesRoot", opts.DotfilesRoot).
		Strs("packNames", opts.PackNames).
		Bool("dryRun", opts.DryRun).
		Bool("force", opts.Force).
		Bool("noProvision", opts.NoProvision).
		Bool("provisionRerun", opts.ProvisionRerun).
		Msg("Starting on command")

	// Track execution details for metadata
	var totalDeployed int
	var errors []error

	// Step 1: Run link (configuration handlers only)
	linkResult, err := core.Execute(core.CommandLink, core.ExecuteOptions{
		DotfilesRoot: opts.DotfilesRoot,
		PackNames:    opts.PackNames,
		DryRun:       opts.DryRun,
		Force:        false,
		FileSystem:   filesystem.NewOS(),
	})
	if err != nil {
		logger.Error().Err(err).Msg("Failed to link packs")
		errors = append(errors, fmt.Errorf("link failed: %w", err))
	} else {
		totalDeployed += linkResult.CompletedHandlers
		// Check for errors in pack results
		for packName, packResult := range linkResult.PackResults {
			if packResult.FailedHandlers > 0 {
				errors = append(errors, fmt.Errorf("pack %s had %d failed handlers", packName, packResult.FailedHandlers))
			}
		}
	}

	// Step 2: Run provision (unless --no-provision was specified)
	if !opts.NoProvision {
		provisionResult, err := core.Execute(core.CommandProvision, core.ExecuteOptions{
			DotfilesRoot:        opts.DotfilesRoot,
			PackNames:           opts.PackNames,
			DryRun:              opts.DryRun,
			Force:               opts.Force,
			ForceReprovisioning: opts.ProvisionRerun,
			FileSystem:          filesystem.NewOS(),
		})
		if err != nil {
			logger.Error().Err(err).Msg("Failed to provision packs")
			errors = append(errors, fmt.Errorf("provision failed: %w", err))
		} else {
			totalDeployed += provisionResult.CompletedHandlers
			// Check for errors in pack results
			for packName, packResult := range provisionResult.PackResults {
				if packResult.FailedHandlers > 0 {
					errors = append(errors, fmt.Errorf("pack %s had %d failed handlers during provisioning", packName, packResult.FailedHandlers))
				}
			}
		}

		// Set up shell integration after successful provisioning (not in dry-run mode)
		if !opts.DryRun && provisionResult != nil && (provisionResult.CompletedHandlers > 0 || provisionResult.SkippedHandlers > 0) {
			logger.Debug().Msg("Installing shell integration")

			// Create paths instance to get data directory
			p, pathErr := paths.New(opts.DotfilesRoot)
			if pathErr != nil {
				logger.Warn().Err(pathErr).Msg("Could not create paths instance for shell integration")
				fmt.Fprintf(os.Stderr, "Warning: Could not set up shell integration: %v\n", pathErr)
			} else {
				dataDir := p.DataDir()
				if err := shell.InstallShellIntegration(dataDir); err != nil {
					logger.Warn().Err(err).Msg("Could not install shell integration")
					fmt.Fprintf(os.Stderr, "Warning: Could not install shell integration: %v\n", err)
				} else {
					logger.Info().Str("dataDir", dataDir).Msg("Shell integration installed successfully")

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
	} else {
		logger.Info().Msg("Skipping provision step due to --no-provision flag")
	}

	logger.Info().
		Int("totalDeployed", totalDeployed).
		Int("errors", len(errors)).
		Bool("dryRun", opts.DryRun).
		Msg("On command completed")

	// Get current pack status
	statusOpts := status.StatusPacksOptions{
		DotfilesRoot: opts.DotfilesRoot,
		PackNames:    opts.PackNames,
		FileSystem:   filesystem.NewOS(),
	}
	packStatus, err := status.StatusPacks(statusOpts)
	if err != nil {
		logger.Error().Err(err).Msg("Failed to get pack status")
		errors = append(errors, fmt.Errorf("failed to get pack status: %w", err))
	}

	// Build result
	result := &types.PackCommandResult{
		Command:   "on",
		Timestamp: time.Now(),
		DryRun:    opts.DryRun,
		Packs:     []types.DisplayPack{},
		Metadata: types.CommandMetadata{
			TotalDeployed:  totalDeployed,
			NoProvision:    opts.NoProvision,
			ProvisionRerun: opts.ProvisionRerun,
		},
	}

	// Copy packs from status if available
	if packStatus != nil {
		result.Packs = packStatus.Packs
	}

	// Generate message
	packNames := make([]string, 0, len(result.Packs))
	for _, pack := range result.Packs {
		packNames = append(packNames, pack.Name)
	}
	result.Message = types.FormatCommandMessage("turned on", packNames)

	if len(errors) > 0 {
		return result, fmt.Errorf("on command encountered %d errors", len(errors))
	}

	return result, nil
}
