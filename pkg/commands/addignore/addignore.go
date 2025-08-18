package addignore

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/packs"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/rs/zerolog"
)

// AddIgnoreOptions holds options for the add-ignore command
type AddIgnoreOptions struct {
	DotfilesRoot string
	PackName     string
}

// AddIgnore creates a .dodotignore file in the specified pack
func AddIgnore(opts AddIgnoreOptions) (*types.AddIgnoreResult, error) {
	logger := logging.GetLogger("commands.addignore")
	logger.Info().
		Str("pack", opts.PackName).
		Str("dotfiles_root", opts.DotfilesRoot).
		Msg("Adding ignore file to pack")

	// Get all packs to verify the pack exists
	candidates, err := core.GetPackCandidates(opts.DotfilesRoot)
	if err != nil {
		return nil, err
	}
	allPacks, err := core.GetPacks(candidates)
	if err != nil {
		return nil, err
	}

	// Find the specific pack
	selectedPacks, err := packs.SelectPacks(allPacks, []string{opts.PackName})
	if err != nil {
		return nil, err
	}

	// SelectPacks returns a slice, but we know it has exactly one pack
	targetPack := selectedPacks[0]

	// Get ignore file path from config
	cfg := config.Default()
	ignoreFilePath := filepath.Join(targetPack.Path, cfg.Patterns.SpecialFiles.IgnoreFile)

	// Check if ignore file already exists
	if _, err := os.Stat(ignoreFilePath); err == nil {
		logger.Info().
			Str("pack", opts.PackName).
			Str("ignore_file", ignoreFilePath).
			Msg("Ignore file already exists")
		result := &types.AddIgnoreResult{
			PackName:       targetPack.Name,
			IgnoreFilePath: ignoreFilePath,
			Created:        false,
			AlreadyExisted: true,
		}
		logAddIgnore(logger, opts, result, nil)
		return result, nil
	}

	// Create the ignore file
	err = os.WriteFile(ignoreFilePath, []byte(""), 0644)
	if err != nil {
		return nil, fmt.Errorf("failed to create ignore file: %w", err)
	}

	logger.Info().
		Str("pack", opts.PackName).
		Str("ignore_file", ignoreFilePath).
		Msg("Created ignore file")

	result := &types.AddIgnoreResult{
		PackName:       targetPack.Name,
		IgnoreFilePath: ignoreFilePath,
		Created:        true,
		AlreadyExisted: false,
	}

	logAddIgnore(logger, opts, result, nil)
	return result, nil
}

// logAddIgnore logs the add-ignore command execution
func logAddIgnore(logger zerolog.Logger, opts AddIgnoreOptions, result *types.AddIgnoreResult, err error) {
	event := logger.Info()
	if err != nil {
		event = logger.Error().Err(err)
	}

	event.
		Str("command", "add-ignore").
		Str("pack", opts.PackName).
		Str("dotfiles_root", opts.DotfilesRoot)

	if result != nil {
		event.
			Str("ignore_file", result.IgnoreFilePath).
			Bool("created", result.Created).
			Bool("already_existed", result.AlreadyExisted)
	}

	if err != nil {
		event.Msg("Add-ignore command failed")
	} else {
		event.Msg("Add-ignore command completed")
	}
}
