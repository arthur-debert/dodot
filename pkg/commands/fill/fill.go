package fill

import (
	"fmt"

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

	// 1. Find the specific pack
	targetPack, err := core.FindPack(opts.DotfilesRoot, opts.PackName)
	if err != nil {
		return nil, err
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
			Target:      targetPack.GetFilePath(template.Filename),
			Content:     template.Content,
			Mode:        template.Mode,
			Pack:        opts.PackName,
			HandlerName: template.HandlerName,
			Priority:    50, // Lower priority for template files
		}
		actions = append(actions, action)
	}

	// 5. Execute actions using DirectExecutor (Operations no longer returned)
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

		// FIXME: ARCHITECTURAL PROBLEM - fill command should return Pack+Handler+File information
		// NOT operation details. See docs/design/display.txxt
		// Operations are no longer returned (part of Operation layer elimination)
		_ = results // Results processed but not exposed in return value
	}

	// 6. Return result (Operations field removed as part of Operation elimination)
	result := &types.FillResult{
		PackName:     opts.PackName,
		FilesCreated: []string{},
	}

	// List files that will be created
	for _, template := range missingTemplates {
		result.FilesCreated = append(result.FilesCreated, template.Filename)
		log.Info().
			Str("file", template.Filename).
			Str("handler", template.HandlerName).
			Msg("Template file to be created")
	}

	log.Debug().
		Int("actionCount", len(actions)).
		Msg("Executed actions for FillPack")

	log.Info().Str("command", "FillPack").
		Str("pack", opts.PackName).
		Int("filesCreated", len(result.FilesCreated)).
		Msg("Command finished")
	return result, nil
}
