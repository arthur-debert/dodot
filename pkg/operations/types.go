package operations

import (
	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/types"
)

// OperationType represents the fundamental operations that dodot performs.
// This is the core insight: dodot only does 4 things, everything else is orchestration.
type OperationType int

const (
	// CreateDataLink creates a link in the datastore pointing to a source file.
	// This is used by symlink, path, and shell handlers to stage files.
	CreateDataLink OperationType = iota

	// CreateUserLink creates a user-visible symlink pointing to the datastore.
	// This is the final step for symlink handler to make files accessible.
	CreateUserLink

	// RunCommand executes a command and records completion with a sentinel.
	// This is used by install and homebrew handlers for provisioning.
	RunCommand

	// CheckSentinel queries if an operation has been completed.
	// This prevents re-running expensive operations.
	CheckSentinel
)

// Operation represents a single atomic unit of work to be performed.
// Operations are the bridge between handlers (which understand file patterns)
// and the datastore (which only knows how to perform these 4 operations).
type Operation struct {
	Type     OperationType
	Pack     string
	Handler  string
	Source   string                 // For link operations: source file path
	Target   string                 // For link operations: target path
	Command  string                 // For RunCommand: command to execute
	Sentinel string                 // For RunCommand/CheckSentinel: completion marker
	Metadata map[string]interface{} // Handler-specific data for customization
}

// OperationResult captures the outcome of executing an operation.
// This is used for status reporting and dry-run output.
type OperationResult struct {
	Operation Operation
	Success   bool
	Message   string
	Error     error
}

// HandlerMetadata provides UI/UX information about a handler.
// This separates presentation concerns from operation logic.
type HandlerMetadata struct {
	Description     string // Human-readable description
	RequiresConfirm bool   // Whether operations need user confirmation
	CanRunMultiple  bool   // Whether handler supports multiple executions
}

// Handler is the simplified interface that all handlers implement.
// The key insight: handlers are just data transformers, not orchestrators.
type Handler interface {
	// Core identification
	Name() string
	Category() handlers.HandlerCategory

	// Core responsibility: transform file matches to operations
	// This is the heart of the simplification - handlers just declare
	// what operations they need, not how to perform them.
	ToOperations(matches []types.RuleMatch) ([]Operation, error)

	// Metadata for UI/UX
	GetMetadata() HandlerMetadata

	// Optional customization points with sensible defaults.
	// Most handlers won't need to implement these.
	GetClearConfirmation(ctx ClearContext) *ConfirmationRequest
	FormatClearedItem(item ClearedItem, dryRun bool) string
	ValidateOperations(ops []Operation) error
	GetStateDirectoryName() string
}

// ConfirmationRequest represents a request for user confirmation.
// This allows handlers like homebrew to customize their confirmation flow.
type ConfirmationRequest struct {
	ID          string   // Unique identifier for the confirmation
	Title       string   // Question to ask the user
	Description string   // Additional context
	Items       []string // List of items affected (e.g., packages to uninstall)
}

// ClearedItem represents something that was removed during a clear operation
type ClearedItem struct {
	Type        string // "symlink", "brew_package", "script_output", etc.
	Path        string // What was removed/affected
	Description string // Human-readable description
}

// ClearContext provides all the resources needed for a handler to clean up
type ClearContext struct {
	Pack   types.Pack   // The pack being cleared
	FS     types.FS     // For file operations
	Paths  types.Pather // For path resolution
	DryRun bool         // Whether this is a dry run
}

// BaseHandler provides default implementations for optional handler methods.
// This is crucial for keeping handlers simple - they only override what they need.
type BaseHandler struct {
	name     string
	category handlers.HandlerCategory
}

// NewBaseHandler creates a new BaseHandler with the given name and category.
func NewBaseHandler(name string, category handlers.HandlerCategory) BaseHandler {
	return BaseHandler{
		name:     name,
		category: category,
	}
}

func (h *BaseHandler) Name() string                       { return h.name }
func (h *BaseHandler) Category() handlers.HandlerCategory { return h.category }

// Default implementations return empty/nil to use system defaults
func (h *BaseHandler) GetClearConfirmation(ctx ClearContext) *ConfirmationRequest {
	return nil
}
func (h *BaseHandler) FormatClearedItem(item ClearedItem, dryRun bool) string { return "" }
func (h *BaseHandler) ValidateOperations(ops []Operation) error               { return nil }
func (h *BaseHandler) GetStateDirectoryName() string                          { return "" }
