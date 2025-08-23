package fill

import (
	"os"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/filesystem"
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

	// 2. Get missing template files using the existing template system
	missingTemplates, err := core.GetMissingTemplateFiles(targetPack.Path, opts.PackName)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrInternal, "failed to get missing templates")
	}

	// 3. Create filesystem instance for file operations
	fs := filesystem.NewOS()
	var filesCreated []string

	// 4. Create each missing template file
	for _, template := range missingTemplates {
		templatePath := filepath.Join(targetPack.Path, template.Filename)
		log.Info().Str("file", template.Filename).Str("handler", template.HandlerName).Msg("Creating missing template file")

		if err := fs.WriteFile(templatePath, []byte(template.Content), os.FileMode(template.Mode)); err != nil {
			return nil, errors.Wrapf(err, errors.ErrInternal, "failed to create template file %s", template.Filename)
		}
		filesCreated = append(filesCreated, template.Filename)
	}

	// 5. Return result
	result := &types.FillResult{
		PackName:     opts.PackName,
		FilesCreated: filesCreated,
	}

	log.Info().Str("command", "FillPack").
		Str("pack", opts.PackName).
		Str("path", targetPack.Path).
		Int("filesCreated", len(filesCreated)).
		Msg("Command finished")

	return result, nil
}
