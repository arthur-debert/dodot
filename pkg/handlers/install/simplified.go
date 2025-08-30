package install

import (
	_ "embed"
	"fmt"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/internal/hashutil"
	"github.com/arthur-debert/dodot/pkg/operations"
	"github.com/arthur-debert/dodot/pkg/types"
)

// SimplifiedHandler implements the new simplified handler interface.
// It transforms install script requests into operations without performing any I/O.
type SimplifiedHandler struct {
	operations.BaseHandler
}

// NewSimplifiedHandler creates a new simplified install handler.
func NewSimplifiedHandler() *SimplifiedHandler {
	return &SimplifiedHandler{
		BaseHandler: operations.NewBaseHandler(InstallHandlerName, handlers.CategoryCodeExecution),
	}
}

// ToOperations converts rule matches to install operations.
// Install scripts use RunCommand for execution with sentinel tracking.
func (h *SimplifiedHandler) ToOperations(matches []types.RuleMatch) ([]operations.Operation, error) {
	var ops []operations.Operation

	for _, match := range matches {
		// Calculate checksum for idempotency
		checksum, err := hashutil.CalculateFileChecksum(match.AbsolutePath)
		if err != nil {
			return nil, fmt.Errorf("failed to calculate checksum for %s: %w", match.AbsolutePath, err)
		}

		// Create sentinel name from script filename
		sentinelName := fmt.Sprintf("%s-%s", filepath.Base(match.Path), checksum)

		// Install scripts are executed with RunCommand
		// The executor will check the sentinel and skip if already run
		ops = append(ops, operations.Operation{
			Type:     operations.RunCommand,
			Pack:     match.Pack,
			Handler:  InstallHandlerName,
			Command:  fmt.Sprintf("bash '%s'", match.AbsolutePath),
			Sentinel: sentinelName,
		})
	}

	return ops, nil
}

// GetMetadata returns handler metadata.
func (h *SimplifiedHandler) GetMetadata() operations.HandlerMetadata {
	return operations.HandlerMetadata{
		Description:     "Runs install.sh scripts for initial setup",
		RequiresConfirm: false, // Install scripts don't need confirmation
		CanRunMultiple:  false, // Only run once per checksum
	}
}

// GetTemplateContent returns the template content for this handler.
func (h *SimplifiedHandler) GetTemplateContent() string {
	return provisionTemplate
}

// GetStateDirectoryName returns the directory name for storing state.
func (h *SimplifiedHandler) GetStateDirectoryName() string {
	return "install"
}

// FormatClearedItem formats a cleared item for display.
func (h *SimplifiedHandler) FormatClearedItem(item types.ClearedItem, dryRun bool) string {
	if dryRun {
		return "Would remove install run records"
	}
	return "Removing install run records"
}

// Verify interface compliance
var _ operations.Handler = (*SimplifiedHandler)(nil)
