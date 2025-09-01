// Package commands provides Command implementations for the pack pipeline.
package commands

import (
	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/handlerpipeline"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/packcommands"
	"github.com/arthur-debert/dodot/pkg/packpipeline"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/shell"
	"github.com/arthur-debert/dodot/pkg/types"
)

// OnCommand implements the "on" command using the pack pipeline.
type OnCommand struct {
	// NoProvision skips the provisioning phase
	NoProvision bool

	// Force forces operations even if they might be destructive
	Force bool
}

// Name returns the command name.
func (c *OnCommand) Name() string {
	return "on"
}

// ExecuteForPack executes the "on" command for a single pack.
func (c *OnCommand) ExecuteForPack(pack types.Pack, opts packpipeline.Options) (*packpipeline.PackResult, error) {
	logger := logging.GetLogger("packpipeline.on")
	logger.Debug().
		Str("pack", pack.Name).
		Bool("noProvision", c.NoProvision).
		Bool("force", c.Force).
		Msg("Executing on command for pack")

	// Initialize filesystem
	fs := opts.FileSystem
	if fs == nil {
		fs = filesystem.NewOS()
	}

	// Initialize paths and datastore
	pathsInstance, err := paths.New(opts.DotfilesRoot)
	if err != nil {
		return &packpipeline.PackResult{
			Pack:    pack,
			Success: false,
			Error:   err,
		}, err
	}
	ds := datastore.New(fs, pathsInstance)

	// Track execution details
	var linkSuccessCount, provisionSuccessCount int
	var linkFailureCount, provisionFailureCount int

	// Phase 1: Execute configuration handlers (symlink, shell, path)
	linkResult, err := handlerpipeline.ExecuteHandlersForPack(
		pack,
		handlerpipeline.ConfigOnly,
		handlerpipeline.Options{
			DryRun:     opts.DryRun,
			Force:      c.Force,
			FileSystem: fs,
			DataStore:  ds,
		},
	)
	if err != nil {
		logger.Error().Err(err).Str("pack", pack.Name).Msg("Configuration phase failed")
		linkFailureCount = 1
	} else {
		linkSuccessCount = linkResult.SuccessCount
		linkFailureCount = linkResult.FailureCount
	}

	// Phase 2: Execute code execution handlers (homebrew, install) if not skipped
	if !c.NoProvision {
		provisionResult, err := handlerpipeline.ExecuteHandlersForPack(
			pack,
			handlerpipeline.ProvisionOnly,
			handlerpipeline.Options{
				DryRun:     opts.DryRun,
				Force:      c.Force,
				FileSystem: fs,
				DataStore:  ds,
			},
		)
		if err != nil {
			logger.Error().Err(err).Str("pack", pack.Name).Msg("Provisioning phase failed")
			provisionFailureCount = 1
		} else {
			provisionSuccessCount = provisionResult.SuccessCount
			provisionFailureCount = provisionResult.FailureCount
		}
	}

	// Install shell integration if needed
	if !opts.DryRun {
		if err := shell.InstallShellIntegration(pathsInstance.DataDir()); err != nil {
			logger.Warn().Err(err).Str("pack", pack.Name).Msg("Failed to install shell integration")
		}
	}

	// Get pack status for result (optional, for display purposes)
	var packStatus *packcommands.StatusResult
	statusOpts := packcommands.StatusOptions{
		Pack:       pack,
		DataStore:  ds,
		FileSystem: fs,
		Paths:      pathsInstance,
	}
	packStatus, err = packcommands.GetStatus(statusOpts)
	if err != nil {
		logger.Warn().Err(err).Str("pack", pack.Name).Msg("Failed to get pack status")
		// Continue without status - it's not critical for the command success
		packStatus = nil
	}

	// Determine overall success
	totalFailures := linkFailureCount + provisionFailureCount
	success := totalFailures == 0

	logger.Info().
		Str("pack", pack.Name).
		Int("linkSuccess", linkSuccessCount).
		Int("linkFailure", linkFailureCount).
		Int("provisionSuccess", provisionSuccessCount).
		Int("provisionFailure", provisionFailureCount).
		Bool("success", success).
		Msg("On command completed for pack")

	return &packpipeline.PackResult{
		Pack:                  pack,
		Success:               success,
		Error:                 nil,
		CommandSpecificResult: packStatus,
	}, nil
}
