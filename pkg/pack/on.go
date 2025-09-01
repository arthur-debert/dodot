package pack

import (
	"fmt"
	"os"
	"time"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/shell"
	"github.com/arthur-debert/dodot/pkg/types"
)

// OnOptions defines the options for turning on packs
type OnOptions struct {
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
	// FileSystem to use (optional, defaults to OS filesystem)
	FileSystem types.FS
}

// TurnOn turns on the specified packs by deploying all handlers.
// This creates symlinks, sets up shell integrations, and runs provisioning.
//
// The function uses core.Execute with appropriate options to control behavior.
func TurnOn(opts OnOptions) (*types.PackCommandResult, error) {
	logger := logging.GetLogger("pack.on")
	logger.Debug().
		Str("dotfilesRoot", opts.DotfilesRoot).
		Strs("packNames", opts.PackNames).
		Bool("dryRun", opts.DryRun).
		Bool("force", opts.Force).
		Bool("noProvision", opts.NoProvision).
		Bool("provisionRerun", opts.ProvisionRerun).
		Msg("Starting turn on operation")

	// Use provided filesystem or default to OS
	fs := opts.FileSystem
	if fs == nil {
		fs = filesystem.NewOS()
	}

	// Track execution details for metadata
	var totalDeployed int
	var errors []error

	// Step 1: Run link (configuration handlers only)
	linkResult, err := core.Execute(core.CommandLink, core.ExecuteOptions{
		DotfilesRoot: opts.DotfilesRoot,
		PackNames:    opts.PackNames,
		DryRun:       opts.DryRun,
		Force:        false,
		FileSystem:   fs,
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
			FileSystem:          fs,
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
					snippet := shell.GetShellIntegrationSnippet("bash", dataDir)

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
		Msg("Turn on operation completed")

	// Get current pack status using pkg/pack/status.go functionality
	// Only attempt status if there were no critical errors during execution
	statusPacks := make([]types.DisplayPack, 0)
	if len(errors) == 0 || len(errors) <= 2 { // Allow some errors but not total failure
		pathsInstance, pathErr := paths.New(opts.DotfilesRoot)
		if pathErr != nil {
			logger.Warn().Err(pathErr).Msg("Failed to create paths for status check")
			// Don't add this to errors since status is supplementary
		} else {
			// Use core pack discovery - this will return empty list if no packs found
			selectedPacks, discErr := core.DiscoverAndSelectPacksFS(opts.DotfilesRoot, opts.PackNames, fs)
			if discErr != nil {
				logger.Warn().Err(discErr).Msg("Failed to discover packs for status")
				// Don't add this to errors since status is supplementary
			} else {
				// Create datastore for status checking
				dataStore := datastore.New(fs, pathsInstance)

				// Get status for each pack using pkg/pack/status.go
				for _, p := range selectedPacks {
					statusOpts := StatusOptions{
						Pack:       p,
						DataStore:  dataStore,
						FileSystem: fs,
						Paths:      pathsInstance,
					}

					packStatus, statusErr := GetStatus(statusOpts)
					if statusErr != nil {
						logger.Warn().Err(statusErr).Str("pack", p.Name).Msg("Failed to get individual pack status")
						continue
					}

					// Convert to display format using the same logic as status command
					displayPack := types.DisplayPack{
						Name:      packStatus.Name,
						HasConfig: packStatus.HasConfig,
						IsIgnored: packStatus.IsIgnored,
						Status:    packStatus.Status,
						Files:     make([]types.DisplayFile, 0, len(packStatus.Files)),
					}

					// Convert each file status
					for _, file := range packStatus.Files {
						displayFile := types.DisplayFile{
							Handler:        file.Handler,
							Path:           file.Path,
							Status:         statusStateToDisplayStatus(file.Status.State),
							Message:        file.Status.Message,
							LastExecuted:   file.Status.Timestamp,
							HandlerSymbol:  types.GetHandlerSymbol(file.Handler),
							AdditionalInfo: file.AdditionalInfo,
						}
						displayPack.Files = append(displayPack.Files, displayFile)
					}

					// Add special files if present
					if packStatus.IsIgnored {
						displayPack.Files = append([]types.DisplayFile{{
							Path:   ".dodotignore",
							Status: "ignored",
						}}, displayPack.Files...)
					}
					if packStatus.HasConfig {
						displayPack.Files = append([]types.DisplayFile{{
							Path:   ".dodot.toml",
							Status: "config",
						}}, displayPack.Files...)
					}

					statusPacks = append(statusPacks, displayPack)
				}
			}
		}
	}

	// Build result
	result := &types.PackCommandResult{
		Command:   "on",
		Timestamp: time.Now(),
		DryRun:    opts.DryRun,
		Packs:     statusPacks,
		Metadata: types.CommandMetadata{
			TotalDeployed:  totalDeployed,
			NoProvision:    opts.NoProvision,
			ProvisionRerun: opts.ProvisionRerun,
		},
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
