package fill

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
)

// FillPackOptions defines the options for the FillPack command.
type FillPackOptions struct {
	// DotfilesRoot is the path to the root of the dotfiles directory.
	DotfilesRoot string
	// PackName is the name of the pack to fill with template files.
	PackName string
}

// FillPack adds placeholder files for handlers to an existing pack.
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
		// TODO: Update fill command to use new V2 executor
		// This command needs to be refactored to work with the new architecture
		return nil, errors.New(errors.ErrNotImplemented, "fill command needs to be updated for V2 architecture")
	}

	// The rest of this function is unreachable until fill command is updated
	panic("unreachable")
}
