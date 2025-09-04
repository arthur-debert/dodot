package commands

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/packs/execution"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
)

// AdoptCommand implements the "adopt" command using the pack execution.
// It moves files from the system into a pack.
type AdoptCommand struct {
	// SourcePaths are the files to adopt
	SourcePaths []string
	// Force allows overwriting existing files
	Force bool
}

// Name returns the command name.
func (c *AdoptCommand) Name() string {
	return "adopt"
}

// ExecuteForPack adopts files into a pack.
func (c *AdoptCommand) ExecuteForPack(pack types.Pack, opts execution.Options) (*execution.PackResult, error) {
	logger := logging.GetLogger("execution.adopt")
	logger.Debug().
		Str("pack", pack.Name).
		Int("files", len(c.SourcePaths)).
		Bool("force", c.Force).
		Msg("Executing adopt command for pack")

	// Initialize filesystem
	fs := opts.FileSystem
	if fs == nil {
		fs = filesystem.NewOS()
	}

	// We don't need paths instance for adopt - removed unused variable

	// Track results
	var adoptedFiles []string
	var errors []error

	// Process each source file
	for _, sourcePath := range c.SourcePaths {
		// Expand tilde if present
		expandedPath := paths.ExpandHome(sourcePath)

		// Get absolute path
		absPath, err := filepath.Abs(expandedPath)
		if err != nil {
			errors = append(errors, fmt.Errorf("invalid path %s: %w", sourcePath, err))
			continue
		}

		// Check if source exists
		sourceInfo, err := fs.Stat(absPath)
		if err != nil {
			if os.IsNotExist(err) {
				errors = append(errors, fmt.Errorf("file not found: %s", sourcePath))
			} else {
				errors = append(errors, fmt.Errorf("cannot access %s: %w", sourcePath, err))
			}
			continue
		}

		// Skip directories
		if sourceInfo.IsDir() {
			errors = append(errors, fmt.Errorf("cannot adopt directory: %s", sourcePath))
			continue
		}

		// Determine destination
		fileName := filepath.Base(absPath)
		destPath := filepath.Join(pack.Path, fileName)

		// Check if destination exists
		if _, err := fs.Stat(destPath); err == nil && !c.Force {
			errors = append(errors, fmt.Errorf("file already exists in pack: %s (use --force to overwrite)", fileName))
			continue
		}

		// Read source file
		content, err := fs.ReadFile(absPath)
		if err != nil {
			errors = append(errors, fmt.Errorf("cannot read %s: %w", sourcePath, err))
			continue
		}

		// Write to destination
		if err := fs.WriteFile(destPath, content, sourceInfo.Mode()); err != nil {
			errors = append(errors, fmt.Errorf("cannot write to pack: %w", err))
			continue
		}

		// Remove original file
		if err := fs.Remove(absPath); err != nil {
			// Try to rollback
			_ = fs.Remove(destPath)
			errors = append(errors, fmt.Errorf("cannot remove original file: %w", err))
			continue
		}

		adoptedFiles = append(adoptedFiles, fileName)
		logger.Info().
			Str("source", sourcePath).
			Str("dest", destPath).
			Msg("Adopted file")
	}

	// Determine success
	success := len(errors) == 0
	var finalError error
	if len(errors) > 0 {
		finalError = fmt.Errorf("adopt encountered %d errors", len(errors))
	}

	// Create status result
	statusResult := &StatusResult{
		Name:   pack.Name,
		Status: "success",
		Files:  []FileStatus{},
	}

	// Add adopted files to status
	for _, fileName := range adoptedFiles {
		statusResult.Files = append(statusResult.Files, FileStatus{
			Path:   fileName,
			Status: Status{State: StatusStateSuccess},
		})
	}

	logger.Info().
		Str("pack", pack.Name).
		Int("adopted", len(adoptedFiles)).
		Int("errors", len(errors)).
		Bool("success", success).
		Msg("Adopt command completed")

	return &execution.PackResult{
		Pack:                  pack,
		Success:               success,
		Error:                 finalError,
		CommandSpecificResult: statusResult,
	}, finalError
}
