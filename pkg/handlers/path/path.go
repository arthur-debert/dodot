package path

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/operations"
	"github.com/arthur-debert/dodot/pkg/types"
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
func (h *Handler) Category() handlers.HandlerCategory {
	// Path handler is a configuration handler - it creates links that shell init reads
	return handlers.CategoryConfiguration
}

// GetMetadata provides UI information about the handler.
func (h *Handler) GetMetadata() operations.HandlerMetadata {
	return operations.HandlerMetadata{
		Description:     "Adds directories to shell PATH",
		RequiresConfirm: false, // PATH additions are safe
		CanRunMultiple:  true,  // Can add multiple directories
	}
}

// ToOperations converts rule matches to operations.
// This is the core simplification - the handler just declares what it wants,
// not how to do it. The executor handles the complexity.
func (h *Handler) ToOperations(matches []types.RuleMatch) ([]operations.Operation, error) {
	var ops []operations.Operation

	// Deduplicate paths - same logic as current handler but simpler
	seen := make(map[string]bool)

	for _, match := range matches {
		key := fmt.Sprintf("%s:%s", match.Pack, match.Path)
		if seen[key] {
			continue // Skip duplicates
		}
		seen[key] = true

		// Path handler only needs one operation per directory:
		// Create a link in the datastore that shell init will read
		ops = append(ops, operations.Operation{
			Type:    operations.CreateDataLink,
			Pack:    match.Pack,
			Handler: "path",
			Source:  match.Path,
			// No Target needed - shell init handles PATH management
			// No Command needed - this is a linking operation
			// No Sentinel needed - links are their own state
		})
	}

	return ops, nil
}

// The following methods use the BaseHandler defaults:
// - GetClearConfirmation: returns nil (no confirmation needed)
// - FormatClearedItem: returns "" (use default formatting)
// - ValidateOperations: returns nil (no validation needed)
// - GetStateDirectoryName: returns "" (use handler name "path")

// That's it! The entire handler in ~40 lines vs ~185 lines currently.
// All the complexity of path resolution, state management, logging, etc.
// is handled by the operation executor and datastore.
