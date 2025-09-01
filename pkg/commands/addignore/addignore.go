package addignore

import (
	"time"

	"github.com/arthur-debert/dodot/pkg/commands/status"
	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/pack"
	"github.com/arthur-debert/dodot/pkg/types"
)

// AddIgnoreOptions holds options for the add-ignore command
type AddIgnoreOptions struct {
	DotfilesRoot string
	PackName     string
}

// AddIgnore creates a .dodotignore file using proper abstractions
func AddIgnore(opts AddIgnoreOptions) (*types.PackCommandResult, error) {
	logger := logging.GetLogger("commands.addignore")
	logger.Info().
		Str("pack", opts.PackName).
		Str("dotfiles_root", opts.DotfilesRoot).
		Msg("Creating ignore file for pack")

	// Get configuration
	cfg := config.Default()

	// Initialize filesystem
	fs := filesystem.NewOS()

	// Find the pack using core abstraction
	targetPack, err := core.FindPack(opts.DotfilesRoot, opts.PackName)
	if err != nil {
		return nil, err
	}

	// Wrap in our enhanced Pack type and delegate to AddIgnore method
	p := pack.New(targetPack)
	ignoreResult, err := p.AddIgnore(fs, cfg)
	if err != nil {
		return nil, err
	}

	// Get current pack status
	statusOpts := status.StatusPacksOptions{
		DotfilesRoot: opts.DotfilesRoot,
		PackNames:    []string{opts.PackName},
		FileSystem:   fs,
	}
	packStatus, statusErr := status.StatusPacks(statusOpts)
	if statusErr != nil {
		logger.Error().Err(statusErr).Msg("Failed to get pack status")
	}

	// Build result
	result := &types.PackCommandResult{
		Command:   "add-ignore",
		Timestamp: time.Now(),
		DryRun:    false,
		Packs:     []types.DisplayPack{},
		Metadata: types.CommandMetadata{
			IgnoreCreated:  ignoreResult.Created,
			AlreadyExisted: ignoreResult.AlreadyExisted,
		},
	}

	// Copy packs from status if available
	if packStatus != nil {
		result.Packs = packStatus.Packs
	}

	// Generate message
	if ignoreResult.AlreadyExisted {
		result.Message = "A .dodotignore file already exists in the pack " + opts.PackName + "."
	} else {
		result.Message = "A .dodotignore file has been added to the pack " + opts.PackName + "."
	}

	return result, nil
}
