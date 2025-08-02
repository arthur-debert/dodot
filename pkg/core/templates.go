package core

import (
	"os"
	"path/filepath"
	"strings"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/matchers"
	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/triggers"
)

// PackTemplateFile represents a template file for pack initialization
type PackTemplateFile struct {
	Filename    string // From matcher configuration
	Content     string // From powerup's GetTemplateContent()
	Mode        uint32
	PowerUpName string
}

// GetCompletePackTemplate returns all template files available for a pack
// by iterating through default matchers and getting templates from powerups
func GetCompletePackTemplate(packName string) ([]PackTemplateFile, error) {
	logger := logging.GetLogger("core.templates")
	logger.Debug().Str("pack", packName).Msg("Getting complete pack template")

	var templates []PackTemplateFile

	// Get default matchers
	defaultMatchers := matchers.DefaultMatchers()

	for _, matcher := range defaultMatchers {
		// We only care about filename triggers for templates
		// Directory triggers don't have template files
		triggerFactory, err := registry.GetTriggerFactory(matcher.TriggerName)
		if err != nil {
			logger.Warn().Str("trigger", matcher.TriggerName).Err(err).Msg("Failed to get trigger factory")
			continue
		}

		trigger, err := triggerFactory(matcher.TriggerOptions)
		if err != nil {
			logger.Warn().Str("trigger", matcher.TriggerName).Err(err).Msg("Failed to create trigger")
			continue
		}

		// Check if this is a filename trigger
		if filenameTrigger, ok := trigger.(*triggers.FileNameTrigger); ok {
			// Get the powerup
			powerupFactory, err := registry.GetPowerUpFactory(matcher.PowerUpName)
			if err != nil {
				logger.Warn().Str("powerup", matcher.PowerUpName).Err(err).Msg("Failed to get powerup factory")
				continue
			}

			powerup, err := powerupFactory(matcher.PowerUpOptions)
			if err != nil {
				logger.Warn().Str("powerup", matcher.PowerUpName).Err(err).Msg("Failed to create powerup")
				continue
			}

			// Get template content
			content := powerup.GetTemplateContent()
			if content != "" {
				// Replace PACK_NAME placeholder
				content = strings.ReplaceAll(content, "PACK_NAME", packName)

				// Get filename from pattern, handling wildcards
				filename := filenameTrigger.GetPattern()
				
				// For wildcard patterns, create a concrete filename
				// Convert *aliases.sh to aliases.sh
				filename = strings.TrimPrefix(filename, "*")
				
				// Determine file mode based on file type
				mode := uint32(0644)
				if strings.HasSuffix(filename, ".sh") || filename == "install.sh" {
					mode = 0755
				}

				templates = append(templates, PackTemplateFile{
					Filename:    filename,
					Content:     content,
					Mode:        mode,
					PowerUpName: powerup.Name(),
				})

				logger.Debug().
					Str("filename", filename).
					Str("powerup", powerup.Name()).
					Msg("Added template file")
			}
		}
	}

	logger.Info().
		Int("templateCount", len(templates)).
		Str("pack", packName).
		Msg("Generated complete pack template")

	return templates, nil
}

// GetMissingTemplateFiles returns template files that don't exist in the given pack directory
func GetMissingTemplateFiles(packPath string, packName string) ([]PackTemplateFile, error) {
	logger := logging.GetLogger("core.templates")
	logger.Debug().Str("packPath", packPath).Msg("Getting missing template files")

	// Get all available templates
	allTemplates, err := GetCompletePackTemplate(packName)
	if err != nil {
		return nil, err
	}

	var missingTemplates []PackTemplateFile

	// Check which ones don't exist
	for _, template := range allTemplates {
		filePath := filepath.Join(packPath, template.Filename)

		// Check if file exists
		exists, err := fileExists(filePath)
		if err != nil {
			logger.Warn().Str("file", filePath).Err(err).Msg("Failed to check file existence")
			continue
		}

		if !exists {
			missingTemplates = append(missingTemplates, template)
			logger.Debug().Str("file", template.Filename).Msg("Template file missing")
		}
	}

	logger.Info().
		Int("missingCount", len(missingTemplates)).
		Int("totalCount", len(allTemplates)).
		Str("packPath", packPath).
		Msg("Identified missing template files")

	return missingTemplates, nil
}

// fileExists checks if a file exists
func fileExists(path string) (bool, error) {
	_, err := os.Stat(path)
	if err == nil {
		return true, nil
	}
	if os.IsNotExist(err) {
		return false, nil
	}
	return false, err
}
