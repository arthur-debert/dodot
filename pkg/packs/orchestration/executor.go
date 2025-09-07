package orchestration

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/packs/discovery"
	"github.com/arthur-debert/dodot/pkg/types"
)

// Execute runs the pack pipeline for a given command.
// It discovers packs, executes the command for each pack, and aggregates results.
func Execute(command Command, packNames []string, opts Options) (*Result, error) {
	logger := logging.GetLogger("packpipeline")
	logger.Debug().
		Str("command", command.Name()).
		Strs("packNames", packNames).
		Str("dotfilesRoot", opts.DotfilesRoot).
		Bool("dryRun", opts.DryRun).
		Msg("Starting pack pipeline execution")

	// Initialize filesystem if not provided
	fs := opts.FileSystem
	if fs == nil {
		fs = filesystem.NewOS()
	}

	// Load root config if not provided
	rootConfig := opts.RootConfig
	if rootConfig == nil {
		cfg, err := config.GetRootConfig(opts.DotfilesRoot)
		if err != nil {
			logger.Error().Err(err).Msg("Failed to load root configuration")
			return nil, fmt.Errorf("failed to load root configuration: %w", err)
		}
		rootConfig = cfg
	}

	// Step 1: Discover and select packs
	packs, err := discovery.DiscoverAndSelectPacksFSWithConfig(opts.DotfilesRoot, packNames, fs, rootConfig)
	if err != nil {
		logger.Error().Err(err).Msg("Failed to discover packs")
		return nil, fmt.Errorf("failed to discover packs: %w", err)
	}

	logger.Info().
		Int("packCount", len(packs)).
		Msg("Discovered packs for processing")

	// Initialize result
	result := &Result{
		Command:     command.Name(),
		TotalPacks:  len(packs),
		PackResults: make([]PackResult, 0, len(packs)),
	}

	// Step 2: Execute command for each pack
	for _, pack := range packs {
		logger.Debug().
			Str("pack", pack.Name).
			Msg("Executing command for pack")

		// Create pack-specific config by merging root config with pack config
		packConfig, err := config.GetPackConfig(rootConfig.(*config.Config), pack.Path)
		if err != nil {
			logger.Error().
				Err(err).
				Str("pack", pack.Name).
				Msg("Failed to load pack configuration")

			result.FailedPacks++
			result.PackResults = append(result.PackResults, PackResult{
				Pack:    pack,
				Success: false,
				Error:   fmt.Errorf("failed to load pack configuration: %w", err),
			})
			continue
		}

		// Create pack-specific options with merged config
		packOpts := opts
		packOpts.RootConfig = packConfig

		packResult, err := command.ExecuteForPack(pack, packOpts)
		if err != nil {
			logger.Error().
				Err(err).
				Str("pack", pack.Name).
				Msg("Command execution failed for pack")

			result.FailedPacks++
			result.PackResults = append(result.PackResults, PackResult{
				Pack:    pack,
				Success: false,
				Error:   err,
			})
		} else {
			if packResult.Success {
				result.SuccessfulPacks++
			} else {
				result.FailedPacks++
			}
			result.PackResults = append(result.PackResults, *packResult)
		}
	}

	// Step 3: Determine overall success
	if result.FailedPacks > 0 {
		result.Error = fmt.Errorf("%d pack(s) failed", result.FailedPacks)
	}

	logger.Info().
		Str("command", command.Name()).
		Int("totalPacks", result.TotalPacks).
		Int("successful", result.SuccessfulPacks).
		Int("failed", result.FailedPacks).
		Msg("Pack pipeline execution completed")

	return result, nil
}

// ExecuteSingle executes a command for a single pack without pack discovery.
// This is useful for commands that already have a specific pack.
func ExecuteSingle(command Command, pack types.Pack, opts Options) (*PackResult, error) {
	logger := logging.GetLogger("packpipeline")
	logger.Debug().
		Str("command", command.Name()).
		Str("pack", pack.Name).
		Bool("dryRun", opts.DryRun).
		Msg("Executing command for single pack")

	return command.ExecuteForPack(pack, opts)
}
