package path

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/operations"
)

// Handler demonstrates how the path handler looks in the new architecture.
// This is a proof of concept for phase 1 - it converts matches to operations.
// Compare this ~40 lines to the current ~185 lines implementation.
type Handler struct {
	operations.BaseHandler
}

// NewHandler creates a new simplified path handler.
// This will eventually replace the current NewHandler function.
func NewHandler() *Handler {
	return &Handler{
		BaseHandler: operations.BaseHandler{
			// These fields would be exported in real implementation
		},
	}
}

// Name returns the handler name.
func (h *Handler) Name() string {
	return "path"
}

// Category returns the handler category.
func (h *Handler) Category() operations.HandlerCategory {
	// Path handler is a configuration handler - it creates links that shell init reads
	return operations.CategoryConfiguration
}

// GetMetadata provides UI information about the handler.
func (h *Handler) GetMetadata() operations.HandlerMetadata {
	return operations.HandlerMetadata{
		Description:     "Adds directories to shell PATH",
		RequiresConfirm: false, // PATH additions are safe
		CanRunMultiple:  true,  // Can add multiple directories
	}
}

// ToOperations converts file inputs to operations.
// This is the core simplification - the handler just declares what it wants,
// not how to do it. The executor handles the complexity.
func (h *Handler) ToOperations(files []operations.FileInput, config interface{}) ([]operations.Operation, error) {
	var ops []operations.Operation

	// Deduplicate paths - same logic as current handler but simpler
	seen := make(map[string]bool)

	for _, file := range files {
		key := fmt.Sprintf("%s:%s", file.PackName, file.RelativePath)
		if seen[key] {
			continue // Skip duplicates
		}
		seen[key] = true

		// Path handler only needs one operation per directory:
		// Create a link in the datastore that shell init will read
		ops = append(ops, operations.Operation{
			Type:    operations.CreateDataLink,
			Pack:    file.PackName,
			Handler: "path",
			Source:  file.RelativePath,
			// No Target needed - shell init handles PATH management
			// No Command needed - this is a linking operation
			// No Sentinel needed - links are their own state
		})
	}

	return ops, nil
}

// CheckStatus checks if the PATH entry has been linked
func (h *Handler) CheckStatus(file operations.FileInput, checker operations.StatusChecker) (operations.HandlerStatus, error) {
	// Check if the data link exists in the datastore
	exists, err := checker.HasDataLink(file.PackName, h.Name(), file.RelativePath)
	if err != nil {
		return operations.HandlerStatus{
			State:   operations.StatusStateError,
			Message: "Failed to check PATH entry status",
		}, err
	}

	if exists {
		// PATH entry is linked
		return operations.HandlerStatus{
			State:   operations.StatusStateReady,
			Message: "added to PATH",
		}, nil
	}

	// PATH entry not linked
	return operations.HandlerStatus{
		State:   operations.StatusStatePending,
		Message: "not in PATH",
	}, nil
}

// The following methods use the BaseHandler defaults:
// - GetClearConfirmation: returns nil (no confirmation needed)
// - FormatClearedItem: returns "" (use default formatting)
// - ValidateOperations: returns nil (no validation needed)
// - GetStateDirectoryName: returns "" (use handler name "path")

// That's it! The entire handler in ~40 lines vs ~185 lines currently.
// All the complexity of path resolution, state management, logging, etc.
// is handled by the operation executor and datastore.
