package symlink

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/operations"
)

const SymlinkHandlerName = "symlink"

// Handler implements the new simplified handler interface.
// It transforms symlink requests into operations without performing any I/O.
type Handler struct {
	operations.BaseHandler
}

// NewHandler creates a new simplified symlink handler.
func NewHandler() *Handler {
	return &Handler{
		BaseHandler: operations.NewBaseHandler(SymlinkHandlerName, operations.CategoryConfiguration),
	}
}

// ToOperations converts file inputs to symlink operations.
// Symlinks require two operations:
// 1. CreateDataLink to store the link in the datastore
// 2. CreateUserLink to create the user-visible symlink
func (h *Handler) ToOperations(files []operations.FileInput) ([]operations.Operation, error) {
	var ops []operations.Operation

	// Get target directory from first file's options or use home
	targetDir := h.getTargetDir(files)

	// Track targets to detect conflicts early
	targetMap := make(map[string]string)

	for _, file := range files {
		// Determine target path
		targetPath := h.computeTargetPath(targetDir, file)

		// Check for conflicts
		if existingSource, exists := targetMap[targetPath]; exists {
			return nil, fmt.Errorf("symlink conflict: both %s and %s want to link to %s",
				existingSource, file.SourcePath, targetPath)
		}
		targetMap[targetPath] = file.SourcePath

		// Create operations
		ops = append(ops,
			operations.Operation{
				Type:    operations.CreateDataLink,
				Pack:    file.PackName,
				Handler: SymlinkHandlerName,
				Source:  file.SourcePath,
			},
			operations.Operation{
				Type:    operations.CreateUserLink,
				Pack:    file.PackName,
				Handler: SymlinkHandlerName,
				Source:  file.SourcePath, // Will be resolved to datastore path
				Target:  targetPath,
			},
		)
	}

	return ops, nil
}

// GetMetadata returns handler metadata.
func (h *Handler) GetMetadata() operations.HandlerMetadata {
	return operations.HandlerMetadata{
		Description:     "Creates symbolic links from dotfiles to target locations",
		RequiresConfirm: false, // Symlinks don't need confirmation
		CanRunMultiple:  true,  // Can link multiple times
	}
}

// GetClearConfirmation returns nil - symlinks don't need clear confirmation.
func (h *Handler) GetClearConfirmation(ctx operations.ClearContext) *operations.ConfirmationRequest {
	return nil
}

// FormatClearedItem formats how cleared symlinks are displayed.
func (h *Handler) FormatClearedItem(item operations.ClearedItem, dryRun bool) string {
	if dryRun {
		return fmt.Sprintf("Would remove symlink %s", filepath.Base(item.Path))
	}
	return fmt.Sprintf("Removed symlink %s", filepath.Base(item.Path))
}

// getTargetDir extracts the target directory from files or returns default.
func (h *Handler) getTargetDir(files []operations.FileInput) string {
	if len(files) > 0 && files[0].Options != nil {
		if target, ok := files[0].Options["target"].(string); ok {
			return os.ExpandEnv(target)
		}
	}

	// Default to home directory
	homeDir := os.Getenv("HOME")
	if homeDir == "" {
		homeDir, _ = os.UserHomeDir()
		if homeDir == "" {
			homeDir = "~"
		}
	}
	return homeDir
}

// computeTargetPath determines where a symlink should point.
func (h *Handler) computeTargetPath(targetDir string, file operations.FileInput) string {
	// Simple case: just join target directory with the relative path
	// The executor will handle path mapping complexity
	return filepath.Join(targetDir, file.RelativePath)
}

// Verify interface compliance
var _ operations.Handler = (*Handler)(nil)
