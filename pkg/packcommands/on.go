package packcommands

import (
	"fmt"
	"os"
	"time"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/handlerpipeline"
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
// The function now uses the handler pipeline for cleaner separation of concerns.
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

	// Initialize paths and datastore
	pathsInstance, err := paths.New(opts.DotfilesRoot)
	if err != nil {
		return nil, fmt.Errorf("failed to initialize paths: %w", err)
	}
	dataStore := datastore.New(fs, pathsInstance)

	// Track execution details
	var totalDeployed int
	var errors []error
	packResults := make(map[string]*handlerpipeline.Result)

	// Step 1: Discover packs
	selectedPacks, err := core.DiscoverAndSelectPacksFS(opts.DotfilesRoot, opts.PackNames, fs)
	if err != nil {
		logger.Error().Err(err).Msg("Failed to discover packs")
		return nil, fmt.Errorf("failed to discover packs: %w", err)
	}

	logger.Debug().
		Int("packCount", len(selectedPacks)).
		Msg("Discovered packs for processing")

	// Step 2: Process each pack with handler pipeline
	for _, pack := range selectedPacks {
		logger.Debug().
			Str("pack", pack.Name).
			Msg("Processing pack")

		// Link phase (configuration handlers only)
		linkResult, err := handlerpipeline.ExecuteHandlersForPack(
			pack,
			handlerpipeline.ConfigOnly,
			handlerpipeline.Options{
				DryRun:     opts.DryRun,
				Force:      false, // Link phase doesn't use force
				FileSystem: fs,
				DataStore:  dataStore,
			},
		)
		if err != nil {
			logger.Error().Err(err).Str("pack", pack.Name).Msg("Failed to link pack")
			errors = append(errors, fmt.Errorf("link failed for pack %s: %w", pack.Name, err))
		} else {
			totalDeployed += linkResult.SuccessCount
			if linkResult.FailureCount > 0 {
				errors = append(errors, fmt.Errorf("pack %s had %d failed handlers during link", pack.Name, linkResult.FailureCount))
			}
			packResults[pack.Name] = linkResult
		}

		// Provision phase (all handlers, unless --no-provision)
		if !opts.NoProvision {
			// Check if already provisioned and handle accordingly
			if !opts.ProvisionRerun {
				// Check if any code execution handlers are already provisioned
				isProvisioned := false
				for _, handlerName := range []string{"homebrew", "install"} {
					if provisioned, _ := pack.IsHandlerProvisioned(dataStore, handlerName); provisioned {
						isProvisioned = true
						break
					}
				}
				if isProvisioned {
					logger.Info().
						Str("pack", pack.Name).
						Msg("Pack already provisioned, skipping (use --provision-rerun to force)")
					continue
				}
			} else {
				// Clear provisioning state if forcing re-run
				for _, handlerName := range []string{"homebrew", "install"} {
					if hasState, _ := dataStore.HasHandlerState(pack.Name, handlerName); hasState {
						if err := dataStore.RemoveState(pack.Name, handlerName); err != nil {
							logger.Warn().Err(err).
								Str("pack", pack.Name).
								Str("handler", handlerName).
								Msg("Failed to clear provisioning state")
						}
					}
				}
			}

			provisionResult, err := handlerpipeline.ExecuteHandlersForPack(
				pack,
				handlerpipeline.All,
				handlerpipeline.Options{
					DryRun:     opts.DryRun,
					Force:      opts.Force,
					FileSystem: fs,
					DataStore:  dataStore,
				},
			)
			if err != nil {
				logger.Error().Err(err).Str("pack", pack.Name).Msg("Failed to provision pack")
				errors = append(errors, fmt.Errorf("provision failed for pack %s: %w", pack.Name, err))
			} else {
				totalDeployed += provisionResult.SuccessCount
				if provisionResult.FailureCount > 0 {
					errors = append(errors, fmt.Errorf("pack %s had %d failed handlers during provisioning", pack.Name, provisionResult.FailureCount))
				}
				// Merge provision results into pack results
				if existing, ok := packResults[pack.Name]; ok {
					// Combine results from link and provision phases
					existing.SuccessCount += provisionResult.SuccessCount
					existing.FailureCount += provisionResult.FailureCount
					existing.SkippedCount += provisionResult.SkippedCount
					existing.TotalHandlers += provisionResult.TotalHandlers
					existing.ExecutedHandlers = append(existing.ExecutedHandlers, provisionResult.ExecutedHandlers...)
				} else {
					packResults[pack.Name] = provisionResult
				}
			}
		}

	}

	// Step 3: Set up shell integration after successful provisioning (not in dry-run mode)
	if !opts.DryRun && !opts.NoProvision && totalDeployed > 0 {
		logger.Debug().Msg("Installing shell integration")

		dataDir := pathsInstance.DataDir()
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

	logger.Info().
		Int("totalDeployed", totalDeployed).
		Int("errors", len(errors)).
		Bool("dryRun", opts.DryRun).
		Msg("Turn on operation completed")

	// Get current pack status using pkg/pack/status.go functionality
	// Only attempt status if there were no critical errors during execution
	statusPacks := make([]types.DisplayPack, 0)
	if len(errors) == 0 || len(errors) <= 2 { // Allow some errors but not total failure
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
