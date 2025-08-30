package fill

import (
	"os"
	"path/filepath"
	"strings"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/rules"
	"github.com/arthur-debert/dodot/pkg/types"
)

// FillPackOptions defines the options for the FillPack command.
type FillPackOptions struct {
	// DotfilesRoot is the path to the root of the dotfiles directory.
	DotfilesRoot string
	// PackName is the name of the pack to fill with template files.
	PackName string
	// FileSystem is the filesystem to use (optional, defaults to OS filesystem)
	FileSystem types.FS
}

// FillPack adds placeholder files for handlers to an existing pack.
func FillPack(opts FillPackOptions) (*types.FillResult, error) {
	log := logging.GetLogger("core.commands")
	log.Debug().Str("command", "FillPack").Str("pack", opts.PackName).Msg("Executing command")

	// Use provided filesystem or default
	fs := opts.FileSystem
	if fs == nil {
		fs = filesystem.NewOS()
	}

	// 1. Find the specific pack
	targetPack, err := core.FindPackFS(opts.DotfilesRoot, opts.PackName, fs)
	if err != nil {
		return nil, err
	}

	// 2. Get handlers that need files
	handlersNeeding, err := rules.GetHandlersNeedingFiles(*targetPack, fs)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrInternal, "failed to get handlers needing files")
	}

	// 3. Build list of templates to create
	missingTemplates := []struct {
		Filename    string
		Content     string
		Mode        uint32
		HandlerName string
	}{}

	for _, handlerName := range handlersNeeding {
		// Get patterns for this handler
		patterns, err := rules.GetPatternsForHandler(handlerName, *targetPack)
		if err != nil {
			return nil, errors.Wrapf(err, errors.ErrInternal, "failed to get patterns for handler %s", handlerName)
		}

		// Get suggested filename
		filename := rules.SuggestFilenameForHandler(handlerName, patterns)
		if filename == "" {
			log.Warn().Str("handler", handlerName).Msg("Could not determine filename for handler")
			continue
		}

		// Get handler to get template content
		handler, err := rules.CreateHandler(handlerName, nil)
		if err != nil {
			return nil, errors.Wrapf(err, errors.ErrInternal, "failed to create handler %s", handlerName)
		}

		var templateContent string
		var fileMode uint32 = 0644 // default file mode

		// Get template content based on handler type
		switch h := handler.(type) {
		case handlers.LinkingHandler:
			templateContent = h.GetTemplateContent()
		case handlers.ProvisioningHandler:
			templateContent = h.GetTemplateContent()
			// Provisioning scripts should be executable
			if handlerName == "install" {
				fileMode = 0755
			}
		default:
			log.Warn().Str("handler", handlerName).Msg("Handler doesn't provide template content")
			continue
		}

		// For path handler, we need to create a directory
		if handlerName == "path" && filename != "" {
			// Path handler returns directory names, handle separately
			// Remove trailing slash if present
			dirName := strings.TrimSuffix(filename, "/")
			missingTemplates = append(missingTemplates, struct {
				Filename    string
				Content     string
				Mode        uint32
				HandlerName string
			}{
				Filename:    dirName,
				Content:     "", // Directory, no content
				Mode:        0755,
				HandlerName: handlerName,
			})
		} else if templateContent != "" {
			// Skip if handler doesn't provide template
			missingTemplates = append(missingTemplates, struct {
				Filename    string
				Content     string
				Mode        uint32
				HandlerName string
			}{
				Filename:    filename,
				Content:     templateContent,
				Mode:        fileMode,
				HandlerName: handlerName,
			})
		} else {
			log.Warn().Str("handler", handlerName).Msg("Handler returned empty template")
		}
	}

	// 4. Create each missing template file
	var filesCreated []string
	for _, template := range missingTemplates {
		templatePath := filepath.Join(targetPack.Path, template.Filename)

		// Handle directory creation for path handler
		if template.HandlerName == "path" && template.Content == "" {
			log.Info().Str("dir", template.Filename).Str("handler", template.HandlerName).Msg("Creating directory")
			if err := fs.MkdirAll(templatePath, os.FileMode(template.Mode)); err != nil {
				return nil, errors.Wrapf(err, errors.ErrInternal, "failed to create directory %s", template.Filename)
			}
		} else {
			log.Info().Str("file", template.Filename).Str("handler", template.HandlerName).Msg("Creating template file")
			if err := fs.WriteFile(templatePath, []byte(template.Content), os.FileMode(template.Mode)); err != nil {
				return nil, errors.Wrapf(err, errors.ErrInternal, "failed to create template file %s", template.Filename)
			}
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
