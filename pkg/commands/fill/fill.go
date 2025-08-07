package fill

import (
	"fmt"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
)

// FillPackOptions defines the options for the FillPack command.
type FillPackOptions struct {
	// DotfilesRoot is the path to the root of the dotfiles directory.
	DotfilesRoot string
	// PackName is the name of the pack to fill with template files.
	PackName string
}

// FillPack adds placeholder files for power-ups to an existing pack.
func FillPack(opts FillPackOptions) (*types.FillResult, error) {
	log := logging.GetLogger("core.commands")
	log.Debug().Str("command", "FillPack").Str("pack", opts.PackName).Msg("Executing command")

	// 1. Get all packs to verify the pack exists
	candidates, err := core.GetPackCandidates(opts.DotfilesRoot)
	if err != nil {
		return nil, err
	}
	allPacks, err := core.GetPacks(candidates)
	if err != nil {
		return nil, err
	}

	// 2. Find the specific pack
	var targetPack *types.Pack
	for i := range allPacks {
		if allPacks[i].Name == opts.PackName {
			targetPack = &allPacks[i]
			break
		}
	}
	if targetPack == nil {
		return nil, errors.Newf(errors.ErrPackNotFound, "pack %q not found", opts.PackName)
	}

	// 3. Get missing template files
	missingTemplates, err := core.GetMissingTemplateFiles(targetPack.Path, opts.PackName)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrInternal, "failed to get missing templates")
	}

	// 4. Generate actions for missing templates
	var actions []types.Action
	for _, template := range missingTemplates {
		action := types.Action{
			Type:        types.ActionTypeWrite,
			Description: fmt.Sprintf("Create template file %s", template.Filename),
			Target:      filepath.Join(targetPack.Path, template.Filename),
			Content:     template.Content,
			Mode:        template.Mode,
			Pack:        opts.PackName,
			PowerUpName: template.PowerUpName,
			Priority:    50, // Lower priority for template files
		}
		actions = append(actions, action)
	}

	// 5. Execute actions using DirectExecutor and get operations from results
	var ops []types.Operation
	if len(actions) > 0 {
		// Initialize paths
		pathsInstance, err := paths.New(opts.DotfilesRoot)
		if err != nil {
			return nil, errors.Wrapf(err, errors.ErrInternal, "failed to initialize paths")
		}

		// Create DirectExecutor
		directExecutorOpts := &core.DirectExecutorOptions{
			Paths:             pathsInstance,
			DryRun:            false,
			Force:             true,
			AllowHomeSymlinks: false,
			Config:            config.Default(),
		}

		executor := core.NewDirectExecutor(directExecutorOpts)

		// Execute actions and extract operations from results
		results, err := executor.ExecuteActions(actions)
		if err != nil {
			return nil, errors.Wrapf(err, errors.ErrActionExecute, "failed to execute fill actions")
		}

		// FIXME: ARCHITECTURAL PROBLEM - fill command should NOT return Operation types!
		// User-facing commands should return Pack+PowerUp+File information:
		// - "Created .vimrc template for symlink PowerUp in vim pack"
		// NOT operation details: "Operation: WriteFile target=/path/vimrc"
		// See docs/design/display.txxt - users understand packs/powerups/files, not operations
		// Extract operations from operation results for compatibility
		for _, result := range results {
			if result.Operation != nil {
				ops = append(ops, *result.Operation)
			}
		}
	}

	// 6. Return result with operations
	result := &types.FillResult{
		PackName:     opts.PackName,
		FilesCreated: []string{},
		Operations:   ops,
	}

	// List files that will be created
	for _, template := range missingTemplates {
		result.FilesCreated = append(result.FilesCreated, template.Filename)
		log.Info().
			Str("file", template.Filename).
			Str("powerup", template.PowerUpName).
			Msg("Template file to be created")
	}

	log.Debug().
		Int("actionCount", len(actions)).
		Int("operationCount", len(ops)).
		Msg("Executed actions for FillPack")

	log.Info().Str("command", "FillPack").
		Str("pack", opts.PackName).
		Int("filesCreated", len(result.FilesCreated)).
		Msg("Command finished")
	return result, nil
}
