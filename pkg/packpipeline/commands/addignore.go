package commands

import (
	"fmt"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/packcommands"
	"github.com/arthur-debert/dodot/pkg/packpipeline"
	"github.com/arthur-debert/dodot/pkg/types"
)

// AddIgnoreCommand implements the "add-ignore" command using the pack pipeline.
// It creates a .dodotignore file in a pack.
type AddIgnoreCommand struct{}

// Name returns the command name.
func (c *AddIgnoreCommand) Name() string {
	return "add-ignore"
}

// ExecuteForPack creates a .dodotignore file in the pack.
func (c *AddIgnoreCommand) ExecuteForPack(pack types.Pack, opts packpipeline.Options) (*packpipeline.PackResult, error) {
	logger := logging.GetLogger("packpipeline.addignore")
	logger.Debug().
		Str("pack", pack.Name).
		Msg("Executing add-ignore command for pack")

	// Initialize filesystem
	fs := opts.FileSystem
	if fs == nil {
		fs = filesystem.NewOS()
	}

	// Check if pack is already ignored
	ignoreFileName := ".dodotignore"
	ignorePath := filepath.Join(pack.Path, ignoreFileName)
	if _, err := fs.Stat(ignorePath); err == nil {
		logger.Info().
			Str("pack", pack.Name).
			Msg("Pack already has .dodotignore file")

		return &packpipeline.PackResult{
			Pack:    pack,
			Success: true,
			Error:   nil,
			CommandSpecificResult: &packcommands.StatusResult{
				Name:      pack.Name,
				IsIgnored: true,
				Status:    "ignored",
				Files: []packcommands.FileStatus{
					{
						Path:   ignoreFileName,
						Status: packcommands.Status{State: packcommands.StatusStateIgnored},
					},
				},
			},
		}, nil
	}

	// Create ignore file content
	content := fmt.Sprintf(`# This file marks the %s pack as ignored by dodot
# Remove this file to re-enable the pack

# Created by: dodot add-ignore %s
`, pack.Name, pack.Name)

	// Write ignore file
	if err := fs.WriteFile(ignorePath, []byte(content), 0644); err != nil {
		logger.Error().
			Err(err).
			Str("pack", pack.Name).
			Msg("Failed to create .dodotignore file")

		return &packpipeline.PackResult{
			Pack:    pack,
			Success: false,
			Error:   fmt.Errorf("failed to create .dodotignore: %w", err),
		}, err
	}

	logger.Info().
		Str("pack", pack.Name).
		Str("file", ignorePath).
		Msg("Created .dodotignore file")

	// Create status result
	statusResult := &packcommands.StatusResult{
		Name:      pack.Name,
		IsIgnored: true,
		Status:    "ignored",
		Files: []packcommands.FileStatus{
			{
				Path:   ignoreFileName,
				Status: packcommands.Status{State: packcommands.StatusStateIgnored},
			},
		},
	}

	return &packpipeline.PackResult{
		Pack:                  pack,
		Success:               true,
		Error:                 nil,
		CommandSpecificResult: statusResult,
	}, nil
}
