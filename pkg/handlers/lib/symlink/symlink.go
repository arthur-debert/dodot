package symlink

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/arthur-debert/dodot/pkg/config"
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
func (h *Handler) ToOperations(files []operations.FileInput, config interface{}) ([]operations.Operation, error) {
	var ops []operations.Operation

	// Get target directory from first file's options or use home
	targetDir := h.getTargetDir(files)

	// Track targets to detect conflicts early
	targetMap := make(map[string]string)

	for _, file := range files {
		// Get protected paths from config if available
		protectedPaths := getProtectedPaths(config)

		// Check if this file path is protected
		if isProtected(file.RelativePath, protectedPaths) {
			return nil, fmt.Errorf("cannot symlink protected file: %s", file.RelativePath)
		}

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

// CheckStatus checks if the symlink has been created for the given file
func (h *Handler) CheckStatus(file operations.FileInput, checker operations.StatusChecker) (operations.HandlerStatus, error) {
	// Check if the data link exists in the datastore
	exists, err := checker.HasDataLink(file.PackName, h.Name(), file.RelativePath)
	if err != nil {
		return operations.HandlerStatus{
			State:   operations.StatusStateError,
			Message: fmt.Sprintf("Failed to check link status: %v", err),
		}, err
	}

	if exists {
		// Link exists in datastore
		return operations.HandlerStatus{
			State:   operations.StatusStateReady,
			Message: fmt.Sprintf("linked to $HOME/%s", filepath.Base(file.RelativePath)),
		}, nil
	}

	// Link doesn't exist
	return operations.HandlerStatus{
		State:   operations.StatusStatePending,
		Message: fmt.Sprintf("will be linked to $HOME/%s", filepath.Base(file.RelativePath)),
	}, nil
}

// isProtected checks if a file path matches any protected path pattern
func isProtected(filePath string, protectedPaths map[string]bool) bool {
	// Normalize the path by removing leading dots and slashes
	normalizedPath := strings.TrimPrefix(filePath, "./")
	normalizedPath = strings.TrimPrefix(normalizedPath, ".")

	// Check exact match
	if protectedPaths[normalizedPath] {
		return true
	}

	// Check with dot prefix (e.g., "ssh/id_rsa" matches ".ssh/id_rsa")
	if protectedPaths["."+normalizedPath] {
		return true
	}

	// Check if any parent directory is protected
	// This handles cases like ".gnupg/private-keys-v1.d/..." being protected by ".gnupg"
	parts := strings.Split(normalizedPath, string(filepath.Separator))
	for i := 1; i <= len(parts); i++ {
		parentPath := strings.Join(parts[:i], string(filepath.Separator))
		if protectedPaths[parentPath] || protectedPaths["."+parentPath] {
			return true
		}
	}

	return false
}

// getProtectedPaths extracts protected paths from the config
func getProtectedPaths(cfg interface{}) map[string]bool {
	// Try to cast to config.Config
	if configData, ok := cfg.(*config.Config); ok && configData != nil {
		// Return the protected paths from config (which already includes defaults)
		if configData.Security.ProtectedPaths != nil {
			return configData.Security.ProtectedPaths
		}
	}

	// Fallback to hardcoded defaults if no config available
	// This should rarely happen as config should always be provided
	return map[string]bool{
		".ssh/id_rsa":          true,
		".ssh/id_ed25519":      true,
		".ssh/id_dsa":          true,
		".ssh/id_ecdsa":        true,
		".gnupg":               true,
		".aws/credentials":     true,
		".ssh/authorized_keys": true,
		".password-store":      true,
		".config/gh/hosts.yml": true,
		".kube/config":         true,
		".docker/config.json":  true,
	}
}

// Verify interface compliance
var _ operations.Handler = (*Handler)(nil)
