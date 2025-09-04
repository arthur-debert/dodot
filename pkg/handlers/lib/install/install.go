package install

import (
	_ "embed"
	"fmt"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/operations"
	"github.com/arthur-debert/dodot/pkg/utils"
)

const InstallHandlerName = "install"

//go:embed install-template.txt
var provisionTemplate string

// Handler implements the new simplified handler interface.
// It transforms install script requests into operations without performing any I/O.
type Handler struct {
	operations.BaseHandler
}

// NewHandler creates a new simplified install handler.
func NewHandler() *Handler {
	return &Handler{
		BaseHandler: operations.NewBaseHandler(InstallHandlerName, operations.CategoryCodeExecution),
	}
}

// ToOperations converts file inputs to install operations.
// Install scripts use RunCommand for execution with sentinel tracking.
func (h *Handler) ToOperations(files []operations.FileInput) ([]operations.Operation, error) {
	var ops []operations.Operation

	for _, file := range files {
		// Calculate checksum for idempotency
		checksum, err := utils.CalculateFileChecksum(file.SourcePath)
		if err != nil {
			return nil, fmt.Errorf("failed to calculate checksum for %s: %w", file.SourcePath, err)
		}

		// Create sentinel name from script filename
		sentinelName := fmt.Sprintf("%s-%s", filepath.Base(file.RelativePath), checksum)

		// Install scripts are executed with RunCommand
		// The executor will check the sentinel and skip if already run
		ops = append(ops, operations.Operation{
			Type:     operations.RunCommand,
			Pack:     file.PackName,
			Handler:  InstallHandlerName,
			Command:  fmt.Sprintf("bash '%s'", file.SourcePath),
			Sentinel: sentinelName,
		})
	}

	return ops, nil
}

// GetMetadata returns handler metadata.
func (h *Handler) GetMetadata() operations.HandlerMetadata {
	return operations.HandlerMetadata{
		Description:     "Runs install.sh scripts for initial setup",
		RequiresConfirm: false, // Install scripts don't need confirmation
		CanRunMultiple:  false, // Only run once per checksum
	}
}

// GetTemplateContent returns the template content for this handler.
func (h *Handler) GetTemplateContent() string {
	return provisionTemplate
}

// GetStateDirectoryName returns the directory name for storing state.
func (h *Handler) GetStateDirectoryName() string {
	return "install"
}

// FormatClearedItem formats a cleared item for display.
func (h *Handler) FormatClearedItem(item operations.ClearedItem, dryRun bool) string {
	if dryRun {
		return "Would remove install run records"
	}
	return "Removing install run records"
}

// Verify interface compliance
var _ operations.Handler = (*Handler)(nil)
