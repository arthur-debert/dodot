package packcommands

import (
	"fmt"
	"os"
	"strings"
	"time"

	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/rules"
	"github.com/arthur-debert/dodot/pkg/statustype"
	"github.com/arthur-debert/dodot/pkg/types"
)

// FillOptions contains options for the Fill operation
type FillOptions struct {
	// PackName is the name of the pack to fill
	PackName string
	// DotfilesRoot is the root directory for dotfiles
	DotfilesRoot string
	// FileSystem is the filesystem to use (optional, defaults to OS filesystem)
	FileSystem types.FS
	// GetPackStatus is a function to get pack status to avoid circular imports
	GetPackStatus statustype.GetPackStatusFunc
}

// Fill adds template files for handlers that need them in the pack.
// It identifies which handlers need files but don't have them, and creates
// appropriate template files for each handler.
func Fill(opts FillOptions) (*types.PackCommandResult, error) {
	log := logging.GetLogger("pack.fill")
	log.Debug().Str("pack", opts.PackName).Msg("Starting fill operation")

	// Use provided filesystem or default
	fs := opts.FileSystem
	if fs == nil {
		fs = filesystem.NewOS()
	}

	// Find the specific pack
	targetPack, err := findPack(opts.DotfilesRoot, opts.PackName, fs)
	if err != nil {
		return nil, err
	}

	// Wrap in our enhanced Pack type
	p := New(targetPack)

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

	log.Info().
		Str("pack", p.Name).
		Str("path", p.Path).
		Int("filesCreated", len(filesCreated)).
		Msg("Fill operation completed")

	// Get current pack status if function provided
	var packStatus []types.DisplayPack
	if opts.GetPackStatus != nil {
		var statusErr error
		packStatus, statusErr = opts.GetPackStatus(opts.PackName, opts.DotfilesRoot, fs)
		if statusErr != nil {
			log.Error().Err(statusErr).Msg("Failed to get pack status")
		}
	}

	// Build result
	result := &types.PackCommandResult{
		Command:   "fill",
		Timestamp: time.Now(),
		DryRun:    false,
		Packs:     []types.DisplayPack{},
		Metadata: types.CommandMetadata{
			FilesCreated: len(filesCreated),
			CreatedPaths: filesCreated,
		},
	}

	// Copy packs from status if available
	if packStatus != nil {
		result.Packs = packStatus
	}

	// Generate message
	if len(filesCreated) == 1 {
		result.Message = "The pack " + opts.PackName + " has been filled with 1 placeholder file."
	} else {
		result.Message = fmt.Sprintf("The pack %s has been filled with %d placeholder files.", opts.PackName, len(filesCreated))
	}

	return result, nil
}

// findPack is a helper to find a pack by name without importing core package
func findPack(dotfilesRoot, packName string, fs types.FS) (*types.Pack, error) {
	if packName == "" {
		return nil, fmt.Errorf("pack(s) not found")
	}

	packPath := dotfilesRoot + "/" + packName
	info, err := fs.Stat(packPath)
	if err != nil {
		return nil, fmt.Errorf("pack(s) not found")
	}

	if !info.IsDir() {
		return nil, fmt.Errorf("pack(s) not found")
	}

	return &types.Pack{
		Name: packName,
		Path: packPath,
	}, nil
}
