package symlink

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/operations"
	"github.com/arthur-debert/dodot/pkg/types"
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
		BaseHandler: operations.NewBaseHandler(SymlinkHandlerName, handlers.CategoryConfiguration),
	}
}

// ToOperations converts rule matches to symlink operations.
// Symlinks require two operations:
// 1. CreateDataLink to store the link in the datastore
// 2. CreateUserLink to create the user-visible symlink
func (h *Handler) ToOperations(matches []types.RuleMatch) ([]operations.Operation, error) {
	var ops []operations.Operation

	// Get target directory from first match options or use home
	targetDir := h.getTargetDir(matches)

	// Track targets to detect conflicts early
	targetMap := make(map[string]string)

	for _, match := range matches {
		// Determine target path
		targetPath := h.computeTargetPath(targetDir, match)

		// Check for conflicts
		if existingSource, exists := targetMap[targetPath]; exists {
			return nil, fmt.Errorf("symlink conflict: both %s and %s want to link to %s",
				existingSource, match.AbsolutePath, targetPath)
		}
		targetMap[targetPath] = match.AbsolutePath

		// Create operations
		ops = append(ops,
			operations.Operation{
				Type:    operations.CreateDataLink,
				Pack:    match.Pack,
				Handler: SymlinkHandlerName,
				Source:  match.AbsolutePath,
			},
			operations.Operation{
				Type:    operations.CreateUserLink,
				Pack:    match.Pack,
				Handler: SymlinkHandlerName,
				Source:  match.AbsolutePath, // Will be resolved to datastore path
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
func (h *Handler) GetClearConfirmation(ctx types.ClearContext) *operations.ConfirmationRequest {
	return nil
}

// FormatClearedItem formats how cleared symlinks are displayed.
func (h *Handler) FormatClearedItem(item types.ClearedItem, dryRun bool) string {
	if dryRun {
		return fmt.Sprintf("Would remove symlink %s", filepath.Base(item.Path))
	}
	return fmt.Sprintf("Removed symlink %s", filepath.Base(item.Path))
}

// getTargetDir extracts the target directory from matches or returns default.
func (h *Handler) getTargetDir(matches []types.RuleMatch) string {
	if len(matches) > 0 && matches[0].HandlerOptions != nil {
		if target, ok := matches[0].HandlerOptions["target"].(string); ok {
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
func (h *Handler) computeTargetPath(targetDir string, match types.RuleMatch) string {
	// Simple case: just join target directory with the relative path
	// The executor will handle path mapping complexity
	return filepath.Join(targetDir, match.Path)
}

// Verify interface compliance
var _ operations.Handler = (*Handler)(nil)
