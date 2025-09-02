package commands

import (
	"fmt"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/packs/commands"
	"github.com/arthur-debert/dodot/pkg/packs/pipeline"
	"github.com/arthur-debert/dodot/pkg/types"
)

// FillCommand implements the "fill" command using the pack pipeline.
// It populates a pack with template files.
type FillCommand struct{}

// Name returns the command name.
func (c *FillCommand) Name() string {
	return "fill"
}

// ExecuteForPack fills a pack with template files.
func (c *FillCommand) ExecuteForPack(pack types.Pack, opts pipeline.Options) (*pipeline.PackResult, error) {
	logger := logging.GetLogger("pipeline.fill")
	logger.Debug().
		Str("pack", pack.Name).
		Msg("Executing fill command for pack")

	// Initialize filesystem
	fs := opts.FileSystem
	if fs == nil {
		fs = filesystem.NewOS()
	}

	// Create template files
	files := []struct {
		name    string
		content string
	}{
		{
			name:    ".dodot.toml",
			content: generatePackConfig(pack.Name),
		},
		{
			name:    "README.md",
			content: generateReadme(pack.Name),
		},
		{
			name:    ".gitkeep",
			content: "",
		},
	}

	// Create each template file
	var createdFiles []string
	for _, file := range files {
		filePath := filepath.Join(pack.Path, file.name)

		// Check if file already exists
		if _, err := fs.Stat(filePath); err == nil {
			logger.Debug().
				Str("file", file.name).
				Msg("File already exists, skipping")
			continue
		}

		// Write file
		if err := fs.WriteFile(filePath, []byte(file.content), 0644); err != nil {
			logger.Error().
				Err(err).
				Str("file", file.name).
				Msg("Failed to create file")
			return &pipeline.PackResult{
				Pack:    pack,
				Success: false,
				Error:   fmt.Errorf("failed to create %s: %w", file.name, err),
			}, err
		}

		createdFiles = append(createdFiles, file.name)
		logger.Info().
			Str("file", file.name).
			Msg("Created template file")
	}

	// We can create a minimal status result since fill doesn't interact with handlers
	statusResult := &commands.StatusResult{
		Name:      pack.Name,
		HasConfig: true, // We just created the config
		Status:    "success",
		Files:     []commands.FileStatus{},
	}

	// Add created files to status
	for _, fileName := range createdFiles {
		statusResult.Files = append(statusResult.Files, commands.FileStatus{
			Path:    fileName,
			Status:  commands.Status{State: commands.StatusStateSuccess},
			Handler: "none", // Template files don't have handlers
		})
	}

	logger.Info().
		Str("pack", pack.Name).
		Int("filesCreated", len(createdFiles)).
		Msg("Fill command completed for pack")

	return &pipeline.PackResult{
		Pack:                  pack,
		Success:               true,
		Error:                 nil,
		CommandSpecificResult: statusResult,
	}, nil
}

// generatePackConfig creates a default pack configuration file
func generatePackConfig(packName string) string {
	return fmt.Sprintf(`# dodot pack configuration for %s

# Enable handlers for this pack
[handlers]
enabled = ["symlink", "homebrew", "shell", "install"]

# Define rules for file matching
[[rules]]
type = "filename"
pattern = "*"
handler = "symlink"
priority = 100

# Example: Handle shell scripts differently
# [[rules]]
# type = "glob"
# pattern = "*.sh"
# handler = "shell"
# priority = 200
`, packName)
}

// generateReadme creates a default README for the pack
func generateReadme(packName string) string {
	return fmt.Sprintf(`# %s

This is a dodot pack for %s configuration files.

## Structure

Place your %s configuration files in this directory. They will be automatically
linked to your home directory when you run:

    dodot on %s

## Customization

Edit the .dodot.toml file to customize how files are handled.

## More Information

See https://github.com/arthur-debert/dodot for more information.
`, packName, packName, packName, packName)
}
