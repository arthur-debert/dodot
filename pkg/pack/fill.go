package pack

import (
	"os"
	"strings"

	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/rules"
	"github.com/arthur-debert/dodot/pkg/types"
)

// FillResult represents the result of filling a pack with template files
type FillResult struct {
	FilesCreated []string `json:"filesCreated"`
}

// Fill adds template files for handlers that need them in the pack.
// It identifies which handlers need files but don't have them, and creates
// appropriate template files for each handler.
func (p *Pack) Fill(fs types.FS) (*FillResult, error) {
	log := logging.GetLogger("pack.fill")
	log.Debug().Str("pack", p.Name).Msg("Starting fill operation")

	// Get handlers that need files - note we pass the embedded types.Pack
	handlersNeeding, err := rules.GetHandlersNeedingFiles(*p.Pack, fs)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrInternal, "failed to get handlers needing files")
	}

	// Build list of templates to create
	missingTemplates := []struct {
		Filename    string
		Content     string
		Mode        uint32
		HandlerName string
	}{}

	for _, handlerName := range handlersNeeding {
		// Get patterns for this handler
		patterns, err := rules.GetPatternsForHandler(handlerName, *p.Pack)
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

		// Get template content from handler
		if h, ok := handler.(interface{ GetTemplateContent() string }); ok {
			templateContent = h.GetTemplateContent()
			// Provisioning scripts should be executable
			if handlerName == "install" {
				fileMode = 0755
			}
		} else if handlerName != "path" {
			// Path handler doesn't provide template content, but that's expected
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

	// Create each missing template file
	var filesCreated []string
	for _, template := range missingTemplates {
		// Handle directory creation for path handler
		if template.HandlerName == "path" && template.Content == "" {
			log.Info().Str("dir", template.Filename).Str("handler", template.HandlerName).Msg("Creating directory")
			if err := p.CreateDirectory(fs, template.Filename); err != nil {
				return nil, errors.Wrapf(err, errors.ErrInternal, "failed to create directory %s", template.Filename)
			}
		} else {
			log.Info().Str("file", template.Filename).Str("handler", template.HandlerName).Msg("Creating template file")
			if err := p.CreateFileWithMode(fs, template.Filename, template.Content, os.FileMode(template.Mode)); err != nil {
				return nil, errors.Wrapf(err, errors.ErrInternal, "failed to create template file %s", template.Filename)
			}
		}
		filesCreated = append(filesCreated, template.Filename)
	}

	// Return result
	result := &FillResult{
		FilesCreated: filesCreated,
	}

	log.Info().
		Str("pack", p.Name).
		Str("path", p.Path).
		Int("filesCreated", len(filesCreated)).
		Msg("Fill operation completed")

	return result, nil
}
